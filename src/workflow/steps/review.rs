//! Review step runners.
//!
//! Contains the workflow step handler for running reviews. Domain types and
//! logic (scope, location, create, detect) live in `crate::reviews`.

use std::thread;
use std::time::Duration;

use super::StepResult;
use super::WorkflowChange;
use super::WorkflowContext;
use crate::error::{AikiError, Result};
use crate::tasks::runner::{
    finalize_agent_run, prepare_task_run, rollback_if_still_reserved, task_run, TaskRunOptions,
};
use crate::tasks::{find_task, materialize_graph, read_events, TaskEvent};

/// Run a pre-created review task from ctx.task_id.
///
/// Used after `SetupReview` has already created the review task. Runs the
/// review agent and reports the issue count. Fix-after-review logic is
/// handled by the `RegressionReview` step via dynamic step injection.
pub(crate) fn run(ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
    let review_id = ctx
        .task_id
        .as_ref()
        .ok_or_else(|| {
            AikiError::InvalidArgument("No review task ID in workflow context".to_string())
        })?
        .clone();

    ctx.status("running review agent");

    if ctx.event_rx.is_some() {
        spawn_drain_finalize(ctx, &review_id)?;
    } else {
        let options = TaskRunOptions::new().quiet();
        task_run(&ctx.cwd, &review_id, options)?;
    }

    ctx.status("collecting results");
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    let ic = find_task(&graph.tasks, &review_id)
        .map(crate::reviews::issue_count)
        .unwrap_or(0);

    let message = if ic > 0 {
        format!("Found {} issues", ic)
    } else {
        "approved".to_string()
    };

    Ok(StepResult {
        change: WorkflowChange::None,
        message,
        task_id: Some(review_id),
    })
}

/// Spawn the review agent via `spawn_monitored`, drain task events to track
/// issue count in real-time, and finalize the agent run.
fn spawn_drain_finalize(ctx: &mut WorkflowContext, review_id: &str) -> Result<()> {
    let options = TaskRunOptions::new();
    let prepared = prepare_task_run(&ctx.cwd, review_id, &options, |_| {})?;

    let mut agent_handle = match prepared.runtime.spawn_monitored(&prepared.spawn_options) {
        Ok(handle) => handle,
        Err(e) => {
            rollback_if_still_reserved(&ctx.cwd, &prepared.task_id, &e);
            return Err(e);
        }
    };

    let output = ctx.output;
    let review_id_owned = review_id.to_string();

    if let Some(ref rx) = ctx.event_rx {
        let mut issue_count: usize = 0;

        let drain = |rx: &crossbeam_channel::Receiver<TaskEvent>,
                     review_id: &str,
                     issue_count: &mut usize| {
            for event in rx.try_iter() {
                if let TaskEvent::CommentAdded { task_ids, .. } = &event {
                    if task_ids.iter().any(|id| id == review_id) {
                        *issue_count += 1;
                    }
                }
            }
        };

        while agent_handle
            .try_wait()
            .map_err(|e| AikiError::AgentSpawnFailed(format!("try_wait failed: {}", e)))?
            .is_none()
        {
            drain(rx, &review_id_owned, &mut issue_count);
            thread::sleep(Duration::from_millis(100));
        }
        // Final drain
        drain(rx, &review_id_owned, &mut issue_count);

        if issue_count > 0 {
            output.emit(&format!(
                "  Found {} issue{}",
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            ));
        }
    }

    // Read any diagnostic output
    let proc_output = agent_handle.read_output();
    if !proc_output.stderr.is_empty() {
        ctx.emit(&format!("  agent stderr: {}", proc_output.stderr));
    }

    finalize_agent_run(&ctx.cwd, review_id)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Verify the review drain logic counts CommentAdded events for the review task.
    #[test]
    fn review_drain_counts_issues_from_comments() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let review_id = "review_001";
        let now = chrono::Utc::now();

        // Comment on the review task (counts as issue)
        tx.send(TaskEvent::CommentAdded {
            task_ids: vec![review_id.to_string()],
            text: "Issue: null check missing".to_string(),
            data: HashMap::new(),
            timestamp: now,
        })
        .unwrap();

        // Comment on a different task (should not count)
        tx.send(TaskEvent::CommentAdded {
            task_ids: vec!["other_task".to_string()],
            text: "Unrelated comment".to_string(),
            data: HashMap::new(),
            timestamp: now,
        })
        .unwrap();

        // Another comment on the review task
        tx.send(TaskEvent::CommentAdded {
            task_ids: vec![review_id.to_string()],
            text: "Issue: error handling missing".to_string(),
            data: HashMap::new(),
            timestamp: now,
        })
        .unwrap();

        drop(tx);

        let mut issue_count: usize = 0;
        for event in rx.try_iter() {
            if let TaskEvent::CommentAdded { task_ids, .. } = &event {
                if task_ids.iter().any(|id| id == review_id) {
                    issue_count += 1;
                }
            }
        }

        assert_eq!(issue_count, 2);
    }

    /// Verify singular/plural formatting of issue count.
    #[test]
    fn review_issue_count_formatting() {
        let fmt = |count: usize| -> String {
            format!(
                "  Found {} issue{}",
                count,
                if count == 1 { "" } else { "s" }
            )
        };

        assert_eq!(fmt(1), "  Found 1 issue");
        assert_eq!(fmt(3), "  Found 3 issues");
    }
}

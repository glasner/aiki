//! Review step runners.
//!
//! Contains the workflow step handler for running reviews. Domain types and
//! logic (scope, location, create, detect) live in `crate::reviews`.

use super::StepResult;
use super::WorkflowChange;
use super::WorkflowContext;
use crate::error::AikiError;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::{find_task, materialize_graph, read_events};

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
    let options = TaskRunOptions::new().quiet();
    task_run(&ctx.cwd, &review_id, options)?;

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

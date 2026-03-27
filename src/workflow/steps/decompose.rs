//! Epic lifecycle helpers used by the decompose workflow step.
//!
//! These functions manage epic state transitions: creation, restart, closure,
//! and blocker checks. Extracted from `commands/build.rs` and `commands/epic.rs`
//! to consolidate duplicates.

use std::path::Path;

use crate::agents::AgentType;
use crate::commands::task::{create_from_template, TemplateTaskParams};
use crate::config::get_aiki_binary_path;
use crate::error::{AikiError, Result};
use crate::jj::get_working_copy_change_id;
use crate::plans::parse_plan_metadata;
use crate::tasks::graph::TaskGraph;
use crate::tasks::id::generate_task_id;
use crate::tasks::runner::{handle_session_result, task_run, task_run_on_session, TaskRunOptions};
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, write_event, write_link_event,
    TaskEvent, TaskOutcome, TaskPriority, TaskStatus,
};
use crate::workflow::{StepResult, WorkflowContext};

/// Create the epic task — the container that holds subtasks.
///
/// Extracts the plan title from the H1 heading (or filename as fallback).
/// Sets `data.plan` and source. The `implements-plan` link is written by
/// `run_decompose()` which is called after this function.
pub(crate) fn create_epic_task(cwd: &Path, plan_path: &str) -> Result<String> {
    let full_path = if plan_path.starts_with('/') {
        std::path::PathBuf::from(plan_path)
    } else {
        cwd.join(plan_path)
    };
    let metadata = parse_plan_metadata(&full_path);

    let plan_title = metadata.title.unwrap_or(metadata.path);

    let epic_name = format!("Epic: {}", plan_title);
    let epic_id = generate_task_id(&epic_name);
    let timestamp = chrono::Utc::now();
    let working_copy = get_working_copy_change_id(cwd);

    let mut data = std::collections::HashMap::new();
    data.insert("plan".to_string(), plan_path.to_string());

    let event = TaskEvent::Created {
        task_id: epic_id.clone(),
        name: epic_name,
        slug: None,
        task_type: None,
        priority: TaskPriority::P2,
        assignee: None,
        sources: vec![format!("file:{}", plan_path)],
        template: None,
        working_copy,
        instructions: None,
        data,
        timestamp,
    };
    write_event(cwd, &event)?;

    Ok(epic_id)
}

/// Undo file changes made by completed subtasks of an epic.
///
/// Invokes `aiki task undo <epic-id> --completed` to revert changes before
/// closing the epic. If no completed subtasks exist, this is a no-op.
pub(crate) fn undo_completed_subtasks(cwd: &Path, epic_id: &str) -> Result<()> {
    let output = std::process::Command::new(get_aiki_binary_path())
        .current_dir(cwd)
        .args(["task", "undo", epic_id, "--completed"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to run task undo: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If there are no completed subtasks, that's fine - nothing to undo
        if stderr.contains("no completed subtasks") || stderr.contains("NoCompletedSubtasks") {
            return Ok(());
        }
        return Err(AikiError::JjCommandFailed(format!(
            "task undo failed: {}",
            stderr.trim()
        )));
    }

    // Forward undo output to stderr so user sees what was reverted
    let stderr_output = String::from_utf8_lossy(&output.stderr);
    if !stderr_output.is_empty() {
        eprint!("{}", stderr_output);
    }

    Ok(())
}

/// Close an existing epic as wont_do.
pub(crate) fn close_epic(cwd: &Path, epic_id: &str) -> Result<()> {
    crate::tasks::close_task_as_wont_do(cwd, epic_id, "Closed by --restart")
}

/// Restart an epic by stopping it and re-starting via `aiki task start`.
///
/// `aiki task start` on a parent with subtasks stops any stale in-progress
/// subtasks, giving the new orchestrator a clean slate.
pub(crate) fn restart_epic(cwd: &Path, epic_id: &str) -> Result<()> {
    // Stop the epic to record why it was restarted
    let stop_event = TaskEvent::Stopped {
        task_ids: vec![epic_id.to_string()],
        reason: Some("Restarted by new build".to_string()),
        session_id: None,
        turn_id: None,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &stop_event)?;

    // Re-start via `aiki task start` which handles stopping stale subtasks
    let output = std::process::Command::new(get_aiki_binary_path())
        .current_dir(cwd)
        .args(["task", "start", epic_id])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to restart epic: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to restart epic: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Close an epic as invalid (no subtasks created).
pub(crate) fn close_epic_as_invalid(cwd: &Path, epic_id: &str) -> Result<()> {
    crate::tasks::close_task_as_wont_do(cwd, epic_id, "No subtasks created — epic invalid")
}

/// Check if an epic is blocked by unresolved dependencies.
///
/// An epic is blocked if any of its `depends-on` targets are not closed with
/// outcome `Done`.
pub(crate) fn check_epic_blockers(graph: &TaskGraph, epic_id: &str) -> Result<()> {
    let blocker_ids: Vec<&str> = graph
        .edges
        .targets(epic_id, "depends-on")
        .iter()
        .filter(|tid| {
            graph.tasks.get(tid.as_str()).map_or(true, |t| {
                !(t.status == TaskStatus::Closed && t.closed_outcome == Some(TaskOutcome::Done))
            })
        })
        .map(|s| s.as_str())
        .collect();

    if !blocker_ids.is_empty() {
        let blocker_names: Vec<String> = blocker_ids
            .iter()
            .map(|id| {
                let name = graph
                    .tasks
                    .get(*id)
                    .map(|t| t.name.as_str())
                    .unwrap_or("unknown");
                let short = &id[..id.len().min(8)];
                format!("{} ({})", short, name)
            })
            .collect();
        return Err(AikiError::InvalidArgument(format!(
            "Epic {} is blocked by unresolved dependencies: {}. Rerun with --restart to start over",
            &epic_id[..epic_id.len().min(8)],
            blocker_names.join(", ")
        )));
    }

    Ok(())
}

/// Options for `run_decompose` that callers can customize.
pub struct DecomposeOptions {
    pub template: Option<String>,
    pub agent: Option<AgentType>,
}

/// Decompose a plan into subtasks under `target_id`.
///
/// Steps:
/// 1. Write `implements-plan` link: target → `file:<plan_path>`
/// 2. Create decompose task from template with `data.target` and `data.plan`
/// 3. Write `decomposes-plan` link: decompose task → `file:<plan_path>`
/// 4. Write `populated-by` link: target → decompose task
/// 5. `task_run(decompose_task)` with agent options
/// 6. Return decompose task ID
pub fn run_decompose(
    cwd: &Path,
    plan_path: &str,
    target_id: &str,
    options: DecomposeOptions,
    show_tui: bool,
) -> Result<String> {
    let spec_target = make_spec_target(plan_path);

    // 0. Validate target exists before emitting any links/events
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    find_task(&graph.tasks, target_id)?;

    // 1. Write implements-plan link: target → file:<plan_path>
    write_link_event(cwd, &graph, "implements-plan", target_id, &spec_target)?;

    // 2. Create decompose task from template with data.target and data.plan
    let params = build_decompose_params(plan_path, target_id, &spec_target, &options);

    let decompose_task_id = create_from_template(cwd, params)?;

    // 3. Write decomposes-plan link: decompose task → file:<plan_path>
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(
        cwd,
        &graph,
        "decomposes-plan",
        &decompose_task_id,
        &spec_target,
    )?;

    // 4. Write populated-by link: target → decompose task
    write_link_event(cwd, &graph, "populated-by", target_id, &decompose_task_id)?;

    // 5. task_run(decompose_task) with agent options
    let run_options = if let Some(agent) = options.agent {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };
    if show_tui {
        let result = task_run_on_session(cwd, &decompose_task_id, run_options, true)?;
        handle_session_result(cwd, &decompose_task_id, result, true)?;
    } else {
        task_run(cwd, &decompose_task_id, run_options.quiet())?;
    }

    // 6. Return decompose task ID
    Ok(decompose_task_id)
}

/// Decompose step: find/create epic, check blockers, run decompose if needed.
pub(crate) fn run_decompose_step(
    ctx: &mut WorkflowContext,
    restart: bool,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let plan_path = ctx
        .plan_path
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No plan path in workflow context".to_string()))?
        .clone();

    // If no epic in context, find or create one
    if ctx.task_id.is_none() {
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let plan_graph = crate::plans::PlanGraph::build(&graph);
        let existing_epic = plan_graph.find_epic_for_plan(&plan_path, &graph);

        let epic_id = if restart {
            if let Some(epic) = existing_epic {
                if epic.status != crate::tasks::TaskStatus::Closed {
                    undo_completed_subtasks(&ctx.cwd, &epic.id)?;
                    close_epic(&ctx.cwd, &epic.id)?;
                }
            }
            None
        } else {
            match existing_epic {
                Some(epic) if epic.status != crate::tasks::TaskStatus::Closed => {
                    let subtasks = get_subtasks(&graph, &epic.id);
                    if subtasks.is_empty() {
                        close_epic_as_invalid(&ctx.cwd, &epic.id)?;
                        None
                    } else {
                        restart_epic(&ctx.cwd, &epic.id)?;
                        Some(epic.id.clone())
                    }
                }
                _ => None,
            }
        };

        let epic_id = match epic_id {
            Some(id) => id,
            None => create_epic_task(&ctx.cwd, &plan_path)?,
        };

        ctx.task_id = Some(epic_id);
    }

    let epic_id = ctx.task_id.as_ref().unwrap().clone();

    // Check blockers
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    if graph.is_blocked(&epic_id) {
        return Err(anyhow::anyhow!(
            "Epic {} is blocked by unresolved dependencies. Rerun with --restart to start over",
            &epic_id[..epic_id.len().min(8)]
        ));
    }

    // Run decompose if no subtasks exist
    let subtasks = get_subtasks(&graph, &epic_id);
    if subtasks.is_empty() {
        let options = DecomposeOptions { template, agent };
        let decompose_task_id = run_decompose(&ctx.cwd, &plan_path, &epic_id, options, false)?;

        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let count = get_subtasks(&graph, &epic_id).len();

        Ok(StepResult {
            message: format!("{} subtasks created", count),
            task_id: Some(decompose_task_id),
        })
    } else {
        Ok(StepResult {
            message: "Epic resumed (subtasks already exist)".to_string(),
            task_id: Some(epic_id),
        })
    }
}

/// Normalize a plan path into a `file:` spec target for link events.
fn make_spec_target(plan_path: &str) -> String {
    if plan_path.starts_with("file:") {
        plan_path.to_string()
    } else {
        format!("file:{}", plan_path)
    }
}

/// Build the `TemplateTaskParams` for the decompose task.
fn build_decompose_params(
    plan_path: &str,
    target_id: &str,
    spec_target: &str,
    options: &DecomposeOptions,
) -> TemplateTaskParams {
    let template = options.template.as_deref().unwrap_or("decompose");

    let assignee = options
        .agent
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let mut data = std::collections::HashMap::new();
    data.insert("plan".to_string(), plan_path.to_string());
    data.insert("target".to_string(), target_id.to_string());

    TemplateTaskParams {
        template_name: template.to_string(),
        data,
        sources: vec![spec_target.to_string()],
        assignee,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_spec_target_adds_file_prefix() {
        assert_eq!(
            make_spec_target("ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_make_spec_target_preserves_existing_prefix() {
        assert_eq!(
            make_spec_target("file:ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_build_decompose_params_defaults() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params(
            "ops/now/feat.md",
            "target123",
            "file:ops/now/feat.md",
            &options,
        );

        assert_eq!(params.template_name, "decompose");
        assert_eq!(params.assignee, Some("claude-code".to_string()));
        assert_eq!(params.data.get("plan").unwrap(), "ops/now/feat.md");
        assert_eq!(params.data.get("target").unwrap(), "target123");
        assert_eq!(params.sources, vec!["file:ops/now/feat.md"]);
    }

    #[test]
    fn test_build_decompose_params_custom_template() {
        let options = DecomposeOptions {
            template: Some("my/custom-decompose".to_string()),
            agent: None,
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.template_name, "my/custom-decompose");
    }

    #[test]
    fn test_build_decompose_params_custom_agent() {
        let options = DecomposeOptions {
            template: None,
            agent: Some(AgentType::Codex),
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.assignee, Some("codex".to_string()));
    }

    #[test]
    fn test_build_decompose_params_data_uses_target_not_epic() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params("plan.md", "target_id", "file:plan.md", &options);

        assert!(params.data.contains_key("target"));
        assert!(!params.data.contains_key("epic"));
        assert_eq!(params.data.get("target").unwrap(), "target_id");
        assert_eq!(params.data.get("plan").unwrap(), "plan.md");
    }

    #[test]
    fn test_decompose_template_uses_data_target_not_data_epic() {
        let template_content = include_str!("../../tasks/templates/core/decompose.md");
        assert!(
            template_content.contains("{{data.target}}"),
            "Decompose template must use {{{{data.target}}}}"
        );
        assert!(
            !template_content.contains("{{data.epic}}"),
            "Decompose template must NOT use {{{{data.epic}}}}"
        );
    }
}

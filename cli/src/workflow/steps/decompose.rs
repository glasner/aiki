//! Epic lifecycle helpers used by the decompose workflow step.
//!
//! These functions manage epic state transitions: creation, restart, closure,
//! and blocker checks. Extracted from `commands/build.rs` and `commands/epic.rs`
//! to consolidate duplicates.

use std::path::Path;

use crate::config::get_aiki_binary_path;
use crate::error::{AikiError, Result};
use crate::jj::get_working_copy_change_id;
use crate::plans::parse_plan_metadata;
use crate::tasks::id::generate_task_id;
use crate::tasks::{write_event, TaskEvent, TaskOutcome, TaskPriority, TaskStatus};
use crate::tasks::graph::TaskGraph;

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

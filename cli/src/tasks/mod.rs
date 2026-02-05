//! Task management system for Aiki
//!
//! Provides an AI-first task tracking system with:
//! - Event-sourced storage on `aiki/tasks` branch
//! - XML output format for agent consumption
//! - Ready queue calculation with priority sorting
//! - Task execution via agent runtimes
//! - Template-based task creation

use std::path::Path;

pub mod id;
pub mod manager;
pub mod runner;
pub mod status_monitor;
pub mod storage;
pub mod templates;
pub mod types;
pub mod xml;

pub use id::{generate_child_id, generate_task_id, get_next_subtask_number, is_task_id};
#[allow(unused_imports)]
pub use manager::{
    all_subtasks_closed, find_task, get_subtasks, get_current_scope_set, get_in_progress,
    get_ready_queue, get_ready_queue_for_agent, get_ready_queue_for_agent_scoped,
    get_ready_queue_for_human, get_ready_queue_for_scope_set, get_scoped_ready_queue,
    get_unclosed_subtasks, has_subtasks, materialize_tasks, materialize_tasks_with_ids, ScopeSet,
};
#[allow(unused_imports)]
pub use runner::{run_task_async_with_xml, task_run_async, terminate_background_task};
#[allow(unused_imports)]
pub use storage::{ensure_tasks_branch, read_events, read_events_with_ids, write_event, EventWithId};
#[allow(unused_imports)]
pub use types::{Task, TaskComment, TaskEvent, TaskOutcome, TaskPriority, TaskStatus};
pub use xml::XmlBuilder;

use crate::error::{AikiError, Result};
use crate::events::{AikiEvent, AikiTaskStartedPayload, TaskEventPayload};
use crate::session::find_active_session;

/// Result of starting tasks via `start_task_core`
#[derive(Debug, Clone)]
pub struct StartTaskResult {
    /// Tasks that were started
    pub started: Vec<Task>,
    /// Tasks that were auto-stopped
    pub stopped: Vec<Task>,
    /// The actual task IDs that were started (may differ from input if parent task with subtasks)
    pub started_ids: Vec<String>,
}

/// Core task start logic. Validates, auto-stops other tasks, emits flow events.
///
/// This is the canonical implementation used by `aiki task start`, `aiki review --start`,
/// and `aiki fix --start`. All start operations should go through this function to ensure
/// consistent behavior.
///
/// # What this function does:
/// - Validates that tasks exist and are not closed
/// - Auto-stops any currently in-progress tasks
/// - Creates TaskEvent::Started with the stopped tasks recorded
/// - Emits task.started flow events via event_bus
///
/// # What this function does NOT do:
/// - Quick-start (description → new task) - caller should create task first
/// - Template creation - caller should create from template first
/// - Reopen logic - caller should reopen before calling this
/// - Parent/subtask handling (.0 planning task) - caller should handle this
///
/// # Arguments
/// * `cwd` - Working directory
/// * `task_ids` - Task IDs to start
///
/// # Returns
/// `StartTaskResult` with the started and stopped tasks
pub fn start_task_core(cwd: &Path, task_ids: &[String]) -> Result<StartTaskResult> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Validate all tasks exist and are not closed
    for id in task_ids {
        let task = find_task(&tasks, id)
            .ok_or_else(|| AikiError::TaskNotFound(id.clone()))?;

        if task.status == TaskStatus::Closed {
            return Err(AikiError::InvalidArgument(format!(
                "Task '{}' is closed. Use --reopen --reason to reopen it.",
                id
            )));
        }
    }

    // Get current in-progress tasks to auto-stop
    let current_in_progress_ids: Vec<String> = get_in_progress(&tasks)
        .iter()
        .map(|t| t.id.clone())
        .collect();

    // Get tasks for result
    let stopped_tasks: Vec<Task> = current_in_progress_ids
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();
    let started_tasks: Vec<Task> = task_ids
        .iter()
        .filter_map(|id| find_task(&tasks, id).cloned())
        .collect();

    // Auto-stop current in-progress tasks
    if !current_in_progress_ids.is_empty() {
        let stop_reason = format!("Started {}", task_ids.join(", "));
        let stop_event = TaskEvent::Stopped {
            task_ids: current_in_progress_ids.clone(),
            reason: Some(stop_reason),
            blocked_reason: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &stop_event)?;
    }

    // Get session info
    let session_match = find_active_session(cwd);
    let agent_type_str = session_match
        .as_ref()
        .map(|m| m.agent_type.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = session_match.as_ref().map(|m| m.session_id.clone());

    // Create started event
    let timestamp = chrono::Utc::now();
    let start_event = TaskEvent::Started {
        task_ids: task_ids.to_vec(),
        agent_type: agent_type_str,
        session_id,
        timestamp,
        stopped: current_in_progress_ids,
    };
    write_event(cwd, &start_event)?;

    // Emit task.started flow events for each started task
    for task_id in task_ids {
        if let Some(task) = find_task(&tasks, task_id) {
            let task_event = AikiEvent::TaskStarted(AikiTaskStartedPayload {
                task: TaskEventPayload {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    task_type: infer_task_type(&task),
                    status: "in_progress".to_string(),
                    assignee: task.assignee.clone(),
                    outcome: None,
                    source: task.sources.first().cloned(),
                    files: None,
                    changes: None,
                },
                cwd: cwd.to_path_buf(),
                timestamp,
            });

            // Dispatch event (fire-and-forget, don't block on failure)
            let _ = crate::event_bus::dispatch(task_event);
        }
    }

    Ok(StartTaskResult {
        started: started_tasks,
        stopped: stopped_tasks,
        started_ids: task_ids.to_vec(),
    })
}

/// Reassign a task to a new agent.
///
/// Creates an Updated event to change the task's assignee field.
pub fn reassign_task(cwd: &Path, task_id: &str, new_assignee: &str) -> Result<()> {
    let update_event = TaskEvent::Updated {
        task_id: task_id.to_string(),
        name: None,
        priority: None,
        assignee: Some(Some(new_assignee.to_string())), // Some(Some(x)) = assign to x
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &update_event)?;
    Ok(())
}

/// Infer task type from task name and sources.
///
/// Looks at task name and sources to determine type:
/// - "review" if task type is explicitly set to "review"
/// - "bug" if task name contains "fix" or "bug"
/// - "feature" otherwise (default)
fn infer_task_type(task: &Task) -> String {
    // Check explicit task_type first
    if let Some(ref task_type) = task.task_type {
        return task_type.clone();
    }

    let name_lower = task.name.to_lowercase();

    // Check for review indicators
    if name_lower.contains("review") {
        return "review".to_string();
    }

    // Check for bug/fix indicators
    if name_lower.contains("fix") || name_lower.contains("bug") {
        return "bug".to_string();
    }

    // Default to feature
    "feature".to_string()
}

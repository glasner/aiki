//! Task execution runner
//!
//! This module provides functions for executing tasks via agent sessions,
//! including blocking and background (async) execution modes.

use std::path::Path;

use crate::agents::{
    get_runtime, AgentSessionResult, AgentSpawnOptions, AgentType, Assignee, BackgroundHandle,
};
use crate::error::{AikiError, Result};
use crate::session::find_runner_session;
use crate::tasks::{
    find_task,
    materialize_tasks,
    read_events,
    types::{TaskEvent, TaskStatus},
    write_event,
    xml::XmlBuilder,
};

/// Options for running a task
#[derive(Debug, Clone)]
pub struct TaskRunOptions {
    /// Override the task's assignee agent
    pub agent_override: Option<AgentType>,
}

impl Default for TaskRunOptions {
    fn default() -> Self {
        Self {
            agent_override: None,
        }
    }
}

impl TaskRunOptions {
    /// Create new task run options
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an agent override
    #[must_use]
    pub fn with_agent(mut self, agent: AgentType) -> Self {
        self.agent_override = Some(agent);
        self
    }
}

/// Run a task by spawning an agent session
///
/// This function:
/// 1. Loads the task from the aiki/tasks branch
/// 2. Validates the task can be run (not closed)
/// 3. Determines which agent to use (from options or task assignee)
/// 4. Spawns the agent session with task context
/// 5. Handles the result and updates task state
pub fn task_run(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Find the task
    let task = find_task(&tasks, task_id).ok_or_else(|| AikiError::TaskNotFound(task_id.to_string()))?;

    // Validate task can be run
    if task.status == TaskStatus::Closed {
        return Err(AikiError::TaskAlreadyClosed(task_id.to_string()));
    }

    // Determine which agent to use
    let agent_type = if let Some(agent) = options.agent_override {
        agent
    } else if let Some(ref assignee_str) = task.assignee {
        // Parse assignee to get agent type
        match Assignee::from_str(assignee_str) {
            Some(Assignee::Agent(agent)) => agent,
            Some(Assignee::Human) => {
                return Err(AikiError::TaskNoAssignee(format!(
                    "Task '{}' is assigned to human, use --agent to specify an agent",
                    task_id
                )));
            }
            Some(Assignee::Unassigned) | None => {
                return Err(AikiError::TaskNoAssignee(task_id.to_string()));
            }
        }
    } else {
        return Err(AikiError::TaskNoAssignee(task_id.to_string()));
    };

    // Get runtime for the agent
    let runtime = get_runtime(agent_type).ok_or_else(|| {
        AikiError::AgentNotSupported(agent_type.as_str().to_string())
    })?;

    // Print status to stderr to avoid corrupting piped output
    eprintln!(
        "Spawning {} agent session for task {}...",
        agent_type.display_name(),
        task_id
    );

    // Build spawn options
    let spawn_options = AgentSpawnOptions::new(cwd, task_id);

    // Spawn agent session (blocking)
    let result = runtime.spawn_blocking(&spawn_options)?;

    // Handle result - the agent is responsible for claiming and closing the task
    // We just need to handle failures where the agent didn't complete properly
    match &result {
        AgentSessionResult::Completed { summary } => {
            // Agent completed successfully - it should have closed the task itself
            // Just print success message (to stderr to avoid corrupting piped output)
            eprintln!("Task run complete");
            if !summary.is_empty() {
                eprintln!("Summary: {}", summary);
            }
        }
        AgentSessionResult::Stopped { reason } => {
            // Agent stopped - emit Stopped event if task is not already closed
            let refreshed_events = read_events(cwd)?;
            let refreshed_tasks = materialize_tasks(&refreshed_events);
            if let Some(refreshed_task) = find_task(&refreshed_tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(reason.clone()),
                        blocked_reason: None,
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;
                }
            }
            eprintln!("Task {} stopped: {}", task_id, reason);
        }
        AgentSessionResult::Failed { error } => {
            // Agent failed - emit Stopped event even if task never reached InProgress
            // This handles spawn failures where the agent never claimed the task
            let refreshed_events = read_events(cwd)?;
            let refreshed_tasks = materialize_tasks(&refreshed_events);
            if let Some(refreshed_task) = find_task(&refreshed_tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(format!("Session failed: {}", error)),
                        blocked_reason: None,
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;
                }
            }
            return Err(AikiError::AgentSpawnFailed(error.clone()));
        }
    }

    Ok(())
}

/// Run a task and output XML result
///
/// Wrapper around `task_run` that outputs XML-formatted results.
pub fn run_task_with_xml(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run(cwd, task_id, options) {
        Ok(()) => {
            // Output success XML
            let xml = XmlBuilder::new("run")
                .build(&format!("  <completed task_id=\"{}\"/>", task_id), &[], &[]);
            println!("{}", xml);
            Ok(())
        }
        Err(e) => {
            // Output error XML
            let xml = XmlBuilder::new("run")
                .error()
                .build_error(&e.to_string());
            println!("{}", xml);
            Err(e)
        }
    }
}

/// Terminate a background task process if running
///
/// This function:
/// 1. Looks up the runner session for the task via session files
/// 2. If found, sends SIGTERM to terminate the agent process
///
/// Returns Ok(true) if a process was terminated, Ok(false) if no session found.
/// Does not fail if the process has already exited (ESRCH error is ignored).
#[cfg(unix)]
pub fn terminate_background_task(_cwd: &Path, task_id: &str) -> Result<bool> {
    let session_info = match find_runner_session(task_id) {
        Some(info) => info,
        None => return Ok(false), // No runner session - task wasn't running async
    };

    // Send SIGTERM to the process
    // SAFETY: libc::kill is safe to call with any pid value
    let result = unsafe { libc::kill(session_info.pid as libc::pid_t, libc::SIGTERM) };

    // Note: Session file cleanup happens naturally when the agent exits
    // and creates its end-of-session events

    if result == 0 {
        // Process was terminated successfully
        Ok(true)
    } else {
        // Check the error
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            // ESRCH = No such process - already exited, that's fine
            Ok(false)
        } else if err.raw_os_error() == Some(libc::EPERM) {
            // EPERM = Permission denied - process exists but we can't kill it
            Ok(false)
        } else {
            // Unexpected error - the process may have exited
            Ok(false)
        }
    }
}

/// Terminate a background task process if running (non-Unix stub)
#[cfg(not(unix))]
pub fn terminate_background_task(_cwd: &Path, task_id: &str) -> Result<bool> {
    // On non-Unix platforms, just check if a runner session exists
    // Process termination is not implemented
    if find_runner_session(task_id).is_some() {
        Ok(false) // Process not actually terminated
    } else {
        Ok(false)
    }
}

/// Spawn a task in the background and return immediately
///
/// This function:
/// 1. Validates the task can be run
/// 2. Determines which agent to use
/// 3. Spawns the agent process in the background (with AIKI_RUNNER_TASK env var)
/// 4. Returns the background handle
///
/// The spawned agent creates a session with runner_task field, which allows
/// `terminate_background_task` to find and terminate it later.
pub fn task_run_async(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
) -> Result<BackgroundHandle> {
    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Find the task
    let task =
        find_task(&tasks, task_id).ok_or_else(|| AikiError::TaskNotFound(task_id.to_string()))?;

    // Validate task can be run
    if task.status == TaskStatus::Closed {
        return Err(AikiError::TaskAlreadyClosed(task_id.to_string()));
    }

    // Determine which agent to use
    let agent_type = if let Some(agent) = options.agent_override {
        agent
    } else if let Some(ref assignee_str) = task.assignee {
        // Parse assignee to get agent type
        match Assignee::from_str(assignee_str) {
            Some(Assignee::Agent(agent)) => agent,
            Some(Assignee::Human) => {
                return Err(AikiError::TaskNoAssignee(format!(
                    "Task '{}' is assigned to human, use --agent to specify an agent",
                    task_id
                )));
            }
            Some(Assignee::Unassigned) | None => {
                return Err(AikiError::TaskNoAssignee(task_id.to_string()));
            }
        }
    } else {
        return Err(AikiError::TaskNoAssignee(task_id.to_string()));
    };

    // Get runtime for the agent
    let runtime = get_runtime(agent_type)
        .ok_or_else(|| AikiError::AgentNotSupported(agent_type.as_str().to_string()))?;

    // Build spawn options
    let spawn_options = AgentSpawnOptions::new(cwd, task_id);

    // Spawn agent session in background
    // The agent inherits AIKI_RUNNER_TASK env var which gets recorded in its session file
    // This allows terminate_background_task to find and kill it later
    let handle = runtime.spawn_background(&spawn_options)?;

    Ok(handle)
}

/// Run a task asynchronously and output XML result
///
/// Wrapper around `task_run_async` that outputs XML-formatted results.
pub fn run_task_async_with_xml(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run_async(cwd, task_id, options) {
        Ok(handle) => {
            // Output success XML with async=true
            let xml = XmlBuilder::new("run").build(
                &format!(
                    "  <started task_id=\"{}\" async=\"true\">\n    Task started asynchronously.\n  </started>",
                    handle.task_id
                ),
                &[],
                &[],
            );
            println!("{}", xml);
            Ok(())
        }
        Err(e) => {
            // Output error XML
            let xml = XmlBuilder::new("run").error().build_error(&e.to_string());
            println!("{}", xml);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_run_options_default() {
        let options = TaskRunOptions::default();
        assert!(options.agent_override.is_none());
    }

    #[test]
    fn test_task_run_options_with_agent() {
        let options = TaskRunOptions::new().with_agent(AgentType::ClaudeCode);
        assert_eq!(options.agent_override, Some(AgentType::ClaudeCode));
    }
}

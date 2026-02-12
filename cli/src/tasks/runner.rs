//! Task execution runner
//!
//! This module provides functions for executing tasks via agent sessions,
//! including blocking and background (async) execution modes.

use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::Ordering;

use crate::agents::{
    detect_agent_from_process_tree, get_runtime, AgentSessionResult, AgentSpawnOptions, AgentType,
    Assignee, BackgroundHandle, MonitoredChild,
};
use crate::error::{AikiError, Result};
use crate::session::find_task_session;
use crate::tasks::{
    find_task,
    materialize_graph,
    read_events,
    status_monitor::{MonitorExitReason, StatusMonitor},
    types::{TaskEvent, TaskStatus},
    write_event,
    md::MdBuilder,
};

/// Options for running a task
#[derive(Debug, Clone)]
pub struct TaskRunOptions {
    /// Override the task's assignee agent
    pub agent_override: Option<AgentType>,
    /// Suppress real-time status updates
    pub quiet: bool,
}

impl Default for TaskRunOptions {
    fn default() -> Self {
        Self {
            agent_override: None,
            quiet: false,
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

    /// Suppress status updates during sync execution
    #[must_use]
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
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
/// 5. Shows real-time status updates while waiting (if TTY)
/// 6. Handles the result and updates task state
pub fn task_run(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    // Find the task
    let task = find_task(&tasks, task_id)?;
    let task_id = &task.id; // rebind to canonical ID

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
                detect_agent_from_process_tree()
                    .ok_or_else(|| AikiError::TaskNoAssignee(task_id.to_string()))?
            }
        }
    } else {
        detect_agent_from_process_tree()
            .ok_or_else(|| AikiError::TaskNoAssignee(task_id.to_string()))?
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

    // Check if we should show live status updates
    let show_status = std::io::stderr().is_terminal() && !options.quiet;

    // Spawn agent session and optionally monitor status
    let result = if show_status {
        // Run agent in background thread while monitoring status in foreground
        run_with_status_monitor(cwd, task_id, runtime.as_ref(), &spawn_options)?
    } else {
        // Simple blocking spawn (no status display)
        runtime.spawn_blocking(&spawn_options)?
    };

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
            let mut refreshed_graph = materialize_graph(&refreshed_events);
            if let Ok(refreshed_task) = find_task(&refreshed_graph.tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let is_orchestrator = refreshed_task.is_orchestrator();
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(reason.clone()),
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;

                    // Cascade-close subtasks if this is an orchestrator task
                    if is_orchestrator {
                        use crate::tasks::manager::get_all_unclosed_descendants;
                        use crate::commands::task::cascade_close_tasks;
                        let unclosed = get_all_unclosed_descendants(&refreshed_graph, task_id);
                        if !unclosed.is_empty() {
                            let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
                            cascade_close_tasks(cwd, &mut refreshed_graph.tasks, &cascade_ids, crate::tasks::types::TaskOutcome::WontDo, "Parent orchestrator stopped")?;
                        }
                    }
                }
            }
            eprintln!("Task {} stopped: {}", task_id, reason);
        }
        AgentSessionResult::Detached => {
            // User detached via Ctrl+C - agent continues running in background
            // Do NOT emit TaskEvent::Stopped since the agent is still working
            eprintln!(
                "Detached. Task {} still running. Use `aiki task show {}` to check status.",
                &task_id[..8.min(task_id.len())],
                task_id
            );
        }
        AgentSessionResult::Failed { error } => {
            // Agent failed - emit Stopped event even if task never reached InProgress
            // This handles spawn failures where the agent never claimed the task
            let refreshed_events = read_events(cwd)?;
            let mut refreshed_graph = materialize_graph(&refreshed_events);
            if let Ok(refreshed_task) = find_task(&refreshed_graph.tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let is_orchestrator = refreshed_task.is_orchestrator();
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(format!("Session failed: {}", error)),
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;

                    // Cascade-close subtasks if this is an orchestrator task
                    if is_orchestrator {
                        use crate::tasks::manager::get_all_unclosed_descendants;
                        use crate::commands::task::cascade_close_tasks;
                        let unclosed = get_all_unclosed_descendants(&refreshed_graph, task_id);
                        if !unclosed.is_empty() {
                            let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
                            cascade_close_tasks(cwd, &mut refreshed_graph.tasks, &cascade_ids, crate::tasks::types::TaskOutcome::WontDo, "Parent orchestrator failed")?;
                        }
                    }
                }
            }
            return Err(AikiError::AgentSpawnFailed(error.clone()));
        }
    }

    Ok(())
}

/// Run agent with real-time status monitoring
///
/// Spawns the agent in background and monitors status in the foreground.
/// Ctrl+C signals the monitor to stop (detach), but the agent continues running.
fn run_with_status_monitor(
    cwd: &Path,
    task_id: &str,
    runtime: &dyn crate::agents::AgentRuntime,
    spawn_options: &AgentSpawnOptions,
) -> Result<AgentSessionResult> {
    // Spawn agent with child handle for proper exit detection
    // This properly handles zombie processes by using try_wait() instead of kill(pid, 0)
    let mut monitored_child = runtime.spawn_monitored(spawn_options)?;

    // Create status monitor (no longer needs PID since we check exit ourselves)
    let mut monitor = StatusMonitor::new(task_id);
    let stop_flag = monitor.stop_flag();

    // Set up Ctrl+C handler to signal monitor to stop
    let stop_flag_ctrlc = stop_flag.clone();
    let _ = ctrlc::set_handler(move || {
        stop_flag_ctrlc.store(true, Ordering::Relaxed);
    });

    // Run status monitor until task completes, user detaches, or agent exits
    let exit_reason = monitor.monitor_until_complete_with_child(cwd, &mut monitored_child)?;

    match exit_reason {
        MonitorExitReason::UserDetached => {
            // User detached via Ctrl+C - agent continues running in background
            Ok(AgentSessionResult::detached())
        }
        MonitorExitReason::AgentExited { stderr } => {
            // Agent exited without task reaching terminal state - check final status
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;

            if let Some(task) = tasks.get(task_id) {
                match task.status {
                    TaskStatus::Closed => {
                        // Task was closed right before agent exited
                        let summary = task
                            .effective_summary()
                            .unwrap_or_default()
                            .to_string();
                        Ok(AgentSessionResult::Completed { summary })
                    }
                    TaskStatus::Stopped => {
                        // Task was stopped
                        let reason = task
                            .stopped_reason
                            .clone()
                            .unwrap_or_else(|| "Task stopped".to_string());
                        Ok(AgentSessionResult::Stopped { reason })
                    }
                    _ => {
                        // Agent crashed without completing task - include stderr if available
                        let error = if stderr.trim().is_empty() {
                            "Agent process exited without completing task".to_string()
                        } else {
                            format!(
                                "Agent process exited without completing task:\n{}",
                                stderr.trim()
                            )
                        };
                        Ok(AgentSessionResult::Failed { error })
                    }
                }
            } else {
                let error = if stderr.trim().is_empty() {
                    "Task not found after agent exit".to_string()
                } else {
                    format!("Task not found after agent exit:\n{}", stderr.trim())
                };
                Ok(AgentSessionResult::Failed { error })
            }
        }
        MonitorExitReason::TaskCompleted => {
            // Task reached terminal state - check final status
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;

            if let Some(task) = tasks.get(task_id) {
                match task.status {
                    TaskStatus::Closed => {
                        let summary = task
                            .effective_summary()
                            .unwrap_or_default()
                            .to_string();
                        Ok(AgentSessionResult::Completed { summary })
                    }
                    TaskStatus::Stopped => {
                        let reason = task
                            .stopped_reason
                            .clone()
                            .unwrap_or_else(|| "Task stopped".to_string());
                        Ok(AgentSessionResult::Stopped { reason })
                    }
                    _ => {
                        // Shouldn't happen but handle gracefully
                        Ok(AgentSessionResult::Completed {
                            summary: String::new(),
                        })
                    }
                }
            } else {
                Ok(AgentSessionResult::Failed {
                    error: "Task not found after completion".to_string(),
                })
            }
        }
    }
}

/// Run a task and output result
///
/// Wrapper around `task_run` that outputs formatted results.
pub fn run_task_with_output(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run(cwd, task_id, options) {
        Ok(()) => {
            let md = MdBuilder::new("run")
                .build(&format!("## Run Completed\n- **Task:** {}\n", task_id), &[], &[]);
            println!("{}", md);
            Ok(())
        }
        Err(e) => {
            let md = MdBuilder::new("run")
                .error()
                .build_error(&e.to_string());
            println!("{}", md);
            Err(e)
        }
    }
}

/// Terminate a background task process if running
///
/// This function:
/// 1. Looks up the task-driven session for the task via session files
/// 2. If found, sends SIGTERM to terminate the agent process
///
/// Returns Ok(true) if a process was terminated, Ok(false) if no session found.
/// Does not fail if the process has already exited (ESRCH error is ignored).
#[cfg(unix)]
pub fn terminate_background_task(_cwd: &Path, task_id: &str) -> Result<bool> {
    let session_info = match find_task_session(task_id) {
        Some(info) => info,
        None => return Ok(false), // No task session - task wasn't running async
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
    // On non-Unix platforms, just check if a task session exists
    // Process termination is not implemented
    if find_task_session(task_id).is_some() {
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
/// 3. Spawns the agent process in the background (with AIKI_TASK env var)
/// 4. Returns the background handle
///
/// The spawned agent creates a session with task field, which allows
/// `terminate_background_task` to find and terminate it later.
pub fn task_run_async(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
) -> Result<BackgroundHandle> {
    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    // Find the task
    let task = find_task(&tasks, task_id)?;
    let task_id = &task.id; // rebind to canonical ID

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
                detect_agent_from_process_tree()
                    .ok_or_else(|| AikiError::TaskNoAssignee(task_id.to_string()))?
            }
        }
    } else {
        detect_agent_from_process_tree()
            .ok_or_else(|| AikiError::TaskNoAssignee(task_id.to_string()))?
    };

    // Get runtime for the agent
    let runtime = get_runtime(agent_type)
        .ok_or_else(|| AikiError::AgentNotSupported(agent_type.as_str().to_string()))?;

    // Build spawn options
    let spawn_options = AgentSpawnOptions::new(cwd, task_id);

    // Spawn agent session in background
    // The agent inherits AIKI_TASK env var which gets recorded in its session file
    // This allows terminate_background_task to find and kill it later
    let handle = runtime.spawn_background(&spawn_options)?;

    Ok(handle)
}

/// Run a task asynchronously and output XML result
///
/// Wrapper around `task_run_async` that outputs formatted results.
pub fn run_task_async_with_output(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run_async(cwd, task_id, options) {
        Ok(handle) => {
            let md = MdBuilder::new("run").build(
                &format!(
                    "## Run Started\n- **Task:** {}\n- Task started asynchronously.\n",
                    handle.task_id
                ),
                &[],
                &[],
            );
            println!("{}", md);
            Ok(())
        }
        Err(e) => {
            let md = MdBuilder::new("run").error().build_error(&e.to_string());
            println!("{}", md);
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
        assert!(!options.quiet);
    }

    #[test]
    fn test_task_run_options_with_agent() {
        let options = TaskRunOptions::new().with_agent(AgentType::ClaudeCode);
        assert_eq!(options.agent_override, Some(AgentType::ClaudeCode));
    }

    #[test]
    fn test_task_run_options_quiet() {
        let options = TaskRunOptions::new().quiet();
        assert!(options.quiet);
        assert!(options.agent_override.is_none());
    }
}

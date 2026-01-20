//! Task execution runner
//!
//! This module provides the `task_run` function that spawns an agent session
//! to work on a task.

use std::path::Path;

use crate::agents::{get_runtime, AgentSessionResult, AgentSpawnOptions, AgentType, Assignee};
use crate::error::{AikiError, Result};
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

    // Print status
    println!(
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
            // Just print success message
            println!("Task run complete");
            if !summary.is_empty() {
                println!("Summary: {}", summary);
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
            println!("Task {} stopped: {}", task_id, reason);
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

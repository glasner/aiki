//! Agent runtime abstraction for spawning and managing agent processes
//!
//! This module provides the `AgentRuntime` trait that defines how to spawn
//! agent sessions and the result types for tracking session outcomes.

mod claude_code;
mod codex;

pub use claude_code::ClaudeCodeRuntime;
pub use codex::CodexRuntime;

use crate::error::Result;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, ExitStatus};

use super::AgentType;

/// Handle for a background agent process
///
/// Returned when spawning an agent in background mode. Contains the
/// task ID for later management (e.g., stopping the process).
#[derive(Debug, Clone)]
pub struct BackgroundHandle {
    /// Task ID being worked on
    pub task_id: String,
    /// Session UUID, resolved after spawn via event polling
    pub session_id: Option<String>,
}

/// Handle for a monitored child process
///
/// Unlike `BackgroundHandle`, this keeps the `Child` handle so we can properly
/// detect when the process exits (including zombie processes). This is used
/// for real-time status monitoring where we need accurate exit detection.
pub struct MonitoredChild {
    /// The child process handle
    child: Child,
    /// Stdout handle for capturing agent output on failure
    stdout: Option<ChildStdout>,
    /// Stderr handle for capturing error output
    stderr: Option<ChildStderr>,
}

impl MonitoredChild {
    /// Create a new monitored child from a Child process
    #[must_use]
    pub fn new(mut child: Child) -> Self {
        let stdout = child.stdout.take();
        // Take stderr handle from child so we can read it later
        let stderr = child.stderr.take();
        Self {
            child,
            stdout,
            stderr,
        }
    }

    /// Check if the process has exited without blocking
    ///
    /// Returns:
    /// - `Ok(Some(status))` if the process has exited
    /// - `Ok(None)` if the process is still running
    /// - `Err` on error
    ///
    /// This properly handles zombie processes by calling `wait()` internally,
    /// which reaps the zombie when the process has exited.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Read any captured stdout/stderr output
    ///
    /// Should be called after the process has exited to get diagnostic messages.
    /// Returns empty strings when a stream wasn't captured or had no output.
    pub fn read_output(&mut self) -> ProcessOutput {
        let mut stdout_output = String::new();
        if let Some(ref mut stdout) = self.stdout {
            let _ = stdout.read_to_string(&mut stdout_output);
        }

        let mut stderr_output = String::new();
        if let Some(ref mut stderr) = self.stderr {
            // Read whatever is available in the stderr buffer
            // This is non-blocking since the process has already exited
            let _ = stderr.read_to_string(&mut stderr_output);
        }

        ProcessOutput {
            stdout: stdout_output,
            stderr: stderr_output,
        }
    }
}

/// Captured output from an exited agent process.
#[derive(Debug, Clone, Default)]
pub struct ProcessOutput {
    /// Anything the agent wrote to stdout before exiting.
    pub stdout: String,
    /// Anything the agent wrote to stderr before exiting.
    pub stderr: String,
}

/// Result of an agent session
#[derive(Debug, Clone)]
pub enum AgentSessionResult {
    /// Agent finished successfully
    Completed {
        /// Summary of what was accomplished
        summary: String,
    },
    /// Agent explicitly stopped (needs user input, blocked, etc.)
    Stopped {
        /// Reason for stopping
        reason: String,
    },
    /// Agent failed (crash, timeout, error)
    Failed {
        /// Error description
        error: String,
    },
    /// User detached from monitoring, but agent continues running in background
    Detached,
}

impl AgentSessionResult {
    /// Create a completed result
    #[must_use]
    pub fn completed(summary: impl Into<String>) -> Self {
        Self::Completed {
            summary: summary.into(),
        }
    }

    /// Create a stopped result
    #[must_use]
    pub fn stopped(reason: impl Into<String>) -> Self {
        Self::Stopped {
            reason: reason.into(),
        }
    }

    /// Create a failed result
    #[must_use]
    pub fn failed(error: impl Into<String>) -> Self {
        Self::Failed {
            error: error.into(),
        }
    }

    /// Create a detached result (user disconnected, agent continues)
    #[must_use]
    pub fn detached() -> Self {
        Self::Detached
    }
}

/// Options for spawning an agent session
#[derive(Debug, Clone)]
pub struct AgentSpawnOptions {
    /// Working directory for the agent
    pub cwd: std::path::PathBuf,
    /// Task ID to work on (first task in chain, or standalone)
    pub task_id: String,
    /// Parent session UUID for workspace isolation chaining
    pub parent_session_uuid: Option<String>,
    /// Ordered chain of task IDs for needs-context sessions (head to tail).
    /// When set, the agent works through all tasks in sequence within one session.
    pub chain_task_ids: Option<Vec<String>>,
}

impl AgentSpawnOptions {
    /// Create new spawn options
    #[must_use]
    pub fn new(cwd: impl AsRef<Path>, task_id: impl Into<String>) -> Self {
        Self {
            cwd: cwd.as_ref().to_path_buf(),
            task_id: task_id.into(),
            parent_session_uuid: None,
            chain_task_ids: None,
        }
    }

    /// Set the parent session UUID for workspace isolation chaining
    #[must_use]
    pub fn with_parent_session_uuid(mut self, uuid: Option<String>) -> Self {
        self.parent_session_uuid = uuid;
        self
    }

    /// Set chain task IDs for needs-context session execution
    #[must_use]
    pub fn with_chain(mut self, chain: Vec<String>) -> Self {
        self.chain_task_ids = Some(chain);
        self
    }

    /// Build the task prompt with instructions for autonomous work
    #[must_use]
    pub fn task_prompt(&self) -> String {
        if let Some(ref chain) = self.chain_task_ids {
            // Chain session: agent works through multiple tasks in sequence
            let first = &chain[0];
            let chain_list = chain
                .iter()
                .enumerate()
                .map(|(i, id)| format!("{}. `{}`", i + 1, id))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"You are assigned a chain of {count} tasks. Work through ALL of them in order.

Tasks:
{chain_list}

SCOPE: Only these tasks. Do not pick up other work from the backlog.
EXIT: After closing the last task in the chain, exit immediately — do not close parent/sibling tasks or look for more work."#,
                count = chain.len(),
                chain_list = chain_list,
            )
        } else {
            // Single task prompt (existing behavior)
            format!(
                r#"You are assigned task `{id}`. Work autonomously until it is closed.

SCOPE: Only task `{id}` and its subtasks. Do not pick up other work from the backlog.
EXIT: After closing your task, exit immediately — do not look for more work."#,
                id = self.task_id
            )
        }
    }
}

/// Trait for agent runtime implementations
///
/// Each agent type (ClaudeCode, Codex, etc.) has its own runtime that knows
/// how to spawn and manage sessions for that agent.
pub trait AgentRuntime {
    /// Spawns an agent session and waits for completion
    ///
    /// This is a blocking operation that:
    /// 1. Spawns the agent process with the task context
    /// 2. Waits for the agent to complete
    /// 3. Returns the session result
    fn spawn_blocking(&self, options: &AgentSpawnOptions) -> Result<AgentSessionResult>;

    /// Spawns an agent session in the background
    ///
    /// This is a non-blocking operation that:
    /// 1. Spawns the agent process detached from the parent
    /// 2. Returns immediately with a handle containing the PID
    /// 3. The agent runs until task completion
    ///
    /// The background process is fully detached and will continue running
    /// even after the parent process exits.
    fn spawn_background(&self, options: &AgentSpawnOptions) -> Result<BackgroundHandle>;

    /// Spawns an agent session for monitoring
    ///
    /// Similar to `spawn_background`, but keeps the Child handle so we can
    /// properly detect when the process exits (including zombie processes).
    /// This should be used when real-time status monitoring is needed.
    fn spawn_monitored(&self, options: &AgentSpawnOptions) -> Result<MonitoredChild>;
}

/// Get the appropriate runtime for an agent type
#[must_use]
pub fn get_runtime(agent_type: AgentType) -> Option<Box<dyn AgentRuntime>> {
    match agent_type {
        AgentType::ClaudeCode => Some(Box::new(ClaudeCodeRuntime::new())),
        AgentType::Codex => Some(Box::new(CodexRuntime::new())),
        // Other agent types not yet supported for task execution
        AgentType::Cursor | AgentType::Gemini | AgentType::Unknown => None,
    }
}

/// Poll session state until a task-bound session is registered.
///
/// After spawning an agent, `session.started` is recorded in conversation
/// history with the run task ID and session UUID. This function polls that log using
/// exponential backoff (100ms → 500ms, 30s total timeout).
pub fn discover_session_id(cwd: &Path, task_id: &str) -> Result<String> {
    use crate::history::storage::find_session_started_for_run_task;
    use std::thread;
    use std::time::{Duration, Instant};

    // Session startup can take longer than the original 5s budget.
    let timeout = Duration::from_secs(30);
    let start = Instant::now();
    let mut delay = Duration::from_millis(100);
    let max_delay = Duration::from_millis(500);

    loop {
        if let Some(session_id) = find_session_started_for_run_task(cwd, task_id)? {
            return Ok(session_id);
        }

        if start.elapsed() >= timeout {
            return Err(anyhow::anyhow!(
                "Session UUID not discovered within timeout"
            )
            .into());
        }

        thread::sleep(delay);
        delay = (delay * 2).min(max_delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_session_result_constructors() {
        let completed = AgentSessionResult::completed("Task done");
        assert!(matches!(completed, AgentSessionResult::Completed { .. }));

        let stopped = AgentSessionResult::stopped("Needs input");
        assert!(matches!(stopped, AgentSessionResult::Stopped { .. }));

        let failed = AgentSessionResult::failed("Crashed");
        assert!(matches!(failed, AgentSessionResult::Failed { .. }));

        let detached = AgentSessionResult::detached();
        assert!(matches!(detached, AgentSessionResult::Detached));
    }

    #[test]
    fn test_spawn_options() {
        let options = AgentSpawnOptions::new("/tmp", "task123");

        assert_eq!(options.cwd.to_string_lossy(), "/tmp");
        assert_eq!(options.task_id, "task123");
    }

    #[test]
    fn test_get_runtime() {
        assert!(get_runtime(AgentType::ClaudeCode).is_some());
        assert!(get_runtime(AgentType::Codex).is_some());
        assert!(get_runtime(AgentType::Cursor).is_none());
        assert!(get_runtime(AgentType::Gemini).is_none());
        assert!(get_runtime(AgentType::Unknown).is_none());
    }
}

//! Agent runtime abstraction for spawning and managing agent processes
//!
//! This module provides the `AgentRuntime` trait that defines how to spawn
//! agent sessions and the result types for tracking session outcomes.

mod claude_code;
mod codex;

pub use claude_code::ClaudeCodeRuntime;
pub use codex::CodexRuntime;

use crate::error::Result;
use std::path::Path;

use super::AgentType;

/// Handle for a background agent process
///
/// Returned when spawning an agent in background mode. Contains the PID
/// and task ID for later management (e.g., stopping the process).
#[derive(Debug, Clone)]
pub struct BackgroundHandle {
    /// Process ID of the spawned agent
    pub pid: u32,
    /// Task ID being worked on
    pub task_id: String,
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

    /// Check if the session completed successfully
    #[must_use]
    #[allow(dead_code)] // Part of AgentSessionResult API
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Check if the session failed
    #[must_use]
    #[allow(dead_code)] // Part of AgentSessionResult API
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// Options for spawning an agent session
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of API
pub struct AgentSpawnOptions {
    /// Working directory for the agent
    pub cwd: std::path::PathBuf,
    /// Task ID to work on
    pub task_id: String,
    /// Override the task's assignee (optional)
    pub agent_override: Option<AgentType>,
}

impl AgentSpawnOptions {
    /// Create new spawn options
    #[must_use]
    pub fn new(cwd: impl AsRef<Path>, task_id: impl Into<String>) -> Self {
        Self {
            cwd: cwd.as_ref().to_path_buf(),
            task_id: task_id.into(),
            agent_override: None,
        }
    }

    /// Set an agent override
    #[must_use]
    #[allow(dead_code)] // Part of builder API
    pub fn with_agent_override(mut self, agent: AgentType) -> Self {
        self.agent_override = Some(agent);
        self
    }
}

/// Trait for agent runtime implementations
///
/// Each agent type (ClaudeCode, Codex, etc.) has its own runtime that knows
/// how to spawn and manage sessions for that agent.
#[allow(dead_code)] // Trait methods are part of runtime API
pub trait AgentRuntime {
    /// Returns the agent type this runtime handles
    fn agent_type(&self) -> AgentType;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_session_result_constructors() {
        let completed = AgentSessionResult::completed("Task done");
        assert!(completed.is_completed());
        assert!(!completed.is_failed());

        let stopped = AgentSessionResult::stopped("Needs input");
        assert!(!stopped.is_completed());
        assert!(!stopped.is_failed());

        let failed = AgentSessionResult::failed("Crashed");
        assert!(!failed.is_completed());
        assert!(failed.is_failed());
    }

    #[test]
    fn test_spawn_options() {
        let options = AgentSpawnOptions::new("/tmp", "task123")
            .with_agent_override(AgentType::ClaudeCode);

        assert_eq!(options.cwd.to_string_lossy(), "/tmp");
        assert_eq!(options.task_id, "task123");
        assert_eq!(options.agent_override, Some(AgentType::ClaudeCode));
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

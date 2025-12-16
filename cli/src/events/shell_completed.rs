use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// shell.completed event payload
///
/// Fires after a shell command completes execution. Contains the command
/// that was run and its output/exit code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiShellCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The shell command that was executed
    pub command: String,
    /// Exit code of the command
    pub exit_code: i32,
    /// Standard output from the command
    #[serde(default)]
    pub stdout: String,
    /// Standard error from the command
    #[serde(default)]
    pub stderr: String,
}

/// Handle shell.completed event
///
/// This event fires after a shell command completes. Can be used to
/// log command execution, react to failures, or trigger follow-up actions.
pub fn handle_shell_completed(payload: AikiShellCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "shell.completed from {:?}, session: {}, command: {}, exit_code: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.command,
            payload.exit_code
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute shell.completed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.shell_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // shell.completed never blocks - always allow (command already executed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

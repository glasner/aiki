use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// shell.permission_asked event payload
///
/// Fires before a shell command is executed. This is the critical event
/// for autonomous review workflows - intercept `git commit`, run checks,
/// provide feedback for self-correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiShellPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The shell command about to be executed
    pub command: String,
}

/// Handle shell.permission_asked event
///
/// This is the autonomous review wedge - intercept shell commands like `git commit`,
/// run review checks, and provide feedback for self-correction.
pub fn handle_shell_permission_asked(payload: AikiShellPermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "shell.permission_asked from {:?}, session: {}, command: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.command
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute shell.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.shell_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // shell.permission_asked is gateable - can block based on flow result
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResult {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => Ok(HookResult {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

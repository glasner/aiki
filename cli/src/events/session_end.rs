use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// Session end event payload (when agent session ends/disconnects)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionEndPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session end event (when agent session ends/disconnects)
///
/// Executes the SessionEnd flow section for user-defined cleanup actions,
/// then cleans up the session file. This event fires when the agent session
/// ends, either explicitly or when PostResponse doesn't generate an autoreply.
pub fn handle_session_end(payload: AikiSessionEndPayload) -> Result<HookResult> {
    debug_log(|| format!("Session ended by {:?}", payload.session.agent_type()));

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload (clone needed for session.end() call below)
    let mut state = AikiState::new(payload.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionEnd statements from the core flow
    let flow_result = FlowEngine::execute_statements(&core_flow.session_end, &mut state)?;

    // Clean up session file (always happens, regardless of flow result)
    payload.session.end(&payload.cwd)?;

    // Extract failures from state
    let failures = state.take_failures();

    // Translate FlowResult to HookResult
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

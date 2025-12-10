use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{Decision, HookResponse};

/// Session end event (when agent session ends/disconnects)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionEndEvent {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session end event (when agent session ends/disconnects)
///
/// Executes the SessionEnd flow section for user-defined cleanup actions,
/// then cleans up the session file. This event fires when the agent session
/// ends, either explicitly or when PostResponse doesn't generate an autoreply.
pub fn handle_session_end(event: AikiSessionEndEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session ended by {:?}", event.session.agent_type());
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionEnd statements from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.session_end, &mut state)?;

    // Clean up session file (always happens, regardless of flow result)
    event.session.end(&event.cwd)?;

    // Extract failures from state
    let failures = state.take_failures();

    // Translate FlowResult to HookResponse
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => Ok(HookResponse {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

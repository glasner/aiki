use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// Session start event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionStartPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(payload: AikiSessionStartPayload) -> Result<HookResult> {
    debug_log(|| format!(
        "Session started by {:?}",
        payload.session.agent_type()
    ));

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionStart statements from the core flow
    let flow_result = FlowEngine::execute_statements(&core_flow.session_start, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

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

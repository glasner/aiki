use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// response.received event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiResponseReceivedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The agent's original response text (immutable)
    pub response: String,
    /// Files that were modified by the agent during this response
    #[serde(default)]
    pub modified_files: Vec<PathBuf>,
}

/// Handle response.received event
///
/// This event fires when the agent finishes generating its response,
/// allowing flows to validate output, detect errors, and optionally send an autoreply to the agent.
/// Returns autoreply via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
pub fn handle_response_received(payload: AikiResponseReceivedPayload) -> Result<HookResult> {
    debug_log(|| format!(
        "response.received event from {:?}, response length: {}",
        payload.session.agent_type(),
        payload.response.len()
    ));

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute response.received actions from the core flow (catch errors for graceful degradation)
    let _flow_result = match FlowEngine::execute_statements(&core_flow.response_received, &mut state) {
        Ok(result) => result,
        Err(e) => {
            // Flow execution failed - log warning and skip autoreply
            eprintln!("\n⚠️ response.received flow failed: {}", e);
            eprintln!("No autoreply generated.\n");
            return Ok(HookResult {
                context: state.build_context(),
                decision: Decision::Allow,
                failures: state.take_failures(),
            });
        }
    };

    // Extract failures from state
    let failures = state.take_failures();

    // response.received never blocks - always allow
    Ok(HookResult {
        context: state.build_context(),
        decision: Decision::Allow,
        failures,
    })
}

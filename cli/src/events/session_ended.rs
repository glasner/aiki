use super::prelude::*;

/// session.ended event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionEndedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.ended event
///
/// Executes the session.ended flow section for user-defined cleanup actions,
/// then cleans up the session file. This event fires when the agent session
/// ends, either explicitly or when response.received doesn't generate an autoreply.
pub fn handle_session_ended(payload: AikiSessionEndedPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| format!("Session ended by {:?}", payload.session.agent_type()));

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload (clone needed for session.end() call below)
    let mut state = AikiState::new(payload.clone());

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let flow_result = execute_flow(
        EventType::SessionEnded,
        &mut state,
        &core_flow.session_ended,
    )?;

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

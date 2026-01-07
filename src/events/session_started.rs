use super::prelude::*;

/// session.started event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionStartPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.started event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_session_started(payload: AikiSessionStartPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| format!("Session started by {:?}", payload.session.agent_type()));

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let flow_result = execute_flow(
        EventType::SessionStarted,
        &mut state,
        &core_flow.session_started,
    )?;

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

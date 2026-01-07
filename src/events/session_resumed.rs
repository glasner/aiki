use super::prelude::*;

/// session.resumed event payload
///
/// Fires when continuing a previous session (as opposed to starting a new one).
/// This allows flows to differentiate between fresh starts and continuations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionResumedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.resumed event
///
/// This event fires when a session is being resumed rather than started fresh.
/// Allows flows to load prior context, apply previous approvals, maintain audit trail continuity.
pub fn handle_session_resumed(payload: AikiSessionResumedPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| {
        format!(
            "Session resumed by {:?}, session: {}",
            payload.session.agent_type(),
            payload.session.external_id()
        )
    });

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let _flow_result = execute_flow(
        EventType::SessionResumed,
        &mut state,
        &core_flow.session_resumed,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // session.resumed never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

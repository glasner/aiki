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
    debug_log(|| {
        format!(
            "Session resumed by {:?}, session: {}",
            payload.session.agent_type(),
            payload.session.external_id()
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute session.resumed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.session_resumed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // session.resumed never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

use super::prelude::*;

/// session.cleared event payload
///
/// Fires after /clear resets the conversation. Re-injects workspace path
/// and task context since the conversation history is wiped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionClearedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.cleared event
///
/// Re-injects critical state after /clear:
/// - Workspace isolation path
/// - Task count / workflow reminders
pub fn handle_session_cleared(payload: AikiSessionClearedPayload) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "Session cleared for {:?}, session: {}",
            payload.session.agent_type(),
            payload.session.external_id()
        )
    });

    // Load core hook for fallback
    let core_hook = crate::flows::load_core_hook();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute hook via HookComposer (with fallback to bundled core hook)
    let _flow_result = execute_hook(
        EventType::SessionCleared,
        &mut state,
        &core_hook.handlers.session_cleared,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // session.cleared returns context (workspace + tasks) but never blocks
    Ok(HookResult {
        context: state.build_context(),
        decision: Decision::Allow,
        failures,
    })
}

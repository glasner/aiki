use super::prelude::*;

/// session.compacted event payload
///
/// Fires after context compaction. Re-injects workspace path, active tasks,
/// and workflow reminders that would otherwise be lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionCompactedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.compacted event
///
/// Re-injects critical state after context compaction:
/// - Workspace isolation path
/// - Active task awareness
/// - Task count / workflow reminders
pub fn handle_session_compacted(payload: AikiSessionCompactedPayload) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "Session compacted for {:?}, session: {}",
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
        EventType::SessionCompacted,
        &mut state,
        &core_hook.handlers.session_compacted,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // session.compacted returns context (workspace + tasks) but never blocks
    Ok(HookResult {
        context: state.build_context(),
        decision: Decision::Allow,
        failures,
    })
}

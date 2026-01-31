use super::prelude::*;

/// read.completed event payload
///
/// Fires after a file read operation completes.
/// Read operations don't need provenance tracking (they don't modify the repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiReadCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool that performed the read (e.g., "Read", "Glob", "Grep")
    pub tool_name: String,
    /// Files that were read
    pub file_paths: Vec<String>,
    /// Whether the operation succeeded (always true for completed events)
    pub success: bool,
}

/// Handle read.completed event
///
/// This event fires after a file read operation completes.
/// Currently a no-op since reads don't need provenance tracking.
pub fn handle_read_completed(payload: AikiReadCompletedPayload) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "read.completed event from {:?}, session: {}, tool: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name
        )
    });

    // Load core hook for fallback
    let core_hook = crate::flows::load_core_hook();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute hook via HookComposer (with fallback to bundled core hook)
    let _flow_result = execute_hook(
        EventType::ReadCompleted,
        &mut state,
        &core_hook.read_completed,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // read.completed never blocks - always allow (operation already completed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

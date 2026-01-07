use crate::tools::WebOperation;

use super::prelude::*;

/// web.completed event payload
///
/// Fires after a web operation completes. Contains the operation type,
/// success status, and response details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWebCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The type of web operation that was performed
    pub operation: WebOperation,
    /// URL that was fetched (for fetch operations)
    #[serde(default)]
    pub url: Option<String>,
    /// Search query that was used (for search operations)
    #[serde(default)]
    pub query: Option<String>,
    /// Whether the operation succeeded (always true for completed events)
    pub success: bool,
}

/// Handle web.completed event
///
/// This event fires after a web operation completes. Can be used to
/// log network access, react to failures, or trigger follow-up actions.
pub fn handle_web_completed(payload: AikiWebCompletedPayload) -> Result<HookResult> {
    use super::prelude::execute_core_flow;

    debug_log(|| {
        format!(
            "web.completed from {:?}, session: {}, operation: {}, success: {:?}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.operation,
            payload.success
        )
    });

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let _flow_result = execute_core_flow(
        EventType::WebCompleted,
        &mut state,
        &core_flow.web_completed,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // web.completed never blocks - always allow (operation already executed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

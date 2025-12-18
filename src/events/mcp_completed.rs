use super::prelude::*;

/// mcp.completed event payload
///
/// Fires after an MCP tool call completes. Contains the tool name,
/// success status, and result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiMcpCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// MCP server name (parsed from tool_name format: mcp__<server>__<tool>)
    #[serde(default)]
    pub server: Option<String>,
    /// Name of the MCP tool that was called
    pub tool_name: String,
    /// Whether the tool call succeeded
    pub success: bool,
    /// Result from the tool call (if any)
    #[serde(default)]
    pub result: Option<String>,
}

/// Handle mcp.completed event
///
/// This event fires after an MCP tool call completes. Can be used to
/// log tool usage, react to failures, or trigger follow-up actions.
pub fn handle_mcp_completed(payload: AikiMcpCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "mcp.completed from {:?}, session: {}, tool: {}, success: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name,
            payload.success
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute mcp.completed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.mcp_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // mcp.completed never blocks - always allow (tool already executed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

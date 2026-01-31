use super::prelude::*;

/// mcp.permission_asked event payload
///
/// Fires before an MCP tool is called. Allows gating expensive operations,
/// enforcing rate limits, or auditing tool usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiMcpPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// MCP server name (parsed from tool_name format: mcp__<server>__<tool>)
    #[serde(default)]
    pub server: Option<String>,
    /// Name of the MCP tool being called
    pub tool_name: String,
    /// Parameters passed to the MCP tool
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Parse server name from MCP tool name format: mcp__<server>__<tool>
///
/// Returns the server name if the tool name follows the convention,
/// or None if the format is not recognized.
#[must_use]
pub fn parse_mcp_server(tool_name: &str) -> Option<String> {
    // MCP tools follow format: mcp__<server>__<tool>
    if !tool_name.starts_with("mcp__") {
        return None;
    }

    // Split after "mcp__" and find the next "__"
    let after_prefix = &tool_name[5..]; // Skip "mcp__"
    if let Some(idx) = after_prefix.find("__") {
        let server = &after_prefix[..idx];
        if !server.is_empty() {
            return Some(server.to_string());
        }
    }

    None
}

/// Handle mcp.permission_asked event
///
/// This event fires before an MCP tool call. Can be used to gate expensive
/// operations, enforce rate limits, or audit tool usage.
pub fn handle_mcp_permission_asked(payload: AikiMcpPermissionAskedPayload) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "mcp.permission_asked from {:?}, session: {}, tool: {}",
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
    let flow_result = execute_hook(
        EventType::McpPermissionAsked,
        &mut state,
        &core_hook.mcp_permission_asked,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // mcp.permission_asked is gateable - can block based on flow result
    match flow_result {
        HookOutcome::Success | HookOutcome::FailedContinue | HookOutcome::FailedStop => {
            Ok(HookResult {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        HookOutcome::FailedBlock => Ok(HookResult {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_server_valid() {
        // Standard format: mcp__<server>__<tool>
        assert_eq!(
            parse_mcp_server("mcp__github__get_issue"),
            Some("github".to_string())
        );
        assert_eq!(
            parse_mcp_server("mcp__filesystem__read_file"),
            Some("filesystem".to_string())
        );
        assert_eq!(
            parse_mcp_server("mcp__my_server__some_tool"),
            Some("my_server".to_string())
        );
    }

    #[test]
    fn test_parse_mcp_server_nested_underscores() {
        // Server name with underscores, tool name after second __
        assert_eq!(
            parse_mcp_server("mcp__my_cool_server__do_thing"),
            Some("my_cool_server".to_string())
        );
    }

    #[test]
    fn test_parse_mcp_server_invalid() {
        // Not MCP tool
        assert_eq!(parse_mcp_server("Edit"), None);
        assert_eq!(parse_mcp_server("Bash"), None);
        assert_eq!(parse_mcp_server("Read"), None);

        // Missing parts
        assert_eq!(parse_mcp_server("mcp__"), None);
        assert_eq!(parse_mcp_server("mcp__server"), None); // No second __
        assert_eq!(parse_mcp_server("mcp____tool"), None); // Empty server name
    }
}

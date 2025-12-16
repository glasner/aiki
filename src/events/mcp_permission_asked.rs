use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// mcp.permission_asked event payload
///
/// Fires before an MCP tool is called. Allows gating expensive operations,
/// enforcing rate limits, or auditing tool usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiMcpPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Name of the MCP tool being called
    pub tool_name: String,
    /// Parameters passed to the MCP tool
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Handle mcp.permission_asked event
///
/// This event fires before an MCP tool call. Can be used to gate expensive
/// operations, enforce rate limits, or audit tool usage.
pub fn handle_mcp_permission_asked(payload: AikiMcpPermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "mcp.permission_asked from {:?}, session: {}, tool: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute mcp.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.mcp_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // mcp.permission_asked is gateable - can block based on flow result
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

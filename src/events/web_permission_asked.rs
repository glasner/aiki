use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use crate::tools::WebOperation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// web.permission_asked event payload
///
/// Fires before a web operation (fetch or search). Allows gating network
/// requests, enforcing rate limits, or auditing web access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWebPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The type of web operation being requested
    pub operation: WebOperation,
    /// URL being fetched (for fetch operations)
    #[serde(default)]
    pub url: Option<String>,
    /// Search query (for search operations)
    #[serde(default)]
    pub query: Option<String>,
}

/// Handle web.permission_asked event
///
/// This event fires before a web operation. Can be used to gate network
/// requests, enforce rate limits, or audit web access.
pub fn handle_web_permission_asked(payload: AikiWebPermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "web.permission_asked from {:?}, session: {}, operation: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.operation
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute web.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.web_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // web.permission_asked is gateable - can block based on flow result
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

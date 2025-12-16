use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use crate::tools::WebOperation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

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
    /// Whether the operation succeeded
    #[serde(default)]
    pub success: Option<bool>,
}

/// Handle web.completed event
///
/// This event fires after a web operation completes. Can be used to
/// log network access, react to failures, or trigger follow-up actions.
pub fn handle_web_completed(payload: AikiWebCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "web.completed from {:?}, session: {}, operation: {}, success: {:?}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.operation,
            payload.success
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute web.completed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.web_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // web.completed never blocks - always allow (operation already executed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

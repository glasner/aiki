use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// delete.completed event payload
///
/// Fires after a file delete operation completes.
/// Basic provenance tracking - full delete provenance is tracked in delete-provenance.md.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiDeleteCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool that performed the delete (always "Bash" for shell-based deletes)
    pub tool_name: String,
    /// Files that were deleted
    pub file_paths: Vec<String>,
    /// Whether the operation succeeded
    #[serde(default)]
    pub success: Option<bool>,
}

/// Handle delete.completed event
///
/// This event fires after a file delete operation completes.
/// Currently just creates a new change to separate delete from other operations.
/// Full delete provenance tracking is tracked in a separate plan.
pub fn handle_delete_completed(payload: AikiDeleteCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "delete.completed event from {:?}, session: {}, tool: {}",
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

    // Execute delete.completed actions from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.delete_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // delete.completed never blocks - always allow (operation already completed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

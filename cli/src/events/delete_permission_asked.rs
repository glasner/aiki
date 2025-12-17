use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// delete.permission_asked event payload
///
/// Fires when the agent requests permission to delete a file.
/// This is a gateable event - flows can block deletion of important files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiDeletePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool requesting the delete (always "Bash" for shell-based deletes)
    pub tool_name: String,
    /// Files about to be deleted
    pub file_paths: Vec<String>,
}

/// Handle delete.permission_asked event
///
/// This event fires when the agent requests permission to delete files.
/// It allows flows to block deletion of important files.
pub fn handle_delete_permission_asked(
    payload: AikiDeletePermissionAskedPayload,
) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "delete.permission_asked event from {:?}, session: {}, tool: {}",
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

    // Execute delete.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.delete_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // delete.permission_asked is gateable - can block deletion of important files
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

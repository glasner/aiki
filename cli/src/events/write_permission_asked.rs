use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// write.permission_asked event payload
///
/// Fires when the agent requests permission to write a file.
/// This is a key event for provenance: stash user changes before AI overwrites them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWritePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool requesting the write (e.g., "Edit", "Write", "NotebookEdit")
    pub tool_name: String,
    /// Files about to be written
    pub file_paths: Vec<String>,
}

/// Handle write.permission_asked event
///
/// This event fires when the agent requests permission to write files.
/// The core flow uses this to stash uncommitted user changes before AI edits.
pub fn handle_write_permission_asked(
    payload: AikiWritePermissionAskedPayload,
) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "write.permission_asked event from {:?}, session: {}, tool: {}",
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

    // Execute write.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.write_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // write.permission_asked is gateable - can block writes to protected files
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

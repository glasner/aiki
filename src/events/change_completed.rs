use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::file_completed::{AikiFileCompletedPayload, EditDetail};
use super::result::{Decision, HookResult};

/// change.completed event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiChangeCompletedPayload {
    pub session: AikiSession,
    pub tool_name: String, // Tool that made the change (e.g., "Edit", "Write")
    pub file_paths: Vec<String>, // Files that were modified (batch support)
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Detailed edit operations (old_string -> new_string pairs) for user edit detection
    /// Only populated when the agent/IDE provides this information (ACP Edit tool, hooks)
    #[serde(default)]
    pub edit_details: Vec<EditDetail>,
}

/// Convert a file.completed payload to a change.completed payload for backward compatibility
///
/// This allows the existing core functions (classify_edits, build_metadata, etc.)
/// to work with the new file.completed events without requiring major refactoring.
impl From<&AikiFileCompletedPayload> for AikiChangeCompletedPayload {
    fn from(file_payload: &AikiFileCompletedPayload) -> Self {
        AikiChangeCompletedPayload {
            session: file_payload.session.clone(),
            tool_name: file_payload.tool_name.clone(),
            file_paths: file_payload.file_paths.clone(),
            cwd: file_payload.cwd.clone(),
            timestamp: file_payload.timestamp,
            edit_details: file_payload.edit_details.clone(),
        }
    }
}

/// Handle change.completed event
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_change_completed(payload: AikiChangeCompletedPayload) -> Result<HookResult> {
    // No validation needed - all required fields are guaranteed by type system

    debug_log(|| {
        format!(
            "Recording change by {:?}, session: {}, tool: {}",
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

    // Execute change.completed actions from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.change_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // change.completed never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::file_permission_asked::FileOperation;
use super::result::{Decision, HookResult};

/// Details about an individual edit operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditDetail {
    /// File path that was edited
    pub file_path: String,
    /// The old string that was replaced (empty if this is an insertion)
    pub old_string: String,
    /// The new string that replaced it (empty if this is a deletion)
    pub new_string: String,
}

impl EditDetail {
    /// Create a new EditDetail
    #[must_use]
    pub fn new(
        file_path: impl Into<String>,
        old_string: impl Into<String>,
        new_string: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            old_string: old_string.into(),
            new_string: new_string.into(),
        }
    }
}

/// file.completed event payload
///
/// Fires after a file operation completes.
/// Replaces the older change.completed event with additional operation info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiFileCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The type of file operation that completed
    pub operation: FileOperation,
    /// Tool that made the change (e.g., "Edit", "Write", "Bash")
    pub tool_name: String,
    /// Files that were accessed (batch support)
    pub file_paths: Vec<String>,
    /// Whether the operation succeeded
    #[serde(default)]
    pub success: Option<bool>,
    /// Detailed edit operations (old_string -> new_string pairs) for user edit detection
    /// Only populated when the agent/IDE provides this information (ACP Edit tool, hooks)
    #[serde(default)]
    pub edit_details: Vec<EditDetail>,
}

/// Handle file.completed event
///
/// This is the core provenance tracking event. Records metadata about
/// the file operation in the JJ change description using the flow engine.
pub fn handle_file_completed(payload: AikiFileCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "Recording file.completed by {:?}, session: {}, tool: {}, operation: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name,
            payload.operation
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute file.completed actions from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.file_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // file.completed never blocks - always allow (operation already completed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

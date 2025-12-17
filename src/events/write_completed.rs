use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// write.completed event payload
///
/// Fires after a file write operation completes.
/// This is the core provenance tracking event for write operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWriteCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool that performed the write (e.g., "Edit", "Write", "NotebookEdit")
    pub tool_name: String,
    /// Files that were written
    pub file_paths: Vec<String>,
    /// Whether the operation succeeded (always true for completed events)
    pub success: bool,
    /// Detailed edit operations (old_string -> new_string pairs) for user edit detection
    /// Only populated for Edit/MultiEdit tools that have old/new string info.
    /// Write tool and NotebookEdit don't have meaningful edit_details.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edit_details: Vec<EditDetail>,
}

/// Handle write.completed event
///
/// This is the core provenance tracking event for write operations.
/// Records metadata about the file changes in the JJ change description.
pub fn handle_write_completed(payload: AikiWriteCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "write.completed event from {:?}, session: {}, tool: {}",
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

    // Execute write.completed actions from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.write_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // write.completed never blocks - always allow (operation already completed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

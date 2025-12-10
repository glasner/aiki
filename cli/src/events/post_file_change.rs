use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::response::{Decision, HookResult};

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

/// Post-file-change event (after file modification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostFileChangeEvent {
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

/// Handle post-file-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_post_file_change(event: AikiPostFileChangeEvent) -> Result<HookResult> {
    // No validation needed - all required fields are guaranteed by type system

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Recording change by {:?}, session: {}, tool: {}",
            event.session.agent_type(),
            event.session.external_id(),
            event.tool_name
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event (clone for error message)
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostFileChange actions from the core flow
    let (_flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.post_file_change, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // PostFileChange never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

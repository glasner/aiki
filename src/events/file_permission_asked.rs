use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use crate::tools::FileOperation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// file.permission_asked event payload
///
/// Fires when the agent requests permission to access a file.
/// Replaces the older change.permission_asked event with additional operation info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiFilePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The type of file operation being requested
    pub operation: FileOperation,
    /// File path or glob pattern being accessed
    #[serde(default)]
    pub path: Option<String>,
    /// Search pattern (grep operations only)
    #[serde(default)]
    pub pattern: Option<String>,
}

/// Handle file.permission_asked event
///
/// This event fires when the agent requests permission to access files.
/// It allows flows to gate file operations based on type (read/write/delete).
pub fn handle_file_permission_asked(payload: AikiFilePermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "file.permission_asked event from {:?}, session: {}, operation: {}",
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

    // Execute file.permission_asked statements from the core flow
    let _flow_result =
        FlowEngine::execute_statements(&core_flow.file_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // file.permission_asked never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

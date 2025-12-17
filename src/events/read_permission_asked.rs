use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// read.permission_asked event payload
///
/// Fires when the agent requests permission to read a file.
/// This is a gateable event - flows can block sensitive file reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiReadPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool requesting the read (e.g., "Read", "Glob", "Grep")
    pub tool_name: String,
    /// Files/directories about to be read or searched
    /// - Read: actual file paths
    /// - Glob/Grep: search directory (defaults to cwd if not specified)
    pub file_paths: Vec<String>,
    /// Search pattern for Glob/Grep tools
    /// - Glob: glob pattern (e.g., "**/*.rs")
    /// - Grep: regex pattern (e.g., "TODO.*fix")
    /// - Read: None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Handle read.permission_asked event
///
/// This event fires when the agent requests permission to read files.
/// It allows flows to gate reads of sensitive files (secrets, .env, etc.).
pub fn handle_read_permission_asked(payload: AikiReadPermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "read.permission_asked event from {:?}, session: {}, tool: {}",
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

    // Execute read.permission_asked statements from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.read_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // read.permission_asked is gateable - can block sensitive file reads
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

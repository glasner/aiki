use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// read.completed event payload
///
/// Fires after a file read operation completes.
/// Read operations don't need provenance tracking (they don't modify the repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiReadCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool that performed the read (e.g., "Read", "Glob", "Grep")
    pub tool_name: String,
    /// Files that were read
    pub file_paths: Vec<String>,
    /// Whether the operation succeeded (always true for completed events)
    pub success: bool,
}

/// Handle read.completed event
///
/// This event fires after a file read operation completes.
/// Currently a no-op since reads don't need provenance tracking.
pub fn handle_read_completed(payload: AikiReadCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "read.completed event from {:?}, session: {}, tool: {}",
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

    // Execute read.completed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.read_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // read.completed never blocks - always allow (operation already completed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

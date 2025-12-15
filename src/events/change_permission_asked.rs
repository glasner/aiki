use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// change.permission_asked event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiChangePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle change.permission_asked event
///
/// This event fires when the agent requests permission to modify files.
/// It allows flows to stash user edits before the AI starts making changes,
/// ensuring clean separation between human and AI work.
pub fn handle_change_permission_asked(payload: AikiChangePermissionAskedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "change.permission_asked event from {:?}, session: {}",
            payload.session.agent_type(),
            payload.session.external_id()
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute change.permission_asked actions from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.change_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // change.permission_asked never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

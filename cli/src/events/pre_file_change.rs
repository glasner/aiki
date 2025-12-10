use crate::error::Result;
use crate::flows::{AikiState, FlowEngine};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::response::{Decision, HookResult};

/// Pre-file-change event (before file modification begins)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPreFileChangeEvent {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle pre-file-change event (before file modification begins)
///
/// This event fires when the agent requests permission to modify files.
/// It allows flows to stash user edits before the AI starts making changes,
/// ensuring clean separation between human and AI work.
pub fn handle_pre_file_change(event: AikiPreFileChangeEvent) -> Result<HookResult> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PreFileChange event from {:?}, session: {}",
            event.session.agent_type(),
            event.session.external_id()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PreFileChange actions from the core flow
    let (_flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.pre_file_change, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // PreFileChange never blocks - always allow
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

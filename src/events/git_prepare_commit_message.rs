use crate::cache::debug_log;
use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::provenance::AgentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::result::{Decision, HookResult};

/// git.prepare_commit_message event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiGitPrepareCommitMessagePayload {
    pub agent_type: AgentType,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Path to the commit message file (COMMIT_EDITMSG)
    pub commit_msg_file: Option<PathBuf>,
}

/// Handle git.prepare_commit_message event
///
/// Executes the git.prepare_commit_message flow section to modify the commit message.
/// Typically used for adding co-author attributions, but can add any content.
/// Called from Git's prepare-commit-msg hook via `aiki event prepare-commit-msg`.
pub fn handle_git_prepare_commit_message(
    payload: AikiGitPrepareCommitMessagePayload,
) -> Result<HookResult> {
    debug_log(|| "Preparing commit message");

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute git.prepare_commit_message actions from the core flow
    let flow_result = FlowEngine::execute_statements(&core_flow.git_prepare_commit_message, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResult {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => {
            // Block the commit
            Ok(HookResult {
                context: None,
                decision: Decision::Block,
                failures,
            })
        }
    }
}

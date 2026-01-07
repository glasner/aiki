use crate::provenance::AgentType;

use super::prelude::*;

/// commit.message_started event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiCommitMessageStartedPayload {
    pub agent_type: AgentType,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Path to the commit message file (COMMIT_EDITMSG)
    pub commit_msg_file: Option<PathBuf>,
}

/// Handle commit.message_started event
///
/// Executes the commit.message_started flow section to modify the commit message.
/// Typically used for adding co-author attributions, but can add any content.
/// Called from Git's prepare-commit-msg hook via `aiki event prepare-commit-msg`.
pub fn handle_commit_message_started(
    payload: AikiCommitMessageStartedPayload,
) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| "Preparing commit message");

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let flow_result = execute_flow(
        EventType::CommitMessageStarted,
        &mut state,
        &core_flow.commit_message_started,
    )?;

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

use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiPrepareCommitMessageEvent};
use crate::provenance::AgentType;
use chrono::Utc;
use std::env;
use std::path::PathBuf;

/// Dispatch a PrepareCommitMessage event through the event bus
pub fn run_prepare_commit_message() -> Result<()> {
    let cwd = env::current_dir()?;

    // Get commit message file path from environment (set by Git hook)
    let commit_msg_file = env::var("AIKI_COMMIT_MSG_FILE").ok().map(PathBuf::from);

    let event = AikiPrepareCommitMessageEvent {
        agent_type: AgentType::ClaudeCode, // Default agent for git hooks
        cwd,
        timestamp: Utc::now(),
        commit_msg_file,
    };

    event_bus::dispatch(AikiEvent::PrepareCommitMessage(event))
}

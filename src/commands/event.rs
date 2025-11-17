use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiPreCommitEvent};
use crate::provenance::AgentType;
use chrono::Utc;
use std::env;

/// Dispatch a PreCommit event through the event bus
pub fn run_pre_commit() -> Result<()> {
    let cwd = env::current_dir()?;

    let event = AikiPreCommitEvent {
        agent_type: AgentType::ClaudeCode, // Default agent for git hooks
        cwd,
        timestamp: Utc::now(),
    };

    event_bus::dispatch(AikiEvent::PreCommit(event))
}

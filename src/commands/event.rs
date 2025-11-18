use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiPrepareCommitMessageEvent};
use crate::provenance::AgentType;
use crate::vendors;
use chrono::Utc;
use std::env;
use std::path::PathBuf;

/// Detect editor type from environment variables
fn detect_editor() -> vendors::EditorType {
    // Detect from environment variables
    if env::var("CLAUDE_SESSION_ID").is_ok() {
        vendors::EditorType::ClaudeCode
    } else if env::var("CURSOR_SESSION_ID").is_ok() {
        vendors::EditorType::Cursor
    } else {
        vendors::EditorType::Unknown
    }
}

/// Dispatch a PrepareCommitMessage event through the event bus
///
/// This is called from Git's prepare-commit-msg hook. It runs the flow
/// to modify the commit message (typically adding co-author attributions),
/// translates the response to editor-specific format, and exits.
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

    // Get generic response from handler
    let response = event_bus::dispatch(AikiEvent::PrepareCommitMessage(event))?;

    // Detect editor and translate using shared translation layer
    let editor = detect_editor();
    let (json_output, exit_code) =
        vendors::translate_response(response, editor, "PrepareCommitMessage");

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with code
    std::process::exit(exit_code);
}

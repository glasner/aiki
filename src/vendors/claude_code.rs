use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiPostChangeEvent, AikiStartEvent};
use crate::provenance::AgentType;

/// Claude Code hook payload structure
///
/// This matches the JSON that Claude Code sends to PostToolUse hooks.
/// See: https://docs.claude.com/claude-code/hooks
#[derive(Deserialize, Debug)]
struct ClaudeCodePayload {
    session_id: String,
    transcript_path: String,
    cwd: String,
    hook_event_name: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Option<ToolInput>,
    #[serde(default)]
    tool_output: String,
}

#[derive(Deserialize, Debug)]
struct ToolInput {
    file_path: String,
    #[serde(default)]
    old_string: String,
    #[serde(default)]
    new_string: String,
}

/// Handle a Claude Code event
///
/// This is the vendor-specific handler for Claude Code hooks.
/// It:
/// 1. Reads Claude Code JSON from stdin
/// 2. Translates vendor event name to Aiki event type
/// 3. Creates a standardized AikiEvent with agent type embedded
/// 4. Dispatches to the event bus
///
/// # Arguments
/// * `event_name` - Vendor event name from CLI flag (e.g., "SessionStart", "PostToolUse")
pub fn handle(event_name: &str) -> Result<()> {
    // Read Claude Code-specific JSON from stdin
    let payload: ClaudeCodePayload = super::read_stdin_json()?;

    // Validate event name matches JSON (optional but good practice)
    if std::env::var("AIKI_DEBUG").is_ok() && payload.hook_event_name != event_name {
        eprintln!(
            "[aiki] Warning: Event name mismatch. CLI: {}, JSON: {}",
            event_name, payload.hook_event_name
        );
    }

    // Create standardized event with embedded agent type
    let event = match event_name {
        "SessionStart" => AikiEvent::Start(AikiStartEvent {
            agent_type: AgentType::ClaudeCode,
            session_id: Some(payload.session_id),
            cwd: PathBuf::from(&payload.cwd),
            timestamp: chrono::Utc::now(),
        }),
        "PostToolUse" => {
            // Extract required fields for PostChange event
            let tool_input = payload
                .tool_input
                .ok_or_else(|| anyhow::anyhow!("PostToolUse requires tool_input"))?;

            AikiEvent::PostChange(AikiPostChangeEvent {
                agent_type: AgentType::ClaudeCode,
                session_id: payload.session_id,
                tool_name: payload.tool_name,
                file_path: tool_input.file_path,
                cwd: PathBuf::from(&payload.cwd),
                timestamp: chrono::Utc::now(),
            })
        }
        // Future events can be added here without hook reinstallation
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Claude Code event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Dispatch to event bus
    event_bus::dispatch(event)?;

    Ok(())
}

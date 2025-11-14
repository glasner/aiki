use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiEventType};
use crate::provenance::AgentType;

/// Claude Code hook payload structure
///
/// This matches the JSON that Claude Code sends to PostToolUse hooks.
/// See: https://docs.claude.com/claude-code/hooks
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ClaudeCodePayload {
    session_id: String,
    transcript_path: String,
    cwd: String,
    hook_event_name: String,
    tool_name: String,
    tool_input: ToolInput,
    tool_output: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
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

    // Translate vendor event name to Aiki event type
    let aiki_event_type = match event_name {
        "SessionStart" => AikiEventType::Start,
        "PostToolUse" => AikiEventType::PostChange,
        // Future events can be added here without hook reinstallation
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Claude Code event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Create standardized event with embedded agent type
    let event = AikiEvent::new(
        aiki_event_type,
        AgentType::ClaudeCode, // ← Agent embedded here
        PathBuf::from(&payload.cwd),
    )
    .with_session_id(payload.session_id)
    .with_metadata("tool_name", payload.tool_name)
    .with_metadata("file_path", payload.tool_input.file_path)
    .with_metadata("vendor_event", event_name); // Track original vendor event name

    // Dispatch to event bus
    event_bus::dispatch(event)?;

    Ok(())
}

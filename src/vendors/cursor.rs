use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiEventType};
use crate::provenance::AgentType;

/// Cursor hook payload structure
///
/// This matches the JSON that Cursor sends to its hooks.
/// Note: Cursor uses camelCase, different from Claude Code's snake_case.
#[derive(Deserialize, Debug)]
struct CursorPayload {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "workingDirectory")]
    working_directory: String,
    #[serde(rename = "eventName")]
    event_name: String,
    #[serde(rename = "editedFile", default)]
    edited_file: String,
    // Additional fields may be present but we don't need them
}

/// Handle a Cursor event
///
/// This is the vendor-specific handler for Cursor hooks.
/// It:
/// 1. Reads Cursor JSON from stdin
/// 2. Translates vendor event name to Aiki event type
/// 3. Creates a standardized AikiEvent with agent type embedded
/// 4. Dispatches to the event bus
///
/// # Arguments
/// * `event_name` - Vendor event name from CLI flag (e.g., "beforeSubmitPrompt", "afterFileEdit")
pub fn handle(event_name: &str) -> Result<()> {
    // Read Cursor-specific JSON from stdin
    let payload: CursorPayload = super::read_stdin_json()?;

    // Validate event name matches JSON (optional but good practice)
    if std::env::var("AIKI_DEBUG").is_ok() && payload.event_name != event_name {
        eprintln!(
            "[aiki] Warning: Event name mismatch. CLI: {}, JSON: {}",
            event_name, payload.event_name
        );
    }

    // Translate vendor event name to Aiki event type
    let aiki_event_type = match event_name {
        "beforeSubmitPrompt" => AikiEventType::Start,
        "afterFileEdit" => AikiEventType::PostChange,
        // Future events can be added here without hook reinstallation
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Cursor event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Create standardized event with embedded agent type
    let event = AikiEvent::new(
        aiki_event_type,
        AgentType::Cursor, // ← Agent embedded here
        PathBuf::from(&payload.working_directory),
    )
    .with_session_id(payload.session_id)
    .with_metadata("tool_name", "edit") // Cursor doesn't distinguish Edit/Write
    .with_metadata("file_path", payload.edited_file)
    .with_metadata("vendor_event", event_name); // Track original vendor event name

    // Dispatch to event bus
    event_bus::dispatch(event)?;

    Ok(())
}

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiPostChangeEvent, AikiStartEvent};
use crate::handlers::HookResponse;
use crate::provenance::AgentType;

/// Cursor hook payload structure
///
/// This matches the JSON that Cursor sends to its hooks.
/// Note: Cursor uses snake_case for afterFileEdit hook.
/// See: https://cursor.com/docs/agent/hooks#afterfileedit
#[derive(Deserialize, Debug)]
struct CursorPayload {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "workingDirectory")]
    working_directory: String,
    #[serde(rename = "eventName")]
    event_name: String,
    // afterFileEdit fields
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    edits: Vec<CursorEdit>,
    // Legacy field (deprecated in favor of file_path)
    #[serde(rename = "editedFile", default)]
    edited_file: String,
}

/// Individual edit operation in Cursor's afterFileEdit hook
#[derive(Deserialize, Debug)]
struct CursorEdit {
    old_string: String,
    new_string: String,
}

/// Handle a Cursor event
///
/// This is the vendor-specific handler for Cursor hooks.
/// It:
/// 1. Reads Cursor JSON from stdin
/// 2. Translates vendor event name to Aiki event type
/// 3. Creates a standardized AikiEvent with agent type embedded
/// 4. Dispatches to the event bus
/// 5. Translates the HookResponse to Cursor JSON format
/// 6. Outputs JSON to stdout and exits with appropriate code
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

    // Create standardized event with embedded agent type
    let event = match event_name {
        "beforeSubmitPrompt" => AikiEvent::SessionStart(AikiStartEvent {
            agent_type: AgentType::Cursor,
            session_id: Some(payload.session_id),
            cwd: PathBuf::from(&payload.working_directory),
            timestamp: chrono::Utc::now(),
        }),
        "afterFileEdit" => {
            // Use new file_path field if available, fallback to legacy editedFile
            let file_path = if !payload.file_path.is_empty() {
                payload.file_path
            } else {
                payload.edited_file
            };

            // Extract edit details from Cursor's edits array for user edit detection
            let edit_details: Vec<crate::events::EditDetail> = payload
                .edits
                .iter()
                .map(|edit| {
                    crate::events::EditDetail::new(
                        file_path.clone(),
                        edit.old_string.clone(),
                        edit.new_string.clone(),
                    )
                })
                .collect();

            if std::env::var("AIKI_DEBUG").is_ok() && !edit_details.is_empty() {
                eprintln!("[aiki] Cursor provided {} edits", edit_details.len());
            }

            AikiEvent::PostChange(AikiPostChangeEvent {
                agent_type: AgentType::Cursor,
                client_name: None, // Hook-based detection doesn't know client (IDE)
                client_version: None,
                agent_version: None,
                session_id: payload.session_id,
                tool_name: "edit".to_string(), // Cursor doesn't distinguish Edit/Write
                file_paths: vec![file_path],
                cwd: PathBuf::from(&payload.working_directory),
                timestamp: chrono::Utc::now(),
                detection_method: crate::provenance::DetectionMethod::Hook,
                edit_details,
            })
        }
        // Future events can be added here without hook reinstallation
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Cursor event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Dispatch to event bus and get generic response
    let response = event_bus::dispatch(event)?;

    // Translate to Cursor JSON format
    let (json_output, exit_code) = translate_response(response);

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with appropriate code
    std::process::exit(exit_code);
}

/// Translate HookResponse to Cursor JSON format
///
/// Cursor uses a simple format with user_message and agent_message fields.
fn translate_response(response: HookResponse) -> (Option<String>, i32) {
    let exit_code = response
        .exit_code
        .unwrap_or(if response.success { 0 } else { 1 });

    match exit_code {
        2 => {
            // Blocking error (exit 2)
            let mut json = Map::new();

            if let Some(msg) = response.user_message {
                json.insert("user_message".to_string(), json!(msg));
            }

            if let Some(agent_msg) = response.agent_message {
                json.insert("agent_message".to_string(), json!(agent_msg));
            }

            (Some(serde_json::to_string(&json).unwrap()), 2)
        }
        0 => {
            // Success or non-blocking
            let mut json = Map::new();

            if let Some(msg) = response.user_message {
                json.insert("user_message".to_string(), json!(msg));
            }

            if let Some(agent_msg) = response.agent_message {
                json.insert("agent_message".to_string(), json!(agent_msg));
            }

            // Metadata for all events
            if !response.metadata.is_empty() {
                let metadata: Map<String, Value> = response
                    .metadata
                    .into_iter()
                    .map(|(k, v)| (k, json!(v)))
                    .collect();
                json.insert("metadata".to_string(), json!(metadata));
            }

            if json.is_empty() {
                (None, 0)
            } else {
                (Some(serde_json::to_string(&json).unwrap()), 0)
            }
        }
        _ => {
            // Exit 1 or other: stderr fallback
            if let Some(msg) = response.user_message {
                eprintln!("{}", msg);
            }
            (None, exit_code)
        }
    }
}

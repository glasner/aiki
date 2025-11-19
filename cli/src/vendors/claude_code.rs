use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Map};
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiPostChangeEvent, AikiStartEvent};
use crate::handlers::HookResponse;
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
/// 5. Translates the HookResponse to Claude Code JSON format
/// 6. Outputs JSON to stdout and exits with appropriate code
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
        "SessionStart" => AikiEvent::SessionStart(AikiStartEvent {
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
                client_name: None, // Hook-based detection doesn't know client (IDE)
                client_version: None,
                agent_version: None,
                session_id: payload.session_id,
                tool_name: payload.tool_name,
                file_path: tool_input.file_path,
                cwd: PathBuf::from(&payload.cwd),
                timestamp: chrono::Utc::now(),
                detection_method: crate::provenance::DetectionMethod::Hook,
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

    // Dispatch to event bus and get generic response
    let response = event_bus::dispatch(event)?;

    // Translate to Claude Code JSON format
    let (json_output, exit_code) = translate_response(response, event_name);

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with appropriate code
    std::process::exit(exit_code);
}

/// Translate HookResponse to Claude Code JSON format
///
/// Claude Code expects different JSON structures depending on the event type
/// and the response status (success, warning, blocking error).
fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
    let exit_code = response
        .exit_code
        .unwrap_or(if response.success { 0 } else { 1 });

    // PostToolUse uses different JSON structure than other hooks
    let is_post_tool_use = event_type == "PostToolUse" || event_type == "PostChange";

    match exit_code {
        2 => {
            // Blocking error
            let mut json = Map::new();

            if is_post_tool_use {
                // PostToolUse: use decision: "block"
                json.insert("decision".to_string(), json!("block"));

                if let Some(msg) = response.user_message {
                    json.insert("reason".to_string(), json!(msg));
                }

                if let Some(agent_msg) = response.agent_message {
                    let mut hook_output = Map::new();
                    hook_output.insert("hookEventName".to_string(), json!("PostToolUse"));
                    hook_output.insert("additionalContext".to_string(), json!(agent_msg));
                    json.insert("hookSpecificOutput".to_string(), json!(hook_output));
                }
            } else {
                // Other hooks: use continue: false
                json.insert("continue".to_string(), json!(false));

                if let Some(msg) = response.user_message {
                    json.insert("stopReason".to_string(), json!(msg));
                }

                if let Some(agent_msg) = response.agent_message {
                    json.insert("systemMessage".to_string(), json!(agent_msg));
                }
            }

            (Some(serde_json::to_string(&json).unwrap()), 0)
        }
        0 => {
            // Success or non-blocking warnings
            let mut json = Map::new();

            // Only include systemMessage for warnings/errors (not pure success)
            let has_warning = response.user_message.as_ref().map_or(false, |msg| {
                msg.starts_with("⚠️") || msg.contains("warning") || msg.contains("failed")
            });

            if has_warning {
                if let Some(msg) = response.user_message {
                    json.insert("systemMessage".to_string(), json!(msg));
                }
            }

            if is_post_tool_use {
                // PostToolUse: use hookSpecificOutput for agent messages
                if let Some(agent_msg) = response.agent_message {
                    let mut hook_output = Map::new();
                    hook_output.insert("hookEventName".to_string(), json!("PostToolUse"));
                    hook_output.insert("additionalContext".to_string(), json!(agent_msg));
                    json.insert("hookSpecificOutput".to_string(), json!(hook_output));
                }
            }

            // Metadata for all events
            if !response.metadata.is_empty() {
                let metadata: Vec<Vec<String>> = response
                    .metadata
                    .into_iter()
                    .map(|(k, v)| vec![k, v])
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

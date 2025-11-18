pub mod claude_code;
pub mod cursor;

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::io::{self, Read};

use crate::handlers::HookResponse;

/// Read and parse JSON from stdin
///
/// Shared utility for all vendor handlers to read hook payload data.
pub fn read_stdin_json<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;
    Ok(serde_json::from_str(&buffer)?)
}

/// Editor type for translation
#[derive(Debug, Clone, Copy)]
pub enum EditorType {
    ClaudeCode,
    Cursor,
    Unknown,
}

/// Translate generic HookResponse to editor-specific JSON format
///
/// This is the central translation point that converts our internal generic
/// HookResponse into the JSON format expected by different editors.
///
/// # Arguments
/// * `response` - Generic response from handler
/// * `editor` - Target editor type
/// * `event_type` - Event type name for editor-specific formatting
///
/// # Returns
/// * `(Option<String>, i32)` - JSON string (if any) and exit code
pub fn translate_response(
    response: HookResponse,
    editor: EditorType,
    event_type: &str,
) -> (Option<String>, i32) {
    let exit_code = response
        .exit_code
        .unwrap_or(if response.success { 0 } else { 1 });

    match editor {
        EditorType::ClaudeCode => translate_claude(response, exit_code, event_type),
        EditorType::Cursor => translate_cursor(response, exit_code, event_type),
        EditorType::Unknown => translate_generic(response, exit_code),
    }
}

/// Translate to Claude Code JSON format
fn translate_claude(
    response: HookResponse,
    exit_code: i32,
    event_type: &str,
) -> (Option<String>, i32) {
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

/// Translate to Cursor JSON format
fn translate_cursor(
    response: HookResponse,
    exit_code: i32,
    _event_type: &str,
) -> (Option<String>, i32) {
    // Cursor uses simple format: user_message and agent_message
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

/// Translate to generic format (stderr only)
fn translate_generic(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    // For unknown editors, log to stderr
    if let Some(msg) = response.user_message {
        eprintln!("[aiki] {}", msg);
    }
    (None, exit_code)
}

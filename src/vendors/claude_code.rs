use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::event_bus;
use crate::events::{AikiEvent, AikiPostFileChangeEvent, AikiPreFileChangeEvent, AikiStartEvent};
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

/// Response structure for Claude Code hooks
struct ClaudeCodeResponse {
    json_value: Option<serde_json::Value>,
    exit_code: i32,
}

impl ClaudeCodeResponse {
    /// Print JSON to stdout if present
    fn print_json(&self) {
        let Some(ref value) = self.json_value else {
            return;
        };

        let Ok(json_string) = serde_json::to_string(value) else {
            return;
        };

        println!("{}", json_string);
    }
}

/// Handle a Claude Code event
///
/// This is the vendor-specific handler for Claude Code hooks.
/// Dispatches to event-specific handlers based on event name.
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

    // Build event from payload
    let aiki_event = match event_name {
        "SessionStart" => build_session_start_event(payload),
        "PreToolUse" => build_pre_file_change_event(payload),
        "PostToolUse" => build_post_file_change_event(payload),
        _ => AikiEvent::Unsupported,
    };

    // Dispatch event and exit with translated response
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let claude_response = translate_response(aiki_response, event_name);

    claude_response.print_json();
    std::process::exit(claude_response.exit_code);
}

/// Build SessionStart event from SessionStart payload
fn build_session_start_event(payload: ClaudeCodePayload) -> AikiEvent {
    AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Claude,
        session_id: Some(payload.session_id),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PreFileChange event from PreToolUse payload
fn build_pre_file_change_event(payload: ClaudeCodePayload) -> AikiEvent {
    // Fire PreFileChange only for file-modifying tools
    if !is_file_modifying_tool(&payload.tool_name) {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[aiki] PreToolUse: Ignoring non-file tool: {}",
                payload.tool_name
            );
        }
        return AikiEvent::Unsupported;
    }

    AikiEvent::PreFileChange(AikiPreFileChangeEvent {
        agent_type: AgentType::Claude,
        session_id: payload.session_id,
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PostFileChange event from PostToolUse payload
fn build_post_file_change_event(payload: ClaudeCodePayload) -> AikiEvent {
    // Extract required fields for PostFileChange event
    let Some(tool_input) = payload.tool_input else {
        eprintln!("[aiki] Warning: PostToolUse missing tool_input, ignoring event");
        return AikiEvent::Unsupported;
    };

    // Extract edit details from tool_input for user edit detection
    let edit_details = if !tool_input.old_string.is_empty() || !tool_input.new_string.is_empty() {
        vec![crate::events::EditDetail::new(
            tool_input.file_path.clone(),
            tool_input.old_string.clone(),
            tool_input.new_string.clone(),
        )]
    } else {
        Vec::new()
    };

    AikiEvent::PostFileChange(AikiPostFileChangeEvent {
        agent_type: AgentType::Claude,
        client_name: None, // Hook-based detection doesn't know client (IDE)
        client_version: None,
        agent_version: None,
        session_id: payload.session_id,
        tool_name: payload.tool_name,
        file_paths: vec![tool_input.file_path],
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        detection_method: crate::provenance::DetectionMethod::Hook,
        edit_details,
    })
}

/// Translate HookResponse to Claude Code JSON format
///
/// Claude Code expects different JSON structures depending on the event type.
/// This function dispatches to event-specific translators that handle the details.
fn translate_response(response: HookResponse, event_type: &str) -> ClaudeCodeResponse {
    match event_type {
        "SessionStart" => translate_session_start(&response),
        "UserPromptSubmit" => translate_user_prompt_submit(&response),
        "PreToolUse" => translate_pre_tool_use(&response),
        "PostToolUse" | "PostFileChange" => translate_post_tool_use(&response),
        "Stop" => translate_stop(&response),
        _ => {
            eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
            ClaudeCodeResponse {
                json_value: None,
                exit_code: 0,
            }
        }
    }
}

/// Translate SessionStart event to Claude Code JSON format
fn translate_session_start(response: &HookResponse) -> ClaudeCodeResponse {
    let combined = response.combined_output();

    let json_value = if let Some(ctx) = combined {
        // Has context - include systemMessage and hookSpecificOutput
        json!({
            "systemMessage": "🎉 aiki initialized",
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": ctx
            }
        })
    } else {
        // No context - return empty object
        json!({})
    };

    ClaudeCodeResponse {
        json_value: Some(json_value),
        exit_code: 0,
    }
}

/// Translate UserPromptSubmit event to Claude Code JSON format
fn translate_user_prompt_submit(response: &HookResponse) -> ClaudeCodeResponse {
    if response.exit_code == 2 {
        // Block the prompt
        let reason = response.format_messages();
        let mut json_value = json!({
            "decision": "block",
            "reason": reason
        });

        // Add hookSpecificOutput if there's context to include
        if let Some(ref ctx) = response.context {
            json_value["hookSpecificOutput"] = json!({
                "hookEventName": "UserPromptSubmit",
                "additionalContext": ctx
            });
        }

        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
    } else {
        // Allow with optional context
        let combined = response.combined_output();
        let json_value = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
    }
}

/// Translate PreToolUse event to Claude Code JSON format
fn translate_pre_tool_use(response: &HookResponse) -> ClaudeCodeResponse {
    let formatted_messages = response.format_messages();

    // Determine permission decision from response
    // For now, default to "allow" unless blocked
    let (permission_decision, reason) = if response.exit_code == 2 {
        ("deny", Some(formatted_messages))
    } else {
        (
            "allow",
            if !formatted_messages.is_empty() {
                Some(formatted_messages)
            } else {
                None
            },
        )
    };

    let mut json_value = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": permission_decision
        }
    });

    // Add reason if present
    if let Some(reason_text) = reason {
        json_value["hookSpecificOutput"]["permissionDecisionReason"] = json!(reason_text);
    }

    ClaudeCodeResponse {
        json_value: Some(json_value),
        exit_code: 0,
    }
}

/// Translate PostToolUse event to Claude Code JSON format
fn translate_post_tool_use(response: &HookResponse) -> ClaudeCodeResponse {
    if response.exit_code == 2 {
        // Block (autoreply with reason)
        let reason = response.format_messages();
        let reason_text = if !reason.is_empty() {
            reason
        } else {
            "Tool execution requires attention".to_string()
        };

        let mut json_value = json!({
            "decision": "block",
            "reason": reason_text
        });

        // Add optional context
        if let Some(ref ctx) = response.context {
            json_value["hookSpecificOutput"] = json!({
                "hookEventName": "PostToolUse",
                "additionalContext": ctx
            });
        }

        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
    } else {
        // Allow with optional context
        let combined = response.combined_output();
        let json_value = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
    }
}

/// Translate Stop event to Claude Code JSON format
fn translate_stop(response: &HookResponse) -> ClaudeCodeResponse {
    let combined = response.combined_output();

    let json_value = if let Some(reason_text) = combined {
        // Block (autoreply/force continuation)
        json!({
            "decision": "block",
            "reason": reason_text
        })
    } else {
        // Allow normal stop
        json!({})
    };

    ClaudeCodeResponse {
        json_value: Some(json_value),
        exit_code: 0,
    }
}

/// Check if a tool modifies files
///
/// Returns true for tools that create, modify, or delete files.
/// PreFileChange events should only fire for these tools to stash user edits.
fn is_file_modifying_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}

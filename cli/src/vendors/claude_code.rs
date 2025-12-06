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
            agent_type: AgentType::Claude,
            session_id: Some(payload.session_id),
            cwd: PathBuf::from(&payload.cwd),
            timestamp: chrono::Utc::now(),
        }),
        "PreToolUse" => {
            // Fire PreFileChange only for file-modifying tools
            if is_file_modifying_tool(&payload.tool_name) {
                AikiEvent::PreFileChange(AikiPreFileChangeEvent {
                    agent_type: AgentType::Claude,
                    session_id: payload.session_id,
                    cwd: PathBuf::from(&payload.cwd),
                    timestamp: chrono::Utc::now(),
                })
            } else {
                // Non-file tools (Bash, Read, etc.) - no PreFileChange needed
                if std::env::var("AIKI_DEBUG").is_ok() {
                    eprintln!(
                        "[aiki] PreToolUse: Ignoring non-file tool: {}",
                        payload.tool_name
                    );
                }
                // Return success without dispatching event
                let response = HookResponse::success();
                let (json_output, exit_code) = translate_response(response, event_name);
                if let Some(json) = json_output {
                    println!("{}", json);
                }
                std::process::exit(exit_code);
            }
        }
        "PostToolUse" => {
            // Extract required fields for PostFileChange event
            let tool_input = payload
                .tool_input
                .ok_or_else(|| anyhow::anyhow!("PostToolUse requires tool_input"))?;

            // Extract edit details from tool_input for user edit detection
            let edit_details =
                if !tool_input.old_string.is_empty() || !tool_input.new_string.is_empty() {
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
/// Claude Code expects different JSON structures depending on the event type.
/// This function dispatches to event-specific translators that handle the details.
fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
    // Claude Code hooks always return exit 0
    let json_output = match event_type {
        "SessionStart" => translate_session_start(&response),
        "UserPromptSubmit" => translate_user_prompt_submit(&response),
        "PreToolUse" => translate_pre_tool_use(&response),
        "PostToolUse" | "PostFileChange" => translate_post_tool_use(&response),
        "Stop" => translate_stop(&response),
        _ => {
            eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
            return (None, 0);
        }
    };

    (json_output, 0)
}

/// Combine formatted messages and context according to Phase 8 architecture
///
/// Returns Some(combined_string) if either messages or context are non-empty,
/// None if both are empty.
fn combine_messages_and_context(response: &HookResponse) -> Option<String> {
    let formatted_messages = crate::handlers::format_messages(response);
    let context = response.context.as_deref().unwrap_or("");

    match (!formatted_messages.is_empty(), !context.is_empty()) {
        (true, true) => Some(format!("{}\n\n{}", formatted_messages, context)),
        (true, false) => Some(formatted_messages),
        (false, true) => Some(context.to_string()),
        (false, false) => None,
    }
}

/// Translate SessionStart event to Claude Code JSON format
fn translate_session_start(response: &HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(response);

    let json = if let Some(ctx) = combined {
        json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": ctx
            }
        })
    } else {
        json!({})
    };

    serde_json::to_string(&json).ok()
}

/// Translate UserPromptSubmit event to Claude Code JSON format
fn translate_user_prompt_submit(response: &HookResponse) -> Option<String> {
    if response.exit_code == 2 {
        // Block the prompt
        let reason = crate::handlers::format_messages(response);
        let json = json!({
            "decision": "block",
            "reason": reason
        });
        serde_json::to_string(&json).ok()
    } else {
        // Allow with optional context
        let combined = combine_messages_and_context(response);
        let json = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        serde_json::to_string(&json).ok()
    }
}

/// Translate PreToolUse event to Claude Code JSON format
fn translate_pre_tool_use(response: &HookResponse) -> Option<String> {
    let formatted_messages = crate::handlers::format_messages(response);

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

    let mut output = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": permission_decision
        }
    });

    // Add reason if present
    if let Some(reason_text) = reason {
        output["hookSpecificOutput"]["permissionDecisionReason"] = json!(reason_text);
    }

    serde_json::to_string(&output).ok()
}

/// Translate PostToolUse event to Claude Code JSON format
fn translate_post_tool_use(response: &HookResponse) -> Option<String> {
    if response.exit_code == 2 {
        // Block (autoreply with reason)
        let reason = crate::handlers::format_messages(response);
        let reason_text = if !reason.is_empty() {
            reason
        } else {
            "Tool execution requires attention".to_string()
        };

        let mut json_obj = json!({
            "decision": "block",
            "reason": reason_text
        });

        // Add optional context
        if let Some(ref ctx) = response.context {
            json_obj["hookSpecificOutput"] = json!({
                "hookEventName": "PostToolUse",
                "additionalContext": ctx
            });
        }

        serde_json::to_string(&json_obj).ok()
    } else {
        // Allow with optional context
        let combined = combine_messages_and_context(response);
        let json = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        serde_json::to_string(&json).ok()
    }
}

/// Translate Stop event to Claude Code JSON format
fn translate_stop(response: &HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(response);

    let json = if let Some(reason_text) = combined {
        // Block (autoreply/force continuation)
        json!({
            "decision": "block",
            "reason": reason_text
        })
    } else {
        // Allow normal stop
        json!({})
    };

    serde_json::to_string(&json).ok()
}

/// Check if a tool modifies files
///
/// Returns true for tools that create, modify, or delete files.
/// PreFileChange events should only fire for these tools to stash user edits.
fn is_file_modifying_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}

use serde_json::json;

use crate::commands::hooks::HookCommandOutput;
use crate::events::result::HookResult;

/// Build HookCommandOutput from HookResult for Claude Code
///
/// Claude Code expects different JSON structures depending on the event type.
/// This function dispatches to event-specific builders that handle the details.
pub fn build_command_output(response: HookResult, event_type: &str) -> HookCommandOutput {
    match event_type {
        "SessionStart" => build_session_start_output(&response),
        "UserPromptSubmit" => build_user_prompt_submit_output(&response),
        "PreToolUse" => build_pre_tool_use_output(&response),
        "PostToolUse" | "PostFileChange" => build_post_tool_use_output(&response),
        "Stop" => build_stop_output(&response),
        _ => {
            eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
            HookCommandOutput::new(None, 0)
        }
    }
}

/// Build SessionStart command output for Claude Code
fn build_session_start_output(response: &HookResult) -> HookCommandOutput {
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

    HookCommandOutput::new(Some(json_value), 0)
}

/// Build UserPromptSubmit command output for Claude Code
fn build_user_prompt_submit_output(response: &HookResult) -> HookCommandOutput {
    if response.decision.is_block() {
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

        HookCommandOutput::new(Some(json_value), 0)
    } else {
        // Allow with optional modified prompt
        // The context field contains the modified prompt text from the flow
        let mut json_value = json!({
            "decision": "continue"
        });

        // If context exists, use it as the modified prompt
        if let Some(ref modified_prompt) = response.context {
            json_value["modifiedPrompt"] = json!(modified_prompt);
        }

        HookCommandOutput::new(Some(json_value), 0)
    }
}

/// Build PreToolUse command output for Claude Code
fn build_pre_tool_use_output(response: &HookResult) -> HookCommandOutput {
    let formatted_messages = response.format_messages();

    // Determine permission decision from response
    // For now, default to "allow" unless blocked
    let (permission_decision, reason) = if response.decision.is_block() {
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

    HookCommandOutput::new(Some(json_value), 0)
}

/// Build PostToolUse command output for Claude Code
fn build_post_tool_use_output(response: &HookResult) -> HookCommandOutput {
    if response.decision.is_block() {
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

        HookCommandOutput::new(Some(json_value), 0)
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
        HookCommandOutput::new(Some(json_value), 0)
    }
}

/// Build Stop command output for Claude Code
fn build_stop_output(response: &HookResult) -> HookCommandOutput {
    // The context field contains the autoreply text from the flow
    let json_value = if let Some(ref autoreply_text) = response.context {
        // Force continuation with autoreply via additionalContext
        json!({
            "decision": "continue",
            "additionalContext": autoreply_text
        })
    } else {
        // No autoreply - allow normal stop
        json!({
            "decision": "stop"
        })
    };

    HookCommandOutput::new(Some(json_value), 0)
}

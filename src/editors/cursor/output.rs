use serde_json::json;

use crate::editors::HookCommandOutput;
use crate::events::result::HookResult;

/// Build HookCommandOutput from HookResult for Cursor
///
/// Cursor expects different JSON structures depending on the event type.
/// This function dispatches to event-specific builders that handle the details.
pub fn build_command_output(response: HookResult, event_type: &str) -> HookCommandOutput {
    match event_type {
        // User interaction
        "beforeSubmitPrompt" => {
            // Note: beforeSubmitPrompt serves dual purpose - SessionStart + PrePrompt
            // For now, treat it as SessionStart/PrePrompt (both have same format)
            build_before_submit_prompt_output(&response)
        }
        "stop" => build_post_response_output(&response),
        // Before hooks (gateable)
        "beforeMCPExecution" | "beforeShellExecution" => build_pre_tool_output(&response),
        // After hooks (notification-only, no response accepted)
        "afterFileEdit" | "afterShellExecution" | "afterMCPExecution" => {
            build_after_hook_output(&response)
        }
        _ => {
            eprintln!("Warning: Unknown Cursor event type: {}", event_type);
            HookCommandOutput::new(None, 0)
        }
    }
}

/// Build beforeSubmitPrompt command output for Cursor
///
/// Maps PrePrompt event responses to Cursor's beforeSubmitPrompt format.
///
/// LIMITATION: Cursor's beforeSubmitPrompt can only BLOCK or ALLOW prompts.
/// It does NOT support modifying the prompt text (no modifiedPrompt field).
/// If the flow returns a modified_prompt in context, it will be IGNORED.
///
/// Supported use cases:
/// - Validation workflows (block prompts that don't meet requirements)
/// - Enforcement (require certain conditions before agent runs)
/// - Warnings (show messages to user based on prompt analysis)
///
/// NOT supported:
/// - Context injection (prepending/appending content to prompts)
/// - Prompt rewriting
fn build_before_submit_prompt_output(response: &HookResult) -> HookCommandOutput {
    // Blocking - combine messages and context for user
    if response.decision.is_block() {
        let combined = response.combined_output();
        let user_message = combined.unwrap_or_default();

        return HookCommandOutput::new(
            Some(json!({
                "continue": false,
                "user_message": user_message
            })),
            2,
        );
    }

    // Success - allow prompt to continue
    // Note: Cursor doesn't accept additional fields on success
    // Note: Any modified_prompt in response.context is IGNORED (not supported by Cursor)
    HookCommandOutput::new(
        Some(json!({
            "continue": true
        })),
        0,
    )
}

/// Build beforeMCPExecution/beforeShellExecution command output for Cursor
fn build_pre_tool_output(response: &HookResult) -> HookCommandOutput {
    // Blocking - prevent tool execution (combine messages and context)
    if response.decision.is_block() {
        let combined = response.combined_output();
        let agent_message = combined.unwrap_or_default();

        return HookCommandOutput::new(
            Some(json!({
                "continue": false,
                "agent_message": agent_message
            })),
            2,
        );
    }

    // Success - allow tool execution
    // Note: Cursor doesn't accept additional fields on success
    HookCommandOutput::new(
        Some(json!({
            "continue": true
        })),
        0,
    )
}

/// Build after-hook command output for Cursor
///
/// Cursor's after-hooks (afterFileEdit, afterShellExecution, afterMCPExecution)
/// are notification-only and do NOT accept JSON responses.
fn build_after_hook_output(_response: &HookResult) -> HookCommandOutput {
    // Cursor doesn't accept responses from after-hooks
    // Return no JSON, always exit 0
    HookCommandOutput::new(None, 0)
}

/// Build stop command output for Cursor
///
/// Combines messages and context into followup_message for the agent.
fn build_post_response_output(response: &HookResult) -> HookCommandOutput {
    // Combine messages + context for followup_message
    let combined = response.combined_output();

    if let Some(followup_text) = combined {
        return HookCommandOutput::new(
            Some(json!({
                "followup_message": followup_text
            })),
            0,
        );
    }

    // No followup - return empty object
    HookCommandOutput::new(Some(json!({})), 0)
}

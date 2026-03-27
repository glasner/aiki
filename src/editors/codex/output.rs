use serde_json::json;

use crate::editors::HookCommandOutput;
use crate::events::result::HookResult;

/// Build HookCommandOutput from HookResult for Codex
///
/// Codex has a simpler hook protocol than Claude Code:
/// - SessionStart: plain text stdout for additionalContext
/// - PreToolUse: only deny is emitted; allow is a no-op
/// - Stop: uses `decision: "block"` to continue the session
/// - No PostToolUse or SessionEnd events
pub fn build_command_output(response: HookResult, event_type: &str) -> HookCommandOutput {
    match event_type {
        "SessionStart" => build_session_start_output(&response),
        "UserPromptSubmit" => build_user_prompt_submit_output(&response),
        "PreToolUse" => build_pre_tool_use_output(&response),
        "Stop" => build_stop_output(&response),
        _ => {
            eprintln!("Warning: Unknown Codex event type: {}", event_type);
            HookCommandOutput::new(None, 0)
        }
    }
}

/// Build SessionStart command output for Codex
///
/// Codex treats plain text stdout as additionalContext, so we emit the
/// combined output as plain text rather than wrapping it in JSON.
fn build_session_start_output(response: &HookResult) -> HookCommandOutput {
    let combined = response.combined_output();

    if let Some(ctx) = combined {
        // Has context - emit as plain text on stdout
        // Codex interprets plain text stdout as additionalContext
        let json_value = json!(ctx);
        // Use raw string output instead of JSON object
        HookCommandOutput {
            json_value: Some(json_value),
            exit_code: 0,
        }
    } else {
        // No context - empty stdout, exit 0
        HookCommandOutput::new(None, 0)
    }
}

/// Build UserPromptSubmit command output for Codex
///
/// - Block: JSON `{ "decision": "block", "reason": "..." }` or exit code 2 with stderr
/// - Allow with context: plain text stdout (treated as additionalContext)
/// - Allow without context: empty stdout
fn build_user_prompt_submit_output(response: &HookResult) -> HookCommandOutput {
    if response.decision.is_block() {
        let reason = response.format_messages();
        let json_value = json!({
            "decision": "block",
            "reason": reason
        });
        HookCommandOutput::new(Some(json_value), 0)
    } else {
        // Allow - emit plain text context if present
        let combined = response.combined_output();
        if let Some(ctx) = combined {
            HookCommandOutput {
                json_value: Some(json!(ctx)),
                exit_code: 0,
            }
        } else {
            HookCommandOutput::new(None, 0)
        }
    }
}

/// Build PreToolUse command output for Codex
///
/// KEY DIFFERENCE from Claude Code: allow is a no-op (empty stdout).
/// Only deny emits JSON. Never emit "approve" or additionalContext
/// as these cause failures in Codex.
fn build_pre_tool_use_output(response: &HookResult) -> HookCommandOutput {
    if response.decision.is_block() {
        let reason = response.format_messages();
        let mut json_value = json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny"
            }
        });

        if !reason.is_empty() {
            json_value["hookSpecificOutput"]["permissionDecisionReason"] = json!(reason);
        }

        HookCommandOutput::new(Some(json_value), 0)
    } else {
        // Allow is a no-op - empty stdout, exit 0
        HookCommandOutput::new(None, 0)
    }
}

/// Build Stop command output for Codex
///
/// - Continue/block: `{ "decision": "block", "reason": "..." }` with the reason
///   serving as a continuation prompt
/// - Allow stop: empty stdout
/// - No additionalContext support for Stop
fn build_stop_output(response: &HookResult) -> HookCommandOutput {
    if response.context.is_some() {
        // Block the stop to continue the session
        let reason = response.context.as_deref().unwrap_or("Continue session");
        let json_value = json!({
            "decision": "block",
            "reason": reason
        });
        HookCommandOutput::new(Some(json_value), 0)
    } else {
        // Allow normal stop - empty stdout
        HookCommandOutput::new(None, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::result::{Decision, Failure};

    fn make_allow_response() -> HookResult {
        HookResult {
            context: None,
            decision: Decision::Allow,
            failures: Vec::new(),
        }
    }

    fn make_allow_with_context(ctx: &str) -> HookResult {
        HookResult {
            context: Some(ctx.to_string()),
            decision: Decision::Allow,
            failures: Vec::new(),
        }
    }

    fn make_block_response(reason: &str) -> HookResult {
        HookResult {
            context: None,
            decision: Decision::Block,
            failures: vec![Failure(reason.to_string())],
        }
    }

    // SessionStart output tests

    #[test]
    fn test_session_start_with_context_emits_plain_text() {
        let response = make_allow_with_context("WORKSPACE ISOLATION: /tmp/test");
        let output = build_command_output(response, "SessionStart");
        // Should emit context as a JSON string value (plain text for Codex)
        assert_eq!(output.exit_code, 0);
        let value = output.json_value.unwrap();
        assert_eq!(value.as_str().unwrap(), "WORKSPACE ISOLATION: /tmp/test");
    }

    #[test]
    fn test_session_start_without_context_emits_nothing() {
        let response = make_allow_response();
        let output = build_command_output(response, "SessionStart");
        assert_eq!(output.exit_code, 0);
        assert!(output.json_value.is_none());
    }

    // PreToolUse output tests

    #[test]
    fn test_pre_tool_use_allow_is_noop() {
        let response = make_allow_response();
        let output = build_command_output(response, "PreToolUse");
        assert_eq!(output.exit_code, 0);
        assert!(
            output.json_value.is_none(),
            "Allow should produce no output"
        );
    }

    #[test]
    fn test_pre_tool_use_deny_emits_json() {
        let response = make_block_response("Not allowed");
        let output = build_command_output(response, "PreToolUse");
        assert_eq!(output.exit_code, 0);
        let value = output.json_value.unwrap();
        let decision = value["hookSpecificOutput"]["permissionDecision"]
            .as_str()
            .unwrap();
        assert_eq!(decision, "deny");
        assert!(value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("Not allowed"));
    }

    #[test]
    fn test_pre_tool_use_deny_never_emits_approve() {
        let response = make_block_response("Blocked");
        let output = build_command_output(response, "PreToolUse");
        let value = output.json_value.unwrap();
        let json_str = serde_json::to_string(&value).unwrap();
        assert!(
            !json_str.contains("approve"),
            "Must never emit 'approve' for Codex PreToolUse"
        );
    }

    // Stop output tests

    #[test]
    fn test_stop_allow_emits_nothing() {
        let response = make_allow_response();
        let output = build_command_output(response, "Stop");
        assert_eq!(output.exit_code, 0);
        assert!(output.json_value.is_none());
    }

    #[test]
    fn test_stop_block_emits_decision_block() {
        let response = make_allow_with_context("Continue working on the task");
        let output = build_command_output(response, "Stop");
        assert_eq!(output.exit_code, 0);
        let value = output.json_value.unwrap();
        assert_eq!(value["decision"].as_str().unwrap(), "block");
        assert_eq!(
            value["reason"].as_str().unwrap(),
            "Continue working on the task"
        );
    }

    // UserPromptSubmit output tests

    #[test]
    fn test_user_prompt_submit_allow_no_context() {
        let response = make_allow_response();
        let output = build_command_output(response, "UserPromptSubmit");
        assert_eq!(output.exit_code, 0);
        assert!(output.json_value.is_none());
    }

    #[test]
    fn test_user_prompt_submit_allow_with_context() {
        let response = make_allow_with_context("Task context: working on xyz");
        let output = build_command_output(response, "UserPromptSubmit");
        assert_eq!(output.exit_code, 0);
        let value = output.json_value.unwrap();
        assert_eq!(value.as_str().unwrap(), "Task context: working on xyz");
    }

    #[test]
    fn test_user_prompt_submit_block() {
        let response = make_block_response("Task is blocked");
        let output = build_command_output(response, "UserPromptSubmit");
        assert_eq!(output.exit_code, 0);
        let value = output.json_value.unwrap();
        assert_eq!(value["decision"].as_str().unwrap(), "block");
        assert!(value["reason"]
            .as_str()
            .unwrap()
            .contains("Task is blocked"));
    }

    // Unknown event test

    #[test]
    fn test_unknown_event_emits_nothing() {
        let response = make_allow_response();
        let output = build_command_output(response, "UnknownEvent");
        assert_eq!(output.exit_code, 0);
        assert!(output.json_value.is_none());
    }
}

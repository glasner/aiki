use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::cache::debug_log;
use crate::commands::hooks::HookCommandOutput;
use crate::event_bus;
use crate::events::result::HookResult;
use crate::events::{
    AikiEvent, AikiPostFileChangePayload, AikiPostResponsePayload, AikiPreFileChangePayload,
    AikiPrePromptPayload, AikiSessionStartPayload,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

/// Get agent version from cache or detect it
///
/// For SessionStart events, detects version and caches it in session file.
/// For other events, reads cached version from session file (fast).
/// Falls back to detection if cache read fails.
fn get_agent_version(payload: &ClaudeCodePayload, repo_path: &Path) -> Option<String> {
    // Compute session file path directly without creating full session object
    let session_uuid = AikiSession::generate_uuid(AgentType::Claude, &payload.session_id);
    let session_file_path = repo_path.join(".aiki/sessions").join(&session_uuid);

    // Try to read cached version from session file
    if let Some(cached_version) = read_agent_version_from_file(&session_file_path) {
        return Some(cached_version);
    }

    // No cache - detect version (this happens on SessionStart or if file missing)
    crate::npm::get_version("@anthropic-ai/claude-code", "claude")
}

/// Read agent_version from session file
fn read_agent_version_from_file(path: &Path) -> Option<String> {
    use std::fs;
    fs::read_to_string(path).ok().and_then(|content| {
        content
            .lines()
            .find(|line| line.starts_with("agent_version="))
            .and_then(|line| line.strip_prefix("agent_version="))
            .map(|v| v.to_string())
    })
}

/// Create a session for Claude Code events
///
/// This helper ensures consistent session creation across all Claude Code event builders.
/// Takes the full payload to allow easy extension if we need additional fields in the future.
/// For SessionStart, detects version (~135ms) and caches in session file.
/// For other events, reads cached version from file (~0ms).
fn create_session(payload: &ClaudeCodePayload) -> AikiSession {
    // Get repo path from cwd
    let repo_path = Path::new(&payload.cwd);
    let agent_version = get_agent_version(payload, repo_path);

    AikiSession::new(
        AgentType::Claude,
        &payload.session_id,
        agent_version,
        DetectionMethod::Hook,
    )
}

/// Claude Code hook payload structure
///
/// This matches the JSON that Claude Code sends to various hooks.
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
    /// User prompt text (for UserPromptSubmit hook)
    #[serde(default)]
    prompt: String,
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
/// Dispatches to event-specific handlers based on event name.
///
/// # Arguments
/// * `claude_event_name` - Vendor event name from CLI flag (e.g., "SessionStart", "PostToolUse")
pub fn handle(claude_event_name: &str) -> Result<()> {
    // Read Claude Code-specific JSON from stdin
    let payload: ClaudeCodePayload = super::read_stdin_json()?;

    // Validate event name matches JSON (optional but good practice)
    if payload.hook_event_name != claude_event_name {
        debug_log(|| {
            format!(
                "Warning: Event name mismatch. CLI: {}, JSON: {}",
                claude_event_name, payload.hook_event_name
            )
        });
    }

    // Build event from payload
    let aiki_event = build_aiki_event(payload, claude_event_name);

    // Dispatch event and exit with command output
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let hook_output = build_command_output(aiki_response, claude_event_name);

    hook_output.print_and_exit();
}

/// Build AikiEvent from Claude Code payload
fn build_aiki_event(payload: ClaudeCodePayload, claude_event_name: &str) -> AikiEvent {
    match claude_event_name {
        "SessionStart" => build_session_start_event(payload),
        "UserPromptSubmit" => build_pre_prompt_event(payload),
        "PreToolUse" => build_pre_file_change_event(payload),
        "PostToolUse" => build_post_file_change_event(payload),
        "Stop" => build_post_response_event(payload),
        _ => AikiEvent::Unsupported,
    }
}

/// Build SessionStart event from SessionStart payload
fn build_session_start_event(payload: ClaudeCodePayload) -> AikiEvent {
    AikiEvent::SessionStart(AikiSessionStartPayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PrePrompt event from UserPromptSubmit payload
fn build_pre_prompt_event(payload: ClaudeCodePayload) -> AikiEvent {
    AikiEvent::PrePrompt(AikiPrePromptPayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        prompt: payload.prompt,
    })
}

/// Build PreFileChange event from PreToolUse payload
fn build_pre_file_change_event(payload: ClaudeCodePayload) -> AikiEvent {
    // Fire PreFileChange only for file-modifying tools
    if !is_file_modifying_tool(&payload.tool_name) {
        debug_log(|| format!("PreToolUse: Ignoring non-file tool: {}", payload.tool_name));
        return AikiEvent::Unsupported;
    }

    AikiEvent::PreFileChange(AikiPreFileChangePayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PostFileChange event from PostToolUse payload
fn build_post_file_change_event(payload: ClaudeCodePayload) -> AikiEvent {
    // Create session first before moving any fields
    let session = create_session(&payload);

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

    AikiEvent::PostFileChange(AikiPostFileChangePayload {
        session,
        tool_name: payload.tool_name,
        file_paths: vec![tool_input.file_path],
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        edit_details,
    })
}

/// Build PostResponse event from Stop payload
fn build_post_response_event(payload: ClaudeCodePayload) -> AikiEvent {
    // Note: Claude Code's Stop hook doesn't include the response text in the payload.
    // This is intentional - flows use self.* functions to check files/run tests
    // rather than parsing the response text.
    AikiEvent::PostResponse(AikiPostResponsePayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        response: String::new(), // Empty - flows check files/run tests via self.* functions
        modified_files: vec![],  // Could track from PostToolUse events if needed
    })
}

/// Build HookCommandOutput from HookResult for Claude Code
///
/// Claude Code expects different JSON structures depending on the event type.
/// This function dispatches to event-specific builders that handle the details.
fn build_command_output(response: HookResult, event_type: &str) -> HookCommandOutput {
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

/// Check if a tool modifies files
///
/// Returns true for tools that create, modify, or delete files.
/// PreFileChange events should only fire for these tools to stash user edits.
fn is_file_modifying_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_includes_version() {
        // Create a test payload
        let payload = ClaudeCodePayload {
            session_id: "test-session-123".to_string(),
            transcript_path: "/tmp/transcript.json".to_string(),
            cwd: "/tmp".to_string(),
            hook_event_name: "SessionStart".to_string(),
            tool_name: String::new(),
            tool_input: None,
            tool_output: String::new(),
            prompt: String::new(),
        };

        let session = create_session(&payload);

        // Verify session was created
        assert_eq!(session.agent_type(), AgentType::Claude);
        assert_eq!(session.external_id(), "test-session-123");
        assert_eq!(session.detection_method(), &DetectionMethod::Hook);

        // Check if version was detected (may be None if claude not in PATH)
        if let Some(version) = session.agent_version() {
            println!("Session created with Claude Code version: {}", version);
            assert!(!version.is_empty());
        } else {
            println!("Session created without version (claude not in PATH)");
        }
    }
}

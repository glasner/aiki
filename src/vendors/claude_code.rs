use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::event_bus;
use crate::events::result::{Decision, HookResult};
use crate::events::{
    AikiEvent, AikiPostFileChangeEvent, AikiPostResponseEvent, AikiPreFileChangeEvent,
    AikiPrePromptEvent, AikiStartEvent,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

/// Detect Claude Code version by running `claude --version`
///
/// Parses output like "2.0.61 (Claude Code)" and returns "2.0.61".
/// Returns None if detection fails (command not found, parse error, etc.)
fn detect_claude_version() -> Option<String> {
    use std::process::Command;

    Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .and_then(|s| {
            // Parse "2.0.61 (Claude Code)" -> "2.0.61"
            s.split_whitespace().next().map(|v| v.to_string())
        })
}

/// Get agent version from cache or detect it
///
/// For SessionStart events, detects version and caches it in session file.
/// For other events, reads cached version from session file (fast).
/// Falls back to detection if cache read fails.
fn get_agent_version(payload: &ClaudeCodePayload, repo_path: &Path) -> Option<String> {
    // Compute session file path directly without creating full session object
    // This is faster than creating a temporary session (avoids UUID generation)
    let session_uuid = compute_session_uuid(AgentType::Claude, &payload.session_id);
    let session_file_path = repo_path.join(".aiki/sessions").join(&session_uuid);

    // Try to read cached version from session file
    if let Some(cached_version) = read_agent_version_from_file(&session_file_path) {
        return Some(cached_version);
    }

    // No cache - detect version (this happens on SessionStart or if file missing)
    detect_claude_version()
}

/// Compute session UUID without creating full AikiSession object
fn compute_session_uuid(agent_type: AgentType, external_id: &str) -> String {
    const NAMESPACE: uuid::Uuid = uuid::Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ]);
    let hash_input = format!("{}:{}", agent_type.to_metadata_string(), external_id);
    uuid::Uuid::new_v5(&NAMESPACE, hash_input.as_bytes()).to_string()
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
/// Panics on failure since session creation errors are unrecoverable in the hook context.
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
    .expect("Failed to create AikiSession for Claude Code")
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
        "UserPromptSubmit" => build_pre_prompt_event(payload),
        "PreToolUse" => build_pre_file_change_event(payload),
        "PostToolUse" => build_post_file_change_event(payload),
        "Stop" => build_post_response_event(payload),
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
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PrePrompt event from UserPromptSubmit payload
fn build_pre_prompt_event(payload: ClaudeCodePayload) -> AikiEvent {
    AikiEvent::PrePrompt(AikiPrePromptEvent {
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
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[aiki] PreToolUse: Ignoring non-file tool: {}",
                payload.tool_name
            );
        }
        return AikiEvent::Unsupported;
    }

    AikiEvent::PreFileChange(AikiPreFileChangeEvent {
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

    AikiEvent::PostFileChange(AikiPostFileChangeEvent {
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
    AikiEvent::PostResponse(AikiPostResponseEvent {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        response: String::new(), // Empty - flows check files/run tests via self.* functions
        modified_files: vec![],  // Could track from PostToolUse events if needed
    })
}

/// Translate HookResult to Claude Code JSON format
///
/// Claude Code expects different JSON structures depending on the event type.
/// This function dispatches to event-specific translators that handle the details.
fn translate_response(response: HookResult, event_type: &str) -> ClaudeCodeResponse {
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
fn translate_session_start(response: &HookResult) -> ClaudeCodeResponse {
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
fn translate_user_prompt_submit(response: &HookResult) -> ClaudeCodeResponse {
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

        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
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

        ClaudeCodeResponse {
            json_value: Some(json_value),
            exit_code: 0,
        }
    }
}

/// Translate PreToolUse event to Claude Code JSON format
fn translate_pre_tool_use(response: &HookResult) -> ClaudeCodeResponse {
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

    ClaudeCodeResponse {
        json_value: Some(json_value),
        exit_code: 0,
    }
}

/// Translate PostToolUse event to Claude Code JSON format
fn translate_post_tool_use(response: &HookResult) -> ClaudeCodeResponse {
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
fn translate_stop(response: &HookResult) -> ClaudeCodeResponse {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_claude_version() {
        // This test verifies that detect_claude_version() works
        // It may return None if `claude` is not in PATH, which is fine
        let version = detect_claude_version();

        if let Some(v) = version {
            // If we got a version, verify it's a reasonable format
            assert!(!v.is_empty(), "Version should not be empty");
            assert!(
                v.chars().next().unwrap().is_ascii_digit(),
                "Version should start with a digit"
            );
            println!("Detected Claude Code version: {}", v);
        } else {
            // If no version detected, that's okay (claude might not be in PATH)
            println!("Claude Code not detected (not in PATH or command failed)");
        }
    }

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

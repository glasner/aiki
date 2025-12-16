use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::cache::debug_log;
use crate::commands::hooks::HookCommandOutput;
use crate::event_bus;
use crate::events::result::HookResult;
use crate::events::{
    AikiEvent, AikiFileCompletedPayload, AikiFilePermissionAskedPayload, AikiMcpCompletedPayload,
    AikiMcpPermissionAskedPayload, AikiPromptSubmittedPayload, AikiResponseReceivedPayload,
    AikiSessionStartPayload, AikiShellCompletedPayload, AikiShellPermissionAskedPayload,
    FileOperation,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

/// Get agent version from cache or detect it
///
/// For SessionStart events, detects version and caches it in session file.
/// For other events, reads cached version from session file (fast).
/// Falls back to detection if cache read fails.
fn get_agent_version(session_id: &str, repo_path: &Path) -> Option<String> {
    // Compute session file path directly without creating full session object
    let session_uuid = AikiSession::generate_uuid(AgentType::Claude, session_id);
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
/// For SessionStart, detects version (~135ms) and caches in session file.
/// For other events, reads cached version from file (~0ms).
fn create_session(session_id: &str, cwd: &str) -> AikiSession {
    let repo_path = Path::new(cwd);
    let agent_version = get_agent_version(session_id, repo_path);

    AikiSession::new(
        AgentType::Claude,
        session_id,
        agent_version,
        DetectionMethod::Hook,
    )
}

// ============================================================================
// Hook Payload Structures (matches Claude Code API)
// See: https://code.claude.com/docs/en/hooks
// ============================================================================

/// Claude Code hook event - discriminated by hook_event_name
#[derive(Deserialize, Debug)]
#[serde(tag = "hook_event_name")]
enum ClaudeEvent {
    #[serde(rename = "SessionStart")]
    SessionStart {
        #[serde(flatten)]
        payload: ClaudeSessionStartPayload,
    },
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit {
        #[serde(flatten)]
        payload: ClaudeUserPromptSubmitPayload,
    },
    #[serde(rename = "PreToolUse")]
    PreToolUse {
        #[serde(flatten)]
        payload: ClaudePreToolUsePayload,
    },
    #[serde(rename = "PostToolUse")]
    PostToolUse {
        #[serde(flatten)]
        payload: ClaudePostToolUsePayload,
    },
    #[serde(rename = "Stop")]
    Stop {
        #[serde(flatten)]
        payload: ClaudeStopPayload,
    },
}

/// SessionStart hook payload
#[derive(Deserialize, Debug)]
struct ClaudeSessionStartPayload {
    session_id: String,
    cwd: String,
}

/// UserPromptSubmit hook payload
#[derive(Deserialize, Debug)]
struct ClaudeUserPromptSubmitPayload {
    session_id: String,
    cwd: String,
    #[serde(default)]
    prompt: String,
}

/// PreToolUse hook payload
#[derive(Deserialize, Debug)]
struct ClaudePreToolUsePayload {
    session_id: String,
    cwd: String,
    tool_name: String,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
}

/// PostToolUse hook payload
#[derive(Deserialize, Debug)]
struct ClaudePostToolUsePayload {
    session_id: String,
    cwd: String,
    tool_name: String,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[serde(default)]
    tool_response: Option<serde_json::Value>,
}

/// Stop hook payload
#[derive(Deserialize, Debug)]
struct ClaudeStopPayload {
    session_id: String,
    cwd: String,
}

// ============================================================================
// Tool Input Structures
// ============================================================================

/// Tool input for file operations (Edit, Write, NotebookEdit)
/// Unified struct that handles all file-modifying tools.
/// See: https://code.claude.com/docs/en/hooks#posttooluse-input
#[derive(Deserialize, Debug)]
struct FileToolInput {
    file_path: String,
    /// Old string to replace (Edit tool)
    #[serde(default)]
    old_string: String,
    /// New string to insert (Edit tool)
    #[serde(default)]
    new_string: String,
    /// File content (Write tool)
    #[serde(default)]
    content: String,
}

/// Tool input for Bash tool
#[derive(Deserialize, Debug)]
struct BashToolInput {
    #[serde(default)]
    command: String,
}

// ============================================================================
// Tool Response Structures (PostToolUse)
// ============================================================================

/// Response structure for Bash tool - includes exit code!
/// This is critical for flows that need to react to command failures.
/// See: https://code.claude.com/docs/en/hooks#posttooluse-input
#[derive(Deserialize, Debug)]
struct BashToolResponse {
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(rename = "exitCode", default)]
    exit_code: i32,
}

/// Tool type classification for event routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolType {
    /// File-modifying tools (Edit, Write, NotebookEdit)
    FileChange,
    /// Shell command execution (Bash)
    Shell,
    /// Read-only tools (Read, Glob, Grep) - no event needed
    ReadOnly,
    /// MCP server tools (anything else)
    Mcp,
}

/// Classify a tool by name into its type
fn classify_tool(tool_name: &str) -> ToolType {
    match tool_name {
        // File-modifying tools
        "Edit" | "Write" | "NotebookEdit" => ToolType::FileChange,
        // Shell command execution
        "Bash" => ToolType::Shell,
        // Read-only tools - no event needed
        "Read" | "Glob" | "Grep" | "LS" | "Task" | "TodoRead" | "WebFetch" | "WebSearch" => {
            ToolType::ReadOnly
        }
        // Everything else is treated as MCP tool
        _ => ToolType::Mcp,
    }
}

/// Handle a Claude Code event
///
/// This is the vendor-specific handler for Claude Code hooks.
/// Parses the payload once and dispatches to event-specific handlers.
///
/// # Arguments
/// * `claude_event_name` - Vendor event name from CLI flag (used for output formatting)
pub fn handle(claude_event_name: &str) -> Result<()> {
    // Parse event - serde discriminates by hook_event_name
    let claude_event: ClaudeEvent = super::read_stdin_json()?;

    // Build Aiki event from Claude event
    let aiki_event = build_aiki_event(claude_event);

    // Dispatch event and exit with command output
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let hook_output = build_command_output(aiki_response, claude_event_name);

    hook_output.print_and_exit();
}

/// Build AikiEvent from Claude Code event
fn build_aiki_event(event: ClaudeEvent) -> AikiEvent {
    match event {
        ClaudeEvent::SessionStart { payload } => build_session_started_event(payload),
        ClaudeEvent::UserPromptSubmit { payload } => build_prompt_submitted_event(payload),
        ClaudeEvent::PreToolUse { payload } => build_pre_tool_event(payload),
        ClaudeEvent::PostToolUse { payload } => build_post_tool_event(payload),
        ClaudeEvent::Stop { payload } => build_response_received_event(payload),
    }
}

/// Build appropriate pre-tool event based on tool type
fn build_pre_tool_event(payload: ClaudePreToolUsePayload) -> AikiEvent {
    let tool_type = classify_tool(&payload.tool_name);

    match tool_type {
        ToolType::FileChange => build_file_permission_asked_event(payload, FileOperation::Write),
        ToolType::Shell => build_shell_permission_asked_event(payload),
        ToolType::Mcp => build_mcp_permission_asked_event(payload),
        ToolType::ReadOnly => {
            debug_log(|| format!("PreToolUse: Ignoring read-only tool: {}", payload.tool_name));
            AikiEvent::Unsupported
        }
    }
}

/// Build appropriate post-tool event based on tool type
fn build_post_tool_event(payload: ClaudePostToolUsePayload) -> AikiEvent {
    let tool_type = classify_tool(&payload.tool_name);

    match tool_type {
        ToolType::FileChange => build_file_completed_event(payload, FileOperation::Write),
        ToolType::Shell => build_shell_completed_event(payload),
        ToolType::Mcp => build_mcp_completed_event(payload),
        ToolType::ReadOnly => {
            debug_log(|| {
                format!(
                    "PostToolUse: Ignoring read-only tool: {}",
                    payload.tool_name
                )
            });
            AikiEvent::Unsupported
        }
    }
}

/// Build session.started event
fn build_session_started_event(payload: ClaudeSessionStartPayload) -> AikiEvent {
    AikiEvent::SessionStarted(AikiSessionStartPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build prompt.submitted event
fn build_prompt_submitted_event(payload: ClaudeUserPromptSubmitPayload) -> AikiEvent {
    AikiEvent::PromptSubmitted(AikiPromptSubmittedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        prompt: payload.prompt,
    })
}

/// Build file.permission_asked event (file tools only)
fn build_file_permission_asked_event(
    payload: ClaudePreToolUsePayload,
    operation: FileOperation,
) -> AikiEvent {
    // Extract file path from tool input
    let path = payload
        .tool_input
        .as_ref()
        .and_then(|v| serde_json::from_value::<FileToolInput>(v.clone()).ok())
        .map(|input| input.file_path);

    AikiEvent::FilePermissionAsked(AikiFilePermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation,
        path,
        pattern: None,
    })
}

/// Build shell.permission_asked event (Bash tool)
fn build_shell_permission_asked_event(payload: ClaudePreToolUsePayload) -> AikiEvent {
    let command = payload
        .tool_input
        .as_ref()
        .and_then(|v| serde_json::from_value::<BashToolInput>(v.clone()).ok())
        .map(|input| input.command)
        .unwrap_or_default();

    AikiEvent::ShellPermissionAsked(AikiShellPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command,
    })
}

/// Build mcp.permission_asked event (MCP tools)
fn build_mcp_permission_asked_event(payload: ClaudePreToolUsePayload) -> AikiEvent {
    let parameters = payload.tool_input.unwrap_or(serde_json::Value::Null);

    AikiEvent::McpPermissionAsked(AikiMcpPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        parameters,
    })
}

/// Build file.completed event (file tools only)
fn build_file_completed_event(
    payload: ClaudePostToolUsePayload,
    operation: FileOperation,
) -> AikiEvent {
    let session = create_session(&payload.session_id, &payload.cwd);

    let Some(tool_input_value) = payload.tool_input else {
        eprintln!("[aiki] Warning: PostToolUse missing tool_input, ignoring event");
        return AikiEvent::Unsupported;
    };

    let Ok(file_input) = serde_json::from_value::<FileToolInput>(tool_input_value) else {
        eprintln!("[aiki] Warning: PostToolUse invalid file tool_input, ignoring event");
        return AikiEvent::Unsupported;
    };

    let edit_details = if !file_input.old_string.is_empty() || !file_input.new_string.is_empty() {
        vec![crate::events::EditDetail::new(
            file_input.file_path.clone(),
            file_input.old_string.clone(),
            file_input.new_string.clone(),
        )]
    } else {
        Vec::new()
    };

    AikiEvent::FileCompleted(AikiFileCompletedPayload {
        session,
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation,
        tool_name: payload.tool_name,
        file_paths: vec![file_input.file_path],
        success: Some(true), // PostToolUse implies success
        edit_details,
    })
}

/// Build shell.completed event (Bash tool)
fn build_shell_completed_event(payload: ClaudePostToolUsePayload) -> AikiEvent {
    let command = payload
        .tool_input
        .as_ref()
        .and_then(|v| serde_json::from_value::<BashToolInput>(v.clone()).ok())
        .map(|input| input.command)
        .unwrap_or_default();

    let (exit_code, stdout, stderr) = payload
        .tool_response
        .as_ref()
        .and_then(|v| serde_json::from_value::<BashToolResponse>(v.clone()).ok())
        .map(|resp| (resp.exit_code, resp.stdout, resp.stderr))
        .unwrap_or_else(|| {
            debug_log(|| "Warning: PostToolUse Bash missing tool_response, assuming exit_code=0");
            (0, String::new(), String::new())
        });

    AikiEvent::ShellCompleted(AikiShellCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command,
        exit_code,
        stdout,
        stderr,
    })
}

/// Build mcp.completed event (MCP tools)
fn build_mcp_completed_event(payload: ClaudePostToolUsePayload) -> AikiEvent {
    let result = payload
        .tool_response
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .filter(|s| !s.is_empty() && s != "null");

    AikiEvent::McpCompleted(AikiMcpCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        success: true,
        result,
    })
}

/// Build response.received event
fn build_response_received_event(payload: ClaudeStopPayload) -> AikiEvent {
    AikiEvent::ResponseReceived(AikiResponseReceivedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        response: String::new(),
        modified_files: vec![],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_includes_version() {
        let session = create_session("test-session-123", "/tmp");

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

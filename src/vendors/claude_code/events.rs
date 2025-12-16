use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::cache::debug_log;
use crate::events::FileOperation;
use crate::events::{
    AikiEvent, AikiFileCompletedPayload, AikiFilePermissionAskedPayload, AikiMcpCompletedPayload,
    AikiMcpPermissionAskedPayload, AikiPromptSubmittedPayload, AikiResponseReceivedPayload,
    AikiSessionStartPayload, AikiShellCompletedPayload, AikiShellPermissionAskedPayload,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;
use crate::tools::ToolType;

use super::tools::{BashToolResponse, ClaudeTool};
use super::version::get_agent_version;

// ============================================================================
// Hook Payload Structures (matches Claude Code API)
// See: https://code.claude.com/docs/en/hooks
// ============================================================================

/// Claude Code hook event - discriminated by hook_event_name
#[derive(Deserialize, Debug)]
#[serde(tag = "hook_event_name")]
pub enum ClaudeEvent {
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
pub struct ClaudePreToolUsePayload {
    pub session_id: String,
    pub cwd: String,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
}

/// PostToolUse hook payload
#[derive(Deserialize, Debug)]
pub struct ClaudePostToolUsePayload {
    pub session_id: String,
    pub cwd: String,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_response: Option<serde_json::Value>,
}

/// Stop hook payload
#[derive(Deserialize, Debug)]
struct ClaudeStopPayload {
    session_id: String,
    cwd: String,
}

// ============================================================================
// Session Creation
// ============================================================================

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
// Event Building
// ============================================================================

/// Build AikiEvent from Claude Code event
pub fn build_aiki_event(event: ClaudeEvent) -> AikiEvent {
    match event {
        ClaudeEvent::SessionStart { payload } => build_session_started_event(payload),
        ClaudeEvent::UserPromptSubmit { payload } => build_prompt_submitted_event(payload),
        ClaudeEvent::PreToolUse { payload } => build_permission_asked_event_for_tool_type(payload),
        ClaudeEvent::PostToolUse { payload } => build_completed_event_for_tool_type(payload),
        ClaudeEvent::Stop { payload } => build_response_received_event(payload),
    }
}

/// Build appropriate pre-tool event based on tool type
fn build_permission_asked_event_for_tool_type(payload: ClaudePreToolUsePayload) -> AikiEvent {
    let tool = ClaudeTool::parse(&payload.tool_name, payload.tool_input.as_ref());

    match tool.tool_type() {
        ToolType::File => build_file_permission_asked_event(payload, tool),
        ToolType::Shell => build_shell_permission_asked_event(payload, tool),
        ToolType::Mcp => build_mcp_permission_asked_event(payload),
        ToolType::Web => {
            // Phase 3: Will emit web.permission_asked events
            debug_log(|| format!("PreToolUse: Web tool {} - Phase 3", payload.tool_name));
            AikiEvent::Unsupported
        }
        ToolType::Internal => AikiEvent::Unsupported,
    }
}

/// Build appropriate post-tool event based on tool type
fn build_completed_event_for_tool_type(payload: ClaudePostToolUsePayload) -> AikiEvent {
    let tool = ClaudeTool::parse(&payload.tool_name, payload.tool_input.as_ref());

    match tool.tool_type() {
        ToolType::File => build_file_completed_event(payload, tool),
        ToolType::Shell => build_shell_completed_event(payload, tool),
        ToolType::Mcp => build_mcp_completed_event(payload),
        ToolType::Web => {
            // Phase 3: Will emit web.completed events
            debug_log(|| format!("PostToolUse: Web tool {} - Phase 3", payload.tool_name));
            AikiEvent::Unsupported
        }
        ToolType::Internal => AikiEvent::Unsupported,
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

/// Build file.permission_asked event for all file tools
fn build_file_permission_asked_event(
    payload: ClaudePreToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    // Extra safety check - should never happen due to tool_type() dispatch
    if !matches!(tool.tool_type(), ToolType::File) {
        eprintln!("[aiki] Error: build_file_permission_asked_event called on non-file tool");
        return AikiEvent::Unsupported;
    }

    let operation = tool.file_operation();

    match operation {
        FileOperation::Write => build_file_write_permission_asked_event(payload, tool),
        FileOperation::Read => build_file_read_permission_asked_event(payload, tool),
        FileOperation::Delete => {
            eprintln!("[aiki] Warning: Delete operation not yet supported in PreToolUse");
            AikiEvent::Unsupported
        }
    }
}

/// Build file.permission_asked event for write operations (Edit, Write, NotebookEdit, MultiEdit)
fn build_file_write_permission_asked_event(
    payload: ClaudePreToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    let path = match tool {
        ClaudeTool::Edit(input) | ClaudeTool::Write(input) | ClaudeTool::NotebookEdit(input) => {
            Some(input.file_path)
        }
        ClaudeTool::MultiEdit(input) => {
            // MultiEdit affects multiple files - use first file or None if empty
            input.edits.first().map(|e| e.file_path.clone())
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            None
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in file.write.permission_asked");
            None
        }
    };

    AikiEvent::FilePermissionAsked(AikiFilePermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Write,
        path,
        pattern: None,
    })
}

/// Build file.permission_asked event for read operations (Read, LS, Glob, Grep)
fn build_file_read_permission_asked_event(
    payload: ClaudePreToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    let (path, pattern) = match tool {
        ClaudeTool::Read(input) => (Some(input.file_path), None),
        ClaudeTool::Glob(input) => {
            // Glob with no path means search from current directory
            let path = input.path.or_else(|| Some(".".to_string()));
            (path, Some(input.pattern))
        }
        ClaudeTool::Grep(input) => {
            // Grep with no path means search from current directory
            let path = input.path.or_else(|| Some(".".to_string()));
            (path, Some(input.pattern))
        }
        ClaudeTool::LS(input) => {
            // LS with no path means list current directory
            let path = input.path.or_else(|| Some(".".to_string()));
            (path, None)
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            (None, None)
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in file.read.permission_asked");
            (None, None)
        }
    };

    AikiEvent::FilePermissionAsked(AikiFilePermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Read,
        path,
        pattern,
    })
}

/// Build file.completed event for all file tools
fn build_file_completed_event(payload: ClaudePostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    // Extra safety check - should never happen due to tool_type() dispatch
    if !matches!(tool.tool_type(), ToolType::File) {
        eprintln!("[aiki] Error: build_file_completed_event called on non-file tool");
        return AikiEvent::Unsupported;
    }

    let operation = tool.file_operation();

    match operation {
        FileOperation::Write => build_file_write_completed_event(payload, tool),
        FileOperation::Read => build_file_read_completed_event(payload, tool),
        FileOperation::Delete => {
            eprintln!("[aiki] Warning: Delete operation not yet supported in PostToolUse");
            AikiEvent::Unsupported
        }
    }
}

/// Build file.completed event for write operations (Edit, Write, NotebookEdit, MultiEdit)
fn build_file_write_completed_event(
    payload: ClaudePostToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    let (file_paths, edit_details) = match tool {
        ClaudeTool::Edit(input) | ClaudeTool::NotebookEdit(input) => {
            // Edit/NotebookEdit use old_string/new_string for replacements
            let details = if !input.old_string.is_empty() || !input.new_string.is_empty() {
                vec![crate::events::EditDetail::new(
                    input.file_path.clone(),
                    input.old_string.clone(),
                    input.new_string.clone(),
                )]
            } else {
                Vec::new()
            };
            (vec![input.file_path], details)
        }
        ClaudeTool::Write(input) => {
            // Write tool uses content field for full file writes
            let details = if !input.content.is_empty() {
                vec![crate::events::EditDetail::new(
                    input.file_path.clone(),
                    String::new(),
                    input.content.clone(),
                )]
            } else {
                Vec::new()
            };
            (vec![input.file_path], details)
        }
        ClaudeTool::MultiEdit(input) => {
            // MultiEdit performs atomic edits across multiple files
            let paths: Vec<String> = input.edits.iter().map(|e| e.file_path.clone()).collect();
            let details: Vec<crate::events::EditDetail> = input
                .edits
                .into_iter()
                .filter(|e| !e.old_string.is_empty() || !e.new_string.is_empty())
                .map(|e| crate::events::EditDetail::new(e.file_path, e.old_string, e.new_string))
                .collect();
            (paths, details)
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            return AikiEvent::Unsupported;
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in file.write.completed");
            return AikiEvent::Unsupported;
        }
    };

    AikiEvent::FileCompleted(AikiFileCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Write,
        tool_name: payload.tool_name,
        file_paths,
        success: Some(true),
        edit_details,
    })
}

/// Build file.completed event for read operations (Read, LS, Glob, Grep)
fn build_file_read_completed_event(
    payload: ClaudePostToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    let file_paths = match tool {
        ClaudeTool::Read(input) => vec![input.file_path],
        ClaudeTool::Glob(input) => {
            // Glob with no path means search from current directory
            input
                .path
                .map(|p| vec![p])
                .unwrap_or_else(|| vec![".".to_string()])
        }
        ClaudeTool::Grep(input) => {
            // Grep with no path means search from current directory
            input
                .path
                .map(|p| vec![p])
                .unwrap_or_else(|| vec![".".to_string()])
        }
        ClaudeTool::LS(input) => {
            // LS with no path means list current directory
            input
                .path
                .map(|p| vec![p])
                .unwrap_or_else(|| vec![".".to_string()])
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            return AikiEvent::Unsupported;
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in file.read.completed");
            return AikiEvent::Unsupported;
        }
    };

    AikiEvent::FileCompleted(AikiFileCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Read,
        tool_name: payload.tool_name,
        file_paths,
        success: Some(true),
        edit_details: Vec::new(),
    })
}

/// Build shell.permission_asked event (Bash tool)
fn build_shell_permission_asked_event(
    payload: ClaudePreToolUsePayload,
    tool: ClaudeTool,
) -> AikiEvent {
    let command = match tool {
        ClaudeTool::Bash(input) => input.command,
        ClaudeTool::Unknown(_) => {
            eprintln!("[aiki] Warning: Failed to parse Bash tool input");
            String::new()
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in shell.permission_asked");
            String::new()
        }
    };

    AikiEvent::ShellPermissionAsked(AikiShellPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command,
    })
}

/// Build shell.completed event (Bash tool)
fn build_shell_completed_event(payload: ClaudePostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let command = match tool {
        ClaudeTool::Bash(input) => input.command,
        ClaudeTool::Unknown(_) => {
            eprintln!("[aiki] Warning: Failed to parse Bash tool input");
            String::new()
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in shell.completed");
            String::new()
        }
    };

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

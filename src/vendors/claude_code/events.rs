use serde::Deserialize;
use std::path::PathBuf;

use crate::cache::debug_log;
use crate::error::Result;
use crate::events::FileOperation;
use crate::events::{
    parse_mcp_server, AikiEvent, AikiMcpCompletedPayload, AikiMcpPermissionAskedPayload,
    AikiPromptSubmittedPayload, AikiReadCompletedPayload, AikiReadPermissionAskedPayload,
    AikiResponseReceivedPayload, AikiSessionStartPayload, AikiShellCompletedPayload,
    AikiShellPermissionAskedPayload, AikiWebCompletedPayload, AikiWebPermissionAskedPayload,
    AikiWriteCompletedPayload, AikiWritePermissionAskedPayload,
};
use crate::tools::ToolType;

use super::session::create_session;
use super::tools::{BashToolResponse, ClaudeTool};

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
        payload: SessionStartPayload,
    },
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit {
        #[serde(flatten)]
        payload: UserPromptSubmitPayload,
    },
    #[serde(rename = "PreToolUse")]
    PreToolUse {
        #[serde(flatten)]
        payload: PreToolUsePayload,
    },
    #[serde(rename = "PostToolUse")]
    PostToolUse {
        #[serde(flatten)]
        payload: PostToolUsePayload,
    },
    #[serde(rename = "Stop")]
    Stop {
        #[serde(flatten)]
        payload: StopPayload,
    },
}

/// SessionStart hook payload
#[derive(Deserialize, Debug)]
struct SessionStartPayload {
    session_id: String,
    cwd: String,
}

/// UserPromptSubmit hook payload
#[derive(Deserialize, Debug)]
struct UserPromptSubmitPayload {
    session_id: String,
    cwd: String,
    #[serde(default)]
    prompt: String,
}

/// PreToolUse hook payload
#[derive(Deserialize, Debug)]
pub struct PreToolUsePayload {
    pub session_id: String,
    pub cwd: String,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
}

/// PostToolUse hook payload
#[derive(Deserialize, Debug)]
pub struct PostToolUsePayload {
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
struct StopPayload {
    session_id: String,
    cwd: String,
}

// ============================================================================
// Event Building
// ============================================================================

/// Build AikiEvent from Claude Code event read from stdin
pub fn build_aiki_event_from_stdin() -> Result<AikiEvent> {
    // Parse event - serde discriminates by hook_event_name
    let event: ClaudeEvent = super::super::read_stdin_json()?;

    let aiki_event = match event {
        ClaudeEvent::SessionStart { payload } => build_session_started_event(payload),
        ClaudeEvent::UserPromptSubmit { payload } => build_prompt_submitted_event(payload),
        ClaudeEvent::PreToolUse { payload } => build_permission_asked_event_for_tool_type(payload),
        ClaudeEvent::PostToolUse { payload } => build_completed_event_for_tool_type(payload),
        ClaudeEvent::Stop { payload } => build_response_received_event(payload),
    };

    Ok(aiki_event)
}

/// Build appropriate pre-tool event based on tool type
fn build_permission_asked_event_for_tool_type(payload: PreToolUsePayload) -> AikiEvent {
    let tool = ClaudeTool::parse(&payload.tool_name, payload.tool_input.as_ref());

    match tool.tool_type() {
        ToolType::File => build_file_permission_asked_event(payload, tool),
        ToolType::Shell => build_shell_permission_asked_event(payload, tool),
        ToolType::Mcp => build_mcp_permission_asked_event(payload),
        ToolType::Web => build_web_permission_asked_event(payload, tool),
        ToolType::Internal => AikiEvent::Unsupported,
    }
}

/// Build appropriate post-tool event based on tool type
fn build_completed_event_for_tool_type(payload: PostToolUsePayload) -> AikiEvent {
    let tool = ClaudeTool::parse(&payload.tool_name, payload.tool_input.as_ref());

    match tool.tool_type() {
        ToolType::File => build_file_completed_event(payload, tool),
        ToolType::Shell => build_shell_completed_event(payload, tool),
        ToolType::Mcp => build_mcp_completed_event(payload),
        ToolType::Web => build_web_completed_event(payload, tool),
        ToolType::Internal => AikiEvent::Unsupported,
    }
}

/// Build session.started event
fn build_session_started_event(payload: SessionStartPayload) -> AikiEvent {
    AikiEvent::SessionStarted(AikiSessionStartPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}

/// Build prompt.submitted event
fn build_prompt_submitted_event(payload: UserPromptSubmitPayload) -> AikiEvent {
    AikiEvent::PromptSubmitted(AikiPromptSubmittedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        prompt: payload.prompt,
    })
}

/// Build file.permission_asked event for all file tools
fn build_file_permission_asked_event(payload: PreToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    // Extra safety check - should never happen due to tool_type() dispatch
    if !matches!(tool.tool_type(), ToolType::File) {
        eprintln!("[aiki] Error: build_file_permission_asked_event called on non-file tool");
        return AikiEvent::Unsupported;
    }

    let Some(operation) = tool.file_operation() else {
        eprintln!("[aiki] Error: Failed to get file operation");
        return AikiEvent::Unsupported;
    };

    match operation {
        FileOperation::Write => build_write_permission_asked_event(payload, tool),
        FileOperation::Read => build_read_permission_asked_event(payload, tool),
        FileOperation::Delete => {
            eprintln!("[aiki] Warning: Delete operation not yet supported in PreToolUse");
            AikiEvent::Unsupported
        }
    }
}

/// Build write.permission_asked event for write operations (Edit, Write, NotebookEdit, MultiEdit)
fn build_write_permission_asked_event(payload: PreToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let file_paths = match tool {
        ClaudeTool::Edit(input) | ClaudeTool::Write(input) | ClaudeTool::NotebookEdit(input) => {
            vec![input.file_path]
        }
        ClaudeTool::MultiEdit(input) => {
            // MultiEdit affects multiple files
            input.edits.iter().map(|e| e.file_path.clone()).collect()
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            Vec::new()
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in write.permission_asked");
            Vec::new()
        }
    };

    AikiEvent::WritePermissionAsked(AikiWritePermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        file_paths,
    })
}

/// Build read.permission_asked event for read operations (Read, LS, Glob, Grep)
fn build_read_permission_asked_event(payload: PreToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let (file_paths, pattern) = match tool {
        ClaudeTool::Read(input) => (vec![input.file_path], None),
        ClaudeTool::Glob(input) => {
            // Glob with no path means search from current directory
            let path = input.path.unwrap_or_else(|| payload.cwd.clone());
            (vec![path], Some(input.pattern))
        }
        ClaudeTool::Grep(input) => {
            // Grep with no path means search from current directory
            let path = input.path.unwrap_or_else(|| payload.cwd.clone());
            (vec![path], Some(input.pattern))
        }
        ClaudeTool::LS(input) => {
            // LS with no path means list current directory
            let path = input.path.unwrap_or_else(|| payload.cwd.clone());
            (vec![path], None)
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            (Vec::new(), None)
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in read.permission_asked");
            (Vec::new(), None)
        }
    };

    AikiEvent::ReadPermissionAsked(AikiReadPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        file_paths,
        pattern,
    })
}

/// Build file.completed event for all file tools
fn build_file_completed_event(payload: PostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    // Extra safety check - should never happen due to tool_type() dispatch
    if !matches!(tool.tool_type(), ToolType::File) {
        eprintln!("[aiki] Error: build_file_completed_event called on non-file tool");
        return AikiEvent::Unsupported;
    }

    let Some(operation) = tool.file_operation() else {
        eprintln!("[aiki] Error: Failed to get file operation");
        return AikiEvent::Unsupported;
    };

    match operation {
        FileOperation::Write => build_write_completed_event(payload, tool),
        FileOperation::Read => build_read_completed_event(payload, tool),
        FileOperation::Delete => {
            eprintln!("[aiki] Warning: Delete operation not yet supported in PostToolUse");
            AikiEvent::Unsupported
        }
    }
}

/// Build write.completed event for write operations (Edit, Write, NotebookEdit, MultiEdit)
fn build_write_completed_event(payload: PostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
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
            eprintln!("[aiki] Warning: Unexpected tool type in write.completed");
            return AikiEvent::Unsupported;
        }
    };

    AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        file_paths,
        success: true,
        edit_details,
    })
}

/// Build read.completed event for read operations (Read, LS, Glob, Grep)
fn build_read_completed_event(payload: PostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let file_paths = match tool {
        ClaudeTool::Read(input) => vec![input.file_path],
        ClaudeTool::Glob(input) => {
            // Glob with no path means search from current directory
            vec![input.path.unwrap_or_else(|| payload.cwd.clone())]
        }
        ClaudeTool::Grep(input) => {
            // Grep with no path means search from current directory
            vec![input.path.unwrap_or_else(|| payload.cwd.clone())]
        }
        ClaudeTool::LS(input) => {
            // LS with no path means list current directory
            vec![input.path.unwrap_or_else(|| payload.cwd.clone())]
        }
        ClaudeTool::Unknown(name) => {
            eprintln!("[aiki] Warning: Failed to parse tool input for '{}'", name);
            return AikiEvent::Unsupported;
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in read.completed");
            return AikiEvent::Unsupported;
        }
    };

    AikiEvent::ReadCompleted(AikiReadCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        file_paths,
        success: true,
    })
}

/// Build shell.permission_asked event (Bash tool)
fn build_shell_permission_asked_event(payload: PreToolUsePayload, tool: ClaudeTool) -> AikiEvent {
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
fn build_shell_completed_event(payload: PostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
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

    // Claude Code provides exit_code, stdout, stderr in tool_response
    let (success, exit_code, stdout, stderr) = payload
        .tool_response
        .as_ref()
        .and_then(|v| serde_json::from_value::<BashToolResponse>(v.clone()).ok())
        .map(|resp| {
            (
                resp.exit_code == 0,
                Some(resp.exit_code),
                Some(resp.stdout),
                Some(resp.stderr),
            )
        })
        .unwrap_or_else(|| {
            debug_log(|| "Warning: PostToolUse Bash missing tool_response, assuming success");
            (true, None, None, None)
        });

    AikiEvent::ShellCompleted(AikiShellCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command,
        success,
        exit_code,
        stdout,
        stderr,
    })
}

/// Build mcp.permission_asked event (MCP tools)
fn build_mcp_permission_asked_event(payload: PreToolUsePayload) -> AikiEvent {
    let parameters = payload.tool_input.unwrap_or(serde_json::Value::Null);
    let server = parse_mcp_server(&payload.tool_name);

    AikiEvent::McpPermissionAsked(AikiMcpPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        server,
        tool_name: payload.tool_name,
        parameters,
    })
}

/// Build mcp.completed event (MCP tools)
fn build_mcp_completed_event(payload: PostToolUsePayload) -> AikiEvent {
    let result = payload
        .tool_response
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .filter(|s| !s.is_empty() && s != "null");
    let server = parse_mcp_server(&payload.tool_name);

    AikiEvent::McpCompleted(AikiMcpCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        server,
        tool_name: payload.tool_name,
        success: true,
        result,
    })
}

/// Build web.permission_asked event (WebFetch, WebSearch)
fn build_web_permission_asked_event(payload: PreToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let Some(operation) = tool.web_operation() else {
        eprintln!("[aiki] Error: Failed to get web operation");
        return AikiEvent::Unsupported;
    };

    let (url, query) = match tool {
        ClaudeTool::WebFetch(input) => (Some(input.url), None),
        ClaudeTool::WebSearch(input) => (None, Some(input.query)),
        ClaudeTool::Unknown(name) => {
            eprintln!(
                "[aiki] Warning: Failed to parse web tool input for '{}'",
                name
            );
            (None, None)
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in web.permission_asked");
            (None, None)
        }
    };

    AikiEvent::WebPermissionAsked(AikiWebPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation,
        url,
        query,
    })
}

/// Build web.completed event (WebFetch, WebSearch)
fn build_web_completed_event(payload: PostToolUsePayload, tool: ClaudeTool) -> AikiEvent {
    let Some(operation) = tool.web_operation() else {
        eprintln!("[aiki] Error: Failed to get web operation");
        return AikiEvent::Unsupported;
    };

    let (url, query) = match tool {
        ClaudeTool::WebFetch(input) => (Some(input.url), None),
        ClaudeTool::WebSearch(input) => (None, Some(input.query)),
        ClaudeTool::Unknown(name) => {
            eprintln!(
                "[aiki] Warning: Failed to parse web tool input for '{}'",
                name
            );
            (None, None)
        }
        _ => {
            eprintln!("[aiki] Warning: Unexpected tool type in web.completed");
            (None, None)
        }
    };

    // Web operations are always considered successful if we reach PostToolUse
    AikiEvent::WebCompleted(AikiWebCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        operation,
        url,
        query,
        success: true,
    })
}

/// Build response.received event
fn build_response_received_event(payload: StopPayload) -> AikiEvent {
    AikiEvent::ResponseReceived(AikiResponseReceivedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        response: String::new(),
        modified_files: vec![],
    })
}

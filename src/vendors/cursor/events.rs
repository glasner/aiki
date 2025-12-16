use serde::Deserialize;
use std::path::PathBuf;

use crate::cache::debug_log;
use crate::error::Result;
use crate::events::{
    AikiEvent, AikiFileCompletedPayload, AikiFilePermissionAskedPayload, AikiMcpCompletedPayload,
    AikiMcpPermissionAskedPayload, AikiPromptSubmittedPayload, AikiShellCompletedPayload,
    AikiShellPermissionAskedPayload, FileOperation,
};
use crate::tools::ToolType;

use super::session::create_session;

// ============================================================================
// Hook Payload Structures (matches Cursor API)
// See: https://cursor.com/docs/agent/hooks
// ============================================================================

/// Cursor hook event - discriminated by eventName
#[derive(Deserialize, Debug)]
#[serde(tag = "eventName")]
enum CursorEvent {
    #[serde(rename = "beforeSubmitPrompt")]
    BeforeSubmitPrompt {
        #[serde(flatten)]
        payload: CursorBeforeSubmitPromptPayload,
    },
    #[serde(rename = "stop")]
    Stop {
        #[serde(flatten)]
        payload: CursorStopPayload,
    },
    #[serde(rename = "beforeShellExecution")]
    BeforeShellExecution {
        #[serde(flatten)]
        payload: CursorBeforeShellExecutionPayload,
    },
    #[serde(rename = "afterShellExecution")]
    AfterShellExecution {
        #[serde(flatten)]
        payload: CursorAfterShellExecutionPayload,
    },
    #[serde(rename = "beforeMCPExecution")]
    BeforeMcpExecution {
        #[serde(flatten)]
        payload: CursorBeforeMcpExecutionPayload,
    },
    #[serde(rename = "afterMCPExecution")]
    AfterMcpExecution {
        #[serde(flatten)]
        payload: CursorAfterMcpExecutionPayload,
    },
    #[serde(rename = "afterFileEdit")]
    AfterFileEdit {
        #[serde(flatten)]
        payload: CursorAfterFileEditPayload,
    },
}

/// beforeSubmitPrompt hook payload
#[derive(Deserialize, Debug)]
struct CursorBeforeSubmitPromptPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    #[serde(default)]
    prompt: String,
}

/// stop hook payload
#[derive(Deserialize, Debug)]
struct CursorStopPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    status: String,
    loop_count: u32,
}

/// beforeShellExecution hook payload
#[derive(Deserialize, Debug)]
struct CursorBeforeShellExecutionPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    command: String,
    cwd: String,
}

/// afterShellExecution hook payload
#[derive(Deserialize, Debug)]
struct CursorAfterShellExecutionPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    command: String,
    output: String,
    duration: u64,
}

/// beforeMCPExecution hook payload
#[derive(Deserialize, Debug)]
pub struct CursorBeforeMcpExecutionPayload {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    #[serde(rename = "generationId")]
    pub generation_id: String,
    pub model: String,
    #[serde(rename = "cursorVersion")]
    pub cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    pub workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    pub user_email: Option<String>,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "toolInput")]
    pub tool_input: String,
}

/// afterMCPExecution hook payload
#[derive(Deserialize, Debug)]
struct CursorAfterMcpExecutionPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    #[serde(rename = "toolName")]
    tool_name: String,
    #[serde(rename = "toolInput")]
    tool_input: String,
    #[serde(rename = "resultJson")]
    result_json: String,
    duration: u64,
}

/// afterFileEdit hook payload
#[derive(Deserialize, Debug)]
struct CursorAfterFileEditPayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    #[serde(rename = "userEmail")]
    user_email: Option<String>,
    #[serde(rename = "filePath")]
    file_path: String,
    edits: Vec<CursorEdit>,
}

/// Individual edit operation in Cursor's afterFileEdit hook
#[derive(Deserialize, Debug)]
struct CursorEdit {
    old_string: String,
    new_string: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get working directory from workspace roots
/// Takes the first workspace root, or current directory as fallback
fn get_cwd(workspace_roots: &[String]) -> PathBuf {
    workspace_roots
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ============================================================================
// Event Building
// ============================================================================

/// Build AikiEvent from Cursor event read from stdin
pub fn build_aiki_event_from_stdin() -> Result<AikiEvent> {
    // Parse event - serde discriminates by eventName
    let cursor_event: CursorEvent = super::super::read_stdin_json()?;

    let aiki_event = match cursor_event {
        CursorEvent::BeforeSubmitPrompt { payload } => build_prompt_submitted_event(payload),
        CursorEvent::Stop { payload } => build_response_received_event(payload),
        CursorEvent::BeforeShellExecution { payload } => {
            build_shell_permission_asked_event(payload)
        }
        CursorEvent::AfterShellExecution { payload } => build_shell_completed_event(payload),
        CursorEvent::BeforeMcpExecution { payload } => build_mcp_or_file_event(payload),
        CursorEvent::AfterMcpExecution { payload } => build_mcp_completed_event(payload),
        CursorEvent::AfterFileEdit { payload } => build_file_completed_event(payload),
    };

    Ok(aiki_event)
}

/// Build appropriate event for beforeMCPExecution based on tool type
fn build_mcp_or_file_event(payload: CursorBeforeMcpExecutionPayload) -> AikiEvent {
    let tool_type = super::tools::classify_mcp_tool(&payload.tool_name);

    match tool_type {
        ToolType::File => build_file_permission_asked_event(payload),
        ToolType::Mcp => build_mcp_permission_asked_event(payload),
        // Cursor only calls beforeMCPExecution for MCP tools, not Shell/Web/Internal
        _ => build_mcp_permission_asked_event(payload),
    }
}

/// Build prompt.submitted event from beforeSubmitPrompt payload
///
/// Note: Cursor's beforeSubmitPrompt fires on EVERY prompt submission.
/// Ideally we should track conversation_id changes to fire session.started only
/// on new conversations, but that requires stateful tracking across invocations.
/// For now, we fire prompt.submitted on every call, which enables validation workflows.
///
/// Limitation: Cursor's beforeSubmitPrompt can only BLOCK prompts, not modify them.
/// The modifiedPrompt field is not supported - only blocking via user_message.
fn build_prompt_submitted_event(payload: CursorBeforeSubmitPromptPayload) -> AikiEvent {
    AikiEvent::PromptSubmitted(AikiPromptSubmittedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        prompt: payload.prompt,
    })
}

/// Build file.permission_asked event from beforeMCPExecution payload (file tools only)
fn build_file_permission_asked_event(payload: CursorBeforeMcpExecutionPayload) -> AikiEvent {
    // Try to extract file path from tool_input JSON
    let path = serde_json::from_str::<serde_json::Value>(&payload.tool_input)
        .ok()
        .and_then(|v| {
            v.get("file_path")
                .and_then(|p| p.as_str())
                .map(String::from)
        });

    AikiEvent::FilePermissionAsked(AikiFilePermissionAskedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Write,
        path,
        pattern: None,
    })
}

/// Build shell.permission_asked event from beforeShellExecution payload
fn build_shell_permission_asked_event(payload: CursorBeforeShellExecutionPayload) -> AikiEvent {
    AikiEvent::ShellPermissionAsked(AikiShellPermissionAskedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command: payload.command,
    })
}

/// Build shell.completed event from afterShellExecution payload
fn build_shell_completed_event(payload: CursorAfterShellExecutionPayload) -> AikiEvent {
    AikiEvent::ShellCompleted(AikiShellCompletedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        command: payload.command,
        // Cursor doesn't provide exit code - assume success (0)
        // TODO: Parse exit code from output if available
        exit_code: 0,
        stdout: payload.output,
        stderr: String::new(), // Cursor combines stdout/stderr in output field
    })
}

/// Build mcp.permission_asked event from beforeMCPExecution payload (non-file tools)
fn build_mcp_permission_asked_event(payload: CursorBeforeMcpExecutionPayload) -> AikiEvent {
    // Parse tool_input as JSON if possible
    let parameters = serde_json::from_str(&payload.tool_input).unwrap_or(serde_json::Value::Null);

    AikiEvent::McpPermissionAsked(AikiMcpPermissionAskedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        parameters,
    })
}

/// Build mcp.completed event from afterMCPExecution payload
fn build_mcp_completed_event(payload: CursorAfterMcpExecutionPayload) -> AikiEvent {
    AikiEvent::McpCompleted(AikiMcpCompletedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        tool_name: payload.tool_name,
        success: true, // Cursor doesn't indicate failure in hook payload
        result: if payload.result_json.is_empty() {
            None
        } else {
            Some(payload.result_json)
        },
    })
}

/// Build file.completed event from afterFileEdit payload
fn build_file_completed_event(payload: CursorAfterFileEditPayload) -> AikiEvent {
    // Create session first before moving any fields
    let session = create_session(&payload.conversation_id, &payload.cursor_version);
    let cwd = get_cwd(&payload.workspace_roots);
    let file_path = payload.file_path;

    // Extract edit details from Cursor's edits array for user edit detection
    let edit_details: Vec<crate::events::EditDetail> = payload
        .edits
        .iter()
        .map(|edit| {
            crate::events::EditDetail::new(
                file_path.clone(),
                edit.old_string.clone(),
                edit.new_string.clone(),
            )
        })
        .collect();

    if !edit_details.is_empty() {
        debug_log(|| format!("Cursor provided {} edits", edit_details.len()));
    }

    AikiEvent::FileCompleted(AikiFileCompletedPayload {
        session,
        cwd,
        timestamp: chrono::Utc::now(),
        operation: FileOperation::Write,
        tool_name: "edit".to_string(), // Cursor doesn't distinguish Edit/Write
        file_paths: vec![file_path],
        success: Some(true), // afterFileEdit implies success
        edit_details,
    })
}

/// Build response.received event from stop payload
fn build_response_received_event(payload: CursorStopPayload) -> AikiEvent {
    AikiEvent::ResponseReceived(crate::events::AikiResponseReceivedPayload {
        session: create_session(&payload.conversation_id, &payload.cursor_version),
        cwd: get_cwd(&payload.workspace_roots),
        timestamp: chrono::Utc::now(),
        response: String::new(), // Cursor doesn't provide response text in stop hook
        modified_files: Vec::new(), // Cursor doesn't track modified files in stop hook
    })
}

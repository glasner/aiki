use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::cache::debug_log;
use crate::commands::hooks::HookCommandOutput;
use crate::event_bus;
use crate::events::result::HookResult;
use crate::events::{
    AikiEvent, AikiFileCompletedPayload, AikiFilePermissionAskedPayload, AikiMcpCompletedPayload,
    AikiMcpPermissionAskedPayload, AikiPromptSubmittedPayload, AikiShellCompletedPayload,
    AikiShellPermissionAskedPayload, FileOperation,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

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
struct CursorBeforeMcpExecutionPayload {
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

/// Create a session from payload fields
fn create_session(conversation_id: &str, cursor_version: &str) -> AikiSession {
    AikiSession::new(
        AgentType::Cursor,
        conversation_id,
        Some(cursor_version),
        DetectionMethod::Hook,
    )
}

/// Get working directory from workspace roots
/// Takes the first workspace root, or current directory as fallback
fn get_cwd(workspace_roots: &[String]) -> PathBuf {
    workspace_roots
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Tool type classification for event routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolType {
    /// File-modifying tools
    FileChange,
    /// MCP tools (non-file)
    Mcp,
}

/// Classify a Cursor MCP tool by name into its type
///
/// Note: Cursor's tool names may differ from Claude Code's.
/// This covers known file-modifying tools and treats everything else as MCP.
fn classify_mcp_tool(tool_name: &str) -> ToolType {
    match tool_name {
        // File-modifying tools (various naming conventions)
        "Edit" | "Write" | "NotebookEdit" | "edit" | "write" | "file_edit" => ToolType::FileChange,
        // Everything else is treated as MCP tool
        _ => ToolType::Mcp,
    }
}

/// Handle a Cursor event
///
/// This is the vendor-specific handler for Cursor hooks.
/// Parses the payload once and dispatches to event-specific handlers.
///
/// # Arguments
/// * `cursor_event_name` - Vendor event name from CLI flag (used for output formatting)
pub fn handle(cursor_event_name: &str) -> Result<()> {
    // Parse event - serde discriminates by eventName
    let cursor_event: CursorEvent = super::read_stdin_json()?;

    // Build Aiki event from Cursor event
    let aiki_event = build_aiki_event(cursor_event);

    // Dispatch event and exit with command output
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let hook_output = build_command_output(aiki_response, cursor_event_name);

    hook_output.print_and_exit();
}

/// Build AikiEvent from Cursor event
fn build_aiki_event(event: CursorEvent) -> AikiEvent {
    match event {
        CursorEvent::BeforeSubmitPrompt { payload } => build_prompt_submitted_event(payload),
        CursorEvent::Stop { payload } => build_response_received_event(payload),
        CursorEvent::BeforeShellExecution { payload } => {
            build_shell_permission_asked_event(payload)
        }
        CursorEvent::AfterShellExecution { payload } => build_shell_completed_event(payload),
        CursorEvent::BeforeMcpExecution { payload } => build_mcp_or_file_event(payload),
        CursorEvent::AfterMcpExecution { payload } => build_mcp_completed_event(payload),
        CursorEvent::AfterFileEdit { payload } => build_file_completed_event(payload),
    }
}

/// Build appropriate event for beforeMCPExecution based on tool type
fn build_mcp_or_file_event(payload: CursorBeforeMcpExecutionPayload) -> AikiEvent {
    let tool_type = classify_mcp_tool(&payload.tool_name);

    match tool_type {
        ToolType::FileChange => build_file_permission_asked_event(payload),
        ToolType::Mcp => build_mcp_permission_asked_event(payload),
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

/// Build HookCommandOutput from HookResult for Cursor
///
/// Cursor expects different JSON structures depending on the event type.
/// This function dispatches to event-specific builders that handle the details.
fn build_command_output(response: HookResult, event_type: &str) -> HookCommandOutput {
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

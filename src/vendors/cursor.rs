use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::event_bus;
use crate::events::result::{Decision, HookResult};
use crate::events::{
    AikiEvent, AikiPostFileChangePayload, AikiPreFileChangePayload, AikiPrePromptPayload,
    AikiSessionStartPayload,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

/// Create a session for Cursor events
///
/// This helper ensures consistent session creation across all Cursor event builders.
/// Takes the full payload to allow easy extension if we need additional fields in the future.
/// Extracts `cursor_version` from the payload to populate `agent_version`.
/// Panics on failure since session creation errors are unrecoverable in the hook context.
fn create_session(payload: &CursorPayload) -> AikiSession {
    AikiSession::new(
        AgentType::Cursor,
        &payload.session_id,
        payload.cursor_version.as_deref(),
        DetectionMethod::Hook,
    )
    .expect("Failed to create AikiSession for Cursor")
}

/// Cursor hook payload structure
///
/// This matches the JSON that Cursor sends to its hooks.
/// Note: Cursor uses snake_case for afterFileEdit hook.
/// See: https://cursor.com/docs/agent/hooks#afterfileedit
#[derive(Deserialize, Debug)]
struct CursorPayload {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "workingDirectory")]
    working_directory: String,
    #[serde(rename = "eventName")]
    event_name: String,
    // Common fields across all hooks
    #[serde(rename = "cursor_version", default)]
    cursor_version: Option<String>,
    #[serde(rename = "conversation_id", default)]
    conversation_id: String,
    #[serde(rename = "generation_id", default)]
    generation_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "workspace_roots", default)]
    workspace_roots: Vec<String>,
    #[serde(rename = "user_email", default)]
    user_email: Option<String>,
    // beforeSubmitPrompt fields
    #[serde(default)]
    prompt: String,
    // beforeMCPExecution fields (TBD - exact structure not yet documented)
    #[serde(rename = "toolName", default)]
    tool_name: String,
    // afterFileEdit fields
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    edits: Vec<CursorEdit>,
}

/// Individual edit operation in Cursor's afterFileEdit hook
#[derive(Deserialize, Debug)]
struct CursorEdit {
    old_string: String,
    new_string: String,
}

/// Response structure for Cursor hooks
struct CursorResponse {
    json_value: Option<serde_json::Value>,
    exit_code: i32,
}

impl CursorResponse {
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

/// Handle a Cursor event
///
/// This is the vendor-specific handler for Cursor hooks.
/// Dispatches to event-specific handlers based on event name.
///
/// # Arguments
/// * `event_name` - Vendor event name from CLI flag (e.g., "beforeSubmitPrompt", "afterFileEdit")
pub fn handle(event_name: &str) -> Result<()> {
    // Read Cursor-specific JSON from stdin
    let payload: CursorPayload = super::read_stdin_json()?;

    // Validate event name matches JSON (optional but good practice)
    if std::env::var("AIKI_DEBUG").is_ok() && payload.event_name != event_name {
        eprintln!(
            "[aiki] Warning: Event name mismatch. CLI: {}, JSON: {}",
            event_name, payload.event_name
        );
    }

    // Build event from payload
    let aiki_event = match event_name {
        "beforeSubmitPrompt" => build_pre_prompt_event(payload),
        "beforeMCPExecution" | "beforeShellExecution" => build_pre_file_change_event(payload),
        "afterFileEdit" => build_post_file_change_event(payload),
        "stop" => build_post_response_event(payload),
        _ => AikiEvent::Unsupported,
    };

    // Dispatch event and exit with translated response
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let cursor_response = translate_response(aiki_response, event_name);

    cursor_response.print_json();
    std::process::exit(cursor_response.exit_code);
}

/// Build PrePrompt event from beforeSubmitPrompt payload
///
/// Note: Cursor's beforeSubmitPrompt fires on EVERY prompt submission.
/// Ideally we should track conversation_id changes to fire SessionStart only
/// on new conversations, but that requires stateful tracking across invocations.
/// For now, we fire PrePrompt on every call, which enables validation workflows.
///
/// Limitation: Cursor's beforeSubmitPrompt can only BLOCK prompts, not modify them.
/// The modifiedPrompt field is not supported - only blocking via user_message.
fn build_pre_prompt_event(payload: CursorPayload) -> AikiEvent {
    AikiEvent::PrePrompt(AikiPrePromptPayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.working_directory),
        timestamp: chrono::Utc::now(),
        prompt: payload.prompt,
    })
}

/// Build PreFileChange event from beforeMCPExecution/beforeShellExecution payload
fn build_pre_file_change_event(payload: CursorPayload) -> AikiEvent {
    // Fire PreFileChange only for file-modifying MCP tools
    if !is_file_modifying_tool(&payload.tool_name) {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[aiki] beforeMCPExecution: Ignoring non-file tool: {}",
                payload.tool_name
            );
        }
        return AikiEvent::Unsupported;
    }

    AikiEvent::PreFileChange(AikiPreFileChangePayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.working_directory),
        timestamp: chrono::Utc::now(),
    })
}

/// Build PostFileChange event from afterFileEdit payload
fn build_post_file_change_event(payload: CursorPayload) -> AikiEvent {
    // Create session first before moving any fields
    let session = create_session(&payload);
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

    if std::env::var("AIKI_DEBUG").is_ok() && !edit_details.is_empty() {
        eprintln!("[aiki] Cursor provided {} edits", edit_details.len());
    }

    AikiEvent::PostFileChange(AikiPostFileChangePayload {
        session,
        tool_name: "edit".to_string(), // Cursor doesn't distinguish Edit/Write
        file_paths: vec![file_path],
        cwd: PathBuf::from(&payload.working_directory),
        timestamp: chrono::Utc::now(),
        edit_details,
    })
}

/// Build PostResponse event from stop payload
fn build_post_response_event(payload: CursorPayload) -> AikiEvent {
    AikiEvent::PostResponse(crate::events::AikiPostResponsePayload {
        session: create_session(&payload),
        cwd: PathBuf::from(&payload.working_directory),
        timestamp: chrono::Utc::now(),
        response: String::new(), // Cursor doesn't provide response text in stop hook
        modified_files: Vec::new(), // Cursor doesn't track modified files in stop hook
    })
}

/// Translate HookResult to Cursor JSON format
///
/// Cursor expects different JSON structures depending on the event type.
/// This function dispatches to event-specific translators that handle the details.
fn translate_response(response: HookResult, event_type: &str) -> CursorResponse {
    match event_type {
        "beforeSubmitPrompt" => {
            // Note: beforeSubmitPrompt serves dual purpose - SessionStart + PrePrompt
            // For now, treat it as SessionStart/PrePrompt (both have same format)
            translate_before_submit_prompt(&response)
        }
        "beforeMCPExecution" | "beforeShellExecution" => translate_pre_file_change(&response),
        "afterFileEdit" => translate_post_file_change(&response),
        "stop" => translate_post_response(&response),
        _ => {
            eprintln!("Warning: Unknown Cursor event type: {}", event_type);
            CursorResponse {
                json_value: None,
                exit_code: 0,
            }
        }
    }
}

/// Translate beforeSubmitPrompt event to Cursor JSON format
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
fn translate_before_submit_prompt(response: &HookResult) -> CursorResponse {
    // Blocking - combine messages and context for user
    if response.decision.is_block() {
        let combined = response.combined_output();
        let user_message = combined.unwrap_or_default();

        return CursorResponse {
            json_value: Some(json!({
                "continue": false,
                "user_message": user_message
            })),
            exit_code: 2,
        };
    }

    // Success - allow prompt to continue
    // Note: Cursor doesn't accept additional fields on success
    // Note: Any modified_prompt in response.context is IGNORED (not supported by Cursor)
    CursorResponse {
        json_value: Some(json!({
            "continue": true
        })),
        exit_code: 0,
    }
}

/// Translate beforeMCPExecution/beforeShellExecution to Cursor JSON format
fn translate_pre_file_change(response: &HookResult) -> CursorResponse {
    // Blocking - prevent tool execution (combine messages and context)
    if response.decision.is_block() {
        let combined = response.combined_output();
        let agent_message = combined.unwrap_or_default();

        return CursorResponse {
            json_value: Some(json!({
                "continue": false,
                "agent_message": agent_message
            })),
            exit_code: 2,
        };
    }

    // Success - allow tool execution
    // Note: Cursor doesn't accept additional fields on success
    CursorResponse {
        json_value: Some(json!({
            "continue": true
        })),
        exit_code: 0,
    }
}

/// Translate afterFileEdit to Cursor JSON format
///
/// Per translator-requirements.md, Cursor's afterFileEdit hook does NOT
/// accept JSON responses - it's notification-only.
fn translate_post_file_change(_response: &HookResult) -> CursorResponse {
    // Cursor doesn't accept responses from afterFileEdit
    // Return no JSON, always exit 0
    CursorResponse {
        json_value: None,
        exit_code: 0,
    }
}

/// Translate stop event to Cursor JSON format
///
/// Combines messages and context into followup_message for the agent.
fn translate_post_response(response: &HookResult) -> CursorResponse {
    // Combine messages + context for followup_message
    let combined = response.combined_output();

    if let Some(followup_text) = combined {
        return CursorResponse {
            json_value: Some(json!({
                "followup_message": followup_text
            })),
            exit_code: 0,
        };
    }

    // No followup - return empty object
    CursorResponse {
        json_value: Some(json!({})),
        exit_code: 0,
    }
}

/// Check if a tool modifies files
///
/// Returns true for tools that create, modify, or delete files.
/// PreFileChange events should only fire for these tools to stash user edits.
///
/// Note: Cursor's tool names may differ from Claude Code's. This will need
/// to be updated once we know the actual tool names used by Cursor's MCP system.
fn is_file_modifying_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "Edit" | "Write" | "NotebookEdit" | "edit" | "write" | "file_edit"
    )
}

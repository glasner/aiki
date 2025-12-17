use crate::cache::{debug_log, DEBUG_ENABLED};
use crate::error::Result;
use crate::events::result::HookResult;
use crate::events::{
    self, AikiDeleteCompletedPayload, AikiDeletePermissionAskedPayload, AikiEvent,
    AikiSessionEndedPayload,
};
use crate::session::AikiSession;
use crate::tools::{parse_file_operation_from_shell_command, FileOperation};
use std::path::PathBuf;

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and handlers return generic
/// HookResult objects that can be translated to editor-specific formats.
pub fn dispatch(event: AikiEvent) -> Result<HookResult> {
    // Handle unsupported events immediately
    if matches!(event, AikiEvent::Unsupported) {
        return Ok(HookResult::success());
    }

    // Log event for debugging (uses cached debug flag)
    if *DEBUG_ENABLED {
        let event_type_name = match &event {
            // Session lifecycle
            AikiEvent::SessionStarted(_) => "session.started",
            AikiEvent::SessionResumed(_) => "session.resumed",
            AikiEvent::SessionEnded(_) => "session.ended",
            // User / agent interaction
            AikiEvent::PromptSubmitted(_) => "prompt.submitted",
            AikiEvent::ResponseReceived(_) => "response.received",
            // File access (unified model) - DEPRECATED
            AikiEvent::FilePermissionAsked(_) => "file.permission_asked",
            AikiEvent::FileCompleted(_) => "file.completed",
            // Read operations
            AikiEvent::ReadPermissionAsked(_) => "read.permission_asked",
            AikiEvent::ReadCompleted(_) => "read.completed",
            // Write operations
            AikiEvent::WritePermissionAsked(_) => "write.permission_asked",
            AikiEvent::WriteCompleted(_) => "write.completed",
            // Delete operations
            AikiEvent::DeletePermissionAsked(_) => "delete.permission_asked",
            AikiEvent::DeleteCompleted(_) => "delete.completed",
            // Shell commands
            AikiEvent::ShellPermissionAsked(_) => "shell.permission_asked",
            AikiEvent::ShellCompleted(_) => "shell.completed",
            // Web access
            AikiEvent::WebPermissionAsked(_) => "web.permission_asked",
            AikiEvent::WebCompleted(_) => "web.completed",
            // MCP tools
            AikiEvent::McpPermissionAsked(_) => "mcp.permission_asked",
            AikiEvent::McpCompleted(_) => "mcp.completed",
            // Git integration
            AikiEvent::CommitMessageStarted(_) => "commit.message_started",
            // Fallback
            AikiEvent::Unsupported => "unsupported",
        };
        debug_log(|| {
            format!(
                "Dispatching event: {} from agent: {:?}",
                event_type_name,
                event.agent_type()
            )
        });
    }

    // Route to appropriate handler
    let result = match event {
        // Session lifecycle
        AikiEvent::SessionStarted(e) => events::handle_session_started(e),
        AikiEvent::SessionResumed(e) => events::handle_session_resumed(e),
        AikiEvent::SessionEnded(e) => events::handle_session_ended(e),

        // User / agent interaction
        AikiEvent::PromptSubmitted(e) => events::handle_prompt_submitted(e),
        AikiEvent::ResponseReceived(e) => {
            // Extract fields we'll need for session.ended before consuming the event
            let session = e.session.clone();
            let cwd = e.cwd.clone();

            // Handle response.received and check for autoreply
            let response = events::handle_response_received(e)?;

            // Allow benchmark to force autoreply behavior (skip session.ended)
            // Preserve actual failures/decisions but override context
            if std::env::var("AIKI_BENCHMARK_FORCE_AUTOREPLY").is_ok() {
                return Ok(HookResult {
                    context: Some("benchmark-autoreply".to_string()),
                    decision: response.decision,
                    failures: response.failures,
                });
            }

            // If response.received produced an autoreply, return it (session continues)
            if response.has_context() {
                return Ok(response);
            }

            // No autoreply - session is done, trigger session.ended event
            trigger_session_ended(session, cwd)
        }

        // File access (unified model) - DEPRECATED
        AikiEvent::FilePermissionAsked(e) => events::handle_file_permission_asked(e),
        AikiEvent::FileCompleted(e) => events::handle_file_completed(e),

        // Read operations
        AikiEvent::ReadPermissionAsked(e) => events::handle_read_permission_asked(e),
        AikiEvent::ReadCompleted(e) => events::handle_read_completed(e),

        // Write operations
        AikiEvent::WritePermissionAsked(e) => events::handle_write_permission_asked(e),
        AikiEvent::WriteCompleted(e) => events::handle_write_completed(e),

        // Delete operations
        AikiEvent::DeletePermissionAsked(e) => events::handle_delete_permission_asked(e),
        AikiEvent::DeleteCompleted(e) => events::handle_delete_completed(e),

        // Shell commands - transform rm/rmdir to delete.* events
        AikiEvent::ShellPermissionAsked(e) => {
            let (file_op, paths) = parse_file_operation_from_shell_command(&e.command);
            if let Some(FileOperation::Delete) = file_op {
                return transform_shell_to_delete_permission_asked(e, paths);
            }
            // Regular shell command (or future: mv, cp, etc.)
            events::handle_shell_permission_asked(e)
        }
        AikiEvent::ShellCompleted(e) => {
            let (file_op, paths) = parse_file_operation_from_shell_command(&e.command);
            if let Some(FileOperation::Delete) = file_op {
                return transform_shell_to_delete_completed(e, paths);
            }
            // Regular shell command (or future: mv, cp, etc.)
            events::handle_shell_completed(e)
        }

        // Web access
        AikiEvent::WebPermissionAsked(e) => events::handle_web_permission_asked(e),
        AikiEvent::WebCompleted(e) => events::handle_web_completed(e),

        // MCP tools
        AikiEvent::McpPermissionAsked(e) => events::handle_mcp_permission_asked(e),
        AikiEvent::McpCompleted(e) => events::handle_mcp_completed(e),

        // Git integration
        AikiEvent::CommitMessageStarted(e) => events::handle_commit_message_started(e),

        // Fallback
        AikiEvent::Unsupported => return Ok(HookResult::success()),
    };

    // If handler fails, return a failure response instead of propagating error
    match result {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("Warning: Aiki event handler failed: {}", e);
            Ok(HookResult::failure(format!("Aiki handler failed: {}", e)))
        }
    }
}

/// Trigger a session.ended event
///
/// Called automatically when response.received doesn't generate an autoreply.
fn trigger_session_ended(session: AikiSession, cwd: PathBuf) -> Result<HookResult> {
    debug_log(|| "No autoreply generated - ending session automatically");

    let session_ended_payload = AikiSessionEndedPayload {
        session,
        cwd,
        timestamp: chrono::Utc::now(),
    };

    dispatch(AikiEvent::SessionEnded(session_ended_payload))
}

/// Transform shell.permission_asked to delete.permission_asked for rm/rmdir commands
///
/// Called when shell command is detected as a delete operation.
fn transform_shell_to_delete_permission_asked(
    shell_event: crate::events::AikiShellPermissionAskedPayload,
    paths: Vec<String>,
) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "Transforming shell.permission_asked (rm/rmdir) to delete.permission_asked: {:?}",
            paths
        )
    });

    let delete_event = AikiDeletePermissionAskedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        file_paths: paths,
    };

    events::handle_delete_permission_asked(delete_event)
}

/// Transform shell.completed to delete.completed for rm/rmdir commands
///
/// Called when shell command is detected as a delete operation.
fn transform_shell_to_delete_completed(
    shell_event: crate::events::AikiShellCompletedPayload,
    paths: Vec<String>,
) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "Transforming shell.completed (rm/rmdir) to delete.completed: {:?}",
            paths
        )
    });

    let delete_event = AikiDeleteCompletedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        file_paths: paths,
        success: Some(shell_event.success),
    };

    events::handle_delete_completed(delete_event)
}

use crate::cache::{debug_log, DEBUG_ENABLED};
use crate::error::Result;
use crate::events::result::HookResult;
use crate::events::{self, AikiEvent, AikiSessionEndedPayload};
use crate::session::AikiSession;
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
            // File access (unified model)
            AikiEvent::FilePermissionAsked(_) => "file.permission_asked",
            AikiEvent::FileCompleted(_) => "file.completed",
            // File changes (deprecated)
            AikiEvent::ChangePermissionAsked(_) => "change.permission_asked",
            AikiEvent::ChangeCompleted(_) => "change.completed",
            // Shell commands
            AikiEvent::ShellPermissionAsked(_) => "shell.permission_asked",
            AikiEvent::ShellCompleted(_) => "shell.completed",
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

        // File access (unified model)
        AikiEvent::FilePermissionAsked(e) => events::handle_file_permission_asked(e),
        AikiEvent::FileCompleted(e) => events::handle_file_completed(e),

        // File changes (deprecated - keeping for backward compatibility)
        AikiEvent::ChangePermissionAsked(e) => events::handle_change_permission_asked(e),
        AikiEvent::ChangeCompleted(e) => events::handle_change_completed(e),

        // Shell commands
        AikiEvent::ShellPermissionAsked(e) => events::handle_shell_permission_asked(e),
        AikiEvent::ShellCompleted(e) => events::handle_shell_completed(e),

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

use crate::error::Result;
use crate::events::result::HookResult;
use crate::events::{self, AikiEvent, AikiSessionEndPayload};
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

    // Log event for debugging (can be controlled by verbosity flag in future)
    if std::env::var("AIKI_DEBUG").is_ok() {
        let event_type_name = match &event {
            AikiEvent::SessionStart(_) => "SessionStart",
            AikiEvent::PrePrompt(_) => "PrePrompt",
            AikiEvent::PreFileChange(_) => "PreFileChange",
            AikiEvent::PostFileChange(_) => "PostFileChange",
            AikiEvent::PostResponse(_) => "PostResponse",
            AikiEvent::SessionEnd(_) => "SessionEnd",
            AikiEvent::PrepareCommitMessage(_) => "PrepareCommitMessage",
            AikiEvent::Unsupported => "Unsupported",
        };
        eprintln!(
            "[aiki] Dispatching event: {} from agent: {:?}",
            event_type_name,
            event.agent_type()
        );
    }

    // Route to appropriate handler
    let result = match event {
        AikiEvent::SessionStart(e) => events::handle_start(e),
        AikiEvent::PrePrompt(e) => events::handle_pre_prompt(e),
        AikiEvent::PreFileChange(e) => events::handle_pre_file_change(e),
        AikiEvent::PostFileChange(e) => events::handle_post_file_change(e),
        AikiEvent::PostResponse(e) => {
            // Extract fields we'll need for SessionEnd before consuming the event
            let session = e.session.clone();
            let cwd = e.cwd.clone();

            // Handle PostResponse and check for autoreply
            let response = events::handle_post_response(e)?;

            // If PostResponse produced an autoreply, return it (session continues)
            if response.has_context() {
                return Ok(response);
            }

            // No autoreply - session is done, trigger SessionEnd event
            trigger_session_end(session, cwd)
        }
        AikiEvent::SessionEnd(e) => events::handle_session_end(e),
        AikiEvent::PrepareCommitMessage(e) => events::handle_prepare_commit_message(e),
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

/// Trigger a SessionEnd event
///
/// Called automatically when PostResponse doesn't generate an autoreply.
fn trigger_session_end(session: AikiSession, cwd: PathBuf) -> Result<HookResult> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] No autoreply generated - ending session automatically");
    }

    let session_end_payload = AikiSessionEndPayload {
        session,
        cwd,
        timestamp: chrono::Utc::now(),
    };

    dispatch(AikiEvent::SessionEnd(session_end_payload))
}

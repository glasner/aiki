use crate::error::Result;
use crate::events::response::HookResponse;
use crate::events::{self, AikiEvent};

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and handlers return generic
/// HookResponse objects that can be translated to editor-specific formats.
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    // Handle unsupported events immediately
    if matches!(event, AikiEvent::Unsupported) {
        return Ok(HookResponse::success());
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
            let mut response = events::handle_post_response(e)?;

            // If PostResponse didn't produce an autoreply, the session is done
            // Automatically fire SessionEnd event for cleanup
            if !response.has_context() {
                if std::env::var("AIKI_DEBUG").is_ok() {
                    eprintln!("[aiki] No autoreply generated - ending session automatically");
                }

                let session_end_event = crate::events::AikiSessionEndEvent {
                    session,
                    cwd,
                    timestamp: chrono::Utc::now(),
                };

                // Dispatch SessionEnd event recursively through this dispatcher
                // Gracefully degrade on SessionEnd errors - log but don't fail PostResponse
                match dispatch(AikiEvent::SessionEnd(session_end_event)) {
                    Ok(session_end_response) => {
                        // If SessionEnd blocked, propagate that decision
                        if session_end_response.is_blocking() {
                            response.decision = session_end_response.decision;
                        }

                        // Merge SessionEnd failures into PostResponse (after checking is_blocking)
                        response.failures.extend(session_end_response.failures);
                    }
                    Err(e) => {
                        // SessionEnd failed entirely - log but don't fail PostResponse
                        eprintln!("Warning: SessionEnd dispatch failed: {}", e);
                        response
                            .failures
                            .push(crate::events::response::Failure(format!(
                                "SessionEnd cleanup failed: {}",
                                e
                            )));
                    }
                }
            }

            Ok(response)
        }
        AikiEvent::SessionEnd(e) => events::handle_session_end(e),
        AikiEvent::PrepareCommitMessage(e) => events::handle_prepare_commit_message(e),
        AikiEvent::Unsupported => return Ok(HookResponse::success()),
    };

    // If handler fails, return a failure response instead of propagating error
    match result {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("Warning: Aiki event handler failed: {}", e);
            Ok(HookResponse::failure(format!("Aiki handler failed: {}", e)))
        }
    }
}

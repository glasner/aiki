use crate::error::Result;
use crate::events::AikiEvent;
use crate::handlers::{self, HookResponse};

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and handlers return generic
/// HookResponse objects that can be translated to editor-specific formats.
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    // Log event for debugging (can be controlled by verbosity flag in future)
    if std::env::var("AIKI_DEBUG").is_ok() {
        let event_type_name = match &event {
            AikiEvent::SessionStart(_) => "SessionStart",
            AikiEvent::PostChange(_) => "PostChange",
            AikiEvent::PrepareCommitMessage(_) => "PrepareCommitMessage",
        };
        eprintln!(
            "[aiki] Dispatching event: {} from agent: {:?}",
            event_type_name,
            event.agent_type()
        );
    }

    // Route to appropriate handler
    let result = match event {
        AikiEvent::SessionStart(e) => handlers::handle_start(e),
        AikiEvent::PostChange(e) => handlers::handle_post_change(e),
        AikiEvent::PrepareCommitMessage(e) => handlers::handle_prepare_commit_message(e),
    };

    // If handler fails, return a failure response instead of propagating error
    match result {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("Warning: Aiki event handler failed: {}", e);
            Ok(HookResponse::failure(
                format!("Aiki handler failed: {}", e),
                Some("Event processing failed, but operation continues".to_string()),
            ))
        }
    }
}

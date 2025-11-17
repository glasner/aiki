use crate::error::Result;
use crate::events::{AikiEvent, AikiEventType};
use crate::handlers;

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and all errors are caught
/// to prevent hooks from blocking editor operation.
pub fn dispatch(event: AikiEvent) -> Result<()> {
    // Log event for debugging (can be controlled by verbosity flag in future)
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Dispatching event: {:?} from agent: {:?}",
            event.event_type, event.agent
        );
    }

    // Route to appropriate handler
    let result = match event.event_type {
        AikiEventType::Start => handlers::handle_start(event),
        AikiEventType::PostChange => handlers::handle_post_change(event),
        AikiEventType::PreCommit => handlers::handle_pre_commit(event),
        AikiEventType::Stop => handlers::handle_stop(event),
    };

    // Never propagate errors to editor hooks - just log and continue
    if let Err(e) = &result {
        eprintln!("Warning: Aiki event handler failed: {}", e);
        // Return Ok to prevent blocking editor
        return Ok(());
    }

    result
}

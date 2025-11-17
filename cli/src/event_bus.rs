use crate::error::Result;
use crate::events::AikiEvent;
use crate::handlers;

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and all errors are caught
/// to prevent hooks from blocking editor operation.
pub fn dispatch(event: AikiEvent) -> Result<()> {
    // Log event for debugging (can be controlled by verbosity flag in future)
    if std::env::var("AIKI_DEBUG").is_ok() {
        let event_type_name = match &event {
            AikiEvent::Start(_) => "Start",
            AikiEvent::PostChange(_) => "PostChange",
            AikiEvent::PreCommit(_) => "PreCommit",
        };
        eprintln!(
            "[aiki] Dispatching event: {} from agent: {:?}",
            event_type_name,
            event.agent_type()
        );
    }

    // Route to appropriate handler
    let result = match event {
        AikiEvent::Start(e) => handlers::handle_start(e),
        AikiEvent::PostChange(e) => handlers::handle_post_change(e),
        AikiEvent::PreCommit(e) => handlers::handle_pre_commit(e),
    };

    // Never propagate errors to editor hooks - just log and continue
    if let Err(e) = &result {
        eprintln!("Warning: Aiki event handler failed: {}", e);
        // Return Ok to prevent blocking editor
        return Ok(());
    }

    result
}

use anyhow::Result;

mod events;
mod output;
mod session;
mod tools;

use events::build_aiki_event_from_stdin;
use output::build_command_output;

/// Handle a Claude Code event
///
/// This is the vendor-specific handler for Claude Code hooks.
/// Parses the payload once and dispatches to event-specific handlers.
///
/// # Arguments
/// * `claude_event_name` - Vendor event name from CLI flag (used for output formatting)
pub fn handle(claude_event_name: &str) -> Result<()> {
    use crate::events::{AikiEvent, AikiSessionStartPayload};

    // Build Aiki event from stdin JSON
    let aiki_event = build_aiki_event_from_stdin()?;

    // Claude Code's /clear fires SessionEnd without a subsequent SessionStart,
    // so we synthesize a session.started after the end to keep it visible.
    let clear_restart = if let AikiEvent::SessionEnded(ref e) = aiki_event {
        if e.reason == "clear" {
            Some(AikiSessionStartPayload {
                session: e.session.clone(),
                cwd: e.cwd.clone(),
                timestamp: chrono::Utc::now(),
            })
        } else {
            None
        }
    } else {
        None
    };

    // Dispatch the primary event
    let aiki_response = crate::event_bus::dispatch(aiki_event)?;

    // If /clear, start a new session immediately
    if let Some(start_payload) = clear_restart {
        let _ = crate::event_bus::dispatch(AikiEvent::SessionStarted(start_payload));
    }

    let hook_output = build_command_output(aiki_response, claude_event_name);

    hook_output.print_and_exit();
}

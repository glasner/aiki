use anyhow::Result;

mod events;
mod output;
mod session;
mod tools;

use events::{build_aiki_event, ClaudeEvent};
use output::build_command_output;

/// Handle a Claude Code event
///
/// This is the vendor-specific handler for Claude Code hooks.
/// Parses the payload once and dispatches to event-specific handlers.
///
/// # Arguments
/// * `claude_event_name` - Vendor event name from CLI flag (used for output formatting)
pub fn handle(claude_event_name: &str) -> Result<()> {
    // Parse event - serde discriminates by hook_event_name
    let claude_event: ClaudeEvent = super::read_stdin_json()?;

    // Build Aiki event from Claude event
    let aiki_event = build_aiki_event(claude_event);

    // Dispatch event and exit with command output
    let aiki_response = crate::event_bus::dispatch(aiki_event)?;
    let hook_output = build_command_output(aiki_response, claude_event_name);

    hook_output.print_and_exit();
}

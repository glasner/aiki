use anyhow::Result;

mod events;
mod output;
mod session;
mod tools;

use events::build_aiki_event_from_stdin;
use output::build_command_output;

/// Handle a Cursor event
///
/// This is the vendor-specific handler for Cursor hooks.
/// Parses the payload once and dispatches to event-specific handlers.
///
/// # Arguments
/// * `cursor_event_name` - Vendor event name from CLI flag (used for output formatting)
pub fn handle(cursor_event_name: &str) -> Result<()> {
    // Build Aiki event from stdin JSON
    let aiki_event = build_aiki_event_from_stdin()?;

    // Dispatch event and exit with command output
    let aiki_response = crate::event_bus::dispatch(aiki_event)?;
    let hook_output = build_command_output(aiki_response, cursor_event_name);

    hook_output.print_and_exit();
}

mod events;
mod output;
pub mod otel;
pub mod session;

use crate::error::Result;

use events::build_aiki_event_from_stdin;
use output::build_command_output;

/// Handle a Codex native hook event (stdin-based)
///
/// Entry point for `aiki hooks stdin --agent codex --event <event_name>`.
/// Reads structured JSON from stdin, builds an AikiEvent, dispatches it,
/// and formats the response for Codex's hook protocol.
///
/// For `source: "clear"` on SessionStart, Codex only fires SessionStart
/// (no preceding SessionEnd), so re-injection is handled directly by the
/// SessionCleared event handler.
pub fn handle_stdin(codex_event_name: &str) -> Result<()> {
    // Normalize camelCase → PascalCase for output module compatibility
    let normalized_event = match codex_event_name {
        "sessionStart" => "SessionStart",
        "userPromptSubmit" => "UserPromptSubmit",
        "preToolUse" => "PreToolUse",
        "stop" => "Stop",
        other => other,
    };

    // Build Aiki event from stdin JSON
    let aiki_event = build_aiki_event_from_stdin()?;

    // Dispatch the primary event
    let aiki_response = crate::event_bus::dispatch(aiki_event)?;

    let hook_output = build_command_output(aiki_response, normalized_event);

    hook_output.print_and_exit();
}

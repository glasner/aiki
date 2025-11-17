use crate::error::Result;
use crate::events::{AikiPostChangeEvent, AikiPrepareCommitMessageEvent, AikiStartEvent};
use crate::flows::{AikiState, FlowExecutor};

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(event: AikiStartEvent) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session started by {:?}", event.agent_type);
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute Start actions from the core flow
    // This ensures the repository is properly initialized
    FlowExecutor::execute_actions(&core_flow.start, &mut state)?;

    Ok(())
}

/// Handle post-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_post_change(event: AikiPostChangeEvent) -> Result<()> {
    // No validation needed - all required fields are guaranteed by type system

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Recording change by {:?}, session: {}, tool: {}",
            event.agent_type, event.session_id, event.tool_name
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostChange actions from the core flow
    // The flow will call the native build_description function
    FlowExecutor::execute_actions(&core_flow.post_change, &mut state)?;

    Ok(())
}

/// Handle prepare-commit-msg event (Git's prepare-commit-msg hook)
///
/// Executes the PrepareCommitMessage flow section to modify the commit message.
/// Typically used for adding co-author attributions, but can add any content.
/// Called from Git's prepare-commit-msg hook via `aiki event prepare-commit-msg`.
pub fn handle_prepare_commit_message(event: AikiPrepareCommitMessageEvent) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Preparing commit message");
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PrepareCommitMessage actions from the core flow
    // The flow will call the native generate_coauthors function
    FlowExecutor::execute_actions(&core_flow.prepare_commit_message, &mut state)?;

    Ok(())
}

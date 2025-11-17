use crate::error::Result;
use crate::events::{AikiEvent, AikiPostChangeEvent, AikiPreCommitEvent, AikiStartEvent};
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

    // Build execution state from event (wrap in enum)
    let mut state = AikiState::new(AikiEvent::Start(event));

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

    // Build execution state from event (wrap in enum)
    let mut state = AikiState::new(AikiEvent::PostChange(event));

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostChange actions from the core flow
    // The flow will call the native build_description function
    FlowExecutor::execute_actions(&core_flow.post_change, &mut state)?;

    Ok(())
}

/// Handle pre-commit event (before Git commit)
///
/// Generates AI co-author attributions for the commit message.
/// Called from Git's prepare-commit-msg hook.
pub fn handle_pre_commit(_event: AikiPreCommitEvent) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Generating co-authors for commit");
    }

    // Currently: aiki authors --format=git --changes=staged
    // This generates Co-authored-by: lines for AI agents

    // TODO: This should call the authors generation logic directly
    // For now, this is a placeholder that will be called from the git hook
    // which already has the logic to append co-authors

    Ok(())
}

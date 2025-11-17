use crate::events::AikiEvent;
use crate::flows::{ExecutionContext, FlowExecutor};
use anyhow::{anyhow, Result};

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(event: AikiEvent) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session started by {:?}", event.agent);
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution context
    let mut context = ExecutionContext::new(event.cwd.clone());

    // Set flow name for self.* function resolution
    context.flow_name = Some("aiki/core".to_string());

    // Add event variables
    context
        .event_vars
        .insert("agent".to_string(), format!("{:?}", event.agent));

    if let Some(session_id) = &event.session_id {
        context
            .event_vars
            .insert("session_id".to_string(), session_id.to_string());
    }

    // Execute Start actions from the core flow
    // This ensures the repository is properly initialized
    FlowExecutor::execute_actions(&core_flow.start, &mut context)?;

    Ok(())
}

/// Handle post-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_post_change(event: AikiEvent) -> Result<()> {
    // Extract required metadata
    let session_id = event
        .session_id
        .ok_or_else(|| anyhow!("post-change event requires session_id"))?;

    let tool_name = event
        .metadata
        .get("tool_name")
        .ok_or_else(|| anyhow!("post-change event requires tool_name in metadata"))?;

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Recording change by {:?}, session: {}, tool: {}",
            event.agent, session_id, tool_name
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution context with event variables
    let mut context = ExecutionContext::new(event.cwd.clone());

    // Set flow name for self.* function resolution
    context.flow_name = Some("aiki/core".to_string());

    // Add event variables - the native function will use these
    context
        .event_vars
        .insert("agent".to_string(), format!("{:?}", event.agent));
    context
        .event_vars
        .insert("session_id".to_string(), session_id.to_string());
    context
        .event_vars
        .insert("tool_name".to_string(), tool_name.to_string());

    // Add file_path if available
    if let Some(file_path) = event.metadata.get("file_path") {
        context
            .event_vars
            .insert("file_path".to_string(), file_path.clone());
    }

    // Execute PostChange actions from the core flow
    // The flow will call the native build_description function
    FlowExecutor::execute_actions(&core_flow.post_change, &mut context)?;

    Ok(())
}

/// Handle pre-commit event (before Git commit)
///
/// Generates AI co-author attributions for the commit message.
/// Called from Git's prepare-commit-msg hook.
pub fn handle_pre_commit(_event: AikiEvent) -> Result<()> {
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

/// Handle session stop event
///
/// Not yet implemented - no reliable trigger mechanism from editors.
/// Future: Session cleanup, summary generation, analytics upload.
pub fn handle_stop(event: AikiEvent) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Session stop requested by {:?} (not yet implemented)",
            event.agent
        );
    }

    // Not yet implemented - no reliable way to detect session end
    eprintln!("Warning: stop event not yet implemented");
    Ok(())
}

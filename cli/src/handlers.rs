use crate::events::AikiEvent;
use anyhow::{anyhow, Result};

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(event: AikiEvent) -> Result<()> {
    // Currently: aiki init --quiet
    // This ensures the repository is initialized when a session starts

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session started by {:?}", event.agent);
    }

    // Call init logic (this should be refactored to use internal API, not shell command)
    // For now, we'll just ensure the .aiki directory exists
    let aiki_dir = event.cwd.join(".aiki");
    if !aiki_dir.exists() {
        std::fs::create_dir_all(&aiki_dir)?;
    }

    Ok(())
}

/// Handle post-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description.
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

    // Call existing record_change with agent from event
    crate::record_change::record_change(event.agent, &session_id, tool_name)?;

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

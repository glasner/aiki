use crate::authors::{AuthorScope, AuthorsCommand, OutputFormat};
use crate::error::Result;
use crate::events::AikiPreCommitEvent;
use crate::flows::state::ActionResult;
use anyhow::Context;

/// Generate co-authors for Git commit from staged changes
///
/// This function is called during PreCommit events to generate Git trailer
/// lines (Co-authored-by:) for AI agents that contributed to the staged changes.
pub fn generate_coauthors(event: &AikiPreCommitEvent) -> Result<ActionResult> {
    // Create authors command using the event's working directory
    let authors_cmd = AuthorsCommand::new(&event.cwd);

    // Get authors from Git staged changes in Git trailer format
    let coauthors = authors_cmd
        .get_authors(AuthorScope::GitStaged, OutputFormat::Git)
        .context("Failed to get co-authors from staged changes")?;

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: coauthors,
        stderr: String::new(),
    })
}

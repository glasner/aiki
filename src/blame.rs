use anyhow::{Context, Result};
use jj_lib::annotate::FileAnnotator;
use jj_lib::object_id::ObjectId;
use jj_lib::repo::{Repo, StoreFactories};
use jj_lib::repo_path::RepoPath;
use jj_lib::revset::RevsetExpression;
use jj_lib::workspace::{default_working_copy_factories, Workspace};
use std::path::{Path, PathBuf};

use crate::jj::JJWorkspace;
use crate::provenance::{AgentType, AttributionConfidence, ProvenanceRecord};

/// Line-by-line attribution information for a file
#[derive(Debug, Clone)]
pub struct LineAttribution {
    pub line_number: usize,
    pub line_text: String,
    pub change_id: String,
    pub commit_id: String,
    pub agent_type: AgentType,
    pub confidence: Option<AttributionConfidence>,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
}

/// Command to show line-by-line blame/attribution for a file
pub struct BlameCommand {
    repo_path: PathBuf,
}

impl BlameCommand {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    /// Get line-by-line attribution for a file
    pub fn blame_file(&self, file_path: &Path) -> Result<Vec<LineAttribution>> {
        // Load the workspace and repository
        let settings = JJWorkspace::create_user_settings()?;
        let store_factories = StoreFactories::default();
        let working_copy_factories = default_working_copy_factories();

        let workspace = Workspace::load(
            &settings,
            &self.repo_path,
            &store_factories,
            &working_copy_factories,
        )
        .context("Failed to load JJ workspace")?;

        let repo = workspace
            .repo_loader()
            .load_at_head()
            .context("Failed to load repository")?;

        // Convert path to RepoPath first (needed for searching)
        let path_str = file_path
            .to_str()
            .context("File path contains invalid UTF-8")?;
        let repo_path =
            RepoPath::from_internal_string(path_str).context("Invalid repository path")?;

        // Find the latest change with content (from git import or recorded changes)
        // Use the heads of the repo - these are the latest changes
        let heads = repo.view().heads();
        let head_ids: Vec<_> = heads.iter().collect();

        if head_ids.is_empty() {
            anyhow::bail!("No heads found in repository");
        }

        // Try each head until we find one that has the file
        let mut commit_to_use = None;
        for head_id in &head_ids {
            let commit = repo.store().get_commit(head_id)?;
            let tree = commit.tree()?;

            let file_value = tree.path_value(repo_path)?;
            if file_value.is_present() {
                commit_to_use = Some(commit);
                break;
            }
        }

        let commit_to_use = commit_to_use
            .context("File not found in any head changes. Has the file been tracked in jj?")?;

        // Create file annotator using from_commit
        let mut file_annotator = FileAnnotator::from_commit(&commit_to_use, repo_path)
            .context("Failed to create file annotator")?;

        // Create a revset expression for all changes (the default domain for blame)
        // We use "all()" to include all changes in the repository
        let revset_expr = RevsetExpression::all();

        // Compute the annotations
        file_annotator
            .compute(repo.as_ref(), &revset_expr)
            .context("Failed to compute annotations")?;

        // Convert to FileAnnotation to get line-by-line data
        let file_annotation = file_annotator.to_annotation();

        // Get annotations using lines() method
        let mut attributions = Vec::new();
        let mut line_num = 1;

        // Process each annotated line
        // lines() returns Iterator<Item = (Result<&CommitId, &CommitId>, &BStr)>
        for (commit_id_result, line_text) in file_annotation.lines() {
            // Extract the commit_id (either Ok or Err variant both contain CommitId)
            let commit_id = match commit_id_result {
                Ok(id) => id,
                Err(id) => id, // Err means the line couldn't be attributed to a single change
            };
            // Load the commit object (represents a change in jj) to get its change_id and description
            let commit = repo.store().get_commit(&commit_id)?;
            let change_id = commit.change_id(); // The stable change identifier
            let description = commit.description();

            // Parse provenance metadata from description
            // If parsing fails (malformed metadata), treat as human commit
            let provenance = ProvenanceRecord::from_description(description).unwrap_or(None);

            let attribution = match provenance {
                Some(prov) => {
                    // AI-generated line
                    LineAttribution {
                        line_number: line_num,
                        line_text: line_text.to_string(),
                        change_id: change_id.hex(),
                        commit_id: commit_id.hex(),
                        agent_type: prov.agent.agent_type,
                        confidence: Some(prov.agent.confidence),
                        session_id: Some(prov.session_id),
                        tool_name: Some(prov.tool_name),
                    }
                }
                None => {
                    // Human-generated line (no aiki metadata)
                    LineAttribution {
                        line_number: line_num,
                        line_text: line_text.to_string(),
                        change_id: change_id.hex(),
                        commit_id: commit_id.hex(),
                        agent_type: AgentType::Unknown, // Could add AgentType::Human variant
                        confidence: None,
                        session_id: None,
                        tool_name: None,
                    }
                }
            };

            attributions.push(attribution);
            line_num += 1;
        }

        Ok(attributions)
    }

    /// Format blame output in git-blame style
    pub fn format_blame(&self, attributions: &[LineAttribution]) -> String {
        let mut output = String::new();

        for attr in attributions {
            // Format: commit_id (agent session confidence) line_num| line_text
            let agent_str = format!("{:?}", attr.agent_type);
            let session_str = attr
                .session_id
                .as_ref()
                .map(|s| truncate_session_id(s))
                .unwrap_or_else(|| "-".to_string());
            let confidence_str = attr
                .confidence
                .as_ref()
                .map(|c| format!("{:?}", c))
                .unwrap_or_else(|| "-".to_string());

            // Truncate commit ID to 8 chars for readability
            let short_commit = &attr.commit_id[..8.min(attr.commit_id.len())];

            output.push_str(&format!(
                "{} ({:12} {:12} {:6}) {:4}| {}\n",
                short_commit,
                agent_str,
                session_str,
                confidence_str,
                attr.line_number,
                attr.line_text
            ));
        }

        output
    }
}

/// Truncate session ID for display (keep first 8 chars)
fn truncate_session_id(session_id: &str) -> String {
    if session_id.len() > 12 {
        format!("{}...", &session_id[..9])
    } else {
        session_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_session_id() {
        assert_eq!(truncate_session_id("short"), "short");
        assert_eq!(truncate_session_id("exactly-12ch"), "exactly-12ch");
        assert_eq!(
            truncate_session_id("this-is-a-very-long-session-id-123456"),
            "this-is-a..."
        );
    }

    #[test]
    fn test_line_attribution_creation() {
        let attr = LineAttribution {
            line_number: 1,
            line_text: "fn main() {".to_string(),
            change_id: "abc123".to_string(),
            commit_id: "def456".to_string(),
            agent_type: AgentType::ClaudeCode,
            confidence: Some(AttributionConfidence::High),
            session_id: Some("session-123".to_string()),
            tool_name: Some("Edit".to_string()),
        };

        assert_eq!(attr.line_number, 1);
        assert_eq!(attr.line_text, "fn main() {");
    }
}

//! Built-in function: aiki/core.build_metadata
//!
//! Builds complete provenance metadata (message + author) from event context.
//!
//! # Validation
//! Required event variables (agent, session_id, tool_name) are validated
//! at the handler level before flow execution. This function expects them
//! to be present.

use crate::error::Result;
use crate::events::AikiPostFileChangeEvent;
use crate::flows::state::ActionResult;
use crate::provenance::ProvenanceRecord;
use std::process::Command;

/// Get the git user (name + email) from git config
///
/// Returns the user in "Name <email>" format, or None if git config is not set.
fn get_git_user() -> Option<String> {
    let name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())?;

    let email = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())?;

    Some(format!("{} <{}>", name, email))
}

/// Build complete metadata (message + author) from event context
///
/// This function returns both the commit message and author in a single call,
/// avoiding duplicate event field access. The output is JSON for easy parsing
/// with native field access syntax.
///
/// # Context-Aware Behavior
/// If the flow context contains `$detection.classification_type == "OverlappingUserEdits"`,
/// this function will add a coauthor field to the provenance record using the git user.
///
/// # Required Event Variables
/// - `$event.agent_type` - Agent type
/// - `$event.session_id` - Session identifier
/// - `$event.tool_name` - Tool name
///
/// # Optional Context Variables
/// - `$detection.classification_type` - Edit classification type
///
/// # Returns
/// An ActionResult with JSON output: `{"author": "...", "message": "..."}`
///
/// # Example Flow Usage
/// ```yaml
/// PostFileChange:
///   - let: detection = self.classify_edits
///   - let: metadata = self.build_metadata
///     on_failure: stop
///   - jj: metaedit -m "$metadata.message" --author "$metadata.author"
/// ```
pub fn build_metadata(
    event: &AikiPostFileChangeEvent,
    context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    let mut provenance = ProvenanceRecord::from_post_file_change_event(event);

    // Check if we have overlapping user edits and should add coauthor
    if let Some(ctx) = context {
        if let Some(detection) = ctx.get_variable("detection") {
            // Try to parse the detection JSON to check classification_type
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(detection) {
                if let Some(classification_type) =
                    json.get("classification_type").and_then(|v| v.as_str())
                {
                    if classification_type == "OverlappingUserEdits" {
                        // Add coauthor from git config
                        provenance.coauthor = get_git_user();
                    }
                }
            }
        }
    }

    let message = provenance.to_description();
    let author = event.agent_type.git_author();

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[flows/core] Generated metadata - author: {}, message length: {}",
            author,
            message.len()
        );
    }

    // Return JSON output for structured data
    let json = serde_json::json!({
        "author": author,
        "message": message,
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AikiPostFileChangeEvent;
    use crate::provenance::AgentType;

    #[test]
    fn test_build_metadata_with_claude() {
        let event = AikiPostFileChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session-123".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/tmp/file.rs".to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![],
        };

        let result = build_metadata(&event, None).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));

        // Parse JSON output
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Claude <noreply@anthropic.com>");
        assert!(json["message"].as_str().unwrap().contains("[aiki]"));
        assert!(json["message"].as_str().unwrap().contains("author=claude"));
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("session=test-session-123"));
    }

    #[test]
    fn test_build_metadata_with_cursor() {
        let event = AikiPostFileChangeEvent {
            agent_type: AgentType::Cursor,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "cursor-session".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/tmp/file.rs".to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![],
        };

        let result = build_metadata(&event, None).unwrap();

        assert!(result.success);
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Cursor <noreply@cursor.com>");
        assert!(json["message"].as_str().unwrap().contains("author=cursor"));
    }
}

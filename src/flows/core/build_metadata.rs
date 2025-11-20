//! Built-in function: aiki/core.build_metadata
//!
//! Builds complete provenance metadata (message + author) from event context.
//!
//! # Validation
//! Required event variables (agent, session_id, tool_name) are validated
//! at the handler level before flow execution. This function expects them
//! to be present.

use crate::error::Result;
use crate::events::AikiPostChangeEvent;
use crate::flows::state::ActionResult;
use crate::provenance::ProvenanceRecord;

/// Build complete metadata (message + author) from event context
///
/// This function returns both the commit message and author in a single call,
/// avoiding duplicate event field access. The output is JSON for easy parsing
/// with native field access syntax.
///
/// # Required Event Variables
/// - `$event.agent_type` - Agent type
/// - `$event.session_id` - Session identifier
/// - `$event.tool_name` - Tool name
///
/// # Returns
/// An ActionResult with JSON output: `{"author": "...", "message": "..."}`
///
/// # Example Flow Usage
/// ```yaml
/// PostChange:
///   - let: metadata = self.build_metadata
///     on_failure: stop
///   - jj: metaedit -m "$metadata.message" --author "$metadata.author"
/// ```
pub fn build_metadata(event: &AikiPostChangeEvent) -> Result<ActionResult> {
    let provenance = ProvenanceRecord::from_post_change_event(event);
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
    use crate::events::AikiPostChangeEvent;
    use crate::provenance::AgentType;

    #[test]
    fn test_build_metadata_with_claude() {
        let event = AikiPostChangeEvent {
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
        };

        let result = build_metadata(&event).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));

        // Parse JSON output
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Claude <noreply@anthropic.com>");
        assert!(json["message"].as_str().unwrap().contains("[aiki]"));
        assert!(json["message"].as_str().unwrap().contains("agent=claude"));
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("session=test-session-123"));
    }

    #[test]
    fn test_build_metadata_with_cursor() {
        let event = AikiPostChangeEvent {
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
        };

        let result = build_metadata(&event).unwrap();

        assert!(result.success);
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Cursor <noreply@cursor.com>");
        assert!(json["message"].as_str().unwrap().contains("agent=cursor"));
    }
}

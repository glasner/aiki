//! Built-in function: aiki/core.build_description
//!
//! Builds a provenance description from event context.
//!
//! # Validation
//! Required event variables (agent, session_id, tool_name) are validated
//! at the handler level before flow execution. This function expects them
//! to be present.

use crate::error::Result;
use crate::events::AikiPostChangeEvent;
use crate::flows::state::ActionResult;
use crate::provenance::ProvenanceRecord;

/// Build a provenance description from event context
///
/// This function extracts agent information, session ID, and tool name from the
/// execution context's event variables, creates a ProvenanceRecord, and formats
/// it into the [aiki]...[/aiki] description block format.
///
/// # Required Event Variables
/// - `$event.agent_type` - Agent type (e.g., "ClaudeCode", "Cursor")
/// - `$event.session_id` - Session identifier for grouping related changes
/// - `$event.tool_name` - Name of the tool that made the change (e.g., "Edit", "Write")
///
/// These variables are validated at the handler level before flow execution.
///
/// # Returns
/// An ActionResult with the formatted provenance description in stdout.
///
/// # Example Flow Usage
/// ```yaml
/// PostChange:
///   - let: description = self.build_description
///     on_failure: stop
///   - jj: describe -m "$description"
/// ```
pub fn build_description(event: &AikiPostChangeEvent) -> Result<ActionResult> {
    // Build provenance record from PostChange event
    let provenance = ProvenanceRecord::from_post_change_event(event);

    // Generate description in [aiki]...[/aiki] format
    let description = provenance.to_description();

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[flows/core] Generated provenance description");
    }

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: description,
        stderr: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AikiPostChangeEvent;
    use crate::provenance::AgentType;

    #[test]
    fn test_build_description_with_claude_code() {
        let event = AikiPostChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session-123".to_string(),
            tool_name: "Edit".to_string(),
            file_path: "/tmp/file.rs".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
        };

        let result = build_description(&event).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("[aiki]"));
        assert!(result.stdout.contains("agent=claude-code"));
        assert!(result.stdout.contains("session=test-session-123"));
        assert!(result.stdout.contains("tool=Edit"));
        assert!(result.stdout.contains("confidence=High"));
        assert!(result.stdout.contains("method=Hook"));
        assert!(result.stdout.contains("[/aiki]"));
    }
}

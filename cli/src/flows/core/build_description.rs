//! Built-in function: aiki/core.build_description
//!
//! Builds a provenance description from event context.
//!
//! # Validation
//! Required event variables (agent, session_id, tool_name) are validated
//! at the handler level before flow execution. This function expects them
//! to be present.

use crate::error::Result;
use crate::flows::state::{ActionResult, AikiState};
use crate::provenance::{AgentInfo, AttributionConfidence, DetectionMethod, ProvenanceRecord};

/// Build a provenance description from event context
///
/// This function extracts agent information, session ID, and tool name from the
/// execution context's event variables, creates a ProvenanceRecord, and formats
/// it into the [aiki]...[/aiki] description block format.
///
/// # Required Event Variables
/// - `$event.agent` - Agent type (e.g., "ClaudeCode", "Cursor")
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
///     on_failure: fail
///   - jj: describe -m "$description"
/// ```
pub fn build_description(aiki: &AikiState) -> Result<ActionResult> {
    // Event variables are validated at the handler level before flow execution
    let agent_type = aiki.agent_type();
    let session_id = aiki
        .event
        .session_id
        .as_ref()
        .expect("session_id should be set by handler");
    let tool_name = aiki
        .event
        .metadata
        .get("tool_name")
        .expect("tool_name should be set by handler");

    // Build provenance record
    let provenance = ProvenanceRecord {
        agent: AgentInfo {
            agent_type,
            version: None,
            detected_at: chrono::Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        session_id: session_id.clone(),
        tool_name: tool_name.clone(),
    };

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
    use crate::events::{AikiEvent, AikiEventType};
    use crate::provenance::AgentType;

    #[test]
    fn test_build_description_with_claude_code() {
        let event = AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, "/tmp")
            .with_session_id("test-session-123")
            .with_metadata("tool_name", "Edit");
        let context = AikiState::new(event);

        let result = build_description(&context).unwrap();

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

    #[test]
    #[should_panic(expected = "session_id should be set by handler")]
    fn test_build_description_missing_session_id() {
        // This test verifies that missing event variables cause a panic
        // since they should always be validated at the handler level.
        // A panic here indicates a programming error, not a user error.
        let event = AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, "/tmp")
            .with_metadata("tool_name", "Edit");
        // Note: session_id is intentionally missing
        let context = AikiState::new(event);

        // This should panic because session_id is missing
        let _ = build_description(&context);
    }
}

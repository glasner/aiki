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
///     on_failure: fail
///   - jj: describe -m "$description"
/// ```
pub fn build_description(aiki: &AikiState) -> Result<ActionResult> {
    // Extract PostChange event (type system guarantees this is only called for PostChange)
    let event = match &aiki.event {
        crate::events::AikiEvent::PostChange(e) => e,
        _ => panic!("build_description should only be called for PostChange events"),
    };

    // Build provenance record
    let provenance = ProvenanceRecord {
        agent: AgentInfo {
            agent_type: event.agent_type,
            version: None,
            detected_at: event.timestamp,
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        session_id: event.session_id.clone(),
        tool_name: event.tool_name.clone(),
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
    use crate::events::{AikiEvent, AikiPostChangeEvent};
    use crate::provenance::AgentType;

    #[test]
    fn test_build_description_with_claude_code() {
        let event = AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: AgentType::ClaudeCode,
            session_id: "test-session-123".to_string(),
            tool_name: "Edit".to_string(),
            file_path: "/tmp/file.rs".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
        });
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
    #[should_panic(expected = "build_description should only be called for PostChange events")]
    fn test_build_description_missing_session_id() {
        // This test verifies that build_description panics when called with wrong event type
        // Type system now guarantees PostChange events have session_id, so we test with Start
        use crate::events::AikiStartEvent;

        let event = AikiEvent::Start(AikiStartEvent {
            agent_type: AgentType::ClaudeCode,
            session_id: None,
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
        });
        let context = AikiState::new(event);

        // This should panic because Start event was passed instead of PostChange
        let _ = build_description(&context);
    }
}

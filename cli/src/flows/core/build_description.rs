//! Built-in function: aiki/core.build_description
//!
//! Builds a provenance description from event context.
//!
//! # Validation
//! Required event variables (agent, session_id, tool_name) are validated
//! at the handler level before flow execution. This function expects them
//! to be present.

use crate::error::Result;
use crate::flows::types::{ActionResult, ExecutionContext};
use crate::provenance::{
    AgentInfo, AgentType, AttributionConfidence, DetectionMethod, ProvenanceRecord,
};

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
pub fn build_description(context: &ExecutionContext) -> Result<ActionResult> {
    // Event variables are validated at the handler level before flow execution
    let agent_str = &context.event_vars["agent"];
    let session_id = &context.event_vars["session_id"];
    let tool_name = &context.event_vars["tool_name"];

    // Parse agent type from serialized string (e.g., "claude-code")
    let agent_type = match agent_str.as_str() {
        "claude-code" => AgentType::ClaudeCode,
        "cursor" => AgentType::Cursor,
        _ => AgentType::Unknown,
    };

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

    #[test]
    fn test_build_description_with_claude_code() {
        let mut context = ExecutionContext::new("/tmp");
        context
            .event_vars
            .insert("agent".to_string(), "claude-code".to_string());
        context
            .event_vars
            .insert("session_id".to_string(), "test-session-123".to_string());
        context
            .event_vars
            .insert("tool_name".to_string(), "Edit".to_string());

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
    #[should_panic(expected = "no entry found for key")]
    fn test_build_description_missing_agent() {
        // This test verifies that missing event variables cause a panic
        // since they should always be validated at the handler level.
        // A panic here indicates a programming error, not a user error.
        let mut context = ExecutionContext::new("/tmp");
        context
            .event_vars
            .insert("session_id".to_string(), "test".to_string());
        context
            .event_vars
            .insert("tool_name".to_string(), "Edit".to_string());

        // This should panic because agent is missing
        let _ = build_description(&context);
    }
}

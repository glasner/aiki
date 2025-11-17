//! Built-in function: aiki/core.build_description
//!
//! Builds a provenance description from event context.

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
    // Extract required event variables
    let agent_str = context
        .event_vars
        .get("agent")
        .ok_or_else(|| anyhow::anyhow!("Missing event variable: $event.agent"))?;

    let session_id = context
        .event_vars
        .get("session_id")
        .ok_or_else(|| anyhow::anyhow!("Missing event variable: $event.session_id"))?;

    let tool_name = context
        .event_vars
        .get("tool_name")
        .ok_or_else(|| anyhow::anyhow!("Missing event variable: $event.tool_name"))?;

    // Parse agent type
    let agent_type = match agent_str.as_str() {
        "ClaudeCode" => AgentType::ClaudeCode,
        "Cursor" => AgentType::Cursor,
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
            .insert("agent".to_string(), "ClaudeCode".to_string());
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
    fn test_build_description_missing_agent() {
        let mut context = ExecutionContext::new("/tmp");
        context
            .event_vars
            .insert("session_id".to_string(), "test".to_string());
        context
            .event_vars
            .insert("tool_name".to_string(), "Edit".to_string());

        let result = build_description(&context);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing event variable: $event.agent"));
    }
}

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about the AI agent that made a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_type: AgentType,
    pub version: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub confidence: AttributionConfidence,
    pub detection_method: DetectionMethod,
}

/// Type of AI agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentType {
    ClaudeCode,
    Unknown,
}

/// Confidence level of the attribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributionConfidence {
    /// 100% - Hook-based detection
    High,
    /// 70-80% - lsof or directory check (Phase 3)
    Medium,
    /// 40-60% - Heuristic (Phase 3)
    Low,
    /// No detection succeeded
    Unknown,
}

/// Method used to detect the AI agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// Claude Code PostToolUse hook
    Hook,
    /// Fallback (Phase 3)
    Unknown,
}

/// A complete provenance record for a change
///
/// This struct stores only metadata that JJ doesn't know about.
/// File paths, diffs, timestamps, and commit IDs are retrieved from JJ when needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// Information about the agent that made the change
    pub agent: AgentInfo,
    /// Claude Code session ID
    pub session_id: String,
    /// Tool name used (e.g., "Edit" or "Write")
    pub tool_name: String,
}

impl ProvenanceRecord {
    /// Serialize provenance metadata to commit description format
    ///
    /// Format:
    /// ```text
    /// [aiki]
    /// agent=claude-code
    /// session=claude-session-abc123xyz
    /// tool=Edit
    /// confidence=High
    /// method=Hook
    /// [/aiki]
    /// ```
    ///
    /// # Example
    /// ```
    /// use aiki::provenance::*;
    /// use chrono::Utc;
    ///
    /// let record = ProvenanceRecord {
    ///     agent: AgentInfo {
    ///         agent_type: AgentType::ClaudeCode,
    ///         version: None,
    ///         detected_at: Utc::now(),
    ///         confidence: AttributionConfidence::High,
    ///         detection_method: DetectionMethod::Hook,
    ///     },
    ///     session_id: "test-session".to_string(),
    ///     tool_name: "Edit".to_string(),
    /// };
    ///
    /// let description = record.to_description();
    /// assert!(description.contains("[aiki]"));
    /// assert!(description.contains("agent=claude-code"));
    /// ```
    pub fn to_description(&self) -> String {
        let agent_type = match self.agent.agent_type {
            AgentType::ClaudeCode => "claude-code",
            AgentType::Unknown => "unknown",
        };

        let confidence = match self.agent.confidence {
            AttributionConfidence::High => "High",
            AttributionConfidence::Medium => "Medium",
            AttributionConfidence::Low => "Low",
            AttributionConfidence::Unknown => "Unknown",
        };

        let method = match self.agent.detection_method {
            DetectionMethod::Hook => "Hook",
            DetectionMethod::Unknown => "Unknown",
        };

        format!(
            "[aiki]\nagent={}\nsession={}\ntool={}\nconfidence={}\nmethod={}\n[/aiki]",
            agent_type, self.session_id, self.tool_name, confidence, method
        )
    }

    /// Parse provenance metadata from commit description
    ///
    /// Extracts the [aiki]...[/aiki] block and parses key=value pairs.
    pub fn from_description(description: &str) -> Result<Self> {
        // Extract [aiki]...[/aiki] block
        let start_marker = "[aiki]";
        let end_marker = "[/aiki]";

        let start = description
            .find(start_marker)
            .context("Description does not contain [aiki] marker")?;
        let end = description
            .find(end_marker)
            .context("Description does not contain [/aiki] marker")?;

        let aiki_block = &description[start + start_marker.len()..end];

        // Parse key=value pairs
        let mut metadata = HashMap::new();
        for line in aiki_block.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                metadata.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        // Extract and parse fields
        let agent_type = match metadata.get("agent").map(|s| s.as_str()) {
            Some("claude-code") => AgentType::ClaudeCode,
            Some("unknown") => AgentType::Unknown,
            _ => return Err(anyhow::anyhow!("Missing or invalid 'agent' field")),
        };

        let session_id = metadata
            .get("session")
            .context("Missing 'session' field")?
            .clone();

        let tool_name = metadata
            .get("tool")
            .context("Missing 'tool' field")?
            .clone();

        let confidence = match metadata.get("confidence").map(|s| s.as_str()) {
            Some("High") => AttributionConfidence::High,
            Some("Medium") => AttributionConfidence::Medium,
            Some("Low") => AttributionConfidence::Low,
            Some("Unknown") => AttributionConfidence::Unknown,
            _ => return Err(anyhow::anyhow!("Missing or invalid 'confidence' field")),
        };

        let method = match metadata.get("method").map(|s| s.as_str()) {
            Some("Hook") => DetectionMethod::Hook,
            Some("Unknown") => DetectionMethod::Unknown,
            _ => return Err(anyhow::anyhow!("Missing or invalid 'method' field")),
        };

        Ok(ProvenanceRecord {
            agent: AgentInfo {
                agent_type,
                version: None,
                detected_at: Utc::now(), // Timestamp comes from jj commit, not stored here
                confidence,
                detection_method: method,
            },
            session_id,
            tool_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_description() {
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: "test-session-123".to_string(),
            tool_name: "Edit".to_string(),
        };

        let description = record.to_description();

        // Check that all required fields are present
        assert!(description.contains("[aiki]"));
        assert!(description.contains("[/aiki]"));
        assert!(description.contains("agent=claude-code"));
        assert!(description.contains("session=test-session-123"));
        assert!(description.contains("tool=Edit"));
        assert!(description.contains("confidence=High"));
        assert!(description.contains("method=Hook"));
        // Note: timestamp not stored, comes from jj commit
    }

    #[test]
    fn test_from_description() {
        let description = "[aiki]\n\
            agent=claude-code\n\
            session=test-session-456\n\
            tool=Write\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let record = ProvenanceRecord::from_description(description).unwrap();

        assert!(matches!(record.agent.agent_type, AgentType::ClaudeCode));
        assert_eq!(record.session_id, "test-session-456");
        assert_eq!(record.tool_name, "Write");
        assert!(matches!(
            record.agent.confidence,
            AttributionConfidence::High
        ));
        assert!(matches!(
            record.agent.detection_method,
            DetectionMethod::Hook
        ));
    }

    #[test]
    fn test_round_trip() {
        let original = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: "round-trip-test".to_string(),
            tool_name: "Edit".to_string(),
        };

        let description = original.to_description();
        let parsed = ProvenanceRecord::from_description(&description).unwrap();

        assert!(matches!(parsed.agent.agent_type, AgentType::ClaudeCode));
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.tool_name, original.tool_name);
        assert!(matches!(
            parsed.agent.confidence,
            AttributionConfidence::High
        ));
        assert!(matches!(
            parsed.agent.detection_method,
            DetectionMethod::Hook
        ));
    }

    #[test]
    fn test_from_description_missing_field() {
        let description = "[aiki]\n\
            agent=claude-code\n\
            tool=Edit\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing"));
    }

    #[test]
    fn test_from_description_invalid_agent() {
        let description = "[aiki]\n\
            agent=invalid-agent\n\
            session=test\n\
            tool=Edit\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description);
        assert!(result.is_err());
    }
}

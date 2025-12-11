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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentType {
    Claude,
    Codex,
    Cursor,
    Gemini,
    Unknown,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AgentType::Claude => write!(f, "Claude"),
            AgentType::Codex => write!(f, "Codex"),
            AgentType::Cursor => write!(f, "Cursor"),
            AgentType::Gemini => write!(f, "Gemini"),
            AgentType::Unknown => write!(f, "Unknown"),
        }
    }
}

impl AgentType {
    /// Get the lowercase identifier for metadata serialization
    pub fn to_metadata_string(&self) -> &'static str {
        match self {
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::Cursor => "cursor",
            AgentType::Gemini => "gemini",
            AgentType::Unknown => "unknown",
        }
    }

    /// Get the email address for this agent type
    pub fn email(&self) -> &'static str {
        match self {
            AgentType::Claude => "noreply@anthropic.com",
            AgentType::Codex => "noreply@openai.com",
            AgentType::Cursor => "noreply@cursor.com",
            AgentType::Gemini => "noreply@google.com",
            AgentType::Unknown => "noreply@aiki.dev",
        }
    }

    /// Format as a git author string (name + email)
    pub fn git_author(&self) -> String {
        format!("{} <{}>", self, self.email())
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// ACP (Agent Client Protocol) bidirectional proxy
    ACP,
    /// Claude Code PostToolUse hook
    Hook,
    /// Fallback (Phase 3)
    Unknown,
}

/// A complete provenance record for a change
///
/// This struct stores only metadata that JJ doesn't know about.
/// File paths, diffs, timestamps, and change IDs are retrieved from JJ when needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// Information about the agent that made the change
    pub agent: AgentInfo,
    /// Client (IDE) name that connected to the agent (e.g., "zed", "neovim")
    /// This is auto-detected from the ACP InitializeRequest
    pub client_name: Option<String>,
    /// Client (IDE) version (e.g., "0.213.3")
    /// This is auto-detected from the ACP InitializeRequest
    pub client_version: Option<String>,
    /// Agent version (e.g., "0.10.6")
    /// This is auto-detected from the ACP InitializeResponse
    pub agent_version: Option<String>,
    /// Session ID from the agent
    pub session_id: String,
    /// Tool name used (e.g., "Edit" or "Write")
    pub tool_name: String,
    /// Optional human coauthor (for overlapping user edits)
    pub coauthor: Option<String>,
}

impl ProvenanceRecord {
    /// Create a ProvenanceRecord from a PostFileChange event
    ///
    /// This constructor extracts all necessary fields from the event and creates
    /// a provenance record with default confidence (High) and the detection
    /// method from the session.
    pub fn from_post_file_change_event(event: &crate::events::AikiPostFileChangePayload) -> Self {
        Self {
            agent: AgentInfo {
                agent_type: event.session.agent_type(),
                version: event.session.agent_version().map(|s| s.to_string()),
                detected_at: event.timestamp,
                confidence: AttributionConfidence::High,
                detection_method: event.session.detection_method().clone(),
            },
            client_name: event.session.client_name().map(|s| s.to_string()),
            client_version: event.session.client_version().map(|s| s.to_string()),
            agent_version: event.session.agent_version().map(|s| s.to_string()),
            session_id: event.session.external_id().to_string(),
            tool_name: event.tool_name.clone(),
            coauthor: None,
        }
    }

    /// Serialize provenance metadata to change description format
    ///
    /// Note: In jj, every working copy state is a "change" with a stable change_id.
    /// The metadata is stored in the change's description field.
    ///
    /// Format:
    /// ```text
    /// [aiki]
    /// author=claude-code
    /// agent_version=0.10.6
    /// client=zed
    /// client_version=0.213.3
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
    ///         agent_type: AgentType::Claude,
    ///         version: None,
    ///         detected_at: Utc::now(),
    ///         confidence: AttributionConfidence::High,
    ///         detection_method: DetectionMethod::Hook,
    ///     },
    ///     client_name: Some("zed".to_string()),
    ///     client_version: None,
    ///     agent_version: None,
    ///     session_id: "test-session".to_string(),
    ///     tool_name: "Edit".to_string(),
    ///     coauthor: None,
    /// };
    ///
    /// let description = record.to_description();
    /// assert!(description.contains("[aiki]"));
    /// assert!(description.contains("author=claude"));
    /// ```
    pub fn to_description(&self) -> String {
        let agent_type = match self.agent.agent_type {
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::Cursor => "cursor",
            AgentType::Gemini => "gemini",
            AgentType::Unknown => "unknown",
        };

        let confidence = match self.agent.confidence {
            AttributionConfidence::High => "High",
            AttributionConfidence::Medium => "Medium",
            AttributionConfidence::Low => "Low",
            AttributionConfidence::Unknown => "Unknown",
        };

        let method = match self.agent.detection_method {
            DetectionMethod::ACP => "ACP",
            DetectionMethod::Hook => "Hook",
            DetectionMethod::Unknown => "Unknown",
        };

        let mut lines = vec![
            "[aiki]".to_string(),
            format!("author={}", agent_type),
            "author_type=agent".to_string(),
        ];

        if let Some(ref agent_ver) = self.agent_version {
            lines.push(format!("agent_version={}", agent_ver));
        }

        if let Some(ref client) = self.client_name {
            lines.push(format!("client={}", client));
        }

        if let Some(ref client_ver) = self.client_version {
            lines.push(format!("client_version={}", client_ver));
        }

        lines.extend(vec![
            format!("session={}", self.session_id),
            format!("tool={}", self.tool_name),
            format!("confidence={}", confidence),
            format!("method={}", method),
        ]);

        if let Some(ref coauthor) = self.coauthor {
            lines.push(format!("coauthor={}", coauthor));
        }

        lines.push("[/aiki]".to_string());

        lines.join("\n")
    }

    /// Parse provenance metadata from change description
    ///
    /// Returns None if no [aiki] metadata found (human change or pre-aiki change)
    /// Returns Some(record) if valid aiki metadata is found
    pub fn from_description(description: &str) -> Result<Option<Self>> {
        // Look for [aiki]...[/aiki] block
        let start_marker = "[aiki]";
        let end_marker = "[/aiki]";

        let start = match description.find(start_marker) {
            Some(pos) => pos,
            None => return Ok(None), // No aiki metadata - human change
        };

        let end = description
            .find(end_marker)
            .context("Found [aiki] start marker but no [/aiki] end marker")?;

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

        // Extract and parse required fields
        let agent_type = match metadata.get("author").map(|s| s.as_str()) {
            Some("claude") => AgentType::Claude,
            Some("codex") => AgentType::Codex,
            Some("cursor") => AgentType::Cursor,
            Some("gemini") => AgentType::Gemini,
            Some("unknown") => AgentType::Unknown,
            _ => return Err(anyhow::anyhow!("Missing or invalid 'author' field")),
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
            Some("ACP") => DetectionMethod::ACP,
            Some("Hook") => DetectionMethod::Hook,
            Some("Unknown") => DetectionMethod::Unknown,
            _ => return Err(anyhow::anyhow!("Missing or invalid 'method' field")),
        };

        let client_name = metadata.get("client").cloned();
        let client_version = metadata.get("client_version").cloned();
        let agent_version = metadata.get("agent_version").cloned();
        let coauthor = metadata.get("coauthor").cloned();

        Ok(Some(ProvenanceRecord {
            agent: AgentInfo {
                agent_type,
                version: agent_version.clone(),
                detected_at: Utc::now(), // Timestamp comes from jj change, not stored here
                confidence,
                detection_method: method,
            },
            client_name,
            client_version,
            agent_version,
            session_id,
            tool_name,
            coauthor,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_description() {
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session-123".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        // Check that all required fields are present
        assert!(description.contains("[aiki]"));
        assert!(description.contains("[/aiki]"));
        assert!(description.contains("author=claude"));
        assert!(description.contains("session=test-session-123"));
        assert!(description.contains("tool=Edit"));
        assert!(description.contains("confidence=High"));
        assert!(description.contains("method=Hook"));
        // Note: timestamp not stored, comes from jj commit
    }

    #[test]
    fn test_to_description_with_special_characters_in_session_id() {
        // Test that special characters in session ID don't break the format
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "session-with-dashes_underscores.dots".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        assert!(description.contains("session=session-with-dashes_underscores.dots"));
        assert!(description.contains("[aiki]"));
        assert!(description.contains("[/aiki]"));
    }

    #[test]
    fn test_to_description_with_very_long_session_id() {
        // Test with a very long session ID (e.g., UUID + timestamp)
        let long_session_id = "claude-session-".to_string() + &"a".repeat(200);
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: long_session_id.clone(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        assert!(description.contains(&format!("session={}", long_session_id)));
        assert!(description.contains("[aiki]"));
        assert!(description.contains("[/aiki]"));
    }

    #[test]
    fn test_to_description_with_special_tool_names() {
        // Test various tool names that might be used
        let tool_names = vec!["Edit", "Write", "Bash", "Read", "mcp__acp__Edit"];

        for tool_name in tool_names {
            let record = ProvenanceRecord {
                agent: AgentInfo {
                    agent_type: AgentType::Claude,
                    version: None,
                    detected_at: Utc::now(),
                    confidence: AttributionConfidence::High,
                    detection_method: DetectionMethod::Hook,
                },
                client_name: None,
                client_version: None,
                agent_version: None,
                session_id: "test-session".to_string(),
                tool_name: tool_name.to_string(),
                coauthor: None,
            };

            let description = record.to_description();
            assert!(description.contains(&format!("tool={}", tool_name)));
        }
    }

    #[test]
    fn test_to_description_all_agent_types() {
        // Test serialization for all agent types
        let agent_types = vec![
            (AgentType::Claude, "claude"),
            (AgentType::Unknown, "unknown"),
        ];

        for (agent_type, expected_str) in agent_types {
            let record = ProvenanceRecord {
                agent: AgentInfo {
                    agent_type,
                    version: None,
                    detected_at: Utc::now(),
                    confidence: AttributionConfidence::High,
                    detection_method: DetectionMethod::Hook,
                },
                client_name: None,
                client_version: None,
                agent_version: None,
                session_id: "test".to_string(),
                tool_name: "Edit".to_string(),
                coauthor: None,
            };

            let description = record.to_description();
            assert!(description.contains(&format!("author={}", expected_str)));
        }
    }

    #[test]
    fn test_to_description_all_confidence_levels() {
        // Test serialization for all confidence levels
        let confidence_levels = vec![
            (AttributionConfidence::High, "High"),
            (AttributionConfidence::Medium, "Medium"),
            (AttributionConfidence::Low, "Low"),
            (AttributionConfidence::Unknown, "Unknown"),
        ];

        for (confidence, expected_str) in confidence_levels {
            let record = ProvenanceRecord {
                agent: AgentInfo {
                    agent_type: AgentType::Claude,
                    version: None,
                    detected_at: Utc::now(),
                    confidence,
                    detection_method: DetectionMethod::Hook,
                },
                client_name: None,
                client_version: None,
                agent_version: None,
                session_id: "test".to_string(),
                tool_name: "Edit".to_string(),
                coauthor: None,
            };

            let description = record.to_description();
            assert!(description.contains(&format!("confidence={}", expected_str)));
        }
    }

    #[test]
    fn test_to_description_all_detection_methods() {
        // Test serialization for all detection methods
        let methods = vec![
            (DetectionMethod::Hook, "Hook"),
            (DetectionMethod::Unknown, "Unknown"),
        ];

        for (method, expected_str) in methods {
            let record = ProvenanceRecord {
                agent: AgentInfo {
                    agent_type: AgentType::Claude,
                    version: None,
                    detected_at: Utc::now(),
                    confidence: AttributionConfidence::High,
                    detection_method: method,
                },
                client_name: None,
                client_version: None,
                agent_version: None,
                session_id: "test".to_string(),
                tool_name: "Edit".to_string(),
                coauthor: None,
            };

            let description = record.to_description();
            assert!(description.contains(&format!("method={}", expected_str)));
        }
    }

    #[test]
    fn test_to_description_format_structure() {
        // Test that the format has correct structure
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        // Should start with [aiki] and end with [/aiki]
        assert!(description.starts_with("[aiki]\n"));
        assert!(description.ends_with("[/aiki]"));

        // Count newlines - should have one per field plus markers
        let line_count = description.lines().count();
        assert_eq!(line_count, 8); // [aiki], author, author_type, session, tool, confidence, method, [/aiki]
    }

    #[test]
    fn test_to_description_no_equals_in_markers() {
        // Verify that the markers themselves don't contain '='
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        // The first and last lines should be just markers
        let lines: Vec<&str> = description.lines().collect();
        assert_eq!(lines[0], "[aiki]");
        assert_eq!(lines[lines.len() - 1], "[/aiki]");

        // All middle lines should contain '='
        for line in &lines[1..lines.len() - 1] {
            assert!(line.contains('='), "Line '{}' should contain '='", line);
        }
    }

    #[test]
    fn test_empty_session_id() {
        // Test edge case with empty session ID
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();

        // Should still be valid format even with empty session
        assert!(description.contains("session="));
        assert!(description.contains("[aiki]"));
        assert!(description.contains("[/aiki]"));
    }

    #[test]
    fn test_serialization_deserialization_roundtrip() {
        // Test that ProvenanceRecord can be serialized and deserialized
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: Some("1.0.0".to_string()),
                detected_at: Utc::now(),
                confidence: AttributionConfidence::Medium,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "roundtrip-test".to_string(),
            tool_name: "Write".to_string(),
            coauthor: None,
        };

        // Test JSON serialization
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ProvenanceRecord = serde_json::from_str(&json).unwrap();

        // Verify key fields match
        assert_eq!(record.session_id, deserialized.session_id);
        assert_eq!(record.tool_name, deserialized.tool_name);
    }

    #[test]
    fn test_from_description_with_aiki_metadata() {
        let description = "[aiki]\n\
            author=claude\n\
            author_type=agent\n\
            session=test-session-456\n\
            tool=Write\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description).unwrap();
        assert!(result.is_some());

        let record = result.unwrap();
        assert!(matches!(record.agent.agent_type, AgentType::Claude));
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
    fn test_from_description_without_aiki_metadata() {
        // Human commit - no aiki metadata
        let description = "Fix bug in parser\n\nThis commit fixes issue #123";

        let result = ProvenanceRecord::from_description(description).unwrap();
        assert!(result.is_none(), "Expected None for human commit");
    }

    #[test]
    fn test_from_description_with_extra_content() {
        // Commit with both aiki metadata and regular description
        let description = "Add new feature\n\n\
            This is a longer description.\n\n\
            [aiki]\n\
            author=claude\n\
            author_type=agent\n\
            session=abc123\n\
            tool=Edit\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]\n\n\
            Additional notes here.";

        let result = ProvenanceRecord::from_description(description).unwrap();
        assert!(result.is_some());

        let record = result.unwrap();
        assert_eq!(record.session_id, "abc123");
    }

    #[test]
    fn test_from_description_round_trip() {
        let original = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Claude,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "round-trip".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = original.to_description();
        let parsed = ProvenanceRecord::from_description(&description)
            .unwrap()
            .unwrap();

        assert!(matches!(parsed.agent.agent_type, AgentType::Claude));
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.tool_name, original.tool_name);
        assert!(matches!(
            parsed.agent.confidence,
            AttributionConfidence::High
        ));
    }

    #[test]
    fn test_from_description_missing_field() {
        let description = "[aiki]\n\
            author=claude-code\n\
            author_type=agent\n\
            tool=Edit\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing"));
    }

    #[test]
    fn test_from_description_invalid_agent() {
        let description = "[aiki]\n\
            author=invalid-agent\n\
            author_type=agent\n\
            session=test\n\
            tool=Edit\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description);
        assert!(result.is_err());
    }

    #[test]
    fn test_cursor_agent_type_serialization() {
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Cursor,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "cursor-session-123".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();
        assert!(description.contains("author=cursor"));
        assert!(description.contains("session=cursor-session-123"));
    }

    #[test]
    fn test_cursor_agent_type_deserialization() {
        let description = "[aiki]\n\
            author=cursor\n\
            author_type=agent\n\
            session=cursor-test-session\n\
            tool=Edit\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description).unwrap();
        assert!(result.is_some());
        let record = result.unwrap();
        assert!(matches!(record.agent.agent_type, AgentType::Cursor));
        assert_eq!(record.session_id, "cursor-test-session");
    }

    #[test]
    fn test_cursor_agent_type_round_trip() {
        let original = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Cursor,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "cursor-roundtrip".to_string(),
            tool_name: "Write".to_string(),
            coauthor: None,
        };

        let description = original.to_description();
        let parsed = ProvenanceRecord::from_description(&description)
            .unwrap()
            .unwrap();

        assert!(matches!(parsed.agent.agent_type, AgentType::Cursor));
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.tool_name, original.tool_name);
    }

    #[test]
    fn test_codex_agent_type_serialization() {
        let record = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Codex,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "codex-session-123".to_string(),
            tool_name: "Edit".to_string(),
            coauthor: None,
        };

        let description = record.to_description();
        assert!(description.contains("author=codex"));
        assert!(description.contains("session=codex-session-123"));
    }

    #[test]
    fn test_codex_agent_type_deserialization() {
        let description = "[aiki]\n\
            author=codex\n\
            author_type=agent\n\
            session=codex-test-session\n\
            tool=Edit\n\
            confidence=High\n\
            method=Hook\n\
            [/aiki]";

        let result = ProvenanceRecord::from_description(description).unwrap();
        assert!(result.is_some());
        let record = result.unwrap();
        assert!(matches!(record.agent.agent_type, AgentType::Codex));
        assert_eq!(record.session_id, "codex-test-session");
    }

    #[test]
    fn test_codex_agent_type_round_trip() {
        let original = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::Codex,
                version: None,
                detected_at: Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "codex-roundtrip".to_string(),
            tool_name: "Write".to_string(),
            coauthor: None,
        };

        let description = original.to_description();
        let parsed = ProvenanceRecord::from_description(&description)
            .unwrap()
            .unwrap();

        assert!(matches!(parsed.agent.agent_type, AgentType::Codex));
        assert_eq!(parsed.session_id, original.session_id);
        assert_eq!(parsed.tool_name, original.tool_name);
    }
}

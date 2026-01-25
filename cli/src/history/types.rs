//! Core types for the prompt history system

use chrono::{DateTime, Utc};

// Re-export AgentType from canonical location
pub use crate::agents::AgentType;

/// Branch name for storing conversation history
pub const CONVERSATIONS_BRANCH: &str = "aiki/conversations";

/// Metadata block start marker
pub const METADATA_START: &str = "[aiki-conversation]";

/// Metadata block end marker
pub const METADATA_END: &str = "[/aiki-conversation]";

/// Events stored on aiki/conversations branch
#[derive(Debug, Clone)]
pub enum ConversationEvent {
    /// User prompt submitted
    Prompt {
        session_id: String,
        agent_type: AgentType,
        content: String,
        /// References to injected context files (not full content)
        injected_refs: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    /// Agent response received
    Response {
        session_id: String,
        agent_type: AgentType,
        /// Files written/modified during this response
        files_written: Vec<String>,
        /// Summary of the response (first paragraph)
        summary: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Session started
    SessionStart {
        session_id: String,
        agent_type: AgentType,
        timestamp: DateTime<Utc>,
    },
    /// Session ended
    SessionEnd {
        session_id: String,
        timestamp: DateTime<Utc>,
        /// Reason for termination (e.g., "clear", "logout", "ttl_expired", "pid_dead")
        reason: String,
    },
}

/// Materialized session view (computed from events)
#[derive(Debug, Clone)]
#[allow(dead_code)] // Part of history API
pub struct Session {
    pub id: String,
    pub agent_type: AgentType,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    /// First line of first prompt (for display)
    pub summary: Option<String>,
}

/// Materialized log entry (from response events)
#[derive(Debug, Clone)]
#[allow(dead_code)] // Part of history API
pub struct LogEntry {
    pub session_id: String,
    pub agent_type: AgentType,
    pub files_written: Vec<String>,
    pub summary: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display() {
        assert_eq!(AgentType::ClaudeCode.to_string(), "claude-code");
        assert_eq!(AgentType::Cursor.to_string(), "cursor");
        assert_eq!(AgentType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_agent_type_from_str() {
        assert_eq!(AgentType::from_str("claude-code"), Some(AgentType::ClaudeCode));
        assert_eq!(AgentType::from_str("CURSOR"), Some(AgentType::Cursor));
        assert_eq!(AgentType::from_str("unknown"), Some(AgentType::Unknown));
        assert_eq!(AgentType::from_str("invalid"), None);
    }
}

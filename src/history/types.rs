//! Core types for the prompt history system

use chrono::{DateTime, Utc};
use std::fmt;

/// Branch name for storing conversation history
pub const CONVERSATIONS_BRANCH: &str = "aiki/conversations";

/// Metadata block start marker
pub const METADATA_START: &str = "[aiki-conversation]";

/// Metadata block end marker
pub const METADATA_END: &str = "[/aiki-conversation]";

/// Agent type that generated the conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentType {
    ClaudeCode,
    Cursor,
    Gemini,
    Other(String),
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentType::ClaudeCode => write!(f, "claude-code"),
            AgentType::Cursor => write!(f, "cursor"),
            AgentType::Gemini => write!(f, "gemini"),
            AgentType::Other(s) => write!(f, "{}", s),
        }
    }
}

impl AgentType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude-code" => AgentType::ClaudeCode,
            "cursor" => AgentType::Cursor,
            "gemini" => AgentType::Gemini,
            _ => AgentType::Other(s.to_string()),
        }
    }
}

/// How intent was derived for a response
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentSource {
    /// User explicitly tagged intent in prompt
    ExplicitTag,
    /// Extracted from agent's response summary
    AgentSummary,
    /// First line of user's prompt
    PromptFirstLine,
    /// Fallback: derived from file actions
    FileAction,
}

impl fmt::Display for IntentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntentSource::ExplicitTag => write!(f, "explicit_tag"),
            IntentSource::AgentSummary => write!(f, "agent_summary"),
            IntentSource::PromptFirstLine => write!(f, "prompt_first_line"),
            IntentSource::FileAction => write!(f, "file_action"),
        }
    }
}

impl IntentSource {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "explicit_tag" => Some(IntentSource::ExplicitTag),
            "agent_summary" => Some(IntentSource::AgentSummary),
            "prompt_first_line" => Some(IntentSource::PromptFirstLine),
            "file_action" => Some(IntentSource::FileAction),
            _ => None,
        }
    }
}

/// Events stored on aiki/conversations branch
#[derive(Debug, Clone)]
pub enum ConversationEvent {
    /// User prompt submitted
    Prompt {
        session_id: String,
        turn: u32,
        agent_type: AgentType,
        content: String,
        /// References to injected context files (not full content)
        injected_refs: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    /// Agent response received
    Response {
        session_id: String,
        turn: u32,
        agent_type: AgentType,
        /// First JJ change_id in this turn (for linking to code)
        first_change_id: Option<String>,
        /// Last JJ change_id in this turn (defines revset range)
        last_change_id: Option<String>,
        /// Short summary of what was done (the "intent")
        intent: Option<String>,
        /// How intent was derived
        intent_source: Option<IntentSource>,
        /// Response duration in milliseconds
        duration_ms: Option<u64>,
        /// Files read during this turn
        files_read: Vec<String>,
        /// Files written/modified during this turn
        files_written: Vec<String>,
        /// Tools used during this turn
        tools_used: Vec<String>,
        /// Summary of the response
        summary: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Session started
    SessionStart {
        session_id: String,
        agent_type: AgentType,
        /// If resuming a previous session
        resume_from: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Session ended
    SessionEnd {
        session_id: String,
        total_turns: u32,
        timestamp: DateTime<Utc>,
    },
}

/// Materialized session view (computed from events)
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub agent_type: AgentType,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub turn_count: u32,
    /// First line of first prompt (for display)
    pub summary: Option<String>,
}

/// Materialized log entry (from response events)
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub session_id: String,
    pub turn: u32,
    pub agent_type: AgentType,
    pub intent: Option<String>,
    pub files_written: Vec<String>,
    pub first_change_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display() {
        assert_eq!(AgentType::ClaudeCode.to_string(), "claude-code");
        assert_eq!(AgentType::Cursor.to_string(), "cursor");
        assert_eq!(AgentType::Other("custom".to_string()).to_string(), "custom");
    }

    #[test]
    fn test_agent_type_from_str() {
        assert_eq!(AgentType::from_str("claude-code"), AgentType::ClaudeCode);
        assert_eq!(AgentType::from_str("CURSOR"), AgentType::Cursor);
        assert_eq!(
            AgentType::from_str("unknown"),
            AgentType::Other("unknown".to_string())
        );
    }

    #[test]
    fn test_intent_source_roundtrip() {
        assert_eq!(
            IntentSource::from_str("agent_summary"),
            Some(IntentSource::AgentSummary)
        );
        assert_eq!(
            IntentSource::AgentSummary.to_string(),
            "agent_summary"
        );
    }
}

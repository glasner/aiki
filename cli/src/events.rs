use crate::provenance::AgentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Session start event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiStartEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Post-change event (after file modification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostChangeEvent {
    pub agent_type: AgentType,
    pub session_id: String, // Required for PostChange events
    pub tool_name: String,  // Tool that made the change (e.g., "Edit", "Write")
    pub file_path: String,  // File that was modified
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Pre-commit event (before Git commit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPreCommitEvent {
    pub agent_type: AgentType,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Core event types in the Aiki system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AikiEvent {
    /// Session initialization (maps to SessionStart, beforeSubmitPrompt)
    Start(AikiStartEvent),
    /// After file modification (maps to PostToolUse, afterFileEdit)
    PostChange(AikiPostChangeEvent),
    /// Before Git commit (prepare-commit-msg hook)
    PreCommit(AikiPreCommitEvent),
}

impl AikiEvent {
    /// Get the working directory for this event
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            Self::Start(e) => &e.cwd,
            Self::PostChange(e) => &e.cwd,
            Self::PreCommit(e) => &e.cwd,
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::Start(e) => e.agent_type,
            Self::PostChange(e) => e.agent_type,
            Self::PreCommit(e) => e.agent_type,
        }
    }

    /// Get the timestamp for this event
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Start(e) => e.timestamp,
            Self::PostChange(e) => e.timestamp,
            Self::PreCommit(e) => e.timestamp,
        }
    }

    /// Get the session ID if present
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Start(e) => e.session_id.as_deref(),
            Self::PostChange(e) => Some(&e.session_id),
            Self::PreCommit(_) => None,
        }
    }
}

// Implement Into<AikiEvent> for each event type to enable ergonomic construction
impl From<AikiStartEvent> for AikiEvent {
    fn from(event: AikiStartEvent) -> Self {
        AikiEvent::Start(event)
    }
}

impl From<AikiPostChangeEvent> for AikiEvent {
    fn from(event: AikiPostChangeEvent) -> Self {
        AikiEvent::PostChange(event)
    }
}

impl From<AikiPreCommitEvent> for AikiEvent {
    fn from(event: AikiPreCommitEvent) -> Self {
        AikiEvent::PreCommit(event)
    }
}

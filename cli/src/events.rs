use crate::provenance::AgentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Core event types in the Aiki system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AikiEventType {
    /// Session initialization (maps to SessionStart, beforeSubmitPrompt)
    Start,
    /// After file modification (maps to PostToolUse, afterFileEdit)
    PostChange,
    /// Before Git commit (prepare-commit-msg hook)
    PreCommit,
    /// Session cleanup (not yet implemented)
    Stop,
}

/// Standardized event structure passed through the event bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiEvent {
    /// Type of event
    pub event_type: AikiEventType,
    /// Agent that triggered this event (embedded by vendor handler)
    pub agent_type: AgentType,
    /// Optional session ID for grouping related events
    pub session_id: Option<String>,
    /// Working directory where event occurred
    pub cwd: PathBuf,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Additional event-specific metadata
    pub metadata: HashMap<String, String>,
}

impl AikiEvent {
    /// Create a new event
    #[must_use]
    pub fn new(event_type: AikiEventType, agent_type: AgentType, cwd: impl AsRef<Path>) -> Self {
        Self {
            event_type,
            agent_type,
            session_id: None,
            cwd: cwd.as_ref().to_path_buf(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add session ID to event
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add metadata to event
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

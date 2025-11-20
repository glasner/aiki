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
    pub client_name: Option<String>, // IDE name (e.g., "zed", "neovim") from ACP InitializeRequest
    pub client_version: Option<String>, // IDE version (e.g., "0.213.3") from ACP InitializeRequest
    pub agent_version: Option<String>, // Agent version (e.g., "0.10.6") from ACP InitializeResponse
    pub session_id: String,          // Required for PostChange events
    pub tool_name: String,           // Tool that made the change (e.g., "Edit", "Write")
    pub file_paths: Vec<String>,     // Files that were modified (batch support)
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub detection_method: crate::provenance::DetectionMethod, // How the change was detected (ACP, Hook, etc.)
}

/// Prepare commit message event (Git's prepare-commit-msg hook)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPrepareCommitMessageEvent {
    pub agent_type: AgentType,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Path to the commit message file (COMMIT_EDITMSG)
    pub commit_msg_file: Option<PathBuf>,
}

/// Core event types in the Aiki system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AikiEvent {
    /// Session initialization (maps to SessionStart, beforeSubmitPrompt)
    SessionStart(AikiStartEvent),
    /// After file modification (maps to PostToolUse, afterFileEdit)
    PostChange(AikiPostChangeEvent),
    /// Prepare commit message (Git's prepare-commit-msg hook)
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
}

impl AikiEvent {
    /// Get the working directory for this event
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            Self::SessionStart(e) => &e.cwd,
            Self::PostChange(e) => &e.cwd,
            Self::PrepareCommitMessage(e) => &e.cwd,
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::SessionStart(e) => e.agent_type,
            Self::PostChange(e) => e.agent_type,
            Self::PrepareCommitMessage(e) => e.agent_type,
        }
    }
}

// Implement Into<AikiEvent> for each event type to enable ergonomic construction
impl From<AikiStartEvent> for AikiEvent {
    fn from(event: AikiStartEvent) -> Self {
        AikiEvent::SessionStart(event)
    }
}

impl From<AikiPostChangeEvent> for AikiEvent {
    fn from(event: AikiPostChangeEvent) -> Self {
        AikiEvent::PostChange(event)
    }
}

impl From<AikiPrepareCommitMessageEvent> for AikiEvent {
    fn from(event: AikiPrepareCommitMessageEvent) -> Self {
        AikiEvent::PrepareCommitMessage(event)
    }
}

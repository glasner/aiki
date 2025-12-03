use crate::provenance::AgentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Details about an individual edit operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditDetail {
    /// File path that was edited
    pub file_path: String,
    /// The old string that was replaced (empty if this is an insertion)
    pub old_string: String,
    /// The new string that replaced it (empty if this is a deletion)
    pub new_string: String,
}

impl EditDetail {
    /// Create a new EditDetail
    #[must_use]
    pub fn new(
        file_path: impl Into<String>,
        old_string: impl Into<String>,
        new_string: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            old_string: old_string.into(),
            new_string: new_string.into(),
        }
    }
}

/// Session start event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiStartEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Pre-file-change event (before file modification begins)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPreFileChangeEvent {
    pub agent_type: AgentType,
    pub session_id: String,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Post-file-change event (after file modification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostFileChangeEvent {
    pub agent_type: AgentType,
    pub client_name: Option<String>, // IDE name (e.g., "zed", "neovim") from ACP InitializeRequest
    pub client_version: Option<String>, // IDE version (e.g., "0.213.3") from ACP InitializeRequest
    pub agent_version: Option<String>, // Agent version (e.g., "0.10.6") from ACP InitializeResponse
    pub session_id: String,          // Required for PostFileChange events
    pub tool_name: String,           // Tool that made the change (e.g., "Edit", "Write")
    pub file_paths: Vec<String>,     // Files that were modified (batch support)
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub detection_method: crate::provenance::DetectionMethod, // How the change was detected (ACP, Hook, etc.)
    /// Detailed edit operations (old_string -> new_string pairs) for user edit detection
    /// Only populated when the agent/IDE provides this information (ACP Edit tool, hooks)
    #[serde(default)]
    pub edit_details: Vec<EditDetail>,
}

/// Pre-prompt event (before agent sees the user's prompt)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPrePromptEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The original prompt text from the user (immutable)
    pub original_prompt: String,
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

/// Post-response event (after agent completes its response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostResponseEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The agent's original response text (immutable)
    pub response: String,
    /// Files that were modified by the agent during this response
    #[serde(default)]
    pub modified_files: Vec<PathBuf>,
}

/// Core event types in the Aiki system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AikiEvent {
    /// Session initialization (maps to SessionStart, beforeSubmitPrompt)
    SessionStart(AikiStartEvent),
    /// Before agent sees the user's prompt (allows context injection)
    PrePrompt(AikiPrePromptEvent),
    /// Before file modification begins (fired when agent requests permission for file-modifying tools)
    PreFileChange(AikiPreFileChangeEvent),
    /// After file modification (maps to PostToolUse, afterFileEdit)
    PostFileChange(AikiPostFileChangeEvent),
    /// After agent completes its response (allows validation and autoreply)
    PostResponse(AikiPostResponseEvent),
    /// Prepare commit message (Git's prepare-commit-msg hook)
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
}

impl AikiEvent {
    /// Get the working directory for this event
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            Self::SessionStart(e) => &e.cwd,
            Self::PrePrompt(e) => &e.cwd,
            Self::PreFileChange(e) => &e.cwd,
            Self::PostFileChange(e) => &e.cwd,
            Self::PostResponse(e) => &e.cwd,
            Self::PrepareCommitMessage(e) => &e.cwd,
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::SessionStart(e) => e.agent_type,
            Self::PrePrompt(e) => e.agent_type,
            Self::PreFileChange(e) => e.agent_type,
            Self::PostFileChange(e) => e.agent_type,
            Self::PostResponse(e) => e.agent_type,
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

impl From<AikiPrePromptEvent> for AikiEvent {
    fn from(event: AikiPrePromptEvent) -> Self {
        AikiEvent::PrePrompt(event)
    }
}

impl From<AikiPreFileChangeEvent> for AikiEvent {
    fn from(event: AikiPreFileChangeEvent) -> Self {
        AikiEvent::PreFileChange(event)
    }
}

impl From<AikiPostFileChangeEvent> for AikiEvent {
    fn from(event: AikiPostFileChangeEvent) -> Self {
        AikiEvent::PostFileChange(event)
    }
}

impl From<AikiPrepareCommitMessageEvent> for AikiEvent {
    fn from(event: AikiPrepareCommitMessageEvent) -> Self {
        AikiEvent::PrepareCommitMessage(event)
    }
}

impl From<AikiPostResponseEvent> for AikiEvent {
    fn from(event: AikiPostResponseEvent) -> Self {
        AikiEvent::PostResponse(event)
    }
}

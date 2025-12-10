use crate::provenance::AgentType;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Result Types Module (contains HookResult, Decision, Failure)
// ============================================================================

pub mod result;

// ============================================================================
// Main Event Enum
// ============================================================================

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
    /// Post-response (after agent response, allows validation and autoreply)
    PostResponse(AikiPostResponseEvent),
    /// Session end (when agent session ends/disconnects)
    SessionEnd(AikiSessionEndEvent),
    /// Prepare commit message (Git's prepare-commit-msg hook)
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
    /// Unsupported event (unknown events or non-file tools that don't require processing)
    Unsupported,
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
            Self::SessionEnd(e) => &e.cwd,
            Self::PrepareCommitMessage(e) => &e.cwd,
            Self::Unsupported => Path::new("."),
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::SessionStart(e) => e.session.agent_type(),
            Self::PrePrompt(e) => e.session.agent_type(),
            Self::PreFileChange(e) => e.session.agent_type(),
            Self::PostFileChange(e) => e.session.agent_type(),
            Self::PostResponse(e) => e.session.agent_type(),
            Self::SessionEnd(e) => e.session.agent_type(),
            Self::PrepareCommitMessage(e) => e.agent_type,
            Self::Unsupported => AgentType::Unknown,
        }
    }
}

// ============================================================================
// Module Declarations
// ============================================================================

mod post_file_change;
mod post_response;
mod pre_file_change;
mod pre_prompt;
mod prepare_commit_msg;
mod session_end;
mod session_start;

// ============================================================================
// Re-exports (maintains existing import paths)
// ============================================================================

pub use post_file_change::*;
pub use post_response::*;
pub use pre_file_change::*;
pub use pre_prompt::*;
pub use prepare_commit_msg::*;
pub use session_end::*;
pub use session_start::*;

// ============================================================================
// From Trait Implementations (enables vendor .into() pattern)
// ============================================================================

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

impl From<AikiSessionEndEvent> for AikiEvent {
    fn from(event: AikiSessionEndEvent) -> Self {
        AikiEvent::SessionEnd(event)
    }
}

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
///
/// Event naming follows semantic conventions:
/// - `{domain}.{state}` format (e.g., `session.started`, `change.done`)
/// - Past tense for completed actions
/// - `permission_asked` suffix for gateable pre-events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    /// Session initialization - new agent session began
    #[serde(rename = "session.started")]
    SessionStarted(AikiSessionStartPayload),
    /// User submitted a prompt to the agent (allows context injection and blocking)
    #[serde(rename = "prompt.submitted")]
    PromptSubmitted(AikiPromptSubmittedPayload),
    /// Agent is about to modify a file (gateable - can approve/deny)
    #[serde(rename = "change.permission_asked")]
    ChangePermissionAsked(AikiChangePermissionAskedPayload),
    /// Agent finished modifying a file
    #[serde(rename = "change.done")]
    ChangeDone(AikiChangeDonePayload),
    /// Agent finished responding (allows validation and autoreply)
    #[serde(rename = "response.received")]
    ResponseReceived(AikiResponseReceivedPayload),
    /// Agent session terminated
    #[serde(rename = "session.ended")]
    SessionEnded(AikiSessionEndedPayload),
    /// Git's prepare-commit-msg hook fired
    #[serde(rename = "git.prepare_commit_message")]
    GitPrepareCommitMessage(AikiGitPrepareCommitMessagePayload),
    /// Unsupported event (unknown events or non-file tools that don't require processing)
    #[serde(other)]
    Unsupported,
}

impl AikiEvent {
    /// Get the working directory for this event
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            Self::SessionStarted(e) => &e.cwd,
            Self::PromptSubmitted(e) => &e.cwd,
            Self::ChangePermissionAsked(e) => &e.cwd,
            Self::ChangeDone(e) => &e.cwd,
            Self::ResponseReceived(e) => &e.cwd,
            Self::SessionEnded(e) => &e.cwd,
            Self::GitPrepareCommitMessage(e) => &e.cwd,
            Self::Unsupported => Path::new("."),
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::SessionStarted(e) => e.session.agent_type(),
            Self::PromptSubmitted(e) => e.session.agent_type(),
            Self::ChangePermissionAsked(e) => e.session.agent_type(),
            Self::ChangeDone(e) => e.session.agent_type(),
            Self::ResponseReceived(e) => e.session.agent_type(),
            Self::SessionEnded(e) => e.session.agent_type(),
            Self::GitPrepareCommitMessage(e) => e.agent_type,
            Self::Unsupported => AgentType::Unknown,
        }
    }
}

// ============================================================================
// Module Declarations (semantic event names)
// ============================================================================

mod change_done;
mod change_permission_asked;
mod git_prepare_commit_message;
mod prompt_submitted;
mod response_received;
mod session_ended;
mod session_started;

// ============================================================================
// Re-exports
// ============================================================================

pub use change_done::*;
pub use change_permission_asked::*;
pub use git_prepare_commit_message::*;
pub use prompt_submitted::*;
pub use response_received::*;
pub use session_ended::*;
pub use session_started::*;

// ============================================================================
// From Trait Implementations (enables vendor .into() pattern)
// ============================================================================

impl From<AikiSessionStartPayload> for AikiEvent {
    fn from(payload: AikiSessionStartPayload) -> Self {
        AikiEvent::SessionStarted(payload)
    }
}

impl From<AikiPromptSubmittedPayload> for AikiEvent {
    fn from(payload: AikiPromptSubmittedPayload) -> Self {
        AikiEvent::PromptSubmitted(payload)
    }
}

impl From<AikiChangePermissionAskedPayload> for AikiEvent {
    fn from(payload: AikiChangePermissionAskedPayload) -> Self {
        AikiEvent::ChangePermissionAsked(payload)
    }
}

impl From<AikiChangeDonePayload> for AikiEvent {
    fn from(payload: AikiChangeDonePayload) -> Self {
        AikiEvent::ChangeDone(payload)
    }
}

impl From<AikiGitPrepareCommitMessagePayload> for AikiEvent {
    fn from(payload: AikiGitPrepareCommitMessagePayload) -> Self {
        AikiEvent::GitPrepareCommitMessage(payload)
    }
}

impl From<AikiResponseReceivedPayload> for AikiEvent {
    fn from(payload: AikiResponseReceivedPayload) -> Self {
        AikiEvent::ResponseReceived(payload)
    }
}

impl From<AikiSessionEndedPayload> for AikiEvent {
    fn from(payload: AikiSessionEndedPayload) -> Self {
        AikiEvent::SessionEnded(payload)
    }
}

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
/// - `{domain}.{state}` format (e.g., `session.started`, `file.done`)
/// - Past tense for completed actions
/// - `permission_asked` suffix for gateable pre-events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    // ========================================================================
    // Session Lifecycle Events
    // ========================================================================
    /// Session initialization - new agent session began
    #[serde(rename = "session.started")]
    SessionStarted(AikiSessionStartPayload),
    /// Session resumed - continuing a previous session
    #[serde(rename = "session.resumed")]
    SessionResumed(AikiSessionResumedPayload),
    /// Agent session terminated
    #[serde(rename = "session.ended")]
    SessionEnded(AikiSessionEndedPayload),

    // ========================================================================
    // User / Agent Interaction Events
    // ========================================================================
    /// User submitted a prompt to the agent (allows context injection and blocking)
    #[serde(rename = "prompt.submitted")]
    PromptSubmitted(AikiPromptSubmittedPayload),
    /// Agent finished responding (allows validation and autoreply)
    #[serde(rename = "response.received")]
    ResponseReceived(AikiResponseReceivedPayload),

    // ========================================================================
    // File Access Events (unified model)
    // ========================================================================
    /// Agent is about to access a file (gateable - can approve/deny)
    /// Operations: read, write, delete
    #[serde(rename = "file.permission_asked")]
    FilePermissionAsked(AikiFilePermissionAskedPayload),
    /// Agent finished accessing a file
    #[serde(rename = "file.completed")]
    FileCompleted(AikiFileCompletedPayload),

    // ========================================================================
    // File Change Events (deprecated - use file.* instead)
    // ========================================================================
    /// DEPRECATED: Use FilePermissionAsked with operation: Write
    #[serde(rename = "change.permission_asked")]
    ChangePermissionAsked(AikiChangePermissionAskedPayload),
    /// DEPRECATED: Use FileCompleted with operation: Write
    #[serde(rename = "change.completed")]
    ChangeCompleted(AikiChangeCompletedPayload),

    // ========================================================================
    // Shell Command Events
    // ========================================================================
    /// Agent is about to execute a shell command (gateable - the autonomous review wedge)
    #[serde(rename = "shell.permission_asked")]
    ShellPermissionAsked(AikiShellPermissionAskedPayload),
    /// Shell command completed
    #[serde(rename = "shell.completed")]
    ShellCompleted(AikiShellCompletedPayload),

    // ========================================================================
    // MCP Tool Events
    // ========================================================================
    /// Agent is about to call an MCP tool (gateable)
    #[serde(rename = "mcp.permission_asked")]
    McpPermissionAsked(AikiMcpPermissionAskedPayload),
    /// MCP tool call completed
    #[serde(rename = "mcp.completed")]
    McpCompleted(AikiMcpCompletedPayload),

    // ========================================================================
    // Commit Integration Events
    // ========================================================================
    /// Git's prepare-commit-msg hook fired
    #[serde(rename = "commit.message_started")]
    CommitMessageStarted(AikiCommitMessageStartedPayload),

    // ========================================================================
    // Fallback
    // ========================================================================
    /// Unsupported event (unknown events or non-file tools that don't require processing)
    #[serde(other)]
    Unsupported,
}

impl AikiEvent {
    /// Get the working directory for this event
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            // Session lifecycle
            Self::SessionStarted(e) => &e.cwd,
            Self::SessionResumed(e) => &e.cwd,
            Self::SessionEnded(e) => &e.cwd,
            // User / agent interaction
            Self::PromptSubmitted(e) => &e.cwd,
            Self::ResponseReceived(e) => &e.cwd,
            // File access (unified)
            Self::FilePermissionAsked(e) => &e.cwd,
            Self::FileCompleted(e) => &e.cwd,
            // File changes (deprecated)
            Self::ChangePermissionAsked(e) => &e.cwd,
            Self::ChangeCompleted(e) => &e.cwd,
            // Shell commands
            Self::ShellPermissionAsked(e) => &e.cwd,
            Self::ShellCompleted(e) => &e.cwd,
            // MCP tools
            Self::McpPermissionAsked(e) => &e.cwd,
            Self::McpCompleted(e) => &e.cwd,
            // Commit integration
            Self::CommitMessageStarted(e) => &e.cwd,
            // Fallback
            Self::Unsupported => Path::new("."),
        }
    }

    /// Get the agent type for this event
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            // Session lifecycle
            Self::SessionStarted(e) => e.session.agent_type(),
            Self::SessionResumed(e) => e.session.agent_type(),
            Self::SessionEnded(e) => e.session.agent_type(),
            // User / agent interaction
            Self::PromptSubmitted(e) => e.session.agent_type(),
            Self::ResponseReceived(e) => e.session.agent_type(),
            // File access (unified)
            Self::FilePermissionAsked(e) => e.session.agent_type(),
            Self::FileCompleted(e) => e.session.agent_type(),
            // File changes (deprecated)
            Self::ChangePermissionAsked(e) => e.session.agent_type(),
            Self::ChangeCompleted(e) => e.session.agent_type(),
            // Shell commands
            Self::ShellPermissionAsked(e) => e.session.agent_type(),
            Self::ShellCompleted(e) => e.session.agent_type(),
            // MCP tools
            Self::McpPermissionAsked(e) => e.session.agent_type(),
            Self::McpCompleted(e) => e.session.agent_type(),
            // Commit integration
            Self::CommitMessageStarted(e) => e.agent_type,
            // Fallback
            Self::Unsupported => AgentType::Unknown,
        }
    }
}

// ============================================================================
// Module Declarations (semantic event names)
// ============================================================================

// Session lifecycle
mod session_ended;
mod session_resumed;
mod session_started;

// User / agent interaction
mod prompt_submitted;
mod response_received;

// File access (unified model)
mod file_completed;
mod file_permission_asked;

// File changes (deprecated - aliases to file.*)
mod change_completed;
mod change_permission_asked;

// Shell commands
mod shell_completed;
mod shell_permission_asked;

// MCP tools
mod mcp_completed;
mod mcp_permission_asked;

// Commit integration
mod commit_message_started;

// ============================================================================
// Re-exports
// ============================================================================

// Session lifecycle
pub use session_ended::*;
pub use session_resumed::*;
pub use session_started::*;

// User / agent interaction
pub use prompt_submitted::*;
pub use response_received::*;

// File access (unified model)
pub use file_completed::*;
pub use file_permission_asked::*;

// File changes (deprecated - kept for backward compatibility)
pub use change_completed::*;
pub use change_permission_asked::*;

// Shell commands
pub use shell_completed::*;
pub use shell_permission_asked::*;

// MCP tools
pub use mcp_completed::*;
pub use mcp_permission_asked::*;

// Commit integration
pub use commit_message_started::*;

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

impl From<AikiFilePermissionAskedPayload> for AikiEvent {
    fn from(payload: AikiFilePermissionAskedPayload) -> Self {
        AikiEvent::FilePermissionAsked(payload)
    }
}

impl From<AikiFileCompletedPayload> for AikiEvent {
    fn from(payload: AikiFileCompletedPayload) -> Self {
        AikiEvent::FileCompleted(payload)
    }
}

// Deprecated: Use AikiFilePermissionAskedPayload instead
impl From<AikiChangePermissionAskedPayload> for AikiEvent {
    fn from(payload: AikiChangePermissionAskedPayload) -> Self {
        AikiEvent::ChangePermissionAsked(payload)
    }
}

// Deprecated: Use AikiFileCompletedPayload instead
impl From<AikiChangeCompletedPayload> for AikiEvent {
    fn from(payload: AikiChangeCompletedPayload) -> Self {
        AikiEvent::ChangeCompleted(payload)
    }
}

impl From<AikiCommitMessageStartedPayload> for AikiEvent {
    fn from(payload: AikiCommitMessageStartedPayload) -> Self {
        AikiEvent::CommitMessageStarted(payload)
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

impl From<AikiSessionResumedPayload> for AikiEvent {
    fn from(payload: AikiSessionResumedPayload) -> Self {
        AikiEvent::SessionResumed(payload)
    }
}

impl From<AikiShellPermissionAskedPayload> for AikiEvent {
    fn from(payload: AikiShellPermissionAskedPayload) -> Self {
        AikiEvent::ShellPermissionAsked(payload)
    }
}

impl From<AikiShellCompletedPayload> for AikiEvent {
    fn from(payload: AikiShellCompletedPayload) -> Self {
        AikiEvent::ShellCompleted(payload)
    }
}

impl From<AikiMcpPermissionAskedPayload> for AikiEvent {
    fn from(payload: AikiMcpPermissionAskedPayload) -> Self {
        AikiEvent::McpPermissionAsked(payload)
    }
}

impl From<AikiMcpCompletedPayload> for AikiEvent {
    fn from(payload: AikiMcpCompletedPayload) -> Self {
        AikiEvent::McpCompleted(payload)
    }
}

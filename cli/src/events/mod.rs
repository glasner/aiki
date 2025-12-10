use crate::provenance::AgentType;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Response Types (shared by all handlers)
// ============================================================================

/// Failure message type
#[derive(Debug, Clone)]
pub struct Failure(pub String);

/// Decision about how to respond to a hook event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the operation to proceed
    Allow,

    /// Block the operation (error messages are in HookResponse.failures)
    Block,
}

impl Decision {
    /// Check if this decision allows the operation to continue
    #[must_use]
    pub fn is_continue(&self) -> bool {
        matches!(self, Decision::Allow)
    }

    /// Check if this decision is to block the operation
    #[must_use]
    pub fn is_block(&self) -> bool {
        matches!(self, Decision::Block)
    }
}

/// Generic hook response (editor-agnostic)
#[derive(Debug, Clone)]
pub struct HookResponse {
    /// Context string for PrePrompt (modified prompt) or PostResponse (autoreply)
    pub context: Option<String>,

    /// Decision about whether to allow or block the operation
    pub decision: Decision,

    /// Failure messages
    pub failures: Vec<Failure>,
}

impl HookResponse {
    #[must_use]
    pub fn success() -> Self {
        Self {
            context: None,
            decision: Decision::Allow,
            failures: Vec::new(),
        }
    }

    #[must_use]
    pub fn success_with_context(context: impl Into<String>) -> Self {
        Self {
            context: Some(context.into()),
            decision: Decision::Allow,
            failures: Vec::new(),
        }
    }

    #[must_use]
    pub fn failure(user_msg: impl Into<String>) -> Self {
        Self {
            context: None,
            decision: Decision::Allow, // Non-blocking - allow operation
            failures: vec![Failure(user_msg.into())],
        }
    }

    #[must_use]
    pub fn blocking_failure(user_msg: impl Into<String>) -> Self {
        Self {
            context: None,
            decision: Decision::Block,
            failures: vec![Failure(user_msg.into())],
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    #[must_use]
    pub fn with_failure(mut self, msg: impl Into<String>) -> Self {
        self.failures.push(Failure(msg.into()));
        self
    }

    /// Check if this response should block the operation
    #[must_use]
    pub fn is_blocking(&self) -> bool {
        self.decision.is_block()
    }

    /// Check if this response is successful (no blocking and no failures)
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.decision.is_continue() && self.failures.is_empty()
    }

    /// Format failure messages with emoji prefixes
    ///
    /// Converts failures into formatted strings with ❌ emoji prefix.
    ///
    /// These formatted messages can be:
    /// - Shown to user (stderr)
    /// - Combined with context and sent to agent (PrePrompt, PostResponse)
    /// - Used in vendor-specific output (Cursor followup_message, Claude Code reason)
    #[must_use]
    pub fn format_messages(&self) -> String {
        self.failures
            .iter()
            .map(|Failure(s)| format!("❌ {}", s))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Combine formatted messages and context according to Phase 8 architecture
    ///
    /// Returns Some(combined_string) if either messages or context are non-empty,
    /// None if both are empty.
    ///
    /// Used by vendor translators to combine validation messages with context
    /// for events like PrePrompt and PostResponse.
    #[must_use]
    pub fn combined_output(&self) -> Option<String> {
        let formatted_messages = self.format_messages();
        let context = self.context.as_deref().unwrap_or("");

        match (!formatted_messages.is_empty(), !context.is_empty()) {
            (true, true) => Some(format!("{}\n\n{}", formatted_messages, context)),
            (true, false) => Some(formatted_messages),
            (false, true) => Some(context.to_string()),
            (false, false) => None,
        }
    }

    /// Check if this response has meaningful context
    ///
    /// Returns true if the context field contains a non-empty string.
    /// Used by the dispatcher to determine if PostResponse generated an autoreply.
    ///
    /// # Examples
    /// ```
    /// # use aiki::events::HookResponse;
    /// let resp1 = HookResponse::success_with_context("autoreply text");
    /// assert!(resp1.has_context());
    ///
    /// let resp2 = HookResponse::success_with_context("");
    /// assert!(!resp2.has_context());
    ///
    /// let resp3 = HookResponse::success();
    /// assert!(!resp3.has_context());
    /// ```
    #[must_use]
    pub fn has_context(&self) -> bool {
        self.context.as_ref().map_or(false, |s| !s.is_empty())
    }
}

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

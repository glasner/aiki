use crate::error::Result;
use crate::events::{
    AikiPostFileChangeEvent, AikiPostResponseEvent, AikiPreFileChangeEvent, AikiPrePromptEvent,
    AikiPrepareCommitMessageEvent, AikiStartEvent,
};
use crate::flows::{AikiState, FlowEngine, FlowResult};

/// Message type for validation and info
#[derive(Debug, Clone)]
pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
}

/// Decision about how to respond to a hook event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the operation to proceed
    Allow,

    /// Block the operation with an error message
    Block(String),
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
        matches!(self, Decision::Block(_))
    }

    /// Get the error message if this is a Block decision
    #[must_use]
    pub fn block_message(&self) -> Option<&str> {
        match self {
            Decision::Block(msg) => Some(msg),
            Decision::Allow => None,
        }
    }
}

/// Generic hook response (editor-agnostic)
#[derive(Debug, Clone)]
pub struct HookResponse {
    /// Context string for PrePrompt (modified prompt) or PostResponse (autoreply)
    pub context: Option<String>,

    /// Decision about whether to allow or block the operation
    pub decision: Decision,

    /// Validation messages (Info/Warning/Error)
    pub messages: Vec<Message>,
}

impl HookResponse {
    #[must_use]
    pub fn success() -> Self {
        Self {
            context: None,
            decision: Decision::Allow,
            messages: Vec::new(),
        }
    }

    #[must_use]
    pub fn success_with_message(user_msg: impl Into<String>) -> Self {
        Self {
            context: None,
            decision: Decision::Allow,
            messages: vec![Message::Info(user_msg.into())],
        }
    }

    #[must_use]
    pub fn success_with_context(context: impl Into<String>) -> Self {
        Self {
            context: Some(context.into()),
            decision: Decision::Allow,
            messages: Vec::new(),
        }
    }

    #[must_use]
    pub fn failure(user_msg: impl Into<String>, agent_msg: Option<String>) -> Self {
        let mut messages = vec![Message::Error(user_msg.into())];
        if let Some(msg) = agent_msg {
            messages.push(Message::Info(msg));
        }
        Self {
            context: None,
            decision: Decision::Allow, // Non-blocking - allow operation
            messages,
        }
    }

    #[must_use]
    pub fn blocking_failure(user_msg: impl Into<String>, agent_msg: Option<String>) -> Self {
        let user_msg_str = user_msg.into();
        let mut messages = vec![Message::Error(user_msg_str.clone())];
        if let Some(msg) = agent_msg {
            messages.push(Message::Info(msg));
        }
        Self {
            context: None,
            decision: Decision::Block(user_msg_str), // Blocking - includes error message
            messages,
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    #[must_use]
    pub fn with_info(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(Message::Info(msg.into()));
        self
    }

    #[must_use]
    pub fn with_warning(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(Message::Warning(msg.into()));
        self
    }

    #[must_use]
    pub fn with_error(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(Message::Error(msg.into()));
        self
    }

    /// Check if this response should block the operation
    #[must_use]
    pub fn is_blocking(&self) -> bool {
        self.decision.is_block()
    }

    /// Check if this response is successful (no blocking and no messages)
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.decision.is_continue() && self.messages.is_empty()
    }

    /// Format validation messages with emoji prefixes
    ///
    /// Converts messages into formatted strings with emoji prefixes:
    /// - Info: ℹ️
    /// - Warning: ⚠️
    /// - Error: ❌
    ///
    /// These formatted messages can be:
    /// - Shown to user (stderr)
    /// - Combined with context and sent to agent (PrePrompt, PostResponse)
    /// - Used in vendor-specific output (Cursor followup_message, Claude Code reason)
    #[must_use]
    pub fn format_messages(&self) -> String {
        let mut parts = vec![];

        for msg in &self.messages {
            match msg {
                Message::Info(s) => parts.push(format!("ℹ️ {}", s)),
                Message::Warning(s) => parts.push(format!("⚠️ {}", s)),
                Message::Error(s) => parts.push(format!("❌ {}", s)),
            }
        }

        parts.join("\n\n")
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
}

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(event: AikiStartEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session started by {:?}", event.agent_type);
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionStart actions from the core flow
    let (flow_result, _timing) = FlowEngine::execute_actions(&core_flow.session_start, &mut state)?;

    // Extract messages from state
    let messages = state.take_messages();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                messages,
            })
        }
        FlowResult::FailedBlock(msg) => Ok(HookResponse {
            context: None,
            decision: Decision::Block(msg),
            messages,
        }),
    }
}

/// Handle pre-prompt event (before agent sees the user's prompt)
///
/// This event fires before the agent receives the user's prompt, allowing flows
/// to inject additional context (e.g., project conventions, active files, etc.).
/// Returns context via `response.context` and messages via `response.messages`,
/// with graceful degradation on errors.
pub fn handle_pre_prompt(event: AikiPrePromptEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PrePrompt event from {:?}, prompt length: {}",
            event.agent_type,
            event.prompt.len()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PrePrompt actions from the core flow (catch errors for graceful degradation)
    let (flow_result, _timing) =
        match FlowEngine::execute_actions(&core_flow.pre_prompt, &mut state) {
            Ok(result) => result,
            Err(e) => {
                // Flow execution failed - log warning and use original prompt
                eprintln!("⚠️ PrePrompt flow failed: {}", e);
                eprintln!("Continuing with original prompt...\n");
                // Return built context (already initialized with original prompt)
                return Ok(HookResponse {
                    context: Some(state.build_context().unwrap_or_default()),
                    decision: Decision::Allow,
                    messages: state.take_messages(),
                });
            }
        };

    // Extract messages from state
    let messages = state.take_messages();

    // Return response based on flow result (build context string)
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => {
            Ok(HookResponse {
                context: Some(state.build_context().unwrap_or_default()),
                decision: Decision::Allow,
                messages,
            })
        }
        FlowResult::FailedBlock(msg) => {
            // Block the prompt - return exit code 2
            Ok(HookResponse {
                context: None,
                decision: Decision::Block(msg),
                messages,
            })
        }
    }
}

/// Handle pre-file-change event (before file modification begins)
///
/// This event fires when the agent requests permission to modify files.
/// It allows flows to stash user edits before the AI starts making changes,
/// ensuring clean separation between human and AI work.
pub fn handle_pre_file_change(event: AikiPreFileChangeEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PreFileChange event from {:?}, session: {}",
            event.agent_type, event.session_id
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PreFileChange actions from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_actions(&core_flow.pre_file_change, &mut state)?;

    // Extract messages from state
    let messages = state.take_messages();

    // PreFileChange never blocks - always allow
    match flow_result {
        FlowResult::Success
        | FlowResult::FailedContinue(_)
        | FlowResult::FailedStop(_)
        | FlowResult::FailedBlock(_) => Ok(HookResponse {
            context: None,
            decision: Decision::Allow,
            messages,
        }),
    }
}

/// Handle post-file-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_post_file_change(event: AikiPostFileChangeEvent) -> Result<HookResponse> {
    // No validation needed - all required fields are guaranteed by type system

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] Recording change by {:?}, session: {}, tool: {}",
            event.agent_type, event.session_id, event.tool_name
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event (clone for error message)
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostFileChange actions from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_actions(&core_flow.post_file_change, &mut state)?;

    // Extract messages from state
    let messages = state.take_messages();

    // PostFileChange never blocks - always allow
    match flow_result {
        FlowResult::Success
        | FlowResult::FailedContinue(_)
        | FlowResult::FailedStop(_)
        | FlowResult::FailedBlock(_) => Ok(HookResponse {
            context: None,
            decision: Decision::Allow,
            messages,
        }),
    }
}

/// Handle post-response event (after agent completes its response)
///
/// This event fires after the agent finishes generating its response, allowing flows
/// to validate output, detect errors, and optionally send an autoreply to the agent.
/// Returns autoreply via `response.context` and messages via `response.messages`,
/// with graceful degradation on errors.
pub fn handle_post_response(event: AikiPostResponseEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PostResponse event from {:?}, response length: {}",
            event.agent_type,
            event.response.len()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostResponse actions from the core flow (catch errors for graceful degradation)
    let (flow_result, _timing) =
        match FlowEngine::execute_actions(&core_flow.post_response, &mut state) {
            Ok(result) => result,
            Err(e) => {
                // Flow execution failed - log warning and skip autoreply
                eprintln!("\n⚠️ PostResponse flow failed: {}", e);
                eprintln!("No autoreply generated.\n");
                return Ok(HookResponse {
                    context: None,
                    decision: Decision::Allow,
                    messages: state.take_messages(),
                });
            }
        };

    // Extract messages from state
    let messages = state.take_messages();

    // PostResponse never blocks - always allow
    match flow_result {
        FlowResult::Success
        | FlowResult::FailedContinue(_)
        | FlowResult::FailedStop(_)
        | FlowResult::FailedBlock(_) => {
            let context = state.build_context().ok().filter(|s| !s.is_empty());
            Ok(HookResponse {
                context,
                decision: Decision::Allow,
                messages,
            })
        }
    }
}

/// Handle prepare-commit-msg event (Git's prepare-commit-msg hook)
///
/// Executes the PrepareCommitMessage flow section to modify the commit message.
/// Typically used for adding co-author attributions, but can add any content.
/// Called from Git's prepare-commit-msg hook via `aiki event prepare-commit-msg`.
pub fn handle_prepare_commit_message(event: AikiPrepareCommitMessageEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Preparing commit message");
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PrepareCommitMessage actions from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_actions(&core_flow.prepare_commit_message, &mut state)?;

    // Extract messages from state
    let messages = state.take_messages();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                messages,
            })
        }
        FlowResult::FailedBlock(msg) => {
            // Block the commit
            Ok(HookResponse {
                context: None,
                decision: Decision::Block(msg),
                messages,
            })
        }
    }
}

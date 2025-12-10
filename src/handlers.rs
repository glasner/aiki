use crate::error::Result;
use crate::events::{
    AikiPostFileChangeEvent, AikiPostResponseEvent, AikiPreFileChangeEvent, AikiPrePromptEvent,
    AikiPrepareCommitMessageEvent, AikiSessionEndEvent, AikiStartEvent,
};
use crate::flows::{AikiState, FlowEngine, FlowResult};

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
    /// # use aiki::handlers::HookResponse;
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

/// Handle session start event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Future: Session logging, environment validation, user-defined startup hooks.
pub fn handle_start(event: AikiStartEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session started by {:?}", event.session.agent_type());
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionStart statements from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.session_start, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => Ok(HookResponse {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

/// Handle pre-prompt event (before agent sees the user's prompt)
///
/// This event fires before the agent receives the user's prompt, allowing flows
/// to inject additional context (e.g., project conventions, active files, etc.).
/// Returns context via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
pub fn handle_pre_prompt(event: AikiPrePromptEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PrePrompt event from {:?}, prompt length: {}",
            event.session.agent_type(),
            event.prompt.len()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PrePrompt statements from the core flow (catch errors for graceful degradation)
    let (flow_result, _timing) =
        match FlowEngine::execute_statements(&core_flow.pre_prompt, &mut state) {
            Ok(result) => result,
            Err(e) => {
                // Flow execution failed - log warning and use original prompt
                eprintln!("⚠️ PrePrompt flow failed: {}", e);
                eprintln!("Continuing with original prompt...\n");
                // Return built context (already initialized with original prompt)
                return Ok(HookResponse {
                    context: state.build_context(),
                    decision: Decision::Allow,
                    failures: state.take_failures(),
                });
            }
        };

    // Extract failures from state
    let failures = state.take_failures();

    // Return response based on flow result (build context string)
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResponse {
                context: state.build_context(),
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => {
            // Block the prompt - return exit code 2
            Ok(HookResponse {
                context: None,
                decision: Decision::Block,
                failures,
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
            event.session.agent_type(),
            event.session.external_id()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PreFileChange actions from the core flow
    let (_flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.pre_file_change, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // PreFileChange never blocks - always allow
    Ok(HookResponse {
        context: None,
        decision: Decision::Allow,
        failures,
    })
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
            event.session.agent_type(),
            event.session.external_id(),
            event.tool_name
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event (clone for error message)
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute PostFileChange actions from the core flow
    let (_flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.post_file_change, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // PostFileChange never blocks - always allow
    Ok(HookResponse {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

/// Handle post-response event (after agent generates response)
///
/// This event fires when the agent finishes generating its response,
/// allowing flows to validate output, detect errors, and optionally send an autoreply to the agent.
/// Returns autoreply via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
pub fn handle_post_response(event: AikiPostResponseEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PostResponse event from {:?}, response length: {}",
            event.session.agent_type(),
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
    let (_flow_result, _timing) =
        match FlowEngine::execute_statements(&core_flow.post_response, &mut state) {
            Ok(result) => result,
            Err(e) => {
                // Flow execution failed - log warning and skip autoreply
                eprintln!("\n⚠️ PostResponse flow failed: {}", e);
                eprintln!("No autoreply generated.\n");
                return Ok(HookResponse {
                    context: state.build_context(),
                    decision: Decision::Allow,
                    failures: state.take_failures(),
                });
            }
        };

    // Extract failures from state
    let failures = state.take_failures();

    // PostResponse never blocks - always allow
    Ok(HookResponse {
        context: state.build_context(),
        decision: Decision::Allow,
        failures,
    })
}

/// Handle session end event (when agent session ends/disconnects)
///
/// Executes the SessionEnd flow section for user-defined cleanup actions,
/// then cleans up the session file. This event fires when the agent session
/// ends, either explicitly or when PostResponse doesn't generate an autoreply.
pub fn handle_session_end(event: AikiSessionEndEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Session ended by {:?}", event.session.agent_type());
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event.clone());

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute SessionEnd statements from the core flow
    let (flow_result, _timing) =
        FlowEngine::execute_statements(&core_flow.session_end, &mut state)?;

    // Clean up session file (always happens, regardless of flow result)
    event.session.end(&event.cwd)?;

    // Extract failures from state
    let failures = state.take_failures();

    // Translate FlowResult to HookResponse
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => Ok(HookResponse {
            context: None,
            decision: Decision::Block,
            failures,
        }),
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
        FlowEngine::execute_statements(&core_flow.prepare_commit_message, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResponse {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => {
            // Block the commit
            Ok(HookResponse {
                context: None,
                decision: Decision::Block,
                failures,
            })
        }
    }
}

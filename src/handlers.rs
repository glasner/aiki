use crate::error::Result;
use crate::events::{
    AikiPostFileChangeEvent, AikiPostResponseEvent, AikiPreFileChangeEvent, AikiPrePromptEvent,
    AikiPrepareCommitMessageEvent, AikiStartEvent,
};
use crate::flows::{AikiState, FlowEngine, FlowResult};

/// Context to prepend to prompts or autoreplies
#[derive(Debug, Clone)]
pub struct Context {
    /// Prepended to context block
    pub prepend: Option<String>,
    /// Appended to context block
    pub append: Option<String>,
}

impl Context {
    /// Build the context block from prepend/append
    #[must_use]
    pub fn build(&self) -> String {
        match (&self.prepend, &self.append) {
            (Some(pre), Some(app)) => format!("{}\n\n{}", pre, app),
            (Some(pre), None) => pre.clone(),
            (None, Some(app)) => app.clone(),
            (None, None) => String::new(),
        }
    }
}

/// Message type for validation and info
#[derive(Debug, Clone)]
pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
}

/// Generic hook response (editor-agnostic)
#[derive(Debug, Clone)]
pub struct HookResponse {
    /// Success or failure
    pub success: bool,

    /// Message shown to user in editor UI (optional)
    pub user_message: Option<String>,

    /// Message sent to AI agent (optional)
    pub agent_message: Option<String>,

    /// Metadata key-value pairs (optional)
    pub metadata: Vec<(String, String)>,

    /// Exit code override (optional, defaults based on success)
    pub exit_code: Option<i32>,

    /// Validation messages (Info/Warning/Error)
    pub messages: Vec<Message>,

    /// Context to prepend to prompts/autoreplies
    pub context: Option<Context>,
}

impl HookResponse {
    #[must_use]
    pub fn success() -> Self {
        Self {
            success: true,
            user_message: None,
            agent_message: None,
            metadata: Vec::new(),
            exit_code: None,
            messages: Vec::new(),
            context: None,
        }
    }

    #[must_use]
    pub fn success_with_message(user_msg: impl Into<String>) -> Self {
        Self {
            success: true,
            user_message: Some(user_msg.into()),
            agent_message: None,
            metadata: Vec::new(),
            exit_code: None,
            messages: Vec::new(),
            context: None,
        }
    }

    #[must_use]
    pub fn success_with_metadata(metadata: Vec<(String, String)>) -> Self {
        Self {
            success: true,
            user_message: None,
            agent_message: None,
            metadata,
            exit_code: None,
            messages: Vec::new(),
            context: None,
        }
    }

    #[must_use]
    pub fn failure(user_msg: impl Into<String>, agent_msg: Option<String>) -> Self {
        Self {
            success: false,
            user_message: Some(user_msg.into()),
            agent_message: agent_msg,
            metadata: Vec::new(),
            exit_code: Some(0), // Exit 0 for non-blocking failure (shows JSON)
            messages: Vec::new(),
            context: None,
        }
    }

    #[must_use]
    pub fn blocking_failure(user_msg: impl Into<String>, agent_msg: Option<String>) -> Self {
        Self {
            success: false,
            user_message: Some(user_msg.into()),
            agent_message: agent_msg,
            metadata: Vec::new(),
            exit_code: Some(2), // Exit 2 to block operation (shows stderr)
            messages: Vec::new(),
            context: None,
        }
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Vec<(String, String)>) -> Self {
        self.metadata = metadata;
        self
    }

    #[must_use]
    pub fn with_agent_message(mut self, msg: impl Into<String>) -> Self {
        self.agent_message = Some(msg.into());
        self
    }

    #[must_use]
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
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
        self.exit_code.map_or(false, |code| code != 0)
    }

    /// Check if this response is successful
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.success && !self.is_blocking()
    }
}

/// Build agent-visible context from messages + context
pub fn build_agent_context(response: &HookResponse) -> String {
    let mut parts = vec![];

    // Add validation messages
    for msg in &response.messages {
        match msg {
            Message::Info(s) => parts.push(format!("ℹ️ {}", s)),
            Message::Warning(s) => parts.push(format!("⚠️ {}", s)),
            Message::Error(s) => parts.push(format!("❌ {}", s)),
        }
    }

    // Add context
    if let Some(context) = &response.context {
        let context_text = context.build();
        if !context_text.is_empty() {
            parts.push(context_text);
        }
    }

    parts.join("\n\n")
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

    match flow_result {
        FlowResult::Success => Ok(HookResponse::success().with_metadata(vec![
            ("session_initialized".to_string(), "true".to_string()),
            (
                "aiki_version".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            ),
        ])),
        FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(
            "⚠️ Session started with warnings",
        )
        .with_agent_message(format!("Some initialization actions failed: {}", msg))),
        FlowResult::FailedStop(_msg) => {
            // Flow stopped silently - return success (no error to user)
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            // Block session start
            Ok(HookResponse::blocking_failure(
                format!("❌ Failed to initialize session: {}", msg),
                Some("Please run 'aiki init' or 'aiki doctor' to fix setup.".to_string()),
            ))
        }
    }
}

/// Handle pre-prompt event (before agent sees the user's prompt)
///
/// This event fires before the agent receives the user's prompt, allowing flows
/// to inject additional context (e.g., project conventions, active files, etc.).
/// Returns the modified prompt via metadata, with graceful degradation on errors.
pub fn handle_pre_prompt(event: AikiPrePromptEvent) -> Result<HookResponse> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[aiki] PrePrompt event from {:?}, original prompt length: {}",
            event.agent_type,
            event.original_prompt.len()
        );
    }

    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;

    // Build execution state from event
    let mut state = AikiState::new(event);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Extract original prompt for error recovery
    let original_prompt = if let crate::events::AikiEvent::PrePrompt(ref evt) = state.event {
        evt.original_prompt.clone()
    } else {
        String::new()
    };

    // Execute PrePrompt actions from the core flow (catch errors for graceful degradation)
    let (flow_result, _timing) =
        match FlowEngine::execute_actions(&core_flow.pre_prompt, &mut state) {
            Ok(result) => result,
            Err(e) => {
                // Flow execution failed - log warning and use original prompt
                eprintln!("⚠️ PrePrompt flow failed: {}", e);
                eprintln!("Continuing with original prompt...\n");
                return Ok(HookResponse::success()
                    .with_metadata(vec![("modified_prompt".to_string(), original_prompt)]));
            }
        };

    // Build final prompt (graceful degradation on error)
    let final_prompt = match state.build_message() {
        Ok(prompt) => prompt,
        Err(e) => {
            // Prompt assembly failed - log warning and use original prompt
            eprintln!("⚠️ PrePrompt flow failed: {}", e);
            eprintln!("Continuing with original prompt...\n");
            original_prompt.clone()
        }
    };

    // Return response based on flow result (all errors use original prompt)
    match flow_result {
        FlowResult::Success => Ok(HookResponse::success()
            .with_metadata(vec![("modified_prompt".to_string(), final_prompt)])),
        FlowResult::FailedContinue(msg) => {
            eprintln!("⚠️ PrePrompt flow failed: {}", msg);
            eprintln!("Continuing with original prompt...\n");
            Ok(HookResponse::success()
                .with_metadata(vec![("modified_prompt".to_string(), original_prompt)]))
        }
        FlowResult::FailedStop(msg) => {
            eprintln!("⚠️ PrePrompt flow stopped: {}", msg);
            eprintln!("Continuing with original prompt...\n");
            Ok(HookResponse::success()
                .with_metadata(vec![("modified_prompt".to_string(), original_prompt)]))
        }
        FlowResult::FailedBlock(msg) => {
            eprintln!("⚠️ PrePrompt flow blocked: {}", msg);
            eprintln!("Continuing with original prompt...\n");
            Ok(HookResponse::success()
                .with_metadata(vec![("modified_prompt".to_string(), original_prompt)]))
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

    match flow_result {
        FlowResult::Success => Ok(HookResponse::success()),
        FlowResult::FailedContinue(msg) => {
            // Log warning but continue
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] PreFileChange flow warning: {}", msg);
            }
            Ok(HookResponse::success())
        }
        FlowResult::FailedStop(_msg) => {
            // Flow stopped silently - no error
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            // PreFileChange should never block - just warn
            eprintln!("Warning: PreFileChange flow failed (not blocking): {}", msg);
            Ok(HookResponse::success())
        }
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

    match flow_result {
        FlowResult::Success => Ok(HookResponse::success_with_message(format!(
            "✅ Provenance recorded for {} files",
            event.file_paths.len()
        ))),
        FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(format!(
            "⚠️ Provenance partially recorded for {} files",
            event.file_paths.len()
        ))
        .with_agent_message(format!("Some actions failed: {}", msg))),
        FlowResult::FailedStop(_msg) => {
            // Flow stopped silently - no error to user
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            // PostFileChange should never block edits, even with on_failure: block
            // Show warning but allow the change to be saved
            Ok(HookResponse::failure(
                format!("⚠️ Provenance recording blocked: {}", msg),
                Some(
                    "Changes saved but provenance tracking failed. Please check your JJ setup."
                        .to_string(),
                ),
            ))
        }
    }
}

/// Handle post-response event (after agent completes its response)
///
/// This event fires after the agent finishes generating its response, allowing flows
/// to validate output, detect errors, and optionally send an autoreply to the agent.
/// Returns the autoreply via metadata if non-empty, with graceful degradation on errors.
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
                return Ok(HookResponse::success());
            }
        };

    // Build final autoreply (graceful degradation on error)
    let autoreply = match state.build_message() {
        Ok(reply) => reply,
        Err(e) => {
            // Autoreply assembly failed - log warning and skip autoreply
            eprintln!("\n⚠️ PostResponse flow failed: {}", e);
            eprintln!("No autoreply generated.\n");
            String::new()
        }
    };

    // Return response based on flow result (all errors result in no autoreply)
    match flow_result {
        FlowResult::Success => {
            if !autoreply.is_empty() {
                Ok(HookResponse::success()
                    .with_metadata(vec![("autoreply".to_string(), autoreply)]))
            } else {
                Ok(HookResponse::success())
            }
        }
        FlowResult::FailedContinue(msg) => {
            eprintln!("\n⚠️ PostResponse flow failed: {}", msg);
            eprintln!("No autoreply generated.\n");
            Ok(HookResponse::success())
        }
        FlowResult::FailedStop(msg) => {
            eprintln!("\n⚠️ PostResponse flow stopped: {}", msg);
            eprintln!("No autoreply generated.\n");
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            eprintln!("\n⚠️ PostResponse flow blocked: {}", msg);
            eprintln!("No autoreply generated.\n");
            Ok(HookResponse::success())
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

    match flow_result {
        FlowResult::Success => Ok(HookResponse::success_with_message("✅ Co-authors added")
            .with_metadata(vec![
                (
                    "aiki_version".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                ),
                ("flow".to_string(), "aiki/core".to_string()),
            ])),
        FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(
            "⚠️ Co-authors partially added",
        )
        .with_agent_message(format!("Some actions failed: {}", msg))),
        FlowResult::FailedStop(_msg) => {
            // Flow stopped silently - return success (no error to user)
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            // Block the commit
            Ok(HookResponse::blocking_failure(
                format!("❌ Commit blocked: {}", msg),
                Some("Fix the error and try committing again.".to_string()),
            ))
        }
    }
}

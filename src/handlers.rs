use crate::error::Result;
use crate::events::{AikiPostChangeEvent, AikiPrepareCommitMessageEvent, AikiStartEvent};
use crate::flows::{AikiState, FlowExecutor, FlowResult};

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
    let flow_result = FlowExecutor::execute_actions(&core_flow.session_start, &mut state)?;

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

/// Handle post-change event (after file modification)
///
/// This is the core provenance tracking event. Records metadata about
/// the change in the JJ change description using the flow engine.
pub fn handle_post_change(event: AikiPostChangeEvent) -> Result<HookResponse> {
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

    // Execute PostChange actions from the core flow
    let flow_result = FlowExecutor::execute_actions(&core_flow.post_change, &mut state)?;

    match flow_result {
        FlowResult::Success => Ok(HookResponse::success_with_message(format!(
            "✅ Provenance recorded for {}",
            event.file_path
        ))),
        FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(format!(
            "⚠️ Provenance partially recorded for {}",
            event.file_path
        ))
        .with_agent_message(format!("Some actions failed: {}", msg))),
        FlowResult::FailedStop(_msg) => {
            // Flow stopped silently - no error to user
            Ok(HookResponse::success())
        }
        FlowResult::FailedBlock(msg) => {
            // PostChange should never block edits, even with on_failure: block
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
    let flow_result = FlowExecutor::execute_actions(&core_flow.prepare_commit_message, &mut state)?;

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

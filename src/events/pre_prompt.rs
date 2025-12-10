use crate::error::Result;
use crate::flows::{AikiState, FlowEngine, FlowResult};
use crate::session::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::response::{Decision, HookResponse};

/// Pre-prompt event (before agent sees the user's prompt)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPrePromptEvent {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The prompt text from the user (immutable)
    pub prompt: String,
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

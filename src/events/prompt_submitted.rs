use super::prelude::*;
use crate::history;

/// prompt.submitted event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPromptSubmittedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The prompt text from the user (immutable)
    pub prompt: String,
    /// References to files injected as context (paths only, not content)
    #[serde(default)]
    pub injected_refs: Vec<String>,
}

/// Handle prompt.submitted event
///
/// This event fires when the user submits a prompt, allowing flows
/// to inject additional context (e.g., project conventions, active files, etc.).
/// Also records the prompt to conversation history and returns the prompt_id
/// so agents can link tasks to the triggering prompt.
/// Returns context via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
pub fn handle_prompt_submitted(payload: AikiPromptSubmittedPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| {
        format!(
            "prompt.submitted event from {:?}, prompt length: {}",
            payload.session.agent_type(),
            payload.prompt.len()
        )
    });

    // Record prompt to conversation history (non-blocking on failure)
    // The prompt's change_id is stored in JJ and can be looked up later via
    // `--source prompt` which resolves to the latest prompt for this session
    if let Err(e) = history::record_prompt(
        &payload.cwd,
        &payload.session,
        &payload.prompt,
        payload.injected_refs.clone(),
        payload.timestamp,
    ) {
        debug_log(|| format!("Failed to record prompt: {}", e));
    }

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let flow_result = match execute_flow(
        EventType::PromptSubmitted,
        &mut state,
        &core_flow.prompt_submitted,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Flow execution failed - log warning and use original prompt
            eprintln!("⚠️ prompt.submitted flow failed: {}", e);
            eprintln!("Continuing with original prompt...\n");
            // Return built context (already initialized with original prompt)
            return Ok(HookResult {
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
            Ok(HookResult {
                context: state.build_context(),
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => {
            // Block the prompt - return exit code 2
            Ok(HookResult {
                context: None,
                decision: Decision::Block,
                failures,
            })
        }
    }
}

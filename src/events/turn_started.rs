use super::prelude::*;
use crate::history;
use crate::session::turn_state::TurnState;

/// Source of a turn (user prompt or autoreply)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnSource {
    /// User-initiated turn (from prompt submission)
    User,
    /// Aiki-initiated turn (from autoreply context injection)
    Autoreply,
}

impl std::fmt::Display for TurnSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnSource::User => write!(f, "user"),
            TurnSource::Autoreply => write!(f, "autoreply"),
        }
    }
}

/// turn.started event payload
///
/// Fires when a turn begins (user submits prompt OR autoreply is generated).
/// Each turn has a sequential number and deterministic turn_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiTurnStartedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Sequential turn number within session (starts at 1, set by handler)
    #[serde(default)]
    pub turn: u32,
    /// Deterministic turn identifier: uuid_v5(session_uuid, turn.to_string())
    #[serde(default)]
    pub turn_id: String,
    /// Source of this turn (user or autoreply)
    #[serde(default = "default_turn_source")]
    pub source: TurnSource,
    /// The prompt text (user input or autoreply context)
    pub prompt: String,
    /// References to files injected as context (paths only, not content)
    #[serde(default)]
    pub injected_refs: Vec<String>,
}

fn default_turn_source() -> TurnSource {
    TurnSource::User
}

/// Handle turn.started event
///
/// This event fires when a turn begins (user submits prompt OR autoreply),
/// allowing flows to inject additional context (e.g., project conventions,
/// active files, etc.).
/// Also records the prompt to conversation history and returns the prompt_id
/// so agents can link tasks to the triggering prompt.
/// Returns context via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
pub fn handle_turn_started(mut payload: AikiTurnStartedPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    // Load turn state and check for pending autoreply flag
    let mut turn_state = TurnState::load(payload.session.uuid(), &payload.cwd);

    // If the previous turn.completed generated an autoreply, this turn was
    // triggered by that autoreply, not by the user directly
    if turn_state.take_pending_autoreply() {
        payload.source = TurnSource::Autoreply;
        debug_log(|| "Detected pending autoreply flag, setting source to Autoreply".to_string());
    }

    // Increment turn counter and generate turn_id
    let turn = turn_state.start_turn(payload.source.clone());
    payload.turn = turn;
    payload.turn_id = turn_state.current_turn_id.clone();

    debug_log(|| {
        format!(
            "turn.started event from {:?}, source: {}, turn: {}, turn_id: {}, prompt length: {}",
            payload.session.agent_type(),
            payload.source,
            payload.turn,
            payload.turn_id,
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
        EventType::TurnStarted,
        &mut state,
        &core_flow.turn_started,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Flow execution failed - log warning and use original prompt
            eprintln!("turn.started flow failed: {}", e);
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

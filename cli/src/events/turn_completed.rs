use super::prelude::*;
use super::turn_started::TurnSource;
use crate::history;
use crate::session::turn_state::TurnState;

/// turn.completed event payload
///
/// Fires when a turn ends (agent finishes processing).
/// Every turn.started has exactly one turn.completed (1:1 correspondence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiTurnCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Sequential turn number within session (loaded from turn state)
    #[serde(default)]
    pub turn: u32,
    /// Deterministic turn identifier: uuid_v5(session_uuid, turn.to_string())
    #[serde(default)]
    pub turn_id: String,
    /// Source of this turn (user or autoreply)
    #[serde(default = "default_turn_source")]
    pub source: TurnSource,
    /// The agent's response text for this turn
    pub response: String,
    /// Files modified during this turn
    #[serde(default)]
    pub modified_files: Vec<PathBuf>,
}

fn default_turn_source() -> TurnSource {
    TurnSource::User
}

/// Handle turn.completed event
///
/// This event fires when the agent finishes generating its response,
/// allowing flows to validate output, detect errors, and optionally send
/// an autoreply to the agent.
/// Also records the response to conversation history (if not opted out).
/// Returns autoreply via `response.context` and failures via `response.failures`,
/// with graceful degradation on errors.
///
/// Note: turn.completed does NOT auto-trigger session.ended. Sessions persist
/// across turns and are only ended explicitly (via session end hooks or TTL cleanup).
pub fn handle_turn_completed(mut payload: AikiTurnCompletedPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    // Load current turn state (set by the preceding turn.started event)
    let turn_state = TurnState::load(payload.session.uuid(), &payload.cwd);
    payload.turn = turn_state.current_turn;
    payload.turn_id = turn_state.current_turn_id.clone();
    payload.source = turn_state.current_turn_source.clone();

    debug_log(|| {
        format!(
            "turn.completed event from {:?}, source: {}, turn: {}, response length: {}",
            payload.session.agent_type(),
            payload.source,
            payload.turn,
            payload.response.len()
        )
    });

    // Record response to conversation history (non-blocking on failure)
    let files_written: Vec<String> = payload
        .modified_files
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    if let Err(e) = history::record_response(
        &payload.cwd,
        &payload.session,
        &payload.response,
        files_written,
        payload.timestamp,
    ) {
        debug_log(|| format!("Failed to record response: {}", e));
    }

    // Capture session/cwd before payload is moved into state
    let session_uuid = payload.session.uuid().to_string();
    let payload_cwd = payload.cwd.clone();

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let _flow_result = match execute_flow(
        EventType::TurnCompleted,
        &mut state,
        &core_flow.turn_completed,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Flow execution failed - log warning and skip autoreply
            eprintln!("\nturn.completed flow failed: {}", e);
            eprintln!("No autoreply generated.\n");
            return Ok(HookResult {
                context: state.build_context(),
                decision: Decision::Allow,
                failures: state.take_failures(),
            });
        }
    };

    // Extract failures from state
    let failures = state.take_failures();

    // Build the autoreply context
    let context = state.build_context();

    // If an autoreply was generated, mark the next turn as autoreply-initiated
    if context.is_some() {
        let turn_state = TurnState::load(&session_uuid, &payload_cwd);
        turn_state.set_pending_autoreply();
        debug_log(|| "Autoreply generated, set pending_autoreply flag for next turn".to_string());
    }

    // turn.completed never blocks - always allow
    Ok(HookResult {
        context,
        decision: Decision::Allow,
        failures,
    })
}

use super::prelude::*;
use crate::history;
use crate::session::{cleanup_stale_sessions, AikiSessionFile};

/// session.started event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionStartPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Handle session.started event
///
/// Currently runs `aiki init --quiet` to ensure repository is initialized.
/// Also records the session start to conversation history (if not opted out).
/// Creates session file for PID-based session detection.
pub fn handle_session_started(payload: AikiSessionStartPayload) -> Result<HookResult> {
    use super::prelude::execute_flow;

    debug_log(|| format!("Session started by {:?}", payload.session.agent_type()));

    // Clean up stale sessions from crashed agents
    cleanup_stale_sessions(&payload.cwd);

    // Create session file for PID-based session detection
    // This preserves the parent_pid from the payload session
    let session_file = AikiSessionFile::new(&payload.session, &payload.cwd);
    if let Err(e) = session_file.create() {
        debug_log(|| format!("Failed to create session file: {}", e));
    }

    // Record session start to conversation history (non-blocking on failure)
    if let Err(e) = history::record_session_start(&payload.cwd, &payload.session, payload.timestamp)
    {
        debug_log(|| format!("Failed to record session start: {}", e));
    }

    // Load core flow for fallback
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute flow via FlowComposer (with fallback to bundled core flow)
    let flow_result = execute_flow(
        EventType::SessionStarted,
        &mut state,
        &core_flow.session_started,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResult {
                context: state.build_context(),
                decision: Decision::Allow,
                failures,
            })
        }
        FlowResult::FailedBlock => Ok(HookResult {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

use super::prelude::*;
use crate::global;
use crate::history;
use crate::repo_id;
use crate::session::{prune_dead_pid_sessions, AikiSessionFile};

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

    // Clean up sessions from crashed agents (PID-based)
    prune_dead_pid_sessions();

    // Create session file for PID-based session detection
    // This preserves the parent_pid from the payload session
    // Session files are stored globally at $AIKI_HOME/sessions/
    let session_file = AikiSessionFile::new(&payload.session);
    if let Err(e) = session_file.create() {
        debug_log(|| format!("Failed to create session file: {}", e));
    }

    // Record session start to conversation history (non-blocking on failure)
    // Uses global JJ repo at ~/.aiki/.jj/ for cross-repo conversation history
    let cwd_str = payload.cwd.to_string_lossy();
    let repo_id = repo_id::compute_repo_id(&payload.cwd).ok();
    if let Err(e) = history::record_session_start(
        &global::global_aiki_dir(),
        &payload.session,
        payload.timestamp,
        repo_id.as_deref(),
        Some(&cwd_str),
    ) {
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

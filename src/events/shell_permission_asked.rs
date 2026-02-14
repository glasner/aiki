use super::prelude::*;

/// shell.permission_asked event payload
///
/// Fires before a shell command is executed. This is the critical event
/// for autonomous review workflows - intercept `git commit`, run checks,
/// provide feedback for self-correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiShellPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The shell command about to be executed
    pub command: String,
}

/// Handle shell.permission_asked event
///
/// This is the autonomous review wedge - intercept shell commands like `git commit`,
/// run review checks, and provide feedback for self-correction.
pub fn handle_shell_permission_asked(
    payload: AikiShellPermissionAskedPayload,
) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "shell.permission_asked from {:?}, session: {}, command: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.command
        )
    });

    // Load core hook for fallback
    let core_hook = crate::flows::load_core_hook();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute hook via HookComposer (with fallback to bundled core hook)
    let flow_result = execute_hook(
        EventType::ShellPermissionAsked,
        &mut state,
        &core_hook.handlers.shell_permission_asked,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // shell.permission_asked is gateable - can block based on flow result
    match flow_result {
        HookOutcome::Success | HookOutcome::FailedContinue | HookOutcome::FailedStop => {
            Ok(HookResult {
                context: None,
                decision: Decision::Allow,
                failures,
            })
        }
        HookOutcome::FailedBlock => Ok(HookResult {
            context: None,
            decision: Decision::Block,
            failures,
        }),
    }
}

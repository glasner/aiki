use super::prelude::*;

/// shell.completed event payload
///
/// Fires after a shell command completes execution. Contains the command
/// that was run and its output/exit code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiShellCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The shell command that was executed
    pub command: String,
    /// Whether the command succeeded (exit_code == 0)
    pub success: bool,
    /// Exit code of the command (optional - available when vendor provides)
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Standard output from the command (optional - available when vendor provides)
    #[serde(default)]
    pub stdout: Option<String>,
    /// Standard error from the command (optional - available when vendor provides)
    #[serde(default)]
    pub stderr: Option<String>,
}

/// Handle shell.completed event
///
/// This event fires after a shell command completes. Can be used to
/// log command execution, react to failures, or trigger follow-up actions.
pub fn handle_shell_completed(payload: AikiShellCompletedPayload) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "shell.completed from {:?}, session: {}, command: {}, success: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.command,
            payload.success
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute shell.completed statements from the core flow
    let _flow_result = FlowEngine::execute_statements(&core_flow.shell_completed, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // shell.completed never blocks - always allow (command already executed)
    Ok(HookResult {
        context: None,
        decision: Decision::Allow,
        failures,
    })
}

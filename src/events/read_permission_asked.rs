use super::prelude::*;

/// read.permission_asked event payload
///
/// Fires when the agent requests permission to read a file.
/// This is a gateable event - flows can block sensitive file reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiReadPermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Tool requesting the read (e.g., "Read", "Glob", "Grep")
    pub tool_name: String,
    /// Files/directories about to be read or searched
    /// - Read: actual file paths
    /// - Glob/Grep: search directory (defaults to cwd if not specified)
    pub file_paths: Vec<String>,
    /// Search pattern for Glob/Grep tools
    /// - Glob: glob pattern (e.g., "**/*.rs")
    /// - Grep: regex pattern (e.g., "TODO.*fix")
    /// - Read: None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Handle read.permission_asked event
///
/// This event fires when the agent requests permission to read files.
/// It allows flows to gate reads of sensitive files (secrets, .env, etc.).
pub fn handle_read_permission_asked(payload: AikiReadPermissionAskedPayload) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "read.permission_asked event from {:?}, session: {}, tool: {}",
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name
        )
    });

    // Load core hook for fallback
    let core_hook = crate::flows::load_core_hook();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Execute hook via HookComposer (with fallback to bundled core hook)
    let flow_result = execute_hook(
        EventType::ReadPermissionAsked,
        &mut state,
        &core_hook.handlers.read_permission_asked,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // read.permission_asked is gateable - can block sensitive file reads
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

use super::change_completed::ChangeOperation;
use super::prelude::*;

/// change.permission_asked event payload
///
/// Fires before a file mutation operation (write, delete, or move).
/// Used to stash user changes before AI edits, and for operation-specific gating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiChangePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The tool requesting permission (e.g., "Edit", "Write", "Delete", "Move", "Bash")
    pub tool_name: String,
    /// The specific operation being requested (contains operation-specific fields)
    #[serde(flatten)]
    pub operation: ChangeOperation,
}

/// Handle change.permission_asked event
///
/// Fires before any file mutation (write, delete, or move).
/// Key purposes:
/// 1. Stash user changes before AI starts editing
/// 2. Gate destructive operations (deletes, moves)
/// 3. Apply operation-specific validation
pub fn handle_change_permission_asked(
    payload: AikiChangePermissionAskedPayload,
) -> Result<HookResult> {
    use super::prelude::execute_hook;

    debug_log(|| {
        format!(
            "change.permission_asked ({}) event from {:?}, session: {}, tool: {}",
            payload.operation.operation_name(),
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
        EventType::ChangePermissionAsked,
        &mut state,
        &core_hook.change_permission_asked,
    )?;

    // Extract failures from state
    let failures = state.take_failures();

    // change.permission_asked is gateable - can block mutations to protected files
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

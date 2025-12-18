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
    debug_log(|| {
        format!(
            "change.permission_asked ({}) event from {:?}, session: {}, tool: {}",
            payload.operation.operation_name(),
            payload.session.agent_type(),
            payload.session.external_id(),
            payload.tool_name
        )
    });

    // Load core flow (cached)
    let core_flow = crate::flows::load_core_flow();

    // Build execution state from payload
    let mut state = AikiState::new(payload);

    // Set flow name for self.* function resolution
    state.flow_name = Some("aiki/core".to_string());

    // Execute change.permission_asked actions from the core flow
    let flow_result =
        FlowEngine::execute_statements(&core_flow.change_permission_asked, &mut state)?;

    // Extract failures from state
    let failures = state.take_failures();

    // change.permission_asked is gateable - can block mutations to protected files
    match flow_result {
        FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
            Ok(HookResult {
                context: None,
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

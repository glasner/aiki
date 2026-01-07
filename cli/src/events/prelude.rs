//! Common imports shared by event handlers
pub use super::result::{Decision, HookResult};
pub use crate::cache::debug_log;
pub use crate::error::{AikiError, Result};
pub use crate::flows::composer::{EventType, FlowComposer};
pub use crate::flows::loader::FlowLoader;
pub use crate::flows::{AikiState, FlowEngine, FlowResult};
pub use crate::session::AikiSession;
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use std::path::PathBuf;

/// Execute flows for an event.
///
/// This function:
/// 1. Always executes bundled aiki/core first (immutable, cannot be overridden)
/// 2. Then executes user's "{project}/.aiki/flows/default.yml" if it exists (optional)
///
/// Note: aiki/core always runs and cannot be overridden.
/// Users should create .aiki/flows/default.yml to add custom behavior.
///
/// # Arguments
///
/// * `event_type` - The event type to handle
/// * `state` - Mutable execution state
/// * `bundled_statements` - The statements from the bundled core flow for this event
///
/// # Returns
///
/// The [`FlowResult`] from flow execution.
pub fn execute_flow(
    event_type: EventType,
    state: &mut AikiState,
    bundled_statements: &[crate::flows::types::FlowStatement],
) -> Result<FlowResult> {
    // Step 1: Always execute bundled aiki/core first
    debug_log(|| "Executing bundled aiki/core");
    state.flow_name = Some("aiki/core".to_string());
    let core_result = FlowEngine::execute_statements(bundled_statements, state)?;

    // If core flow blocked or stopped, don't continue to user's default.yml
    match core_result {
        FlowResult::FailedBlock | FlowResult::FailedStop => {
            return Ok(core_result);
        }
        _ => {}
    }

    // Step 2: Try to execute user's .aiki/flows/default.yml
    match FlowLoader::with_start_dir(state.cwd()) {
        Ok(mut loader) => {
            let mut composer = FlowComposer::new(&mut loader);

            let default_flow_path = state.cwd().join(".aiki/flows/default.yml");

            // No user default.yml - just return core result
            if !default_flow_path.exists() {
                return Ok(core_result);
            }

            // Execute user's default.yml
            debug_log(|| "Executing user's .aiki/flows/default.yml");
            let user_result =
                composer.compose_flow_from_path(&default_flow_path, event_type, state)?;

            // Combine results: if either failed, return the failure
            match (core_result, user_result) {
                (FlowResult::Success, user) => Ok(user),
                (core, FlowResult::Success) => Ok(core),
                (_, user_fail) => Ok(user_fail), // User failure takes precedence
            }
        }
        Err(AikiError::NotInAikiProject { .. }) => {
            // Not in an Aiki project - just return core result
            Ok(core_result)
        }
        Err(e) => Err(e),
    }
}

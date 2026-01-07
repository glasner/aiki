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

/// Execute the core flow for an event, with FlowComposer support.
///
/// This function:
/// 1. Tries to load user's "aiki/core" override via FlowComposer (supports before/after)
/// 2. Falls back to bundled core flow if no user override exists
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
pub fn execute_core_flow(
    event_type: EventType,
    state: &mut AikiState,
    bundled_statements: &[crate::flows::types::FlowStatement],
) -> Result<FlowResult> {
    // Try to use FlowComposer (supports user overrides with before/after)
    match FlowLoader::with_start_dir(state.cwd()) {
        Ok(mut loader) => {
            let mut composer = FlowComposer::new(&mut loader);

            match composer.compose_flow("aiki/core", event_type, state) {
                Ok(result) => {
                    debug_log(|| "Executed via FlowComposer (user override or aiki/core)");
                    Ok(result)
                }
                // Only fall back if "aiki/core" itself is missing, NOT for missing dependencies
                Err(AikiError::FlowNotFound { ref path, .. }) if path == "aiki/core" => {
                    debug_log(|| "No user override found, using bundled core flow");
                    state.flow_name = Some("aiki/core".to_string());
                    FlowEngine::execute_statements(bundled_statements, state)
                }
                Err(AikiError::NotInAikiProject { .. }) => {
                    // Not in an Aiki project - fall back to bundled core flow
                    debug_log(|| "Not in Aiki project (from compose), using bundled core flow");
                    state.flow_name = Some("aiki/core".to_string());
                    FlowEngine::execute_statements(bundled_statements, state)
                }
                // Propagate all other errors (including FlowNotFound for dependencies)
                Err(e) => Err(e),
            }
        }
        Err(AikiError::NotInAikiProject { .. }) => {
            // Not in an Aiki project - use bundled core flow directly
            debug_log(|| "Not in Aiki project, using bundled core flow");
            state.flow_name = Some("aiki/core".to_string());
            FlowEngine::execute_statements(bundled_statements, state)
        }
        Err(e) => Err(e),
    }
}

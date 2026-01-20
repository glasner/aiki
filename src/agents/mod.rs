//! Agent types and assignee definitions
//!
//! This module provides the canonical agent type definitions used throughout Aiki,
//! as well as the runtime abstraction for spawning agent sessions.

mod detect;
pub mod runtime;
mod types;

#[allow(unused_imports)]
pub use detect::detect_agent_from_process_tree;
#[allow(unused_imports)]
pub use runtime::{get_runtime, AgentRuntime, AgentSessionResult, AgentSpawnOptions};
pub use types::{AgentType, Assignee};

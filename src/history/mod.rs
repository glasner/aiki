//! Prompt history system for Aiki
//!
//! Provides conversation history tracking with:
//! - Event-sourced storage on `aiki/conversations` branch
//! - Session and log materialization
//! - Search/filter capabilities

pub mod manager;
pub mod storage;
pub mod types;

pub use manager::{filter_log_entries, get_sessions_by_agent, materialize_log_entries, materialize_sessions};
pub use storage::{ensure_conversations_branch, read_events, write_event};
pub use types::{
    AgentType, ConversationEvent, IntentSource, LogEntry, Session, CONVERSATIONS_BRANCH,
    METADATA_END, METADATA_START,
};

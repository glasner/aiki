//! Prompt history system for Aiki
//!
//! Provides conversation history recording with:
//! - Event-sourced storage on `aiki/conversations` branch
//! - Content truncation for large prompts/responses

pub mod recorder;
pub mod storage;
pub mod types;

pub use recorder::{record_prompt, record_response, record_session_end, record_session_start};
#[allow(unused_imports)]
pub use storage::{ensure_conversations_branch, get_latest_prompt_change_id, read_events, write_event};
#[allow(unused_imports)]
pub use types::{AgentType, ConversationEvent, Session, CONVERSATIONS_BRANCH, METADATA_END, METADATA_START};

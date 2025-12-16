use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;

/// Create a session from Cursor payload fields
pub fn create_session(conversation_id: &str, cursor_version: &str) -> AikiSession {
    AikiSession::new(
        AgentType::Cursor,
        conversation_id,
        Some(cursor_version),
        DetectionMethod::Hook,
    )
}

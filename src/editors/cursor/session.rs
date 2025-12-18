use crate::provenance::AgentType;
use crate::session::AikiSession;

/// Create a session from Cursor payload fields
pub fn create_session(conversation_id: &str, cursor_version: &str) -> AikiSession {
    AikiSession::for_hook(AgentType::Cursor, conversation_id, Some(cursor_version))
}

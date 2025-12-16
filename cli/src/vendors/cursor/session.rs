use std::path::PathBuf;

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

/// Get working directory from workspace roots
/// Takes the first workspace root, or current directory as fallback
pub fn get_cwd(workspace_roots: &[String]) -> PathBuf {
    workspace_roots
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

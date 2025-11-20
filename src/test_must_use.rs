// Test to verify event construction works properly
#![cfg(test)]

use crate::events::{AikiEvent, AikiPostChangeEvent, AikiStartEvent};
use crate::provenance::AgentType;
use std::path::PathBuf;

#[test]
fn test_must_use_warning_compilation() {
    // Test that event construction works
    let _event = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Claude,
        session_id: Some("session-123".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });
}

#[test]
fn test_impl_asref_path_ergonomics() {
    // Test that events can be constructed with various path types

    // Using &str
    let _event1 = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Claude,
        session_id: None,
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });

    // Using String
    let _event2 = AikiEvent::PostChange(AikiPostChangeEvent {
        agent_type: AgentType::Claude,
        client_name: None,
        client_version: None,
        agent_version: None,
        session_id: "session-123".to_string(),
        tool_name: "Edit".to_string(),
        file_paths: vec!["/tmp/file.rs".to_string()],
        cwd: PathBuf::from(String::from("/tmp")),
        timestamp: chrono::Utc::now(),
        detection_method: crate::provenance::DetectionMethod::Hook,
        edit_details: vec![],
    });

    // Using &String
    let s = String::from("/tmp");
    let _event3 = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Cursor,
        session_id: None,
        cwd: PathBuf::from(&s),
        timestamp: chrono::Utc::now(),
    });

    // Using PathBuf
    let _event4 = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Claude,
        session_id: None,
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });

    // Using &PathBuf
    let pb = PathBuf::from("/tmp");
    let _event5 = AikiEvent::PostChange(AikiPostChangeEvent {
        agent_type: AgentType::Claude,
        client_name: None,
        client_version: None,
        agent_version: None,
        session_id: "session-123".to_string(),
        tool_name: "Write".to_string(),
        file_paths: vec!["/tmp/file.rs".to_string()],
        cwd: pb.clone(),
        timestamp: chrono::Utc::now(),
        detection_method: crate::provenance::DetectionMethod::Hook,
        edit_details: vec![],
    });

    // Using &Path
    let _event6 = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Cursor,
        session_id: None,
        cwd: pb.as_path().to_path_buf(),
        timestamp: chrono::Utc::now(),
    });
}

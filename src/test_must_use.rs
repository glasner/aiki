// Test to verify event construction works properly
#![cfg(test)]

use crate::events::{AikiEvent, AikiPostFileChangePayload, AikiSessionStartPayload};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;
use std::path::PathBuf;

#[test]
fn test_must_use_warning_compilation() {
    // Test that event construction works
    let session = AikiSession::new(
        AgentType::Claude,
        "session-123".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event = AikiEvent::SessionStart(AikiSessionStartPayload {
        session,
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });
}

#[test]
fn test_impl_asref_path_ergonomics() {
    // Test that events can be constructed with various path types

    // Using &str
    let session1 = AikiSession::new(
        AgentType::Claude,
        "session-1".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event1 = AikiEvent::SessionStart(AikiSessionStartPayload {
        session: session1,
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });

    // Using String
    let session2 = AikiSession::new(
        AgentType::Claude,
        "session-123".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event2 = AikiEvent::PostFileChange(AikiPostFileChangePayload {
        session: session2,
        tool_name: "Edit".to_string(),
        file_paths: vec!["/tmp/file.rs".to_string()],
        cwd: PathBuf::from(String::from("/tmp")),
        timestamp: chrono::Utc::now(),
        edit_details: vec![],
    });

    // Using &String
    let s = String::from("/tmp");
    let session3 = AikiSession::new(
        AgentType::Cursor,
        "session-2".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event3 = AikiEvent::SessionStart(AikiSessionStartPayload {
        session: session3,
        cwd: PathBuf::from(&s),
        timestamp: chrono::Utc::now(),
    });

    // Using PathBuf
    let session4 = AikiSession::new(
        AgentType::Claude,
        "session-3".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event4 = AikiEvent::SessionStart(AikiSessionStartPayload {
        session: session4,
        cwd: PathBuf::from("/tmp"),
        timestamp: chrono::Utc::now(),
    });

    // Using &PathBuf
    let pb = PathBuf::from("/tmp");
    let session5 = AikiSession::new(
        AgentType::Claude,
        "session-123".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event5 = AikiEvent::PostFileChange(AikiPostFileChangePayload {
        session: session5,
        tool_name: "Write".to_string(),
        file_paths: vec!["/tmp/file.rs".to_string()],
        cwd: pb.clone(),
        timestamp: chrono::Utc::now(),
        edit_details: vec![],
    });

    // Using &Path
    let session6 = AikiSession::new(
        AgentType::Cursor,
        "session-4".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    );
    let _event6 = AikiEvent::SessionStart(AikiSessionStartPayload {
        session: session6,
        cwd: pb.as_path().to_path_buf(),
        timestamp: chrono::Utc::now(),
    });
}

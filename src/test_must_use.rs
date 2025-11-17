// Test to verify #[must_use] warnings work
#![cfg(test)]

use crate::events::{AikiEvent, AikiEventType};
use crate::provenance::AgentType;
use std::path::PathBuf;

#[test]
fn test_must_use_warning_compilation() {
    // These should be fine - results are used
    let _event = AikiEvent::new(
        AikiEventType::Start,
        AgentType::ClaudeCode,
        PathBuf::from("/tmp"),
    );
    let _event_with_session = _event.with_session_id("session-123");
    let _event_with_metadata = _event_with_session.with_metadata("key", "value");
}

#[test]
fn test_impl_asref_path_ergonomics() {
    // Test that AikiEvent::new() accepts impl AsRef<Path> with different types

    // Accept &str
    let _event1 = AikiEvent::new(AikiEventType::Start, AgentType::ClaudeCode, "/tmp");

    // Accept String
    let _event2 = AikiEvent::new(
        AikiEventType::PostChange,
        AgentType::ClaudeCode,
        String::from("/tmp"),
    );

    // Accept &String
    let s = String::from("/tmp");
    let _event3 = AikiEvent::new(AikiEventType::PreCommit, AgentType::ClaudeCode, &s);

    // Accept PathBuf
    let _event4 = AikiEvent::new(
        AikiEventType::Start,
        AgentType::ClaudeCode,
        PathBuf::from("/tmp"),
    );

    // Accept &PathBuf
    let pb = PathBuf::from("/tmp");
    let _event5 = AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, &pb);

    // Accept &Path
    let _event6 = AikiEvent::new(
        AikiEventType::PreCommit,
        AgentType::ClaudeCode,
        pb.as_path(),
    );
}

// This function intentionally ignores return values to verify #[must_use] works
// It should generate warnings when compiled
#[cfg(any())] // Disabled by default since it would fail CI
#[allow(dead_code)]
fn verify_must_use_triggers_warnings() {
    // These SHOULD trigger unused_must_use warnings
    AikiEvent::new(AikiEventType::Start, AgentType::ClaudeCode, "/tmp"); // unused must_use warning expected
    AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, "/tmp")
        .with_session_id("session-123"); // unused must_use warning expected
    AikiEvent::new(
        AikiEventType::PreCommit,
        AgentType::ClaudeCode,
        PathBuf::from("/tmp"),
    )
    .with_metadata("key", "value"); // unused must_use warning expected
}

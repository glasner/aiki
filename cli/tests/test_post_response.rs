/// Integration tests for PostResponse event and autoreply functionality
use aiki::events::{AikiEvent, AikiPostResponseEvent};
use aiki::flows::{AikiState, FlowEngine};
use aiki::provenance::AgentType;
use chrono::Utc;
use std::path::PathBuf;

#[test]
fn test_post_response_event_creation() {
    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "Agent completed the task successfully.".to_string(),
        modified_files: vec![PathBuf::from("/tmp/test.rs")],
    };

    assert_eq!(event.response, "Agent completed the task successfully.");
    assert_eq!(event.modified_files.len(), 1);
}

#[test]
fn test_post_response_state_initialization() {
    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "Test response".to_string(),
        modified_files: vec![],
    };

    let state = AikiState::new(event);

    // Verify message assembler is initialized for PostResponse events
    let message = state.build_message();
    assert!(message.is_ok());
    assert_eq!(message.unwrap(), ""); // No chunks added yet, empty autoreply
}

#[test]
fn test_autoreply_simple_flow() {
    use aiki::flows::types::{Action, AutoreplyAction, AutoreplyContent, FailureMode};

    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "Test response".to_string(),
        modified_files: vec![],
    };

    let mut state = AikiState::new(event);

    // Create a simple autoreply action
    let action = Action::Autoreply(AutoreplyAction {
        autoreply: AutoreplyContent::Simple("Please fix the errors above.".to_string()),
        on_failure: FailureMode::Continue,
    });

    // Execute the action
    let result = FlowEngine::execute_actions(&[action], &mut state);
    assert!(result.is_ok());

    // Build autoreply
    let autoreply = state.build_message();
    assert!(autoreply.is_ok());
    assert!(autoreply.unwrap().contains("Please fix the errors above."));
}

#[test]
fn test_autoreply_explicit_form() {
    use aiki::flows::messages::{MessageChunk, TextLines};
    use aiki::flows::types::{Action, AutoreplyAction, AutoreplyContent, FailureMode};

    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "Test response".to_string(),
        modified_files: vec![],
    };

    let mut state = AikiState::new(event);

    // Create an explicit autoreply action with prepend and append
    let action = Action::Autoreply(AutoreplyAction {
        autoreply: AutoreplyContent::Explicit(MessageChunk {
            prepend: Some(TextLines::Single("🚨 Errors detected:".to_string())),
            append: Some(TextLines::Single(
                "Please address these issues.".to_string(),
            )),
        }),
        on_failure: FailureMode::Continue,
    });

    // Execute the action
    let result = FlowEngine::execute_actions(&[action], &mut state);
    assert!(result.is_ok());

    // Build autoreply
    let autoreply = state.build_message().unwrap();
    assert!(autoreply.contains("🚨 Errors detected:"));
    assert!(autoreply.contains("Please address these issues."));
}

#[test]
fn test_multiple_autoreply_actions_accumulate() {
    use aiki::flows::types::{Action, AutoreplyAction, AutoreplyContent, FailureMode};

    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "Test response".to_string(),
        modified_files: vec![],
    };

    let mut state = AikiState::new(event);

    // Create multiple autoreply actions
    let actions = vec![
        Action::Autoreply(AutoreplyAction {
            autoreply: AutoreplyContent::Simple(
                "Error 1: TypeScript compilation failed.".to_string(),
            ),
            on_failure: FailureMode::Continue,
        }),
        Action::Autoreply(AutoreplyAction {
            autoreply: AutoreplyContent::Simple("Error 2: Tests are failing.".to_string()),
            on_failure: FailureMode::Continue,
        }),
        Action::Autoreply(AutoreplyAction {
            autoreply: AutoreplyContent::Simple("Error 3: Lint warnings detected.".to_string()),
            on_failure: FailureMode::Continue,
        }),
    ];

    // Execute all actions
    let result = FlowEngine::execute_actions(&actions, &mut state);
    assert!(result.is_ok());

    // Build autoreply - should contain all three messages
    let autoreply = state.build_message().unwrap();
    assert!(autoreply.contains("Error 1: TypeScript compilation failed."));
    assert!(autoreply.contains("Error 2: Tests are failing."));
    assert!(autoreply.contains("Error 3: Lint warnings detected."));
}

#[test]
fn test_event_variables_in_autoreply() {
    use aiki::flows::types::{Action, AutoreplyAction, AutoreplyContent, FailureMode};

    let event = AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some("test-session-123".to_string()),
        cwd: PathBuf::from("/tmp"),
        timestamp: Utc::now(),
        response: "I've completed the refactoring.".to_string(),
        modified_files: vec![
            PathBuf::from("/tmp/file1.rs"),
            PathBuf::from("/tmp/file2.rs"),
        ],
    };

    let mut state = AikiState::new(event);

    // Create autoreply with variable references
    let action = Action::Autoreply(AutoreplyAction {
        autoreply: AutoreplyContent::Simple(
            "Session: $event.session_id - Modified files detected.".to_string(),
        ),
        on_failure: FailureMode::Continue,
    });

    // Execute the action
    let result = FlowEngine::execute_actions(&[action], &mut state);
    assert!(result.is_ok());

    // Build autoreply - should have variables resolved
    let autoreply = state.build_message().unwrap();
    assert!(autoreply.contains("Session: test-session-123"));
}

/// Tests for the new flow statement-based execution engine
use aiki::events::AikiEvent;
use aiki::flows::types::{
    Action, FlowStatement, IfStatement, LogAction, OnFailure, OnFailureShortcut, ShellAction,
    SwitchStatement,
};
use aiki::flows::{AikiState, FlowEngine, FlowResult};
use std::collections::HashMap;

fn create_test_event() -> AikiEvent {
    AikiEvent::Unsupported
}

#[test]
fn test_simple_action_statement() {
    // Test that a simple action wrapped in a statement works
    let statements = vec![FlowStatement::Action(Action::Log(LogAction {
        log: "Hello, World!".to_string(),
        alias: None,
    }))];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
}

#[test]
fn test_if_statement_true_branch() {
    // Test that if statement executes then branch when condition is true
    let statements = vec![FlowStatement::If(IfStatement {
        condition: "1 == 1".to_string(),
        then: vec![FlowStatement::Action(Action::Log(LogAction {
            log: "Condition was true".to_string(),
            alias: None,
        }))],
        else_: None,
    })];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
}

#[test]
fn test_if_statement_false_branch() {
    // Test that if statement executes else branch when condition is false
    let statements = vec![FlowStatement::If(IfStatement {
        condition: "1 == 2".to_string(),
        then: vec![FlowStatement::Action(Action::Log(LogAction {
            log: "Should not execute".to_string(),
            alias: None,
        }))],
        else_: Some(vec![FlowStatement::Action(Action::Log(LogAction {
            log: "Condition was false".to_string(),
            alias: None,
        }))]),
    })];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
}

#[test]
fn test_switch_statement() {
    // Test that switch statement matches cases correctly
    let mut cases = HashMap::new();
    cases.insert(
        "test".to_string(),
        vec![FlowStatement::Action(Action::Log(LogAction {
            log: "Matched test case".to_string(),
            alias: None,
        }))],
    );

    let statements = vec![FlowStatement::Switch(SwitchStatement {
        expression: "test".to_string(),
        cases,
        default: None,
    })];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
}

#[test]
fn test_nested_if_in_switch() {
    // Test nested control flow structures
    let mut cases = HashMap::new();
    cases.insert(
        "nested".to_string(),
        vec![FlowStatement::If(IfStatement {
            condition: "1 == 1".to_string(),
            then: vec![FlowStatement::Action(Action::Log(LogAction {
                log: "Nested if in switch".to_string(),
                alias: None,
            }))],
            else_: None,
        })],
    );

    let statements = vec![FlowStatement::Switch(SwitchStatement {
        expression: "nested".to_string(),
        cases,
        default: None,
    })];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
}

#[test]
fn test_action_with_on_failure() {
    // Test that on_failure behavior works with the new statement structure
    let statements = vec![FlowStatement::Action(Action::Shell(ShellAction {
        shell: "false".to_string(), // Command that always fails
        timeout: None,
        on_failure: OnFailure::Statements(vec![FlowStatement::Action(Action::Log(LogAction {
            log: "Handling failure".to_string(),
            alias: None,
        }))]),
        alias: None,
    }))];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    // Should return FailedContinue because on_failure handled the error
    assert!(matches!(result, FlowResult::FailedContinue));
}

#[test]
fn test_action_continue_on_failure() {
    // Test that continue shortcut works
    let statements = vec![
        FlowStatement::Action(Action::Shell(ShellAction {
            shell: "false".to_string(), // Command that always fails
            timeout: None,
            on_failure: OnFailure::Shortcut(OnFailureShortcut::Continue),
            alias: None,
        })),
        FlowStatement::Action(Action::Log(LogAction {
            log: "This should still execute".to_string(),
            alias: None,
        })),
    ];

    let mut state = AikiState::new(create_test_event());
    let result = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    // Should be FailedContinue because first action failed but continued
    assert!(matches!(result, FlowResult::FailedContinue));
}

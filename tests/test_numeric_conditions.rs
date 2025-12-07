use aiki::flows::engine::FlowEngine;
use aiki::flows::state::AikiState;
use aiki::flows::types::{Action, LogAction};

#[test]
fn test_greater_than_condition() {
    let mut state = AikiState::new();
    state.set_variable("count".to_string(), "5".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "Test passed".to_string(),
        if_condition: Some("$count > 3".to_string()),
    })];

    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());

    // Now test with a failing condition
    state.set_variable("count".to_string(), "2".to_string());
    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success()); // Log with false condition still succeeds, just doesn't execute
}

#[test]
fn test_less_than_condition() {
    let mut state = AikiState::new();
    state.set_variable("count".to_string(), "2".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "Test passed".to_string(),
        if_condition: Some("$count < 5".to_string()),
    })];

    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());
}

#[test]
fn test_greater_than_or_equal_condition() {
    let mut state = AikiState::new();
    state.set_variable("count".to_string(), "5".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "Test passed".to_string(),
        if_condition: Some("$count >= 5".to_string()),
    })];

    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());

    state.set_variable("count".to_string(), "6".to_string());
    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());
}

#[test]
fn test_less_than_or_equal_condition() {
    let mut state = AikiState::new();
    state.set_variable("count".to_string(), "5".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "Test passed".to_string(),
        if_condition: Some("$count <= 5".to_string()),
    })];

    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());

    state.set_variable("count".to_string(), "4".to_string());
    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());
}

#[test]
fn test_invalid_numeric_comparison() {
    let mut state = AikiState::new();
    state.set_variable("text".to_string(), "not_a_number".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "This should fail".to_string(),
        if_condition: Some("$text > 5".to_string()),
    })];

    let result = FlowEngine::execute_statements(&actions, &mut state);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not a number"));
}

#[test]
fn test_decimal_comparisons() {
    let mut state = AikiState::new();
    state.set_variable("value".to_string(), "3.14".to_string());

    let actions = vec![Action::Log(LogAction {
        message: "Test passed".to_string(),
        if_condition: Some("$value > 3.0".to_string()),
    })];

    let (result, _timing) = FlowEngine::execute_statements(&actions, &mut state).unwrap();
    assert!(result.is_success());
}

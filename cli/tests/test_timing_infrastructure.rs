/// Tests for statement-level timing infrastructure
use aiki::events::AikiEvent;
use aiki::flows::types::{Action, FlowStatement, IfStatement, LogAction};
use aiki::flows::{AikiState, FlowEngine, FlowResult};

fn create_test_event() -> AikiEvent {
    AikiEvent::Unsupported
}

#[test]
fn test_timing_simple_statements() {
    let statements = vec![
        FlowStatement::Action(Action::Log(LogAction {
            log: "First".to_string(),
            alias: None,
        })),
        FlowStatement::Action(Action::Log(LogAction {
            log: "Second".to_string(),
            alias: None,
        })),
    ];

    let mut state = AikiState::new(create_test_event());
    let (result, timing) = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
    assert_eq!(timing.statement_timings.len(), 2);
    assert_eq!(timing.statement_timings[0].statement_type, "Log");
    assert_eq!(timing.statement_timings[1].statement_type, "Log");
    assert!(timing.duration_secs >= 0.0);
}

#[test]
fn test_timing_if_statement() {
    let statements = vec![FlowStatement::If(IfStatement {
        condition: "1 == 1".to_string(),
        then: vec![FlowStatement::Action(Action::Log(LogAction {
            log: "In then branch".to_string(),
            alias: None,
        }))],
        else_: None,
    })];

    let mut state = AikiState::new(create_test_event());
    let (result, timing) = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));
    assert_eq!(timing.statement_timings.len(), 1);
    assert_eq!(timing.statement_timings[0].statement_type, "If");
    // The If statement should have nested timing for the then branch
    assert_eq!(timing.statement_timings[0].nested.len(), 1);
    assert_eq!(timing.statement_timings[0].nested[0].statement_type, "Log");
}

#[test]
fn test_timing_nested_control_flow() {
    let statements = vec![FlowStatement::If(IfStatement {
        condition: "1 == 1".to_string(),
        then: vec![FlowStatement::If(IfStatement {
            condition: "2 == 2".to_string(),
            then: vec![FlowStatement::Action(Action::Log(LogAction {
                log: "Deeply nested".to_string(),
                alias: None,
            }))],
            else_: None,
        })],
        else_: None,
    })];

    let mut state = AikiState::new(create_test_event());
    let (result, timing) = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    assert!(matches!(result, FlowResult::Success));

    // Top level: one If statement
    assert_eq!(timing.statement_timings.len(), 1);
    assert_eq!(timing.statement_timings[0].statement_type, "If");

    // First nested level: another If statement
    let first_nested = &timing.statement_timings[0].nested;
    assert_eq!(first_nested.len(), 1);
    assert_eq!(first_nested[0].statement_type, "If");

    // Second nested level: the Log action
    let second_nested = &first_nested[0].nested;
    assert_eq!(second_nested.len(), 1);
    assert_eq!(second_nested[0].statement_type, "Log");
}

#[test]
fn test_timing_preserves_durations() {
    let statements = vec![FlowStatement::Action(Action::Log(LogAction {
        log: "Test".to_string(),
        alias: None,
    }))];

    let mut state = AikiState::new(create_test_event());
    let (_, timing) = FlowEngine::execute_statements(&statements, &mut state).unwrap();

    // All durations should be non-negative
    assert!(timing.duration_secs >= 0.0);
    for stmt_timing in &timing.statement_timings {
        assert!(stmt_timing.duration >= 0.0);
    }
}

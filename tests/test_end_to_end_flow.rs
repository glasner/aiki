/// End-to-end test for the complete flow control refactoring
use aiki::events::{AikiEvent, AikiPostFileChangePayload};
use aiki::flows::types::{
    Action, Flow, FlowStatement, IfStatement, LogAction, OnFailure, OnFailureShortcut, ShellAction,
    SwitchStatement,
};
use aiki::flows::{AikiState, FlowEngine, FlowResult};
use aiki::provenance::{AgentType, DetectionMethod};
use aiki::session::AikiSession;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_full_flow_execution_with_yaml() {
    // Create a complete flow definition in YAML
    let yaml_flow = r#"
name: complete-test-flow
description: Tests all features of the new flow control system
version: "1"

SessionStart:
  - log: "Session starting"

PrePrompt:
  - if: "$event.session_id != ''"
    then:
      - log: "Existing session detected"
      - shell: "echo 'Session: $event.session_id'"
        alias: session_info
    else:
      - log: "New session"

PostFileChange:
  - switch: "$event.tool_name"
    cases:
      Edit:
        - log: "File edited"
        - if: "$event.file_count > 1"
          then:
            - log: "Multiple files changed"
      Write:
        - log: "File written"
      Read:
        - log: "File read"
    default:
      - log: "Unknown tool: $event.tool_name"

  - shell: "test -f /nonexistent"
    on_failure:
      - if: "$EXIT_CODE == 1"
        then:
          - log: "File doesn't exist (expected)"
        else:
          - stop: "Unexpected error"

SessionEnd:
  - if: "$event.modified_files != ''"
    then:
      - switch: "$event.agent_type"
        cases:
          claude:
            - log: "Claude made changes"
          cursor:
            - log: "Cursor made changes"
        default:
          - log: "Agent made changes"
"#;

    // Parse the YAML into a Flow
    let flow: Flow = serde_yaml::from_str(yaml_flow).expect("Failed to parse YAML flow");

    // Verify flow was parsed correctly
    assert_eq!(flow.name, "complete-test-flow");
    assert_eq!(flow.version, "1");

    // Test SessionStart execution
    {
        let event = AikiEvent::Unsupported;
        let mut state = AikiState::new(event);

        let (result, timing) = FlowEngine::execute_statements_with_timing(&flow.session_start, &mut state)
            .expect("Failed to execute SessionStart");

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(timing.statement_timings.len(), 1);
        assert_eq!(timing.statement_timings[0].statement_type, "Log");
    }

    // Test PostFileChange with switch statement
    {
        let session = AikiSession::new(
            AgentType::Claude,
            "test-123".to_string(),
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        let event = AikiPostFileChangePayload {
            session,
            cwd: PathBuf::from("/tmp"),
            timestamp: Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["file1.rs".to_string(), "file2.rs".to_string()],
            edit_details: Vec::new(),
        };

        let mut state = AikiState::new(AikiEvent::PostFileChange(event));

        let (result, timing) = FlowEngine::execute_statements_with_timing(&flow.post_file_change, &mut state)
            .expect("Failed to execute PostFileChange");

        // The shell command will fail but on_failure handles it
        println!("Result: {:?}", result);
        assert!(matches!(result, FlowResult::FailedContinue));

        // Verify timing includes both the switch and the shell with on_failure
        assert!(timing.statement_timings.len() >= 2);
        assert_eq!(timing.statement_timings[0].statement_type, "Switch");

        // The switch should have nested timings for the matched case
        assert!(!timing.statement_timings[0].nested.is_empty());
    }
}

#[test]
fn test_nested_control_flow_with_timing() {
    // Create deeply nested control flow
    let statements = vec![FlowStatement::If(IfStatement {
        condition: "1 == 1".to_string(),
        then: vec![
            FlowStatement::Action(Action::Log(LogAction {
                log: "Outer if".to_string(),
                alias: None,
            })),
            FlowStatement::Switch(SwitchStatement {
                expression: "test".to_string(),
                cases: {
                    let mut cases = HashMap::new();
                    cases.insert(
                        "test".to_string(),
                        vec![FlowStatement::If(IfStatement {
                            condition: "2 == 2".to_string(),
                            then: vec![FlowStatement::Action(Action::Log(LogAction {
                                log: "Deeply nested".to_string(),
                                alias: None,
                            }))],
                            else_: None,
                        })],
                    );
                    cases
                },
                default: None,
            }),
        ],
        else_: None,
    })];

    let mut state = AikiState::new(AikiEvent::Unsupported);
    let (result, timing) = FlowEngine::execute_statements_with_timing(&statements, &mut state)
        .expect("Failed to execute nested flow");

    assert!(matches!(result, FlowResult::Success));

    // Verify timing hierarchy
    assert_eq!(timing.statement_timings.len(), 1);
    assert_eq!(timing.statement_timings[0].statement_type, "If");

    // First level nested (then branch)
    let then_timings = &timing.statement_timings[0].nested;
    assert_eq!(then_timings.len(), 2); // Log and Switch
    assert_eq!(then_timings[0].statement_type, "Log");
    assert_eq!(then_timings[1].statement_type, "Switch");

    // Second level nested (switch case)
    let case_timings = &then_timings[1].nested;
    assert_eq!(case_timings.len(), 1); // If
    assert_eq!(case_timings[0].statement_type, "If");

    // Third level nested (inner if then)
    let inner_then = &case_timings[0].nested;
    assert_eq!(inner_then.len(), 1); // Log
    assert_eq!(inner_then[0].statement_type, "Log");
}

#[test]
fn test_on_failure_with_statements() {
    let statements = vec![
        FlowStatement::Action(Action::Shell(ShellAction {
            shell: "exit 42".to_string(),
            timeout: None,
            on_failure: OnFailure::Statements(vec![FlowStatement::If(IfStatement {
                condition: "$EXIT_CODE == 42".to_string(),
                then: vec![FlowStatement::Action(Action::Log(LogAction {
                    log: "Got expected exit code 42".to_string(),
                    alias: None,
                }))],
                else_: Some(vec![FlowStatement::Action(Action::Stop(
                    aiki::flows::types::StopAction {
                        failure: "Unexpected exit code".to_string(),
                    },
                ))]),
            })]),
            alias: None,
        })),
        FlowStatement::Action(Action::Log(LogAction {
            log: "This should execute after on_failure handles the error".to_string(),
            alias: None,
        })),
    ];

    let mut state = AikiState::new(AikiEvent::Unsupported);
    let (result, timing) = FlowEngine::execute_statements_with_timing(&statements, &mut state)
        .expect("Failed to execute on_failure test");

    // Should return FailedContinue because on_failure handled the error and continued
    assert!(matches!(result, FlowResult::FailedContinue));
    assert_eq!(timing.statement_timings.len(), 2); // Shell and Log
}

#[test]
fn test_backwards_compatibility_shortcuts() {
    // Test that on_failure shortcuts still work
    let statements = vec![
        FlowStatement::Action(Action::Shell(ShellAction {
            shell: "false".to_string(),
            timeout: None,
            on_failure: OnFailure::Shortcut(OnFailureShortcut::Continue),
            alias: None,
        })),
        FlowStatement::Action(Action::Shell(ShellAction {
            shell: "false".to_string(),
            timeout: None,
            on_failure: OnFailure::Shortcut(OnFailureShortcut::Stop),
            alias: None,
        })),
        FlowStatement::Action(Action::Log(LogAction {
            log: "Should not execute".to_string(),
            alias: None,
        })),
    ];

    let mut state = AikiState::new(AikiEvent::Unsupported);
    let (result, timing) = FlowEngine::execute_statements_with_timing(&statements, &mut state)
        .expect("Failed to execute shortcuts test");

    // Should stop at the second shell command
    assert!(matches!(result, FlowResult::FailedStop));
    assert_eq!(timing.statement_timings.len(), 2); // Only two statements executed
}

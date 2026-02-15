/// Tests for YAML parsing of new flow statement syntax
use aiki::flows::types::{Action, Hook, HookStatement};

#[test]
fn test_parse_flow_with_statements() {
    let yaml = r#"
name: test-flow
description: Test flow
version: "1"

session.started:
  - if: "$SESSION_ID != ''"
    then:
      - log: "Resuming session"
    else:
      - shell: "echo 'New session'"
"#;

    let hook: Hook = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    assert_eq!(hook.name, "test-flow");
    assert_eq!(hook.handlers.session_started.len(), 1);

    // Verify it's an if statement
    match &hook.handlers.session_started[0] {
        HookStatement::If(if_stmt) => {
            assert_eq!(if_stmt.condition, "$SESSION_ID != ''");
            assert_eq!(if_stmt.then.len(), 1);
            assert!(if_stmt.else_.is_some());
        }
        _ => panic!("Expected If statement"),
    }
}

#[test]
fn test_parse_switch_statement() {
    let yaml = r#"
name: test-flow
version: "1"

turn.started:
  - switch: "$agent_type"
    cases:
      claude:
        - log: "Claude"
      cursor:
        - log: "Cursor"
    default:
      - log: "Unknown"
"#;

    let hook: Hook = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    assert_eq!(hook.handlers.turn_started.len(), 1);

    match &hook.handlers.turn_started[0] {
        HookStatement::Switch(switch_stmt) => {
            assert_eq!(switch_stmt.expression, "$agent_type");
            assert_eq!(switch_stmt.cases.len(), 2);
            assert!(switch_stmt.cases.contains_key("claude"));
            assert!(switch_stmt.default.is_some());
        }
        _ => panic!("Expected Switch statement"),
    }
}

#[test]
fn test_parse_nested_statements() {
    let yaml = r#"
name: test-flow
version: "1"

change.completed:
  - if: "$success"
    then:
      - switch: "$file_type"
        cases:
          rust:
            - log: "Rust file"
          python:
            - log: "Python file"
"#;

    let hook: Hook = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    match &hook.handlers.change_completed[0] {
        HookStatement::If(if_stmt) => {
            assert_eq!(if_stmt.then.len(), 1);
            match &if_stmt.then[0] {
                HookStatement::Switch(_) => {
                    // Success - found nested switch
                }
                _ => panic!("Expected nested Switch statement"),
            }
        }
        _ => panic!("Expected If statement"),
    }
}

#[test]
fn test_parse_action_with_on_failure_statements() {
    let yaml = r#"
name: test-flow
version: "1"

session.ended:
  - shell: "risky-command"
    on_failure:
      - if: "$EXIT_CODE == 1"
        then:
          - log: "Recoverable"
        else:
          - stop: "Fatal"
"#;

    let hook: Hook = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    match &hook.handlers.session_ended[0] {
        HookStatement::Action(Action::Shell(shell_action)) => {
            match &shell_action.on_failure {
                aiki::flows::types::OnFailure::Statements(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    match &stmts[0] {
                        HookStatement::If(_) => {
                            // Success
                        }
                        _ => panic!("Expected If in on_failure"),
                    }
                }
                _ => panic!("Expected Statements in on_failure"),
            }
        }
        _ => panic!("Expected Shell action"),
    }
}

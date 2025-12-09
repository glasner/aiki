/// Tests for YAML parsing of new flow statement syntax
use aiki::flows::types::{Action, Flow, FlowStatement, IfStatement, SwitchStatement};

#[test]
fn test_parse_flow_with_statements() {
    let yaml = r#"
name: test-flow
description: Test flow
version: "1"

SessionStart:
  - if: "$SESSION_ID != ''"
    then:
      - log: "Resuming session"
    else:
      - shell: "echo 'New session'"
"#;

    let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    assert_eq!(flow.name, "test-flow");
    assert_eq!(flow.session_start.len(), 1);

    // Verify it's an if statement
    match &flow.session_start[0] {
        FlowStatement::If(if_stmt) => {
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

PrePrompt:
  - switch: "$agent_type"
    cases:
      claude:
        - log: "Claude"
      cursor:
        - log: "Cursor"
    default:
      - log: "Unknown"
"#;

    let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    assert_eq!(flow.pre_prompt.len(), 1);

    match &flow.pre_prompt[0] {
        FlowStatement::Switch(switch_stmt) => {
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

PostFileChange:
  - if: "$success"
    then:
      - switch: "$file_type"
        cases:
          rust:
            - log: "Rust file"
          python:
            - log: "Python file"
"#;

    let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    match &flow.post_file_change[0] {
        FlowStatement::If(if_stmt) => {
            assert_eq!(if_stmt.then.len(), 1);
            match &if_stmt.then[0] {
                FlowStatement::Switch(_) => {
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

SessionEnd:
  - shell: "risky-command"
    on_failure:
      - if: "$EXIT_CODE == 1"
        then:
          - log: "Recoverable"
        else:
          - stop: "Fatal"
"#;

    let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    match &flow.session_end[0] {
        FlowStatement::Action(Action::Shell(shell_action)) => {
            match &shell_action.on_failure {
                aiki::flows::types::OnFailure::Statements(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    match &stmts[0] {
                        FlowStatement::If(_) => {
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

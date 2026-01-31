use anyhow::{Context, Result};
use serde_yaml::Value;

use super::sugar::expand_sugar_patterns;
use super::types::Hook;

/// Parser for flow YAML files
pub struct HookParser;

impl HookParser {
    /// Parse a flow from a YAML string.
    ///
    /// This function handles sugar pattern expansion for task lifecycle events:
    /// - `{type}.started` expands to `task.started` with `if: $event.task.type == "{type}"`
    /// - `{type}.completed` expands to `task.closed` with `if: $event.task.type == "{type}" && $event.task.outcome == "done"`
    pub fn parse_str(yaml: &str) -> Result<Hook> {
        // First parse to Value for preprocessing
        let mut value: Value =
            serde_yaml::from_str(yaml).context("Failed to parse flow YAML")?;

        // Expand sugar patterns
        if let Value::Mapping(ref mut map) = value {
            expand_sugar_patterns(map)?;
        }

        // Now deserialize to Hook
        serde_yaml::from_value(value).context("Failed to deserialize flow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_flow() {
        let yaml = r#"
name: Test Flow
version: "1"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.name, "Test Flow");
        assert_eq!(hook.version, "1");
        assert!(hook.change_completed.is_empty());
        assert!(hook.commit_message_started.is_empty());
        assert!(hook.before.is_empty());
        assert!(hook.after.is_empty());
    }

    #[test]
    fn test_parse_flow_with_before_after() {
        let yaml = r#"
name: Composed Flow
version: "1"

before:
  - aiki/quick-lint
  - eslint/check

after:
  - aiki/cleanup

change.completed:
  - shell: echo "main logic"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.name, "Composed Flow");
        assert_eq!(hook.before.len(), 2);
        assert_eq!(hook.before[0], "aiki/quick-lint");
        assert_eq!(hook.before[1], "eslint/check");
        assert_eq!(hook.after.len(), 1);
        assert_eq!(hook.after[0], "aiki/cleanup");
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_shell_action() {
        let yaml = r#"
name: Lint Flow
version: "1"
change.completed:
  - shell: ruff check $event.file_paths
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.name, "Lint Flow");
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_jj_action() {
        let yaml = r#"
name: JJ Flow
version: "1"
change.completed:
  - jj: describe -m "AI generated change"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_log_action() {
        let yaml = r#"
name: Log Flow
version: "1"
change.completed:
  - log: "File edited: $event.file_paths"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_multiple_actions() {
        let yaml = r#"
name: Multi Action Flow
version: "1"
change.completed:
  - shell: echo "Starting"
  - log: "Processing file"
  - jj: describe -m "Done"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 3);
    }

    #[test]
    fn test_parse_flow_with_on_failure() {
        let yaml = r#"
name: Failure Handling Flow
version: "1"
change.completed:
  - shell: ruff check .
    on_failure:
      - stop: "Ruff check failed"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_timeout() {
        let yaml = r#"
name: Timeout Flow
version: "1"
change.completed:
  - shell: pytest
    timeout: 60s
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_multiple_events() {
        let yaml = r#"
name: Multi Event Flow
version: "1"
change.completed:
  - shell: ruff check $event.file_paths
commit.message_started:
  - shell: pytest
session.started:
  - log: "Session started"
session.ended:
  - log: "Session ended"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();
        assert_eq!(hook.change_completed.len(), 1);
        assert_eq!(hook.commit_message_started.len(), 1);
        assert_eq!(hook.session_started.len(), 1);
        assert_eq!(hook.session_ended.len(), 1);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = r#"
name: Invalid Flow
this is not valid yaml: [
"#;

        let result = HookParser::parse_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_name() {
        let yaml = r#"
version: "1"
change.completed:
  - shell: echo "test"
"#;

        let result = HookParser::parse_str(yaml);
        assert!(result.is_err());
    }

    // ========================================================================
    // Sugar Pattern Expansion Tests
    // ========================================================================

    #[test]
    fn test_parse_review_completed_sugar() {
        use super::super::types::HookStatement;

        let yaml = r#"
name: Review Sugar Test
version: "1"
review.completed:
  - log: "Review done"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // Should expand to task.closed with if wrapper
        assert!(!hook.task_closed.is_empty());
        assert_eq!(hook.task_closed.len(), 1);

        // Verify it's wrapped in an if statement
        match &hook.task_closed[0] {
            HookStatement::If(if_stmt) => {
                assert_eq!(
                    if_stmt.condition,
                    "$event.task.type == \"review\" && $event.task.outcome == \"done\""
                );
                assert_eq!(if_stmt.then.len(), 1);
            }
            _ => panic!("Expected If statement wrapping the sugar pattern"),
        }
    }

    #[test]
    fn test_parse_feature_started_sugar() {
        use super::super::types::HookStatement;

        let yaml = r#"
name: Feature Sugar Test
version: "1"
feature.started:
  - log: "Feature started"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // Should expand to task.started with if wrapper
        assert!(!hook.task_started.is_empty());
        assert_eq!(hook.task_started.len(), 1);

        // Verify it's wrapped in an if statement
        match &hook.task_started[0] {
            HookStatement::If(if_stmt) => {
                assert_eq!(if_stmt.condition, "$event.task.type == \"feature\"");
                assert_eq!(if_stmt.then.len(), 1);
            }
            _ => panic!("Expected If statement wrapping the sugar pattern"),
        }
    }

    #[test]
    fn test_parse_multiple_sugar_patterns_same_base() {
        use super::super::types::HookStatement;

        let yaml = r#"
name: Multi Sugar Test
version: "1"
review.completed:
  - log: "Review done"
bugfix.completed:
  - log: "Bugfix done"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // Both should expand to task.closed
        assert_eq!(hook.task_closed.len(), 2);

        // Both should be wrapped in if statements
        for stmt in &hook.task_closed {
            match stmt {
                HookStatement::If(_) => {}
                _ => panic!("Expected If statement"),
            }
        }
    }

    #[test]
    fn test_parse_sugar_merged_with_existing_task_handlers() {
        use super::super::types::HookStatement;

        let yaml = r#"
name: Merged Handlers Test
version: "1"
task.started:
  - log: "Generic task started"
review.started:
  - log: "Review started"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // task.started should have both: the direct handler and the expanded sugar
        assert_eq!(hook.task_started.len(), 2);

        // First statement should be the direct log action
        match &hook.task_started[0] {
            HookStatement::Action(_) => {}
            _ => panic!("Expected first statement to be direct Action"),
        }

        // Second statement should be the wrapped sugar pattern
        match &hook.task_started[1] {
            HookStatement::If(if_stmt) => {
                assert_eq!(if_stmt.condition, "$event.task.type == \"review\"");
            }
            _ => panic!("Expected second statement to be If wrapper"),
        }
    }

    #[test]
    fn test_parse_known_triggers_not_treated_as_sugar() {
        let yaml = r#"
name: Known Triggers Test
version: "1"
session.started:
  - log: "Session started"
turn.completed:
  - log: "Turn completed"
task.started:
  - log: "Task started"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // These should be handled directly, not as sugar patterns
        assert_eq!(hook.session_started.len(), 1);
        assert_eq!(hook.turn_completed.len(), 1);
        assert_eq!(hook.task_started.len(), 1);

        // task.closed should be empty (no sugar expanded to it)
        assert!(hook.task_closed.is_empty());
    }

    #[test]
    fn test_parse_sugar_with_complex_statements() {
        use super::super::types::HookStatement;

        let yaml = r#"
name: Complex Sugar Test
version: "1"
review.started:
  - shell: echo "Starting review"
    timeout: 30s
  - log: "Review initiated"
  - if: "$event.task.priority == 'p0'"
    then:
      - log: "High priority review!"
"#;

        let hook = HookParser::parse_str(yaml).unwrap();

        // Should expand with all statements inside the if wrapper
        assert_eq!(hook.task_started.len(), 1);

        match &hook.task_started[0] {
            HookStatement::If(if_stmt) => {
                assert_eq!(if_stmt.condition, "$event.task.type == \"review\"");
                // Should have 3 statements inside the then branch
                assert_eq!(if_stmt.then.len(), 3);
            }
            _ => panic!("Expected If statement"),
        }
    }
}

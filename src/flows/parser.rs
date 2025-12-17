use anyhow::{Context, Result};

use super::types::Flow;

/// Parser for flow YAML files
pub struct FlowParser;

impl FlowParser {
    /// Parse a flow from a YAML string
    pub fn parse_str(yaml: &str) -> Result<Flow> {
        serde_yaml::from_str(yaml).context("Failed to parse flow YAML")
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

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.name, "Test Flow");
        assert_eq!(flow.version, "1");
        assert!(flow.write_completed.is_empty());
        assert!(flow.commit_message_started.is_empty());
    }

    #[test]
    fn test_parse_flow_with_shell_action() {
        let yaml = r#"
name: Lint Flow
version: "1"
write.completed:
  - shell: ruff check $event.file_paths
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.name, "Lint Flow");
        assert_eq!(flow.write_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_jj_action() {
        let yaml = r#"
name: JJ Flow
version: "1"
write.completed:
  - jj: describe -m "AI generated change"
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_log_action() {
        let yaml = r#"
name: Log Flow
version: "1"
write.completed:
  - log: "File edited: $event.file_paths"
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_multiple_actions() {
        let yaml = r#"
name: Multi Action Flow
version: "1"
write.completed:
  - shell: echo "Starting"
  - log: "Processing file"
  - jj: describe -m "Done"
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 3);
    }

    #[test]
    fn test_parse_flow_with_on_failure() {
        let yaml = r#"
name: Failure Handling Flow
version: "1"
write.completed:
  - shell: ruff check .
    on_failure:
      - stop: "Ruff check failed"
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_timeout() {
        let yaml = r#"
name: Timeout Flow
version: "1"
write.completed:
  - shell: pytest
    timeout: 60s
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 1);
    }

    #[test]
    fn test_parse_flow_with_multiple_events() {
        let yaml = r#"
name: Multi Event Flow
version: "1"
write.completed:
  - shell: ruff check $event.file_paths
commit.message_started:
  - shell: pytest
session.started:
  - log: "Session started"
session.ended:
  - log: "Session ended"
"#;

        let flow = FlowParser::parse_str(yaml).unwrap();
        assert_eq!(flow.write_completed.len(), 1);
        assert_eq!(flow.commit_message_started.len(), 1);
        assert_eq!(flow.session_started.len(), 1);
        assert_eq!(flow.session_ended.len(), 1);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = r#"
name: Invalid Flow
this is not valid yaml: [
"#;

        let result = FlowParser::parse_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_name() {
        let yaml = r#"
version: "1"
write.completed:
  - shell: echo "test"
"#;

        let result = FlowParser::parse_str(yaml);
        assert!(result.is_err());
    }
}

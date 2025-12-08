use anyhow::Result;

use super::parser::FlowParser;
use super::types::Flow;

/// Load the core system flow
///
/// The core flow is embedded in the binary and handles both Start and PostFileChange events.
pub fn load_core_flow() -> Result<Flow> {
    let core_yaml = include_str!("core/flow.yaml");
    FlowParser::parse_str(core_yaml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::types::FlowStatement;

    #[test]
    fn test_load_core_flow() {
        let core = load_core_flow().unwrap();
        assert_eq!(core.name, "Aiki Core");
    }

    #[test]
    fn test_core_flow_has_start() {
        let core = load_core_flow().unwrap();

        // Should have Start handler
        assert!(!core.session_start.is_empty());
    }

    #[test]
    fn test_core_flow_has_post_file_change() {
        let core = load_core_flow().unwrap();

        // Should have PostFileChange handler
        assert!(!core.post_file_change.is_empty());
    }

    #[test]
    fn test_core_flow_has_pre_file_change() {
        let core = load_core_flow().unwrap();

        // Should have PreFileChange handler
        assert!(!core.pre_file_change.is_empty());
    }

    #[test]
    fn test_core_flow_metadata() {
        let core = load_core_flow().unwrap();

        assert_eq!(core.name, "Aiki Core");
        assert_eq!(core.version, "1");
    }

    #[test]
    fn test_core_flow_uses_let_syntax() {
        use super::super::types::Action;

        let core = load_core_flow().unwrap();

        // Should have PostFileChange handler
        assert!(!core.post_file_change.is_empty());

        // First statement should be an If with inline function call to classify_edits
        match &core.post_file_change[0] {
            FlowStatement::If(if_stmt) => {
                // Verify uses inline self.classify_edits for condition
                assert!(
                    if_stmt.condition.contains("self.classify_edits"),
                    "Flow should use inline 'self.classify_edits' in condition"
                );
                // First statement in then block should be the prepare_separation let binding
                assert!(!if_stmt.then.is_empty());
                match &if_stmt.then[0] {
                    FlowStatement::Action(Action::Let(let_action)) => {
                        assert_eq!(
                            let_action.let_, "prep = self.prepare_separation",
                            "Flow should prepare separation when user edits are detected"
                        );
                    }
                    _ => panic!("Expected Let action as first step in then block"),
                }
            }
            _ => panic!("Expected If statement as first step"),
        }
    }
}

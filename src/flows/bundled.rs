use anyhow::Result;

use super::parser::FlowParser;
use super::types::Flow;

/// Load the core system flow (uncached).
///
/// The core flow is embedded in the binary and handles all event types.
/// This function parses the YAML on every call. For cached access, use
/// `crate::cache::get_core_flow()` instead.
///
/// # Note
///
/// This is the uncached version used internally by the cache module.
/// Most code should use `load_core_flow()` which returns a cached reference.
pub fn load_core_flow_uncached() -> Result<Flow> {
    let core_yaml = include_str!("core/flow.yaml");
    FlowParser::parse_str(core_yaml)
}

/// Load the core system flow (cached).
///
/// Returns a reference to the cached core flow that is parsed once per process.
/// This is the preferred entry point for accessing the core flow.
#[must_use]
pub fn load_core_flow() -> &'static Flow {
    crate::cache::get_core_flow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::types::FlowStatement;

    #[test]
    fn test_load_core_flow_uncached() {
        // Test the uncached version directly
        let core = load_core_flow_uncached().unwrap();
        assert_eq!(core.name, "Aiki Core");
    }

    #[test]
    fn test_load_core_flow_cached() {
        // Test the cached version
        let core = load_core_flow();
        assert_eq!(core.name, "Aiki Core");
    }

    #[test]
    fn test_core_flow_has_start() {
        let core = load_core_flow();

        // Should have session.started handler
        assert!(!core.session_started.is_empty());
    }

    #[test]
    fn test_core_flow_has_file_completed() {
        let core = load_core_flow();

        // Should have file.completed handler
        assert!(!core.file_completed.is_empty());
    }

    #[test]
    fn test_core_flow_has_file_permission_asked() {
        let core = load_core_flow();

        // Should have file.permission_asked handler
        assert!(!core.file_permission_asked.is_empty());
    }

    #[test]
    fn test_core_flow_metadata() {
        let core = load_core_flow();

        assert_eq!(core.name, "Aiki Core");
        assert_eq!(core.version, "1");
    }

    #[test]
    fn test_core_flow_uses_let_syntax() {
        use super::super::types::Action;

        let core = load_core_flow();

        // Should have file.completed handler
        assert!(!core.file_completed.is_empty());

        // First statement is the operation check: if: $event.operation == "write"
        match &core.file_completed[0] {
            FlowStatement::If(outer_if) => {
                // Outer if checks for write operation
                assert!(
                    outer_if.condition.contains("$event.operation"),
                    "Flow should check event.operation first"
                );

                // Inside that, first statement should be the classify_edits check
                assert!(!outer_if.then.is_empty());
                match &outer_if.then[0] {
                    FlowStatement::If(classify_if) => {
                        // Verify uses inline self.classify_edits for condition
                        assert!(
                            classify_if.condition.contains("self.classify_edits"),
                            "Flow should use inline 'self.classify_edits' in condition"
                        );
                        // First statement in then block should be the prepare_separation let binding
                        assert!(!classify_if.then.is_empty());
                        match &classify_if.then[0] {
                            FlowStatement::Action(Action::Let(let_action)) => {
                                assert_eq!(
                                    let_action.let_, "prep = self.prepare_separation",
                                    "Flow should prepare separation when user edits are detected"
                                );
                            }
                            _ => panic!("Expected Let action as first step in then block"),
                        }
                    }
                    _ => panic!("Expected If statement for classify_edits"),
                }
            }
            _ => panic!("Expected If statement as first step"),
        }
    }
}

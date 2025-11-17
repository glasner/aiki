use anyhow::Result;

use super::parser::FlowParser;
use super::types::Flow;

/// Load the core system flow
///
/// The core flow is embedded in the binary and handles both Start and PostChange events.
pub fn load_core_flow() -> Result<Flow> {
    let core_yaml = include_str!("core/core.yaml");
    FlowParser::parse_str(core_yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_core_flow() {
        let core = load_core_flow().unwrap();
        assert_eq!(core.name, "Aiki Core");
    }

    #[test]
    fn test_core_flow_has_start() {
        let core = load_core_flow().unwrap();

        // Should have Start handler
        assert!(!core.start.is_empty());
    }

    #[test]
    fn test_core_flow_has_post_change() {
        let core = load_core_flow().unwrap();

        // Should have PostChange handler
        assert!(!core.post_change.is_empty());
    }

    #[test]
    fn test_core_flow_metadata() {
        let core = load_core_flow().unwrap();

        assert_eq!(core.name, "Aiki Core");
        assert_eq!(core.version, "1");
    }

    #[test]
    fn test_core_flow_uses_let_syntax() {
        use super::super::types::{Action, FailureMode};

        let core = load_core_flow().unwrap();

        // Should have PostChange handler with let action
        assert!(!core.post_change.is_empty());

        // First action should be a Let binding with self reference
        match &core.post_change[0] {
            Action::Let(let_action) => {
                // Verify uses self reference for portability
                assert_eq!(
                    let_action.let_, "description = self.build_description",
                    "Flow should use 'self.build_description' for portability"
                );
                assert_eq!(let_action.on_failure, FailureMode::Fail);
            }
            _ => panic!("Expected Let action as first step"),
        }
    }
}

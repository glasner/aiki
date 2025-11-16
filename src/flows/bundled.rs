use anyhow::Result;
use std::collections::HashMap;

use super::parser::FlowParser;
use super::types::Flow;

/// Load all bundled system flows
///
/// System flows are embedded in the binary and loaded at runtime.
/// These flows are in the "aiki" namespace and cannot be overridden by users.
pub fn load_system_flows() -> Result<HashMap<String, Flow>> {
    let mut flows = HashMap::new();

    // Load init flow (runs first on Start to ensure repo is initialized)
    let init_yaml = include_str!("../../flows/init.yaml");
    let init_flow = FlowParser::parse_str(init_yaml)?;
    flows.insert("aiki/init".to_string(), init_flow);

    // Load provenance flow (runs on PostChange to record metadata)
    let provenance_yaml = include_str!("../../flows/provenance.yaml");
    let provenance_flow = FlowParser::parse_str(provenance_yaml)?;
    flows.insert("aiki/provenance".to_string(), provenance_flow);

    Ok(flows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_system_flows() {
        let flows = load_system_flows().unwrap();
        assert!(flows.contains_key("aiki/init"));
        assert!(flows.contains_key("aiki/provenance"));
    }

    #[test]
    fn test_init_flow_has_start() {
        let flows = load_system_flows().unwrap();
        let init = &flows["aiki/init"];

        // Should have Start handler
        assert!(!init.start.is_empty());
    }

    #[test]
    fn test_init_flow_metadata() {
        let flows = load_system_flows().unwrap();
        let init = &flows["aiki/init"];

        assert_eq!(init.name, "Aiki Init");
        assert_eq!(init.version, "1");
    }

    #[test]
    fn test_provenance_flow_has_post_change() {
        let flows = load_system_flows().unwrap();
        let provenance = &flows["aiki/provenance"];

        // Should have PostChange handler
        assert!(!provenance.post_change.is_empty());
    }

    #[test]
    fn test_provenance_flow_metadata() {
        let flows = load_system_flows().unwrap();
        let provenance = &flows["aiki/provenance"];

        assert_eq!(provenance.name, "Aiki Provenance Recording");
        assert_eq!(provenance.version, "2"); // Updated to v2 for let syntax migration
    }

    #[test]
    fn test_provenance_flow_uses_let_syntax() {
        use super::super::types::{Action, FailureMode};

        let flows = load_system_flows().unwrap();
        let provenance = &flows["aiki/provenance"];

        // Should have PostChange handler with let action
        assert!(!provenance.post_change.is_empty());

        // First action should be a Let binding with self reference
        match &provenance.post_change[0] {
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

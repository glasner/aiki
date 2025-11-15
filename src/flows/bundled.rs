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

    // Load provenance flow
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
        assert!(flows.contains_key("aiki/provenance"));
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
        assert_eq!(provenance.version, "1");
    }
}

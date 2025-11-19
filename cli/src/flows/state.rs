use std::collections::HashMap;

/// Result of executing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ActionResult {
    #[must_use]
    pub fn success() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    #[must_use]
    pub fn failure(exit_code: i32, stderr: String) -> Self {
        Self {
            success: false,
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr,
        }
    }
}

/// Aiki execution state for flow processing
///
/// This holds the mutable state that accumulates during flow execution:
/// - The original triggering event (immutable)
/// - Let-bound variables computed during execution
/// - Metadata about action results
/// - Current flow context
#[derive(Debug, Clone)]
pub struct AikiState {
    /// The original event that triggered this execution
    pub event: crate::events::AikiEvent,

    /// Let-bound variables (user-defined, accessed without $event prefix)
    let_vars: HashMap<String, String>,

    /// Structured metadata for variables (stores ActionResult for each variable)
    variable_metadata: HashMap<String, ActionResult>,

    /// Current flow name (e.g., "aiki/core") for self references
    pub flow_name: Option<String>,
}

impl AikiState {
    #[must_use]
    pub fn new(event: impl Into<crate::events::AikiEvent>) -> Self {
        Self {
            event: event.into(),
            let_vars: HashMap::new(),
            variable_metadata: HashMap::new(),
            flow_name: None,
        }
    }

    /// Helper to get the current working directory
    #[must_use]
    pub fn cwd(&self) -> &std::path::Path {
        self.event.cwd()
    }

    /// Helper to get the agent type
    #[must_use]
    pub fn agent_type(&self) -> crate::provenance::AgentType {
        self.event.agent_type()
    }

    /// Get a variable value by name
    #[must_use]
    pub fn get_variable(&self, name: &str) -> Option<&String> {
        self.let_vars.get(name)
    }

    /// Set a variable value
    pub fn set_variable(&mut self, name: String, value: String) {
        self.let_vars.insert(name, value);
    }

    /// Iterate over all variables (for VariableResolver)
    pub fn iter_variables(&self) -> impl Iterator<Item = (&String, &String)> {
        self.let_vars.iter()
    }

    /// Store an action result with its metadata
    ///
    /// This stores both the primary value and structured metadata for a variable.
    pub fn store_action_result(&mut self, name: String, result: ActionResult) {
        // Store the primary value
        self.let_vars.insert(name.clone(), result.stdout.clone());

        // Store structured metadata
        self.variable_metadata.insert(name, result);
    }

    /// Get metadata for a variable (test-only)
    #[cfg(test)]
    pub fn get_metadata(&self, name: &str) -> Option<&ActionResult> {
        self.variable_metadata.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure(1, "error".to_string());
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(result.stderr, "error");
    }

    #[test]
    fn test_execution_context_with_event() {
        use crate::events::{AikiEvent, AikiPostChangeEvent};
        use crate::provenance::AgentType;

        let event = AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_path: "/test/file.rs".to_string(),
            cwd: std::path::PathBuf::from("/test"),
            timestamp: chrono::Utc::now(),
        });
        let ctx = AikiState::new(event);

        // Verify we can access event fields through the enum
        match &ctx.event {
            AikiEvent::PostChange(e) => {
                assert_eq!(e.file_path, "/test/file.rs");
            }
            _ => panic!("Expected PostChange event"),
        }
        assert_eq!(ctx.cwd(), std::path::Path::new("/test"));
    }
}

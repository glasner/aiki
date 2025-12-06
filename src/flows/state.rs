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
/// - Prompt assembler (for PrePrompt events)
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

    /// Context assembler for events that build messages
    /// - PrePrompt: accumulates prompt modifications and context
    /// - PostResponse: accumulates autoreply content
    context_assembler: Option<crate::flows::context::ContextAssembler>,

    /// Messages emitted by the flow (info, warning, error)
    messages: Vec<crate::handlers::Message>,
}

impl AikiState {
    #[must_use]
    pub fn new(event: impl Into<crate::events::AikiEvent>) -> Self {
        let event = event.into();

        // Initialize context assembler based on event type
        let context_assembler = match &event {
            crate::events::AikiEvent::PrePrompt(e) => {
                // PrePrompt: start with original prompt
                Some(crate::flows::context::ContextAssembler::new(
                    Some(e.prompt.clone()),
                    "\n\n",
                ))
            }
            crate::events::AikiEvent::PostResponse(_) => {
                // PostResponse: build autoreply from scratch
                Some(crate::flows::context::ContextAssembler::new(None, "\n\n"))
            }
            _ => None,
        };

        Self {
            event,
            let_vars: HashMap::new(),
            variable_metadata: HashMap::new(),
            flow_name: None,
            context_assembler,
            messages: Vec::new(),
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

    /// Get mutable reference to the context assembler
    /// Only available for PrePrompt and PostResponse events
    pub fn get_context_assembler_mut(
        &mut self,
    ) -> crate::error::Result<&mut crate::flows::context::ContextAssembler> {
        self.context_assembler.as_mut().ok_or_else(|| {
            crate::error::AikiError::Other(anyhow::anyhow!(
                "Context assembler not available (not a PrePrompt or PostResponse event)"
            ))
        })
    }

    /// Build the final context from accumulated chunks
    /// Works for PrePrompt (builds prompt) and PostResponse (builds autoreply)
    /// Returns None if this event doesn't have a context assembler
    #[must_use]
    pub fn build_context(&self) -> Option<String> {
        self.context_assembler
            .as_ref()
            .map(|assembler| assembler.build())
    }

    /// Add a message to the messages list
    pub fn add_message(&mut self, message: crate::handlers::Message) {
        self.messages.push(message);
    }

    /// Take all messages (consumes and returns them, leaving empty Vec)
    pub fn take_messages(&mut self) -> Vec<crate::handlers::Message> {
        std::mem::take(&mut self.messages)
    }

    /// Get a reference to the messages
    #[must_use]
    pub fn messages(&self) -> &[crate::handlers::Message] {
        &self.messages
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
        use crate::events::{AikiEvent, AikiPostFileChangeEvent};
        use crate::provenance::AgentType;

        let event = AikiEvent::PostFileChange(AikiPostFileChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/test/file.rs".to_string()],
            cwd: std::path::PathBuf::from("/test"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![],
        });
        let ctx = AikiState::new(event);

        // Verify we can access event fields through the enum
        match &ctx.event {
            AikiEvent::PostFileChange(e) => {
                assert_eq!(e.file_paths, vec!["/test/file.rs".to_string()]);
            }
            _ => panic!("Expected PostFileChange event"),
        }
        assert_eq!(ctx.cwd(), std::path::Path::new("/test"));
    }
}

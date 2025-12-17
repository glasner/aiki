use std::collections::HashMap;

/// Result of executing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
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
    /// - prompt.submitted: accumulates prompt modifications and context
    /// - response.received: accumulates autoreply content
    context_assembler: Option<crate::flows::context::ContextAssembler>,

    /// Failure messages emitted by the flow
    failures: Vec<crate::events::result::Failure>,
}

impl AikiState {
    #[must_use]
    pub fn new(event: impl Into<crate::events::AikiEvent>) -> Self {
        let event = event.into();

        // Initialize context assembler based on event type
        let context_assembler = match &event {
            crate::events::AikiEvent::PromptSubmitted(e) => {
                // prompt.submitted: start with original prompt
                Some(crate::flows::context::ContextAssembler::new(
                    Some(e.prompt.clone()),
                    "\n\n",
                ))
            }
            crate::events::AikiEvent::ResponseReceived(_) => {
                // response.received: build autoreply from scratch
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
            failures: Vec::new(),
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
    /// Only available for prompt.submitted and response.received events
    pub fn get_context_assembler_mut(
        &mut self,
    ) -> crate::error::Result<&mut crate::flows::context::ContextAssembler> {
        self.context_assembler.as_mut().ok_or_else(|| {
            crate::error::AikiError::Other(anyhow::anyhow!(
                "Context assembler not available (not a prompt.submitted or response.received event)"
            ))
        })
    }

    /// Build the final context from accumulated chunks
    /// Works for prompt.submitted (builds prompt) and response.received (builds autoreply)
    /// Returns None if:
    /// - This event doesn't have a context assembler, OR
    /// - The assembler is empty (no Context actions were executed)
    #[must_use]
    pub fn build_context(&self) -> Option<String> {
        self.context_assembler.as_ref().and_then(|assembler| {
            if assembler.is_empty() {
                None
            } else {
                Some(assembler.build())
            }
        })
    }

    /// Add a failure to the failures list
    pub fn add_failure(&mut self, failure: crate::events::result::Failure) {
        self.failures.push(failure);
    }

    /// Take all failures (consumes and returns them, leaving empty Vec)
    pub fn take_failures(&mut self) -> Vec<crate::events::result::Failure> {
        std::mem::take(&mut self.failures)
    }

    /// Get a reference to the failures
    #[must_use]
    pub fn failures(&self) -> &[crate::events::result::Failure] {
        &self.failures
    }

    /// Get the number of failures
    #[must_use]
    pub fn failures_count(&self) -> usize {
        self.failures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result_success() {
        let result = ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error".to_string(),
        };
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(result.stderr, "error");
    }

    #[test]
    fn test_execution_context_with_event() {
        use crate::events::{AikiEvent, AikiWriteCompletedPayload};
        use crate::provenance::AgentType;
        use crate::session::AikiSession;

        let session = AikiSession::new(
            AgentType::Claude,
            "test-session".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        let event = AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
            session,
            cwd: std::path::PathBuf::from("/test"),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/test/file.rs".to_string()],
            success: true,
            edit_details: vec![],
        });
        let ctx = AikiState::new(event);

        // Verify we can access event fields through the enum
        match &ctx.event {
            AikiEvent::WriteCompleted(e) => {
                assert_eq!(e.file_paths, vec!["/test/file.rs".to_string()]);
            }
            _ => panic!("Expected WriteCompleted event"),
        }
        assert_eq!(ctx.cwd(), std::path::Path::new("/test"));
    }
}

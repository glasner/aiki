use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A complete flow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    /// Flow name
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Flow version
    #[serde(default = "default_version")]
    pub version: String,

    /// PostChange event handler
    #[serde(rename = "PostChange", default)]
    pub post_change: Vec<Action>,

    /// PreCommit event handler
    #[serde(rename = "PreCommit", default)]
    pub pre_commit: Vec<Action>,

    /// Start event handler
    #[serde(rename = "Start", default)]
    pub start: Vec<Action>,

    /// Stop event handler
    #[serde(rename = "Stop", default)]
    pub stop: Vec<Action>,
}

fn default_version() -> String {
    "1".to_string()
}

/// An action to execute in a flow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    /// Shell command
    Shell(ShellAction),
    /// JJ command
    Jj(JjAction),
    /// Log message
    Log(LogAction),
    /// Call a built-in Aiki function
    Aiki(AikiAction),
}

/// Shell command action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAction {
    pub shell: String,

    #[serde(default)]
    pub timeout: Option<String>,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// JJ command action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JjAction {
    pub jj: String,

    #[serde(default)]
    pub timeout: Option<String>,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Log message action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAction {
    pub log: String,
}

/// Aiki built-in function call action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiAction {
    pub aiki: String,

    #[serde(default)]
    pub args: HashMap<String, String>,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// What to do when an action fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FailureMode {
    /// Continue to next action (default)
    Continue,
    /// Stop flow execution and return error
    Fail,
}

fn default_on_failure() -> FailureMode {
    FailureMode::Continue
}

/// Result of executing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ActionResult {
    pub fn success() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    pub fn failure(exit_code: i32, stderr: String) -> Self {
        Self {
            success: false,
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr,
        }
    }
}

/// Execution context for a flow
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Current working directory
    pub cwd: std::path::PathBuf,

    /// Event-specific variables ($event.*)
    pub event_vars: HashMap<String, String>,

    /// Environment variables to pass to shell commands
    pub env_vars: HashMap<String, String>,
}

impl ExecutionContext {
    pub fn new(cwd: std::path::PathBuf) -> Self {
        Self {
            cwd,
            event_vars: HashMap::new(),
            env_vars: std::env::vars().collect(),
        }
    }

    pub fn with_event_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.event_vars.insert(key.into(), value.into());
        self
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
    fn test_execution_context_with_event_var() {
        let ctx = ExecutionContext::new(std::path::PathBuf::from("/test"))
            .with_event_var("file_path", "/test/file.rs");

        assert_eq!(
            ctx.event_vars.get("file_path"),
            Some(&"/test/file.rs".to_string())
        );
    }

    #[test]
    fn test_default_failure_mode() {
        assert_eq!(default_on_failure(), FailureMode::Continue);
    }
}

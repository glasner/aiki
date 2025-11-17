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
    /// Let binding (function call or variable aliasing)
    Let(LetAction),
    /// Call a built-in Aiki function (deprecated, use Let)
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

    /// Optional variable name to store the result
    #[serde(default)]
    pub alias: Option<String>,
}

/// JJ command action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JjAction {
    pub jj: String,

    #[serde(default)]
    pub timeout: Option<String>,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,

    /// Optional variable name to store the result
    #[serde(default)]
    pub alias: Option<String>,
}

/// Log message action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAction {
    pub log: String,

    /// Optional variable name to store the result
    #[serde(default)]
    pub alias: Option<String>,
}

/// Let binding action (function call or variable aliasing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetAction {
    /// The let binding in format "variable = expression"
    #[serde(rename = "let")]
    pub let_: String,

    /// What to do when the action fails
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
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
    Stop,
}

fn default_on_failure() -> FailureMode {
    FailureMode::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_failure_mode() {
        assert_eq!(default_on_failure(), FailureMode::Continue);
    }
}

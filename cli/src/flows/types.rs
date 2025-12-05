use serde::{Deserialize, Serialize};

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

    /// SessionStart event handler
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<Action>,

    /// PrePrompt event handler (before agent sees the user's prompt)
    #[serde(rename = "PrePrompt", default)]
    pub pre_prompt: Vec<Action>,

    /// PreFileChange event handler (before file modification begins)
    #[serde(rename = "PreFileChange", default)]
    pub pre_file_change: Vec<Action>,

    /// PostFileChange event handler
    #[serde(rename = "PostFileChange", default)]
    pub post_file_change: Vec<Action>,

    /// PostResponse event handler (after agent completes its response)
    #[serde(rename = "PostResponse", default)]
    pub post_response: Vec<Action>,

    /// PrepareCommitMessage event handler (Git's prepare-commit-msg hook)
    #[serde(rename = "PrepareCommitMessage", default)]
    pub prepare_commit_message: Vec<Action>,

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
    /// Conditional execution (if/then/else)
    If(IfAction),
    /// Switch/case statement
    Switch(SwitchAction),
    /// Shell command
    Shell(ShellAction),
    /// JJ command
    Jj(JjAction),
    /// Log message
    Log(LogAction),
    /// Let binding (function call or variable aliasing)
    Let(LetAction),
    /// Self function call (call a function without storing result)
    Self_(SelfAction),
    /// Context injection (for PrePrompt events) - NEW: replaces Prompt
    Context(ContextAction),
    /// Prompt modification (for PrePrompt events) - DEPRECATED: use Context instead
    Prompt(PromptAction),
    /// Autoreply (for PostResponse events)
    Autoreply(AutoreplyAction),
    /// Commit message (for PrepareCommitMessage events)
    CommitMessage(CommitMessageAction),
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

    /// Optional author to set for this command via JJ_USER and JJ_EMAIL
    /// Format: "Name <email>"
    #[serde(default)]
    pub with_author: Option<String>,

    /// Optional function that returns {author, message}
    /// Automatically sets with_author and makes $message available
    #[serde(default)]
    pub with_author_and_message: Option<String>,
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

/// Self function call action (calls a function without storing result)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfAction {
    /// The function to call in format "self.function_name"
    /// Example: "self.write_ai_files"
    #[serde(rename = "self")]
    pub self_: String,

    /// What to do when the action fails
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Conditional action (if/then/else)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfAction {
    /// Condition to evaluate (supports variable access with $, JSON field access with .)
    /// Examples: "$detection.all_exact_match == true", "$metadata.tool == Edit"
    #[serde(rename = "if")]
    pub condition: String,

    /// Actions to execute if condition is true
    pub then: Vec<Action>,

    /// Optional actions to execute if condition is false
    #[serde(default, rename = "else")]
    pub else_: Option<Vec<Action>>,

    /// What to do when condition evaluation fails
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Switch/case action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchAction {
    /// Expression to evaluate and match against cases
    /// Examples: "$detection.classification", "$metadata.tool"
    #[serde(rename = "switch")]
    pub expression: String,

    /// Map of case values to actions
    /// The key is matched against the evaluated expression
    pub cases: std::collections::HashMap<String, Vec<Action>>,

    /// Optional default case if no cases match
    #[serde(default)]
    pub default: Option<Vec<Action>>,

    /// What to do when switch evaluation fails
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Context action (for PrePrompt events)
/// Injects context that is prepended to the user's prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAction {
    /// The context content to inject
    pub context: ContextContent,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Content for context action
/// Can be a simple string (defaults to append) or explicit prepend/append
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextContent {
    /// Simple form: defaults to append
    /// YAML: `context: "text"`
    Simple(String),

    /// Explicit form with prepend/append
    /// YAML: `context: { prepend: "...", append: "..." }`
    Explicit {
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        append: Option<String>,
    },
}

/// Prompt action (for PrePrompt events) - DEPRECATED
/// Modifies the user's prompt before it's sent to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAction {
    /// The prompt modification content (MessageChunk)
    pub prompt: PromptContent,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Content for prompt action
/// Can be a simple string (defaults to append) or explicit prepend/append
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptContent {
    /// Short form: defaults to append
    /// YAML: `prompt: "text"`
    Simple(String),

    /// Explicit form with prepend/append
    /// YAML: `prompt: { prepend: "...", append: "..." }`
    Explicit(crate::flows::messages::MessageChunk),
}

/// Autoreply action (for PostResponse events)
/// Sends an additional message to the agent after it completes its response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoreplyAction {
    /// The autoreply content (MessageChunk)
    pub autoreply: AutoreplyContent,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Content for autoreply action
/// Can be a simple string or explicit prepend/append
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutoreplyContent {
    /// Short form: simple text message
    /// YAML: `autoreply: "text"`
    Simple(String),

    /// Explicit form with prepend/append
    /// YAML: `autoreply: { prepend: "...", append: "..." }`
    Explicit {
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        append: Option<String>,
    },
}

/// Commit message action (for PrepareCommitMessage events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessageAction {
    pub commit_message: CommitMessageOp,

    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Operations for commit messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessageOp {
    /// Append Git trailers (after existing trailers)
    #[serde(default)]
    pub append_trailer: Option<String>,

    /// Append to message body (before trailers)
    #[serde(default)]
    pub append_body: Option<String>,

    /// Prepend to subject line (before first line)
    #[serde(default)]
    pub prepend_subject: Option<String>,

    /// Append footer (after everything)
    #[serde(default)]
    pub append_footer: Option<String>,
}

/// What to do when an action fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FailureMode {
    /// Continue to next action (default)
    Continue,
    /// Stop flow execution (silent, no error to editor)
    Stop,
    /// Stop flow and block editor operation (exit 2)
    Block,
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

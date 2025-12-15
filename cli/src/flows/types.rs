use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::flows::context::TextLines;

/// Flow control statement - top level unit of execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlowStatement {
    /// Conditional if/then/else
    If(IfStatement),
    /// Switch/case statement
    Switch(SwitchStatement),
    /// Action to execute
    Action(Action),
}

/// Conditional if statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStatement {
    /// Condition to evaluate
    #[serde(rename = "if")]
    pub condition: String,

    /// Statements to execute if condition is true
    pub then: Vec<FlowStatement>,

    /// Optional statements to execute if condition is false
    #[serde(default, rename = "else")]
    pub else_: Option<Vec<FlowStatement>>,
}

/// Switch/case statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchStatement {
    /// Expression to evaluate and match against cases
    #[serde(rename = "switch")]
    pub expression: String,

    /// Map of case values to statements
    pub cases: HashMap<String, Vec<FlowStatement>>,

    /// Optional default case if no cases match
    #[serde(default)]
    pub default: Option<Vec<FlowStatement>>,
}

/// Strongly-typed shortcuts for on_failure behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnFailureShortcut {
    /// Continue: Log failure and continue execution (default)
    Continue,
    /// Stop: Add failure and stop flow silently
    Stop,
    /// Block: Add failure and block operation with exit code 2
    Block,
}

/// On-failure behavior for actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFailure {
    /// Shortcut: "continue", "stop", or "block"
    Shortcut(OnFailureShortcut),
    /// Full statements list for complex failure handling
    Statements(Vec<FlowStatement>),
}

impl Default for OnFailure {
    fn default() -> Self {
        OnFailure::Shortcut(OnFailureShortcut::Continue)
    }
}

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

    /// session.started event handler
    #[serde(rename = "session.started", default)]
    pub session_started: Vec<FlowStatement>,

    /// prompt.submitted event handler (user submitted a prompt to the agent)
    #[serde(rename = "prompt.submitted", default)]
    pub prompt_submitted: Vec<FlowStatement>,

    /// change.permission_asked event handler (agent is about to modify a file)
    #[serde(rename = "change.permission_asked", default)]
    pub change_permission_asked: Vec<FlowStatement>,

    /// change.done event handler (agent finished modifying a file)
    #[serde(rename = "change.done", default)]
    pub change_done: Vec<FlowStatement>,

    /// response.received event handler (agent finished responding)
    #[serde(rename = "response.received", default)]
    pub response_received: Vec<FlowStatement>,

    /// session.ended event handler (agent session terminated)
    #[serde(rename = "session.ended", default)]
    pub session_ended: Vec<FlowStatement>,

    /// git.prepare_commit_message event handler (Git's prepare-commit-msg hook)
    #[serde(rename = "git.prepare_commit_message", default)]
    pub git_prepare_commit_message: Vec<FlowStatement>,

    /// Stop event handler (legacy - kept for compatibility)
    #[serde(rename = "Stop", default)]
    pub stop: Vec<FlowStatement>,
}

fn default_version() -> String {
    "1".to_string()
}

/// Continue flow execution action (generates Failure and continues)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinueAction {
    #[serde(rename = "continue")]
    pub failure: String,
}

/// Stop flow execution action (generates Failure and stops silently)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopAction {
    #[serde(rename = "stop")]
    pub failure: String,
}

/// Block editor operation action (generates Failure and blocks with exit 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAction {
    #[serde(rename = "block")]
    pub failure: String,
}

/// An action to execute in a flow (no flow control)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    /// Let binding (function call or variable aliasing)
    Let(LetAction),
    /// Self function call (call a function without storing result)
    Self_(SelfAction),
    /// Shell command
    Shell(ShellAction),
    /// JJ command
    Jj(JjAction),
    /// Log message
    Log(LogAction),
    /// Context injection (for PrePrompt events)
    Context(ContextAction),
    /// Autoreply (for SessionEnd events)
    Autoreply(AutoreplyAction),
    /// Commit message (for PrepareCommitMessage events)
    CommitMessage(CommitMessageAction),
    /// Continue flow execution (generates Failure and returns FailedContinue)
    Continue(ContinueAction),
    /// Stop flow execution (emits warning and stops silently)
    Stop(StopAction),
    /// Block editor operation (emits error and blocks with exit 2)
    Block(BlockAction),
}

/// Shell command action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAction {
    pub shell: String,

    #[serde(default)]
    pub timeout: Option<String>,

    #[serde(default)]
    pub on_failure: OnFailure,

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

    #[serde(default)]
    pub on_failure: OnFailure,

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
    #[serde(default)]
    pub on_failure: OnFailure,
}

/// Self function call action (calls a function without storing result)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfAction {
    /// The function to call in format "self.function_name"
    /// Example: "self.write_ai_files"
    #[serde(rename = "self")]
    pub self_: String,

    /// What to do when the action fails
    #[serde(default)]
    pub on_failure: OnFailure,
}

/// Context action (for PrePrompt events)
/// Injects context that is prepended to the user's prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAction {
    /// The context content to inject
    pub context: ContextContent,

    #[serde(default)]
    pub on_failure: OnFailure,
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
    /// Supports both scalar and array forms:
    /// `context: { prepend: "single line" }`
    /// `context: { prepend: ["line 1", "line 2"] }`
    Explicit {
        #[serde(default)]
        prepend: Option<TextLines>,
        #[serde(default)]
        append: Option<TextLines>,
    },
}

/// Autoreply action (for PostResponse events)
/// Sends an additional message to the agent after it completes its response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoreplyAction {
    /// The autoreply content (MessageChunk)
    pub autoreply: AutoreplyContent,

    #[serde(default)]
    pub on_failure: OnFailure,
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
    /// Supports both scalar and array forms:
    /// `autoreply: { prepend: "single line" }`
    /// `autoreply: { prepend: ["line 1", "line 2"] }`
    Explicit {
        #[serde(default)]
        prepend: Option<TextLines>,
        #[serde(default)]
        append: Option<TextLines>,
    },
}

/// Commit message action (for PrepareCommitMessage events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessageAction {
    pub commit_message: CommitMessageOp,

    #[serde(default)]
    pub on_failure: OnFailure,
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

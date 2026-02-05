use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::flows::context::TextLines;

/// Hook control statement - top level unit of execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookStatement {
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
    pub then: Vec<HookStatement>,

    /// Optional statements to execute if condition is false
    #[serde(default, rename = "else")]
    pub else_: Option<Vec<HookStatement>>,
}

/// Switch/case statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchStatement {
    /// Expression to evaluate and match against cases
    #[serde(rename = "switch")]
    pub expression: String,

    /// Map of case values to statements
    pub cases: HashMap<String, Vec<HookStatement>>,

    /// Optional default case if no cases match
    #[serde(default)]
    pub default: Option<Vec<HookStatement>>,
}

/// Strongly-typed shortcuts for on_failure behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnFailureShortcut {
    /// Continue: Log failure and continue execution (default)
    Continue,
    /// Stop: Add failure and stop hook silently
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
    Statements(Vec<HookStatement>),
}

impl Default for OnFailure {
    fn default() -> Self {
        OnFailure::Shortcut(OnFailureShortcut::Continue)
    }
}

/// A complete hook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Hook name
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Hook version
    #[serde(default = "default_version")]
    pub version: String,

    // ========================================================================
    // Hook Composition (Milestone 1.3)
    // ========================================================================
    /// Hooks to run before this hook's actions (in order)
    /// Supports: {namespace}/{name} format (e.g., aiki/*, eslint/*, mycompany/*)
    #[serde(default)]
    pub before: Vec<String>,

    /// Hooks to run after this hook's actions (in order)
    /// Supports: {namespace}/{name} format (e.g., aiki/*, eslint/*, mycompany/*)
    #[serde(default)]
    pub after: Vec<String>,

    // ========================================================================
    // Session Lifecycle Events
    // ========================================================================
    /// session.started event handler (new agent session began)
    #[serde(rename = "session.started", default)]
    pub session_started: Vec<HookStatement>,

    /// session.resumed event handler (continuing a previous session)
    #[serde(rename = "session.resumed", default)]
    pub session_resumed: Vec<HookStatement>,

    /// session.ended event handler (agent session terminated)
    #[serde(rename = "session.ended", default)]
    pub session_ended: Vec<HookStatement>,

    // ========================================================================
    // Turn Lifecycle Events
    // ========================================================================
    /// turn.started event handler (turn began - user prompt or autoreply)
    #[serde(rename = "turn.started", default)]
    pub turn_started: Vec<HookStatement>,

    /// turn.completed event handler (turn ended - agent finished processing)
    #[serde(rename = "turn.completed", default)]
    pub turn_completed: Vec<HookStatement>,

    // ========================================================================
    // Read Operation Events
    // ========================================================================
    /// read.permission_asked event handler (agent is about to read a file)
    #[serde(rename = "read.permission_asked", default)]
    pub read_permission_asked: Vec<HookStatement>,

    /// read.completed event handler (agent finished reading a file)
    #[serde(rename = "read.completed", default)]
    pub read_completed: Vec<HookStatement>,

    // ========================================================================
    // Change Operation Events (Unified mutations: write, delete, move)
    // ========================================================================
    /// change.permission_asked event handler (agent is about to mutate files)
    /// Unified handler for write, delete, and move operations.
    /// Use `$event.write`, `$event.delete`, `$event.move` for operation-specific logic.
    #[serde(rename = "change.permission_asked", default)]
    pub change_permission_asked: Vec<HookStatement>,

    /// change.completed event handler (agent finished mutating files)
    /// Unified handler for write, delete, and move operations.
    /// Use `$event.write`, `$event.delete`, `$event.move` for operation-specific logic.
    #[serde(rename = "change.completed", default)]
    pub change_completed: Vec<HookStatement>,

    // ========================================================================
    // Shell Command Events
    // ========================================================================
    /// shell.permission_asked event handler (agent is about to execute a shell command)
    /// This is the autonomous review wedge - intercept git commit, run review, provide feedback
    #[serde(rename = "shell.permission_asked", default)]
    pub shell_permission_asked: Vec<HookStatement>,

    /// shell.completed event handler (shell command completed)
    #[serde(rename = "shell.completed", default)]
    pub shell_completed: Vec<HookStatement>,

    // ========================================================================
    // Web Access Events
    // ========================================================================
    /// web.permission_asked event handler (agent is about to make a web request)
    /// Operations: fetch, search
    #[serde(rename = "web.permission_asked", default)]
    pub web_permission_asked: Vec<HookStatement>,

    /// web.completed event handler (web request completed)
    #[serde(rename = "web.completed", default)]
    pub web_completed: Vec<HookStatement>,

    // ========================================================================
    // MCP Tool Events
    // ========================================================================
    /// mcp.permission_asked event handler (agent is about to call an MCP tool)
    #[serde(rename = "mcp.permission_asked", default)]
    pub mcp_permission_asked: Vec<HookStatement>,

    /// mcp.completed event handler (MCP tool call completed)
    #[serde(rename = "mcp.completed", default)]
    pub mcp_completed: Vec<HookStatement>,

    // ========================================================================
    // Commit Integration Events
    // ========================================================================
    /// commit.message_started event handler (Git's prepare-commit-msg hook)
    #[serde(rename = "commit.message_started", default)]
    pub commit_message_started: Vec<HookStatement>,

    // ========================================================================
    // Task Lifecycle Events
    // ========================================================================
    /// task.started event handler (task transitioned to in_progress)
    #[serde(rename = "task.started", default)]
    pub task_started: Vec<HookStatement>,

    /// task.closed event handler (task transitioned to closed)
    #[serde(rename = "task.closed", default)]
    pub task_closed: Vec<HookStatement>,

    // ========================================================================
    // Legacy
    // ========================================================================
    /// Stop event handler (legacy - kept for compatibility)
    #[serde(rename = "Stop", default)]
    pub stop: Vec<HookStatement>,
}

fn default_version() -> String {
    "1".to_string()
}

/// Continue hook execution action (generates Failure and continues)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinueAction {
    #[serde(rename = "continue")]
    pub failure: String,
}

/// Stop hook execution action (generates Failure and stops silently)
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

/// Session end action - terminates the current session gracefully
///
/// Used for task-driven sessions that should auto-end when their driving task closes.
/// Sends SIGTERM to the parent process (the agent) after a short delay to allow
/// the hook to complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndAction {
    /// Reason for ending (logged)
    #[serde(rename = "session.end")]
    pub reason: String,

    #[serde(default)]
    pub on_failure: OnFailure,
}

/// An action to execute in a hook (no hook control)
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
    /// Run a task by spawning an agent session
    TaskRun(TaskRunAction),
    /// Create and run a review task
    Review(ReviewAction),
    /// Continue hook execution (generates Failure and returns FailedContinue)
    Continue(ContinueAction),
    /// Stop hook execution (emits warning and stops silently)
    Stop(StopAction),
    /// Block editor operation (emits error and blocks with exit 2)
    Block(BlockAction),
    /// End the current session gracefully (for task-driven sessions)
    SessionEnd(SessionEndAction),
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

/// Task run action - spawns an agent session to work on a task
/// YAML: `task.run: { task_id: "abc123" }` or `task.run: { task_id: "$var" }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunAction {
    /// The task run configuration
    #[serde(rename = "task.run")]
    pub task_run: TaskRunConfig,

    #[serde(default)]
    pub on_failure: OnFailure,
}

/// Configuration for task.run action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunConfig {
    /// Task ID to run (supports variable interpolation)
    pub task_id: String,

    /// Optional agent override (claude-code, codex)
    #[serde(default)]
    pub agent: Option<String>,
}

/// Review action - creates and runs a code review task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewAction {
    /// The review configuration
    pub review: ReviewConfig,

    #[serde(default)]
    pub on_failure: OnFailure,
}

/// Configuration for review action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Task ID to review (supports variable interpolation)
    #[serde(default)]
    pub task_id: Option<String>,

    /// Optional agent override (claude-code, codex)
    #[serde(default)]
    pub agent: Option<String>,

    /// Optional template override (default: aiki/review)
    #[serde(default)]
    pub template: Option<String>,
}

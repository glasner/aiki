use std::path::PathBuf;
use thiserror::Error;

/// Format a call stack for display in error messages
fn format_call_stack(stack: &[String]) -> String {
    if stack.is_empty() {
        return String::from("  (empty)");
    }

    let mut result = String::new();
    for (i, path) in stack.iter().enumerate() {
        if i == 0 {
            result.push_str(&format!("  {}", path));
        } else {
            result.push_str(&format!("\n  → {} (before)", path));
        }
    }
    result
}

/// Aiki-specific errors with structured error types
#[derive(Error, Debug)]
#[allow(dead_code)] // Error variants exist for API completeness
pub enum AikiError {
    // Repository errors
    #[error("Not in a JJ repository. Run 'jj init' or 'aiki init' first")]
    NotInJjRepo,

    #[error("Failed to initialize JJ workspace")]
    JjInitFailed,

    // File errors
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("File not found in working copy and no parents available")]
    FileNotFoundNoParents,

    #[error("File not found in working copy or its parent")]
    FileNotFoundInParent,

    // Agent/vendor errors
    #[error(
        "Unknown agent type: '{0}'. Supported values: 'claude-code', 'codex', 'cursor', 'gemini'"
    )]
    UnknownAgentType(String),

    #[error("Unsupported agent type: {0:?}")]
    UnsupportedAgentType(String),

    #[error(
        "Unknown assignee: '{0}'. Valid values: 'claude-code', 'codex', 'cursor', 'gemini', 'human'"
    )]
    UnknownAssignee(String),

    // Flow execution errors
    #[error("Invalid let syntax: '{0}'. Expected 'variable = expression'")]
    InvalidLetSyntax(String),

    #[error("Invalid variable name: '{0}'. Variable names must start with a letter or underscore, and contain only letters, numbers, and underscores")]
    InvalidVariableName(String),

    #[error("Invalid condition: {0}")]
    InvalidCondition(String),

    #[error("Action failed with on_failure: stop")]
    ActionFailed,

    // Message assembly errors
    #[error("Invalid message chunk: {0}")]
    InvalidContextChunk(String),

    #[error("Missing function: {0}")]
    MissingFunction(String),

    #[error("Function '{0}' not found in namespace '{1}'")]
    FunctionNotFoundInNamespace(String, String),

    #[error("Unsupported function namespace in '{0}'. Only 'aiki/*' functions are supported")]
    UnsupportedFunctionNamespace(String),

    #[error("Invalid timeout format: {0}. Use 's', 'm', or 'h' suffix")]
    InvalidTimeoutFormat(String),

    // Scope errors
    #[error("Unknown scope: '{0}'. Supported values: 'staged'")]
    UnknownScope(String),

    #[error("Unknown format: '{0}'. Supported values: 'plain', 'git', 'json'")]
    UnknownFormat(String),

    // Command execution errors
    #[error("jj command failed: {0}")]
    JjCommandFailed(String),

    #[error("jj status failed: {0}")]
    JjStatusFailed(String),

    #[error("git diff failed: {0}")]
    GitDiffFailed(String),

    // Signing/GPG errors
    #[error("GPG-SM key generation not yet supported. Use --key to specify an existing key")]
    GpgSmNotSupported,

    #[error("SSH key file not found: {0}")]
    SshKeyNotFound(PathBuf),

    #[error("No user.email configured in git config")]
    NoUserEmailConfigured,

    #[error("Git user.name or user.email not configured")]
    GitUserNotConfigured,

    #[error("Could not extract key ID from GPG output")]
    GpgKeyIdExtractionFailed,

    #[error("Failed to generate GPG key: {0}")]
    GpgKeyGenerationFailed(String),

    #[error("Failed to locate SSH signing key: {0}")]
    SshKeyLocationFailed(String),

    // Configuration errors
    #[error("Failed to read config file: {0}")]
    ConfigReadFailed(String),

    #[error("Failed to write config file: {0}")]
    ConfigWriteFailed(String),

    // Flow composition errors (Milestone 1.3)
    #[error("Not in an Aiki project. No .aiki/ directory found searching upward from: {searched_from}")]
    NotInAikiProject { searched_from: PathBuf },

    #[error("Invalid path: '{path}'. {reason}")]
    InvalidPath { path: String, reason: String },

    #[error("Invalid flow path: '{path}'. {reason}")]
    InvalidFlowPath { path: String, reason: String },

    #[error("Flow not found: '{path}'. Resolved to: {resolved_path}")]
    FlowNotFound {
        path: String,
        resolved_path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Circular flow dependency detected: '{path}' (canonical: {canonical_path})\n\nFlow execution chain:\n{}", format_call_stack(.stack))]
    CircularFlowDependency {
        path: String,
        canonical_path: String,
        stack: Vec<String>,
    },

    // ACP/Zed integration errors
    #[error(
        "ACP binary not found for agent '{agent_type}'.

Zed installation not found or agent not installed.
Please ensure:
  1. Zed editor is installed (https://zed.dev)
  2. You've created a {agent_type} thread in Zed (cmd-? → '+' → {agent_type})
  3. Zed has completed its one-time package installation

Alternatively, install the agent globally:
  npm install -g {executable_name}"
    )]
    AcpBinaryNotFound {
        agent_type: String,
        executable_name: String,
    },

    #[error("Zed installation not found at {0}. Install Zed from https://zed.dev")]
    ZedNotInstalled(PathBuf),

    #[error("Agent '{0}' not installed by Zed. Create a thread with this agent in Zed first (cmd-? → '+' → agent)")]
    ZedAgentNotInstalled(String),

    #[error("Node.js not found. Zed-installed agents require Node.js. Install from: https://nodejs.org or 'brew install node'")]
    NodeJsNotFound,

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    // Task system errors
    #[error("Task not found: '{0}'")]
    TaskNotFound(String),

    #[error("Cannot create subtask: parent task '{0}' is closed")]
    ParentTaskClosed(String),

    #[error("No tasks in ready queue")]
    NoTasksReady,

    #[error("Failed to initialize aiki/tasks branch: {0}")]
    TaskBranchInitFailed(String),

    #[error("Failed to parse task event: {0}")]
    TaskEventParseFailed(String),

    #[error("{0}")]
    TaskCommentRequired(String),

    #[error("Task '{0}' is already closed")]
    TaskAlreadyClosed(String),

    #[error("Invalid task source: '{0}'. Sources must have a prefix: 'file:', 'task:', 'comment:', 'issue:', or 'prompt:'")]
    InvalidTaskSource(String),

    #[error("Task '{0}' has no assignee and no --agent specified")]
    TaskNoAssignee(String),

    #[error("Agent '{0}' does not support task execution")]
    AgentNotSupported(String),

    #[error("Failed to spawn agent: {0}")]
    AgentSpawnFailed(String),

    // History/conversation errors
    #[error("Failed to initialize aiki/conversations branch: {0}")]
    ConversationsBranchInitFailed(String),

    #[error("Cannot resolve --source prompt: no active session found. Use --source prompt:<change_id> to specify explicitly.")]
    NoActiveSessionForPromptSource,

    #[error("Cannot resolve --source prompt: no prompt events found for this session.")]
    NoPromptEventsForSession,

    // Template system errors
    #[error("Template not found: '{name}'\n  Expected: {expected_path}{suggestions}")]
    TemplateNotFound {
        name: String,
        expected_path: String,
        suggestions: String,
    },

    #[error("Variable '{{{variable}}}' referenced but not provided{template_info}\n  {hint}")]
    TemplateVariableNotFound {
        variable: String,
        hint: String,
        template_info: String,
    },

    #[error("Invalid template frontmatter\n  File: {file}\n  {details}")]
    TemplateFrontmatterInvalid { file: String, details: String },

    #[error("Invalid template structure\n  File: {file}\n  {details}")]
    TemplateStructureInvalid { file: String, details: String },

    #[error("No templates directory found at: {path}")]
    TemplatesDirectoryNotFound { path: String },

    // Generic wrapper for underlying errors
    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias for Aiki operations
pub type Result<T> = std::result::Result<T, AikiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AikiError::NotInJjRepo;
        assert_eq!(
            err.to_string(),
            "Not in a JJ repository. Run 'jj init' or 'aiki init' first"
        );
    }

    #[test]
    fn test_unknown_agent_type() {
        let err = AikiError::UnknownAgentType("vscode".to_string());
        assert_eq!(
            err.to_string(),
            "Unknown agent type: 'vscode'. Supported values: 'claude-code', 'codex', 'cursor', 'gemini'"
        );
    }

    #[test]
    fn test_invalid_let_syntax() {
        let err = AikiError::InvalidLetSyntax("x".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid let syntax: 'x'. Expected 'variable = expression'"
        );
    }

    #[test]
    fn test_file_not_found() {
        let err = AikiError::FileNotFound(PathBuf::from("/tmp/test.txt"));
        assert!(err.to_string().contains("/tmp/test.txt"));
    }

    #[test]
    fn test_not_in_aiki_project() {
        let err = AikiError::NotInAikiProject {
            searched_from: PathBuf::from("/home/user/project"),
        };
        assert!(err.to_string().contains("/home/user/project"));
        assert!(err.to_string().contains(".aiki/"));
    }

    #[test]
    fn test_invalid_flow_path() {
        let err = AikiError::InvalidFlowPath {
            path: "invalid".to_string(),
            reason: "Must start with aiki/, vendor/, @/, ./, ../, or /".to_string(),
        };
        assert!(err.to_string().contains("invalid"));
        assert!(err.to_string().contains("Must start with"));
    }

    #[test]
    fn test_circular_flow_dependency() {
        let err = AikiError::CircularFlowDependency {
            path: "aiki/flow-a".to_string(),
            canonical_path: "/project/.aiki/flows/aiki/flow-a.yml".to_string(),
            stack: vec![
                "my-workflow.yml".to_string(),
                "aiki/flow-a.yml".to_string(),
                "aiki/flow-b.yml".to_string(),
            ],
        };
        let msg = err.to_string();
        assert!(msg.contains("Circular flow dependency"));
        assert!(msg.contains("aiki/flow-a"));
        assert!(msg.contains("my-workflow.yml"));
        assert!(msg.contains("→"));
    }

    #[test]
    fn test_format_call_stack_empty() {
        let result = format_call_stack(&[]);
        assert_eq!(result, "  (empty)");
    }

    #[test]
    fn test_format_call_stack_single() {
        let result = format_call_stack(&["my-flow.yml".to_string()]);
        assert_eq!(result, "  my-flow.yml");
    }

    #[test]
    fn test_format_call_stack_multiple() {
        let result = format_call_stack(&[
            "top.yml".to_string(),
            "middle.yml".to_string(),
            "bottom.yml".to_string(),
        ]);
        assert!(result.contains("top.yml"));
        assert!(result.contains("→ middle.yml"));
        assert!(result.contains("→ bottom.yml"));
    }
}

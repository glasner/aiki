use std::path::PathBuf;
use thiserror::Error;

/// Aiki-specific errors with structured error types
#[derive(Error, Debug)]
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

    // Flow execution errors
    #[error("Invalid let syntax: '{0}'. Expected 'variable = expression'")]
    InvalidLetSyntax(String),

    #[error("Invalid variable name: '{0}'. Variable names must start with a letter or underscore, and contain only letters, numbers, and underscores")]
    InvalidVariableName(String),

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
}

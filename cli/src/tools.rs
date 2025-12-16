//! Tool classification for vendor-agnostic event routing
//!
//! This module provides shared types for classifying AI agent tools
//! into categories that map to the unified event model.

use serde::{Deserialize, Serialize};

/// Tool type classification for event routing
///
/// Represents the category of tool being used, which determines
/// which event type should be emitted. This enum is shared across
/// vendors; each vendor implements its own `classify_tool()` function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    /// File tools (Read, Edit, Write, Glob, Grep, LS, NotebookEdit)
    File,
    /// Shell command execution (Bash)
    Shell,
    /// Web access tools (WebFetch, WebSearch) - Phase 3
    Web,
    /// Internal orchestration tools (Task, TodoRead) - no event needed
    Internal,
    /// MCP server tools (anything else)
    Mcp,
}

/// File operation type
///
/// Represents the type of file operation being performed.
/// Used by flows to gate operations differently (e.g., allow reads, block deletes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
    /// Read operations: Read, LS, Glob, Grep
    Read,
    /// Write operations: Edit, Write, NotebookEdit, MultiEdit
    Write,
    /// Delete operations: rm, rmdir (parsed from shell commands)
    Delete,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Read => write!(f, "read"),
            FileOperation::Write => write!(f, "write"),
            FileOperation::Delete => write!(f, "delete"),
        }
    }
}

/// Web operation type
///
/// Represents the type of web operation being performed.
/// Used by flows to gate operations differently (e.g., allow search, block fetch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebOperation {
    /// Fetch a specific URL
    Fetch,
    /// Web search query
    Search,
}

impl std::fmt::Display for WebOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebOperation::Fetch => write!(f, "fetch"),
            WebOperation::Search => write!(f, "search"),
        }
    }
}

// ============================================================================
// Shell Command Parsing
// ============================================================================

/// Parse a shell command to detect file operations
///
/// Returns `Some(FileOperation::Delete)` if the command is a file deletion (rm/rmdir),
/// otherwise returns `None` for regular shell commands.
///
/// When a file operation is detected, also returns the paths being operated on.
///
/// # Examples
/// ```
/// use aiki::tools::{parse_file_operation_from_shell_command, FileOperation};
///
/// let (op, paths) = parse_file_operation_from_shell_command("rm file.txt");
/// assert_eq!(op, Some(FileOperation::Delete));
/// assert_eq!(paths, vec!["file.txt"]);
///
/// let (op, paths) = parse_file_operation_from_shell_command("git status");
/// assert_eq!(op, None);
/// assert_eq!(paths, Vec::<String>::new());
/// ```
pub fn parse_file_operation_from_shell_command(
    command: &str,
) -> (Option<FileOperation>, Vec<String>) {
    let parts: Vec<&str> = command.trim().split_whitespace().collect();

    match parts.first() {
        Some(&"rm") | Some(&"rmdir") => {
            // Extract file paths from command (skip options starting with -)
            let paths: Vec<String> = parts[1..]
                .iter()
                .filter(|arg| !arg.starts_with('-'))
                .map(|s| s.to_string())
                .collect();

            if paths.is_empty() {
                // rm with no arguments - treat as regular shell command
                (None, Vec::new())
            } else {
                (Some(FileOperation::Delete), paths)
            }
        }
        _ => (None, Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rm_single_file() {
        let (op, paths) = parse_file_operation_from_shell_command("rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_rm_multiple_files() {
        let (op, paths) =
            parse_file_operation_from_shell_command("rm file1.txt file2.txt file3.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file1.txt", "file2.txt", "file3.txt"]);
    }

    #[test]
    fn test_rm_with_flags() {
        let (op, paths) = parse_file_operation_from_shell_command("rm -rf directory/");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["directory/"]);
    }

    #[test]
    fn test_rm_with_multiple_flags_and_files() {
        let (op, paths) = parse_file_operation_from_shell_command("rm -r -f file1.txt file2.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file1.txt", "file2.txt"]);
    }

    #[test]
    fn test_rmdir() {
        let (op, paths) = parse_file_operation_from_shell_command("rmdir old_directory");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["old_directory"]);
    }

    #[test]
    fn test_rm_no_args() {
        let (op, paths) = parse_file_operation_from_shell_command("rm");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_rm_only_flags() {
        let (op, paths) = parse_file_operation_from_shell_command("rm -rf");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_git_command() {
        let (op, paths) = parse_file_operation_from_shell_command("git status");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_ls_command() {
        let (op, paths) = parse_file_operation_from_shell_command("ls -la");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_empty_command() {
        let (op, paths) = parse_file_operation_from_shell_command("");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_whitespace_handling() {
        let (op, paths) = parse_file_operation_from_shell_command("  rm   -rf   file.txt  ");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }
}

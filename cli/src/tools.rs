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
/// This function handles:
/// - Common shell prefixes like `sudo`, `env`, `nice`, etc.
/// - Quoted paths with spaces: `rm "my file.txt"` → ["my file.txt"]
/// - Escaped spaces: `rm file\ name.txt` → ["file name.txt"]
///
/// # Examples
/// ```
/// use aiki::tools::{parse_file_operation_from_shell_command, FileOperation};
///
/// let (op, paths) = parse_file_operation_from_shell_command("rm file.txt");
/// assert_eq!(op, Some(FileOperation::Delete));
/// assert_eq!(paths, vec!["file.txt"]);
///
/// let (op, paths) = parse_file_operation_from_shell_command("sudo rm -rf /tmp/test");
/// assert_eq!(op, Some(FileOperation::Delete));
/// assert_eq!(paths, vec!["/tmp/test"]);
///
/// let (op, paths) = parse_file_operation_from_shell_command("git status");
/// assert_eq!(op, None);
/// assert_eq!(paths, Vec::<String>::new());
/// ```
pub fn parse_file_operation_from_shell_command(
    command: &str,
) -> (Option<FileOperation>, Vec<String>) {
    // Parse command respecting shell quoting
    let tokens = tokenize_shell_command(command);

    // Skip common shell prefixes to find the actual command
    let cmd_idx = find_command_index(&tokens);

    let cmd = match tokens.get(cmd_idx) {
        Some(cmd) => cmd.as_str(),
        None => return (None, Vec::new()),
    };

    match cmd {
        "rm" | "rmdir" => {
            // Extract file paths from command (skip options starting with -)
            let paths: Vec<String> = tokens[(cmd_idx + 1)..]
                .iter()
                .filter(|arg| !arg.starts_with('-'))
                .cloned()
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

/// Common shell prefixes that wrap other commands
const SHELL_PREFIXES: &[&str] = &[
    "sudo", "doas", // privilege escalation (doas is BSD's sudo)
    "env", "nohup", "nice", // execution modifiers
    "time", "timeout", // timing
    "strace",
    "ltrace", // debugging
              // Note: xargs is intentionally excluded - it reads args from stdin,
              // not from the command line, so we can't parse file paths from tokens
];

/// Find the index of the actual command, skipping shell prefixes and their arguments
fn find_command_index(tokens: &[String]) -> usize {
    let mut idx = 0;

    while idx < tokens.len() {
        let token = tokens[idx].as_str();

        // Check if this is a shell prefix
        if SHELL_PREFIXES.contains(&token) {
            let prefix_start = idx;
            idx += 1;

            // Skip arguments to prefix commands (e.g., env VAR=value, sudo -u user)
            while idx < tokens.len() {
                let arg = tokens[idx].as_str();

                // sudo/doas flags that take arguments: -u user, -g group, -C num
                if matches!(token, "sudo" | "doas") {
                    match arg {
                        // Flags that take an argument
                        "-u" | "-g" | "-C" => {
                            idx += 2; // Skip flag and its argument
                            idx = idx.min(tokens.len()); // Clamp to array bounds
                            continue;
                        }
                        // Login shell flags - the next token is the command, not an arg to sudo
                        "-i" | "-s" | "--login" | "--shell" => {
                            idx += 1;
                            break; // Next token is the actual command
                        }
                        // Other single-letter flags without arguments
                        _ if arg.starts_with('-') && arg.len() == 2 => {
                            idx += 1;
                            continue;
                        }
                        _ => {}
                    }
                }

                // nice flags that take arguments: -n priority
                if token == "nice" && arg == "-n" {
                    idx += 2; // Skip flag and its argument
                    idx = idx.min(tokens.len()); // Clamp to array bounds
                    continue;
                }

                // timeout takes a duration argument (first non-flag arg after timeout)
                if token == "timeout" && !arg.starts_with('-') && idx == prefix_start + 1 {
                    // First arg after timeout is the duration
                    idx += 1;
                    continue;
                }

                // env VAR=value pairs
                if token == "env" && arg.contains('=') {
                    idx += 1;
                    continue;
                }

                // Generic flags (start with -)
                if arg.starts_with('-') {
                    idx += 1;
                    continue;
                }

                // Found non-flag, non-prefix argument - this is the command
                break;
            }
        } else {
            // Not a prefix, this is the command
            break;
        }
    }

    idx
}

/// Tokenize a shell command using POSIX shell word splitting rules
///
/// Uses the `shell-words` crate which handles:
/// - Single and double quotes: `rm "my file.txt"` → ["rm", "my file.txt"]
/// - Escape sequences: `rm file\ name.txt` → ["rm", "file name.txt"]
/// - Nested quotes: `echo "it's fine"` → ["echo", "it's fine"]
///
/// Returns an empty vector if parsing fails (e.g., unclosed quotes).
fn tokenize_shell_command(command: &str) -> Vec<String> {
    shell_words::split(command).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Basic rm/rmdir tests
    // ========================================================================

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

    // ========================================================================
    // Shell prefix tests (Finding 8)
    // ========================================================================

    #[test]
    fn test_sudo_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("sudo rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_sudo_rm_with_flags() {
        let (op, paths) = parse_file_operation_from_shell_command("sudo rm -rf /tmp/test");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["/tmp/test"]);
    }

    #[test]
    fn test_sudo_with_user_flag() {
        let (op, paths) = parse_file_operation_from_shell_command("sudo -u root rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_env_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("env rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_env_with_vars_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("env VAR=value rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_nice_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("nice rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_nice_with_priority_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("nice -n 10 rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_time_rm() {
        let (op, paths) = parse_file_operation_from_shell_command("time rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_chained_prefixes() {
        let (op, paths) = parse_file_operation_from_shell_command("sudo nice rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_sudo_ls_not_delete() {
        let (op, paths) = parse_file_operation_from_shell_command("sudo ls -la");
        assert_eq!(op, None);
        assert!(paths.is_empty());
    }

    // ========================================================================
    // Quoted path tests (Finding 9)
    // ========================================================================

    #[test]
    fn test_double_quoted_path_with_space() {
        let (op, paths) = parse_file_operation_from_shell_command(r#"rm "my file.txt""#);
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["my file.txt"]);
    }

    #[test]
    fn test_single_quoted_path_with_space() {
        let (op, paths) = parse_file_operation_from_shell_command("rm 'my file.txt'");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["my file.txt"]);
    }

    #[test]
    fn test_escaped_space_in_path() {
        let (op, paths) = parse_file_operation_from_shell_command(r"rm my\ file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["my file.txt"]);
    }

    #[test]
    fn test_mixed_quoted_and_unquoted() {
        let (op, paths) =
            parse_file_operation_from_shell_command(r#"rm simple.txt "with space.txt""#);
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["simple.txt", "with space.txt"]);
    }

    #[test]
    fn test_double_quoted_secret_file() {
        let (op, paths) = parse_file_operation_from_shell_command(r#"rm "my secret.env""#);
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["my secret.env"]);
    }

    #[test]
    fn test_sudo_rm_quoted_path() {
        let (op, paths) = parse_file_operation_from_shell_command(r#"sudo rm -f "my file.txt""#);
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["my file.txt"]);
    }

    // ========================================================================
    // Tokenizer unit tests
    // ========================================================================

    #[test]
    fn test_tokenize_simple() {
        let tokens = tokenize_shell_command("rm file.txt");
        assert_eq!(tokens, vec!["rm", "file.txt"]);
    }

    #[test]
    fn test_tokenize_double_quotes() {
        let tokens = tokenize_shell_command(r#"rm "my file.txt""#);
        assert_eq!(tokens, vec!["rm", "my file.txt"]);
    }

    #[test]
    fn test_tokenize_single_quotes() {
        let tokens = tokenize_shell_command("rm 'my file.txt'");
        assert_eq!(tokens, vec!["rm", "my file.txt"]);
    }

    #[test]
    fn test_tokenize_escaped_space() {
        let tokens = tokenize_shell_command(r"rm my\ file.txt");
        assert_eq!(tokens, vec!["rm", "my file.txt"]);
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize_shell_command(r#"cmd arg1 "arg 2" 'arg 3' arg\ 4"#);
        assert_eq!(tokens, vec!["cmd", "arg1", "arg 2", "arg 3", "arg 4"]);
    }

    #[test]
    fn test_tokenize_nested_quotes() {
        // Double quotes containing single quote
        let tokens = tokenize_shell_command(r#"echo "it's fine""#);
        assert_eq!(tokens, vec!["echo", "it's fine"]);
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize_shell_command("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let tokens = tokenize_shell_command("   ");
        assert!(tokens.is_empty());
    }

    // ========================================================================
    // Tests for fixed edge cases
    // ========================================================================

    #[test]
    fn test_sudo_login_shell() {
        // sudo -i means "login shell" - rm is the command, not an arg to sudo
        let (op, paths) = parse_file_operation_from_shell_command("sudo -i rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_sudo_shell() {
        // sudo -s means "run shell" - rm is the command
        let (op, paths) = parse_file_operation_from_shell_command("sudo -s rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_chained_timeout() {
        // sudo timeout 5 rm file.txt - timeout positioning after sudo
        let (op, paths) = parse_file_operation_from_shell_command("sudo timeout 5 rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_doas_rm() {
        // doas is BSD's sudo equivalent
        let (op, paths) = parse_file_operation_from_shell_command("doas rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_doas_with_user() {
        let (op, paths) = parse_file_operation_from_shell_command("doas -u root rm file.txt");
        assert_eq!(op, Some(FileOperation::Delete));
        assert_eq!(paths, vec!["file.txt"]);
    }
}

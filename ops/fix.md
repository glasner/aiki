# Fix: Use JJ to Get Accurate File Paths for Delete/Move Operations

## Problem Statement

`MOVE_DIR_CACHE` ([@event_bus.rs#L14:25](file:///Users/glasner/code/aiki/cli/src/event_bus.rs#L14:25)) is an in-memory cache that doesn't work across separate CLI process invocations.

**Current broken flow:**
1. `shell.permission_asked` fires → detect if dest is directory → cache result in memory
2. Hook process exits (cache is lost)
3. `shell.completed` fires in NEW process → try to lookup cache → cache is empty → fall back to syntactic-only detection

**Root cause:** Hooks run as separate processes, so in-memory state doesn't persist.

---

## Solution: Use JJ to Get Accurate File Paths

### Scope

**JJ is ONLY used for getting accurate file paths, NOT for operation detection.**

- **Operation detection:** Still use shell command parsing (tells us it's a `mv` or `rm`)
- **File path resolution:** Use JJ to get accurate source/destination paths (replaces MOVE_DIR_CACHE)

### Why This Works

**For `shell.completed` with move operations:**
1. Shell command parsing → "this is a move operation"
2. Run `jj diff -r @ --summary` → get actual paths from `R {src => dest}` output
3. Emit `change.completed` with Move operation using JJ's accurate paths

**For `shell.completed` with delete operations:**
1. Shell command parsing → "this is a delete operation"
2. Run `jj diff -r @ --summary` → get list of actually deleted files (D lines)
3. Emit `change.completed` with Delete operation using JJ's file list

**Benefits:**
1. **No cache needed** - JJ state persists on disk across processes
2. **100% accurate paths** - JJ knows exactly what moved/deleted
3. **Handles edge cases** - `mv file existing_dir` works correctly (JJ shows final paths)
4. **Simple** - Keep existing operation detection, just improve path resolution

---

## Implementation Plan

### Phase 1: Restructure JJ Module and Add Diff Functions

**Step 1: Convert jj.rs to jj/ module**

1. Create directory: `cli/src/jj/`
2. Extract `JJWorkspace` from `cli/src/jj.rs` → `cli/src/jj/workspace.rs`
3. Add new methods to `JJWorkspace`:
   - `pub fn find(path: &Path) -> Result<Self>` - Find workspace root from any path (walks up looking for `.jj/`)
   - `pub fn workspace_root(&self) -> &Path` - Get the workspace root path
4. Create `cli/src/jj/mod.rs` that re-exports:
   ```rust
   pub mod workspace;
   pub mod diff;
   
   pub use workspace::JJWorkspace;
   ```
5. No changes needed to `cli/src/lib.rs` (still `pub mod jj;`)

**Step 2: Add diff.rs submodule**

**New file:** `cli/src/jj/diff.rs`

```rust
use crate::error::Result;
use anyhow::Context;
use globset::{Glob, GlobSetBuilder};
use std::path::{Path, PathBuf};
use std::process::Command;
use super::workspace::JJWorkspace;

/// Get list of deleted file paths from JJ, filtered by command arguments
///
/// Parses `jj diff -r @ --summary` and extracts all D (deleted) lines,
/// then filters to only include paths that match the command arguments.
/// This prevents re-emitting earlier deletes from the same working copy change.
///
/// Returns paths relative to shell_cwd, not workspace root, so downstream
/// consumers can join them onto event.cwd without path confusion.
///
/// # Arguments
/// * `shell_cwd` - Directory where the shell command was executed
/// * `command_args` - Raw command arguments (relative to shell_cwd)
pub fn get_deleted_paths(shell_cwd: &Path, command_args: &[String]) -> Result<Vec<String>> {
    // Find workspace root and normalize command args to be workspace-relative
    let workspace = JJWorkspace::find(shell_cwd)?;
    let workspace_root = workspace.workspace_root();
    let normalized_args = normalize_paths_to_workspace_root(command_args, shell_cwd, workspace_root)?;
    
    let output = run_jj_diff_summary(workspace_root)?;
    
    let all_deleted: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("D ") {
                Some(line[2..].trim().to_string())
            } else {
                None
            }
        })
        .collect();
    
    // Filter to only include paths mentioned in command or their descendants
    // Both all_deleted (from JJ) and normalized_args are now workspace-relative
    let filtered = filter_paths_by_command(&all_deleted, &normalized_args);
    
    // Rebase filtered paths to be relative to shell_cwd, not workspace root
    Ok(rebase_paths_to_cwd(&filtered, shell_cwd, workspace_root)?)

/// Get move operations (source → destination pairs) from JJ, filtered by command arguments
///
/// Parses `jj diff -r @ --summary` and extracts all R (rename/move) lines,
/// then filters to only include paths that match the command arguments.
/// This prevents re-emitting earlier moves from the same working copy change.
///
/// Returns (source, destination) tuples with paths relative to shell_cwd, not
/// workspace root, so downstream consumers can join them onto event.cwd.
///
/// # Arguments
/// * `shell_cwd` - Directory where the shell command was executed
/// * `command_args` - Raw command arguments (relative to shell_cwd)
pub fn get_move_operations(shell_cwd: &Path, command_args: &[String]) -> Result<Vec<(String, String)>> {
    // Find workspace root and normalize command args to be workspace-relative
    let workspace = JJWorkspace::find(shell_cwd)?;
    let workspace_root = workspace.workspace_root();
    let normalized_args = normalize_paths_to_workspace_root(command_args, shell_cwd, workspace_root)?;
    
    let output = run_jj_diff_summary(workspace_root)?;
    
    let all_moves: Vec<(String, String)> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(content) = line.strip_prefix("R ") {
                parse_move_line(content)
            } else {
                None
            }
        })
        .collect();
    
    // Filter to only include moves where source matches command arguments
    // Both move sources (from JJ) and normalized_args are now workspace-relative
    let filtered_moves: Vec<(String, String)> = all_moves
        .into_iter()
        .filter(|(source, _dest)| path_matches_command(source, &normalized_args))
        .collect();
    
    // Rebase both source and destination to be relative to shell_cwd
    let sources: Vec<String> = filtered_moves.iter().map(|(s, _)| s.clone()).collect();
    let destinations: Vec<String> = filtered_moves.iter().map(|(_, d)| d.clone()).collect();
    
    let rebased_sources = rebase_paths_to_cwd(&sources, shell_cwd, workspace_root)?;
    let rebased_destinations = rebase_paths_to_cwd(&destinations, shell_cwd, workspace_root)?;
    
    Ok(rebased_sources.into_iter().zip(rebased_destinations).collect())
}

/// Parse a JJ move/rename line
///
/// JJ outputs rename lines as: "R {old_path => new_path}"
/// The braces contain the old and new paths separated by " => ".
/// Paths with spaces are NOT quoted - they appear as-is within the braces.
///
/// Examples:
///   - R {old.txt => new.txt}
///   - R {src/old.rs => src/new.rs}
///   - R {file with spaces.txt => renamed.txt}
///   - R {file.txt => dir/file.txt}
fn parse_move_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    
    // Extract content between braces: "R {old => new}" → "old => new"
    let braces_content = line.strip_prefix('{')?.strip_suffix('}')?;
    
    // Split on " => " separator
    let mut parts = braces_content.split(" => ");
    let source = parts.next()?.trim().to_string();
    let destination = parts.next()?.trim().to_string();
    
    Some((source, destination))
}

/// Run `jj diff -r @ --summary` and return stdout
///
/// Runs JJ from the workspace root directory.
/// Callers should use `JJWorkspace::find()` to get the workspace_root.
///
/// # Arguments
/// * `workspace_root` - JJ workspace root directory (contains .jj/)
///
/// # Errors
/// Returns error if JJ command fails (not installed, not a workspace, etc.)
/// Callers should handle errors and fall back to syntactic detection.
fn run_jj_diff_summary(workspace_root: &Path) -> Result<String> {
    let output = Command::new("jj")
        .arg("diff")
        .arg("-r")
        .arg("@")
        .arg("--summary")
        .current_dir(workspace_root)
        .output()
        .context("Failed to execute jj command (is jj installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj diff command failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Filter paths to only include those matching command arguments
///
/// A path matches if it exactly matches a command argument OR is a descendant
/// of a directory argument (for operations like `rm -rf dir/`).
///
/// Supports glob patterns like `*.txt` or `src/{a,b}.rs` using the `globset`
/// crate for proper pattern matching. This prevents re-emitting unrelated files
/// from previous operations in the same working copy change.
///
/// NOTE: This function assumes both `paths` (from JJ) and `command_args` are
/// relative to the workspace root. Callers must normalize command_args before
/// calling this function (see `normalize_paths_to_workspace_root`).
///
/// Each shell command is processed independently, so prefix matching does not
/// cause duplicate reports across commands.
fn filter_paths_by_command(paths: &[String], command_args: &[String]) -> Vec<String> {
    // Empty args means all operands normalized outside workspace (e.g., `rm /tmp/foo`)
    // Return empty vec so caller falls back to syntactic detection
    if command_args.is_empty() {
        return vec![];
    }
    
    // Check if any argument contains glob metacharacters
    let has_glob = command_args.iter().any(|arg| {
        arg.contains('*') || arg.contains('?') || arg.contains('[') || arg.contains('{')
    });
    
    if has_glob {
        // Build a glob matcher for all patterns
        let mut builder = GlobSetBuilder::new();
        for arg in command_args {
            // Add the pattern itself
            if let Ok(glob) = Glob::new(arg) {
                builder.add(glob);
            }
            // Also add pattern/** for directory matching (e.g., src/* matches src/foo/bar.rs)
            let dir_pattern = format!("{}/**", arg.trim_end_matches('/').trim_end_matches('\\'));
            if let Ok(glob) = Glob::new(&dir_pattern) {
                builder.add(glob);
            }
        }
        
        // If glob compilation failed, fall back to returning empty vec
        let Ok(globset) = builder.build() else {
            return vec![];
        };
        
        // Filter paths using glob matching
        return paths
            .iter()
            .filter(|path| globset.is_match(path))
            .cloned()
            .collect();
    }
    
    // No globs - use exact/prefix matching
    paths
        .iter()
        .filter(|path| path_matches_command(path, command_args))
        .cloned()
        .collect()
}

/// Normalize command arguments to be relative to workspace root
///
/// When a shell command is run from a subdirectory (e.g., `cd src && rm foo.rs`),
/// the command arguments are relative to the shell's cwd, not the workspace root.
/// JJ's diff output always uses workspace-root-relative paths, so we need to
/// normalize the command arguments to match.
///
/// This function handles deleted files gracefully (canonicalization fails for non-existent paths)
/// and filters out paths outside the workspace instead of treating them as errors.
///
/// # Arguments
/// * `command_args` - Raw arguments from the shell command (relative to shell_cwd)
/// * `shell_cwd` - Directory where shell command was executed
/// * `workspace_root` - JJ workspace root directory
///
/// # Returns
/// Vector of workspace-relative paths (may be empty if all paths are outside workspace)
fn normalize_paths_to_workspace_root(
    command_args: &[String],
    shell_cwd: &Path,
    workspace_root: &Path,
) -> Result<Vec<String>> {
    // Canonicalize both shell_cwd and workspace_root to support symlinks
    let canonical_cwd = shell_cwd.canonicalize()
        .context("Failed to canonicalize shell cwd")?;
    let canonical_root = workspace_root.canonicalize()
        .context("Failed to canonicalize workspace root")?;
    
    // Collect successfully normalized paths, skip those outside workspace
    let normalized: Vec<String> = command_args
        .iter()
        .filter_map(|arg| {
            // Try to resolve to absolute path
            // For deleted files, canonicalize() fails, so use the path as-is
            let absolute = if Path::new(arg).is_absolute() {
                PathBuf::from(arg).canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(arg))
            } else {
                // Try canonicalization first (works for existing files)
                // Fall back to simple join (works for deleted files)
                canonical_cwd.join(arg).canonicalize()
                    .unwrap_or_else(|_| canonical_cwd.join(arg))
            };
            
            // Try to make workspace-relative
            // If strip_prefix fails (path outside workspace), skip this path
            absolute.strip_prefix(&canonical_root)
                .ok()
                .or_else(|| {
                    // If canonicalization failed (deleted file), try with non-canonical paths
                    let non_canonical_absolute = if Path::new(arg).is_absolute() {
                        PathBuf::from(arg)
                    } else {
                        shell_cwd.join(arg)
                    };
                    non_canonical_absolute.strip_prefix(workspace_root).ok()
                })
                .map(|p| p.to_string_lossy().to_string())
        })
        .collect();
    
    Ok(normalized)  // Return empty vec if nothing in workspace
}

/// Rebase workspace-relative paths to be relative to shell_cwd
///
/// JJ outputs paths relative to workspace root, but downstream consumers
/// expect paths relative to the cwd where the command was executed.
/// This function converts workspace-relative paths to cwd-relative paths.
///
/// # Arguments
/// * `workspace_paths` - Paths relative to workspace root (from JJ)
/// * `shell_cwd` - Directory where shell command was executed
/// * `workspace_root` - JJ workspace root directory
///
/// # Returns
/// Vector of paths relative to shell_cwd
///
/// # Example
/// ```
/// workspace_root = /repo
/// shell_cwd = /repo/src
/// workspace_path = "src/foo.rs"
/// result = "foo.rs"  // Relative to /repo/src
/// ```
fn rebase_paths_to_cwd(
    workspace_paths: &[String],
    shell_cwd: &Path,
    workspace_root: &Path,
) -> Result<Vec<String>> {
    // Canonicalize both to handle symlinks
    let canonical_cwd = shell_cwd.canonicalize()
        .context("Failed to canonicalize shell cwd")?;
    let canonical_root = workspace_root.canonicalize()
        .context("Failed to canonicalize workspace root")?;
    
    // Get the relative path from workspace root to shell cwd
    let cwd_relative_to_root = canonical_cwd.strip_prefix(&canonical_root)
        .context("Shell cwd is not within workspace root")?;
    
    workspace_paths
        .iter()
        .map(|workspace_path| {
            let workspace_path = Path::new(workspace_path);
            
            // If cwd is workspace root, paths are already correct
            if cwd_relative_to_root.as_os_str().is_empty() {
                return Ok(workspace_path.to_string_lossy().to_string());
            }
            
            // Try to strip the cwd prefix from the workspace path
            // e.g., workspace_path="src/foo.rs", cwd_relative="src" → "foo.rs"
            if let Ok(relative) = workspace_path.strip_prefix(cwd_relative_to_root) {
                Ok(relative.to_string_lossy().to_string())
            } else {
                // Path is outside the cwd directory, need to use ../ notation
                // e.g., workspace_path="lib.rs", cwd_relative="src" → "../lib.rs"
                let mut result = PathBuf::new();
                
                // Count how many levels to go up from cwd to root
                let levels = cwd_relative_to_root.components().count();
                for _ in 0..levels {
                    result.push("..");
                }
                
                // Then append the workspace path
                result.push(workspace_path);
                
                Ok(result.to_string_lossy().to_string())
            }
        })
        .collect()
}

/// Check if a path matches any command argument
///
/// A path matches if:
/// - It exactly equals a command argument
/// - It is a descendant of a directory argument (prefix match)
///
/// Uses Path semantics for cross-platform compatibility (handles both
/// forward and backslashes correctly on Windows).
///
/// Examples:
/// - path="src/foo.rs", arg="src/foo.rs" → match (exact)
/// - path="src/foo.rs", arg="src/" → match (directory)
/// - path="src/foo.rs", arg="src" → match (directory without trailing slash)
fn path_matches_command(path: &str, command_args: &[String]) -> bool {
    let path = Path::new(path);
    
    command_args.iter().any(|arg| {
        let arg_path = Path::new(arg.trim_end_matches('/').trim_end_matches('\\'));
        
        // Exact match
        path == arg_path
        // OR path is under a directory argument (starts with)
        || path.starts_with(arg_path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_move_line() {
        // Standard rename
        assert_eq!(
            parse_move_line("{old.txt => new.txt}"),
            Some(("old.txt".to_string(), "new.txt".to_string()))
        );

        // Move to directory
        assert_eq!(
            parse_move_line("{file.txt => dir/file.txt}"),
            Some(("file.txt".to_string(), "dir/file.txt".to_string()))
        );

        // Path with spaces (no quotes in JJ output)
        assert_eq!(
            parse_move_line("{file with spaces.txt => renamed.txt}"),
            Some(("file with spaces.txt".to_string(), "renamed.txt".to_string()))
        );
        
        // Destination with spaces
        assert_eq!(
            parse_move_line("{old.txt => path/with spaces/new.txt}"),
            Some(("old.txt".to_string(), "path/with spaces/new.txt".to_string()))
        );
        
        // Nested paths
        assert_eq!(
            parse_move_line("{src/old.rs => src/new.rs}"),
            Some(("src/old.rs".to_string(), "src/new.rs".to_string()))
        );
    }

    #[test]
    fn test_get_deleted_paths_parsing() {
        let output = "D a.txt\nM file.txt\nR b.txt => renamed.txt";
        let deleted: Vec<String> = output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.starts_with("D ") {
                    Some(line[2..].trim().to_string())
                } else {
                    None
                }
            })
            .collect();
        
        assert_eq!(deleted, vec!["a.txt"]);
    }

    #[test]
    fn test_get_move_operations_parsing() {
        let output = "D a.txt\nM file.txt\nR {b.txt => renamed.txt}\nR {old => new/path}";
        let moves: Vec<(String, String)> = output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if let Some(content) = line.strip_prefix("R ") {
                    parse_move_line(content)
                } else {
                    None
                }
            })
            .collect();
        
        assert_eq!(moves, vec![
            ("b.txt".to_string(), "renamed.txt".to_string()),
            ("old".to_string(), "new/path".to_string())
        ]);
    }

    #[test]
    fn test_path_matches_command() {
        let args = vec!["src/foo.rs".to_string(), "lib.rs".to_string()];
        
        // Exact matches
        assert!(path_matches_command("src/foo.rs", &args));
        assert!(path_matches_command("lib.rs", &args));
        
        // No match
        assert!(!path_matches_command("other.rs", &args));
        
        // Directory matching (with trailing slash)
        let dir_args = vec!["src/".to_string()];
        assert!(path_matches_command("src/foo.rs", &dir_args));
        assert!(path_matches_command("src/nested/bar.rs", &dir_args));
        assert!(!path_matches_command("lib.rs", &dir_args));
        
        // Directory matching (without trailing slash)
        let dir_args = vec!["src".to_string()];
        assert!(path_matches_command("src/foo.rs", &dir_args));
        assert!(path_matches_command("src/nested/bar.rs", &dir_args));
        assert!(!path_matches_command("lib.rs", &dir_args));
    }

    #[test]
    fn test_filter_paths_by_command() {
        let paths = vec![
            "src/foo.rs".to_string(),
            "src/bar.rs".to_string(),
            "lib.rs".to_string(),
        ];
        
        // Filter to only src/foo.rs (exact match)
        let args = vec!["src/foo.rs".to_string()];
        let filtered = filter_paths_by_command(&paths, &args);
        assert_eq!(filtered, vec!["src/foo.rs"]);
        
        // Directory matching (matches all descendants)
        let args = vec!["src/".to_string()];
        let filtered = filter_paths_by_command(&paths, &args);
        assert_eq!(filtered, vec!["src/foo.rs", "src/bar.rs"]);
        
        // Multiple exact matches
        let args = vec!["src/foo.rs".to_string(), "lib.rs".to_string()];
        let filtered = filter_paths_by_command(&paths, &args);
        assert_eq!(filtered, vec!["src/foo.rs", "lib.rs"]);
        
        // Empty args = return empty vec (fallback to syntactic)
        let filtered = filter_paths_by_command(&paths, &[]);
        assert_eq!(filtered, Vec::<String>::new());
    }

    #[test]
    fn test_normalize_paths_to_workspace_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Create subdirectory
        let subdir = workspace_root.join("src");
        std::fs::create_dir(&subdir).unwrap();
        
        // Test 1: Command from workspace root
        let args = vec!["src/foo.rs".to_string()];
        let normalized = normalize_paths_to_workspace_root(&args, workspace_root, workspace_root).unwrap();
        assert_eq!(normalized, vec!["src/foo.rs"]);
        
        // Test 2: Command from subdirectory (relative path)
        let args = vec!["foo.rs".to_string()];
        let normalized = normalize_paths_to_workspace_root(&args, &subdir, workspace_root).unwrap();
        assert_eq!(normalized, vec!["src/foo.rs"]);
        
        // Test 3: Multiple paths from subdirectory
        let args = vec!["foo.rs".to_string(), "bar.rs".to_string()];
        let normalized = normalize_paths_to_workspace_root(&args, &subdir, workspace_root).unwrap();
        assert_eq!(normalized, vec!["src/foo.rs", "src/bar.rs"]);
        
        // Test 4: Absolute path
        let absolute_path = workspace_root.join("lib.rs");
        let args = vec![absolute_path.to_string_lossy().to_string()];
        let normalized = normalize_paths_to_workspace_root(&args, &subdir, workspace_root).unwrap();
        assert_eq!(normalized, vec!["lib.rs"]);
    }
    
    #[test]
    fn test_normalize_paths_outside_workspace_filtered() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        
        let workspace_root = workspace_dir.path();
        let shell_cwd = workspace_root;
        
        // Create a file inside workspace
        let inside_file = workspace_root.join("inside.txt");
        std::fs::write(&inside_file, "test").unwrap();
        
        // Path outside workspace should be filtered out (not error)
        let outside_path = outside_dir.path().join("file.txt");
        let args = vec![
            outside_path.to_string_lossy().to_string(),
            "inside.txt".to_string(),
        ];
        let result = normalize_paths_to_workspace_root(&args, shell_cwd, workspace_root).unwrap();
        
        // Only the inside path should be included
        assert_eq!(result, vec!["inside.txt"]);
    }
    
    #[test]
    fn test_normalize_deleted_file_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Create and then delete a file to simulate deleted file scenario
        let file_path = workspace_root.join("deleted.txt");
        std::fs::write(&file_path, "test").unwrap();
        std::fs::remove_file(&file_path).unwrap();
        
        // Should still normalize the path even though file doesn't exist
        let args = vec!["deleted.txt".to_string()];
        let result = normalize_paths_to_workspace_root(&args, workspace_root, workspace_root).unwrap();
        assert_eq!(result, vec!["deleted.txt"]);
    }
    
    #[test]
    fn test_rebase_paths_to_cwd_from_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Command from workspace root - paths stay the same
        let workspace_paths = vec!["src/foo.rs".to_string(), "lib.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, workspace_root, workspace_root).unwrap();
        assert_eq!(result, vec!["src/foo.rs", "lib.rs"]);
    }
    
    #[test]
    fn test_rebase_paths_to_cwd_from_subdir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Create subdirectory
        let subdir = workspace_root.join("src");
        std::fs::create_dir(&subdir).unwrap();
        
        // JJ reports "src/foo.rs" (workspace-relative)
        // Command ran from src/, so result should be "foo.rs" (cwd-relative)
        let workspace_paths = vec!["src/foo.rs".to_string(), "src/bar.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, &subdir, workspace_root).unwrap();
        assert_eq!(result, vec!["foo.rs", "bar.rs"]);
    }
    
    #[test]
    fn test_rebase_paths_to_cwd_outside_cwd() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Create subdirectory
        let subdir = workspace_root.join("src");
        std::fs::create_dir(&subdir).unwrap();
        
        // JJ reports "lib.rs" (at workspace root)
        // Command ran from src/, so result should be "../lib.rs"
        let workspace_paths = vec!["lib.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, &subdir, workspace_root).unwrap();
        assert_eq!(result, vec!["../lib.rs"]);
    }
    
    #[test]
    fn test_rebase_paths_to_cwd_nested_subdir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path();
        
        // Create nested subdirectory
        let nested = workspace_root.join("src/nested");
        std::fs::create_dir_all(&nested).unwrap();
        
        // JJ reports "src/nested/foo.rs" (workspace-relative)
        // Command ran from src/nested/, so result should be "foo.rs"
        let workspace_paths = vec!["src/nested/foo.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, &nested, workspace_root).unwrap();
        assert_eq!(result, vec!["foo.rs"]);
        
        // JJ reports "src/other.rs" (sibling directory)
        // Command ran from src/nested/, so result should be "../other.rs"
        let workspace_paths = vec!["src/other.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, &nested, workspace_root).unwrap();
        assert_eq!(result, vec!["../other.rs"]);
        
        // JJ reports "lib.rs" (at workspace root)
        // Command ran from src/nested/, so result should be "../../lib.rs"
        let workspace_paths = vec!["lib.rs".to_string()];
        let result = rebase_paths_to_cwd(&workspace_paths, &nested, workspace_root).unwrap();
        assert_eq!(result, vec!["../../lib.rs"]);
    }
    
    #[test]
    fn test_path_matches_command_windows_separators() {
        // Test that Windows backslash separators work correctly
        let args = vec!["src\\foo.rs".to_string(), "lib.rs".to_string()];
        
        // Forward slash in path should match backslash in arg (Path normalization)
        assert!(path_matches_command("src/foo.rs", &args));
        assert!(path_matches_command("lib.rs", &args));
        assert!(!path_matches_command("other.rs", &args));
        
        // Directory matching with backslashes
        let dir_args = vec!["src\\".to_string()];
        assert!(path_matches_command("src/foo.rs", &dir_args));
        assert!(path_matches_command("src/nested/bar.rs", &dir_args));
    }
    
    #[test]
    fn test_filter_paths_with_glob_patterns() {
        let paths = vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "src/c.rs".to_string(),
            "src/d.rs".to_string(),
        ];
        
        // Star pattern - should match only .txt files
        let glob_args = vec!["*.txt".to_string()];
        let filtered = filter_paths_by_command(&paths, &glob_args);
        assert_eq!(filtered, vec!["a.txt", "b.txt"]);
        
        // Brace expansion pattern - should match c.rs and d.rs
        let glob_args = vec!["src/{c,d}.rs".to_string()];
        let filtered = filter_paths_by_command(&paths, &glob_args);
        assert_eq!(filtered, vec!["src/c.rs", "src/d.rs"]);
        
        // Question mark pattern - should match single-char filenames
        let glob_args = vec!["?.txt".to_string()];
        let filtered = filter_paths_by_command(&paths, &glob_args);
        assert_eq!(filtered, vec!["a.txt", "b.txt"]);
        
        // Bracket pattern - should match a.txt and b.txt
        let glob_args = vec!["[ab].txt".to_string()];
        let filtered = filter_paths_by_command(&paths, &glob_args);
        assert_eq!(filtered, vec!["a.txt", "b.txt"]);
        
        // Directory glob - should match all files in src/
        let glob_args = vec!["src/*".to_string()];
        let filtered = filter_paths_by_command(&paths, &glob_args);
        assert_eq!(filtered, vec!["src/c.rs", "src/d.rs"]);
        
        // Non-glob literal - should filter normally
        let literal_args = vec!["a.txt".to_string()];
        let filtered = filter_paths_by_command(&paths, &literal_args);
        assert_eq!(filtered, vec!["a.txt"]);
    }
}
```

**Update `cli/src/jj/mod.rs`:**
```rust
pub mod workspace;
pub mod diff;

pub use workspace::JJWorkspace;
```

**New file `cli/src/jj/workspace.rs`:**
Move the entire contents of current `cli/src/jj.rs` (the `JJWorkspace` struct and implementation), then add:

```rust
use anyhow::{Context, Result};
use jj_lib::config::StackedConfig;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;
use std::path::{Path, PathBuf};

/// Wrapper for JJ workspace operations using jj-lib
pub struct JJWorkspace {
    workspace_root: PathBuf,
}

impl JJWorkspace {
    /// Create a new JJ workspace manager for the given path
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    /// Find JJ workspace root by walking up from given path
    ///
    /// Searches parent directories for `.jj/` directory.
    /// Returns error if not in a JJ workspace.
    pub fn find(path: &Path) -> Result<Self> {
        let mut current = path.canonicalize()
            .context("Failed to resolve path")?;
        
        loop {
            let jj_dir = current.join(".jj");
            if jj_dir.is_dir() {
                return Ok(Self::new(current));
            }
            
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => anyhow::bail!("Not in a JJ workspace (no .jj directory found)"),
            }
        }
    }

    /// Get the workspace root path
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    // ... existing methods (create_user_settings, init) ...
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_find_workspace_from_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".jj")).unwrap();
        
        let workspace = JJWorkspace::find(temp_dir.path()).unwrap();
        assert_eq!(workspace.workspace_root(), temp_dir.path());
    }

    #[test]
    fn test_find_workspace_from_subdir() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".jj")).unwrap();
        let subdir = temp_dir.path().join("src/nested");
        fs::create_dir_all(&subdir).unwrap();
        
        let workspace = JJWorkspace::find(&subdir).unwrap();
        assert_eq!(workspace.workspace_root(), temp_dir.path());
    }

    #[test]
    fn test_find_workspace_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = JJWorkspace::find(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not in a JJ workspace"));
    }

    #[test]
    fn test_workspace_root_getter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = JJWorkspace::new(temp_dir.path());
        assert_eq!(workspace.workspace_root(), temp_dir.path());
    }

    // ... existing tests (workspace_init_creates_jj_directory, etc.) ...
}
```

---

### Phase 2: Update Shell Command Transformations to Use JJ

**Note:** `MoveOperation::from_move_paths()` is an existing helper method defined in `cli/src/events/change_completed.rs:94` that creates a `MoveOperation` from raw command arguments using syntactic detection (trailing slash, multiple sources). This is used as the fallback when JJ is unavailable.

**File:** `cli/src/event_bus.rs`

**Current (uses cache):**
```rust
AikiEvent::ShellCompleted(e) => {
    let command = e.command.clone();
    let (file_op, paths) = parse_file_operation_from_shell_command(&command);
    match file_op {
        Some(FileOperation::Delete) => {
            transform_shell_delete_to_change_completed(e, paths)
        }
        Some(FileOperation::Move) => {
            transform_shell_move_to_change_completed(e, paths, &command)
        }
        _ => events::handle_shell_completed(e),
    }
}
```

**New (uses JJ for paths):**
```rust
AikiEvent::ShellCompleted(e) => {
    let command = e.command.clone();
    let (file_op, paths) = parse_file_operation_from_shell_command(&command);
    match file_op {
        Some(FileOperation::Delete) => {
            // Use JJ to get accurate deleted file paths, filtered by command args
            transform_shell_delete_to_change_completed(e, paths)
        }
        Some(FileOperation::Move) => {
            // Use JJ to get accurate move source/destination paths, filtered by command args
            transform_shell_move_to_change_completed(e, paths)
        }
        _ => events::handle_shell_completed(e),
    }
}
```

**New transformation function for delete:**
```rust
/// Transform shell.completed to change.completed for rm/rmdir commands
///
/// First tries to use JJ for accurate path resolution. If JJ is unavailable
/// or returns no matching paths, falls back to syntactic detection from
/// command arguments (preserves existing behavior for Git-only repos).
fn transform_shell_delete_to_change_completed(
    shell_event: crate::events::AikiShellCompletedPayload,
    command_args: Vec<String>,
) -> Result<HookResult> {
    // Try to get actual deleted paths from JJ, filtered by command arguments
    let deleted_paths = match crate::jj::diff::get_deleted_paths(&shell_event.cwd, &command_args) {
        Ok(paths) if !paths.is_empty() => {
            debug_log(|| format!("Using JJ-detected paths: {:?}", paths));
            paths
        }
        Ok(_) => {
            // No deletions detected by JJ (or filtered out) - fall back to syntactic
            debug_log(|| "JJ detected no matching deletions, falling back to syntactic detection");
            // Use command args as syntactic fallback (existing behavior)
            command_args
        }
        Err(e) => {
            // JJ not available or error - fall back to syntactic detection
            debug_log(|| format!("JJ error ({}), falling back to syntactic detection", e));
            // Use command args as syntactic fallback (existing behavior)
            command_args
        }
    };

    debug_log(|| {
        format!(
            "Transforming shell.completed (rm/rmdir) to change.completed with {} paths",
            deleted_paths.len()
        )
    });

    let change_event = AikiChangeCompletedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        success: shell_event.success,
        operation: ChangeOperation::Delete(DeleteOperation {
            file_paths: deleted_paths,
        }),
    };

    events::handle_change_completed(change_event)
}
```

**New transformation function for move:**
```rust
/// Transform shell.completed to change.completed for mv commands
///
/// First tries to use JJ for accurate path resolution. If JJ is unavailable
/// or returns no matching paths, falls back to syntactic detection from
/// command arguments (preserves existing behavior for Git-only repos).
fn transform_shell_move_to_change_completed(
    shell_event: crate::events::AikiShellCompletedPayload,
    command_args: Vec<String>,
) -> Result<HookResult> {
    // Try to get actual move operations from JJ, filtered by command arguments
    let move_op = match crate::jj::diff::get_move_operations(&shell_event.cwd, &command_args) {
        Ok(ops) if !ops.is_empty() => {
            debug_log(|| format!("Using JJ-detected move operations: {:?}", ops));
            // Extract sources and destinations from JJ
            let sources: Vec<String> = ops.iter().map(|(s, _)| s.clone()).collect();
            let destinations: Vec<String> = ops.iter().map(|(_, d)| d.clone()).collect();
            
            MoveOperation {
                file_paths: destinations.clone(),
                source_paths: sources,
                destination_paths: destinations,
            }
        }
        Ok(_) => {
            // No moves detected by JJ (or filtered out) - fall back to syntactic
            debug_log(|| "JJ detected no matching moves, falling back to syntactic detection");
            // Use syntactic detection from command args (existing behavior)
            MoveOperation::from_move_paths(command_args)
        }
        Err(e) => {
            // JJ not available or error - fall back to syntactic detection
            debug_log(|| format!("JJ error ({}), falling back to syntactic detection", e));
            // Use syntactic detection from command args (existing behavior)
            MoveOperation::from_move_paths(command_args)
        }
    };

    debug_log(|| {
        format!(
            "Transforming shell.completed (mv) to change.completed with {} move operations",
            move_op.file_paths.len()
        )
    });

    let change_event = AikiChangeCompletedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        success: shell_event.success,
        operation: ChangeOperation::Move(move_op),
    };

    events::handle_change_completed(change_event)
}
```

---

### Phase 3: Remove MOVE_DIR_CACHE

Delete the following from `cli/src/event_bus.rs`:

1. **Lines 14-25:** `MOVE_DIR_CACHE` static declaration and documentation
2. **Lines 269-286:** Caching logic in `transform_shell_move_to_change_permission_asked()`
3. **Lines 323-335:** Cache lookup in `transform_shell_move_to_change_completed()`
4. **Delete entire function:** `transform_shell_move_to_change_completed()` (replaced by `transform_shell_move_to_change_completed_with_jj`)
5. **Delete entire function:** `transform_shell_delete_to_change_completed()` (replaced by `transform_shell_delete_to_change_completed_with_jj`)

---

### Phase 4: Simplify permission_asked Move Transformation

**Remove caching logic:**

```rust
fn transform_shell_move_to_change_permission_asked(
    shell_event: crate::events::AikiShellPermissionAskedPayload,
    paths: Vec<String>,
    _command: &str,  // No longer needed for cache key
) -> Result<HookResult> {
    debug_log(|| {
        format!(
            "Transforming shell.permission_asked (mv) to change.permission_asked: {:?}",
            paths
        )
    });

    // Use syntactic-only detection for pre-event (best effort)
    // The completed event will use JJ for accurate paths
    let move_op = MoveOperation::from_move_paths(paths);

    let change_event = AikiChangePermissionAskedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        operation: ChangeOperation::Move(move_op),
    };

    events::handle_change_permission_asked(change_event)
}
```

---

## Review Findings Addressed

**All blocking issues from ops/review.md have been fixed:**
1. ✅ `run_jj_diff_summary` now propagates errors via `anyhow::bail!()` instead of returning `Ok(String::new())`
2. ✅ Transform functions fall back to syntactic detection (using `command_args`) instead of calling `handle_shell_completed()`
3. ✅ Added missing `use anyhow::Context;` import to `cli/src/jj/diff.rs`

---

### Blocking Issue #1: Canonicalization fails on deleted files
**Problem:** `normalize_paths_to_workspace_root` calls `canonicalize()` on every path, which fails with ENOENT for deleted files (the common case for delete operations). This causes the transformation to fall back to syntactic detection, losing JJ accuracy.

**Solution:** Use `canonicalize().unwrap_or_else(|_| path)` pattern:
- Try to canonicalize first (works for existing files and supports symlinks)
- Fall back to non-canonical path if canonicalization fails (handles deleted files)
- This allows deleted file paths to still be normalized to workspace-relative form

### Blocking Issue #2: Paths outside workspace are filtered out (not fatal errors)
**Problem:** Commands like `mv /tmp/foo src/` or `mv file ../outside` include paths both inside and outside the workspace. The original plan treated out-of-workspace paths as fatal errors, causing the entire transformation to fail and fall back to syntactic detection.

**Solution:** Filter instead of error:
- Use `filter_map()` instead of `map()` in `normalize_paths_to_workspace_root`
- Silently skip paths that can't be made workspace-relative (when `strip_prefix()` fails)
- Return a vector containing only the workspace paths (may be empty if all paths are outside)
- Example: `mv /tmp/foo src/bar` normalizes to `["src/bar"]`, and JJ tracks the workspace side
- If all paths are outside workspace, return empty vec so caller falls back to syntactic detection

This handles legitimate cross-boundary operations like imports/exports without throwing away JJ accuracy for the in-workspace operands.

### Blocking Issue #3: Exact-match filtering loses directory operations
**Problem:** Original plan used exact string matching to prevent re-emitting stale JJ entries. This breaks directory operations like `rm -rf dir` or `mv dir new_dir` because JJ outputs `D dir/file` which doesn't equal the command argument `dir`.

**Solution:** Use prefix matching without cross-event deduplication:
- Each shell command fires a separate `shell.completed` event
- Each event is processed independently with its own command arguments
- The command arguments naturally filter out paths from previous commands
- Example: `rm a.txt` then `rm b.txt` → second event's args are `["b.txt"]`, which doesn't match `a.txt` from JJ diff
- Edge case of overlapping directory operations (rare) is acceptable

No deduplication state needed because events are independent.

### Finding #0: Missing JJWorkspace APIs and path normalization
**Problem:** The original plan referenced `JJWorkspace::find()` and `.workspace_root()` without documenting how to implement them. Additionally, it compared JJ's repo-relative paths against raw command arguments without normalization, which breaks for commands run from subdirectories.

**Solution:**
1. **Added `JJWorkspace::find(path: &Path) -> Result<Self>`**
   - Walks up directory tree looking for `.jj/`
   - Returns error with clear message if not in JJ workspace
   - Tested from root and subdirectories
   
2. **Added `JJWorkspace::workspace_root(&self) -> &Path`**
   - Simple getter for the workspace root path
   - Used by diff functions to determine where to run JJ
   
3. **Added `normalize_paths_to_workspace_root()` function**
   - Takes command args (relative to shell cwd) and converts to workspace-relative paths
   - **Canonicalizes both shell_cwd and workspace_root** to support symlinked working directories
   - Handles relative paths: `shell_cwd.canonicalize()?.join(arg)` → `strip_prefix(workspace_root.canonicalize()?)`
   - Handles absolute paths: `canonicalize()` then `strip_prefix(workspace_root.canonicalize()?)`
   - Returns error if path is outside workspace (catches mistakes)
   
4. **Updated `get_deleted_paths()` and `get_move_operations()`**
   - Now call `JJWorkspace::find(shell_cwd)?` first
   - Normalize command args before filtering: `normalize_paths_to_workspace_root(args, shell_cwd, workspace_root)?`
   - Both JJ paths and normalized args are now workspace-relative (apples-to-apples comparison)
   - Pass `workspace_root` to `run_jj_diff_summary()` (no longer needs to find it)

**Why this fixes the subdirectory problem:**
```rust
// Before (BROKEN):
// shell_cwd = /workspace/src
// command_args = ["foo.rs"]  // Relative to src/
// JJ output = "D src/foo.rs"  // Relative to workspace root
// Comparison: "src/foo.rs" == "foo.rs" → false (NO MATCH)

// After (FIXED):
// shell_cwd = /workspace/src
// command_args = ["foo.rs"]
// normalized_args = ["src/foo.rs"]  // Normalized to workspace root
// JJ output = "D src/foo.rs"
// Comparison: "src/foo.rs" == "src/foo.rs" → true (MATCH!)
```

**Why canonicalization fixes symlink support:**
```rust
// Before (BROKEN):
// workspace_root = /real/path/workspace (canonicalized by JJWorkspace::find)
// shell_cwd = /symlink/to/workspace/src (original symlinked path)
// shell_cwd.join("foo.rs") = /symlink/to/workspace/src/foo.rs
// strip_prefix(/real/path/workspace) → ERROR: not a prefix

// After (FIXED):
// workspace_root = /real/path/workspace (canonicalized)
// shell_cwd = /symlink/to/workspace/src → canonicalize() → /real/path/workspace/src
// shell_cwd.join("foo.rs") = /real/path/workspace/src/foo.rs
// strip_prefix(/real/path/workspace) → OK: src/foo.rs
```

### Finding #1: JJ diff returns all pending operations, not just current command
**Problem:** JJ shows all pending changes in the working copy, not just the ones from the current command. We need to filter JJ's output to match the current shell command's arguments, while supporting:
1. Directory operations like `rm -rf src/`
2. Glob patterns like `rm *.tmp` or `mv src/{a,b}.rs dest/`
3. Paths outside the workspace (e.g., `mv /tmp/foo src/bar`)

**Solution:** Multi-stage filtering approach in `filter_paths_by_command()`:

1. **Empty args handling** - If `normalized_args` is empty (all paths were outside workspace), return empty vec to trigger syntactic fallback
2. **Glob detection** - Check if any arg contains glob metacharacters (`*`, `?`, `[`, `{`)
3. **Glob matching** - If globs detected, use `globset` crate for proper pattern matching
4. **Prefix matching** - If no globs, use exact/prefix matching for directory operations

**Why cross-command duplicates don't occur:**
- Each shell command fires a separate `shell.completed` event
- Each event is processed independently with its own command arguments
- The filtering naturally excludes paths from previous commands in the same working copy change

**Example showing why duplicates don't occur:**
```rust
// Command 1: rm a.txt
// → shell.completed event with args=["a.txt"]
// → JJ shows: "D a.txt"
// → Filter: "a.txt" matches ["a.txt"] → emits ["a.txt"]

// Command 2: rm b.txt (separate event)
// → shell.completed event with args=["b.txt"]
// → JJ shows: "D a.txt\nD b.txt"
// → Filter: "a.txt" matches ["b.txt"] → NO
// → Filter: "b.txt" matches ["b.txt"] → YES
// → emits ["b.txt"] only (no duplicate)

// Command 3: rm *.tmp (glob pattern)
// → shell.completed event with args=["*.tmp"]
// → JJ shows: "D a.txt\nD b.txt\nD foo.tmp\nD bar.tmp"
// → Glob filter: only "foo.tmp" and "bar.tmp" match pattern
// → emits ["foo.tmp", "bar.tmp"] (no previous files)
```

**Updated `filter_paths_by_command()` implementation:**
```rust
/// Filter paths to only include those matching command arguments
///
/// Supports exact matches, directory prefix matches, and glob patterns.
/// Returns empty vec if command_args is empty (all paths outside workspace).
fn filter_paths_by_command(paths: &[String], command_args: &[String]) -> Vec<String> {
    // Empty args = all operands outside workspace, return empty for syntactic fallback
    if command_args.is_empty() {
        return vec![];
    }
    
    // Check if any argument contains glob metacharacters
    let has_glob = command_args.iter().any(|arg| {
        arg.contains('*') || arg.contains('?') || arg.contains('[') || arg.contains('{')
    });
    
    if has_glob {
        // Build a glob matcher for all patterns
        let mut builder = GlobSetBuilder::new();
        for arg in command_args {
            // Add the pattern itself
            if let Ok(glob) = Glob::new(arg) {
                builder.add(glob);
            }
            // Also add pattern/** for directory matching (e.g., src/* matches src/foo/bar.rs)
            let dir_pattern = format!("{}/**", arg.trim_end_matches('/').trim_end_matches('\\'));
            if let Ok(glob) = Glob::new(&dir_pattern) {
                builder.add(glob);
            }
        }
        
        // If glob compilation failed, fall back to returning empty vec
        let Ok(globset) = builder.build() else {
            return vec![];
        };
        
        // Filter paths using glob matching
        return paths
            .iter()
            .filter(|path| globset.is_match(path))
            .cloned()
            .collect();
    }
    
    // No globs - use exact/prefix matching
    paths
        .iter()
        .filter(|path| path_matches_command(path, command_args))
        .cloned()
        .collect()
}
```

### Finding #2: Parser must handle `{old => new}` format with braces
**Solution:** Parser correctly handles actual JJ output format (verified)
- JJ outputs: `R {old_path => new_path}` (WITH braces)
- Paths with spaces are NOT quoted, they appear as-is within braces
- Examples from actual `jj diff -r @ --summary` output:
  - `R {old.txt => new.txt}`
  - `R {file with spaces.txt => renamed.txt}`
  - `R {file.txt => dir/file.txt}`
- `parse_move_line()` strips braces then splits on ` => `
- Tests updated to match verified output format

### Finding #3: `run_jj_diff_summary` shells out from command's cwd
**Solution:** Use JJWorkspace to find repo root
- `run_jj_diff_summary()` now calls `JJWorkspace::find(cwd)?` first
- Always runs JJ from workspace root, not from arbitrary subdirectory
- Handles shell commands executed from nested directories correctly

### Finding #4: No fallback plan when JJ is missing/not initialized
**Solution:** Fall back to syntactic detection (existing behavior)
- When JJ is unavailable (not installed, not initialized, or errors), use `command_args` directly
- When JJ returns empty results (no matching paths), use `command_args` directly
- This preserves existing behavior for Git-only environments
- `change.completed` events still fire with syntactic paths (may be less accurate than JJ)
- No regression from current behavior - syntactic detection was the original approach

**Updated behavior:**
```rust
let deleted_paths = match crate::jj::diff::get_deleted_paths(&shell_event.cwd, &command_args) {
    Ok(paths) if !paths.is_empty() => {
        // Use JJ paths (accurate)
        paths
    }
    Ok(_) | Err(_) => {
        // JJ returned no deletions OR JJ not available
        // Fall back to syntactic detection (command_args)
        command_args
    }
};
```

This means:
- **JJ available + paths found:** Use JJ paths (most accurate)
- **JJ available + no paths:** Use syntactic paths (command might have failed or operated outside working copy)
- **JJ unavailable:** Use syntactic paths (same as before JJ integration)

**No regression:** Git-only repos continue to work with syntactic detection, just like they do today.

### Finding #5: Mismatch between permission_asked and completed paths
**Status:** Documented as known limitation
- `permission_asked` fires BEFORE operation, so can't use JJ (changes haven't happened)
- Uses syntactic parsing for best-effort paths (may be approximate)
- `completed` uses JJ for accurate paths (always correct)
- This is acceptable because provenance uses `completed` event paths
- Users may see different paths between prompt and completion (rare edge cases only)

---

## Key Design Decisions

### 1. Operation Detection vs Path Resolution

**Operation detection (shell command parsing):**
- Tells us "this is a `mv` command" or "this is a `rm` command"
- Quick, doesn't require JJ
- Allows `permission_asked` to work (fires before operation)

**Path resolution (JJ):**
- Tells us exactly which files were moved/deleted
- Always accurate (handles `mv file existing_dir` correctly)
- Only works after operation completes

### 2. Keep permission_asked Simple

`permission_asked` fires BEFORE the operation, so:
- Can't use JJ (changes haven't happened yet)
- Use shell command parsing for best-effort paths
- Accept that paths may be approximate (e.g., `mv file dir` might report wrong dest)
- Not critical since `completed` will have accurate paths for provenance

### 3. Handle No JJ Changes Gracefully

If shell command parsing says "this is a move" but JJ shows no moves:
- Command might have failed
- Or moved files outside working copy
- Fall back to normal `shell.completed` handling (no change event)

---

## Testing Strategy

### Unit Tests (cli/src/jj/diff.rs)
- [x] Parse move line: `old.txt => new.txt` (no braces)
- [x] Parse move line with quoted source: `"file with spaces.txt" => dir/file.txt`
- [x] Parse move line with quoted dest: `old.txt => "path/with spaces/new.txt"`
- [x] Extract deleted paths from summary output
- [x] Extract move operations from summary output
- [x] Test `path_matches_command()` with exact matches
- [x] Test `path_matches_command()` with directory descendants (with and without trailing slash)
- [x] Test `filter_paths_by_command()` with exact matches
- [x] Test `filter_paths_by_command()` with directory matching
- [x] Handle empty output
- [x] Test `normalize_paths_to_workspace_root()` from workspace root
- [x] Test `normalize_paths_to_workspace_root()` from subdirectory
- [x] Test `normalize_paths_to_workspace_root()` with absolute paths
- [x] Test `normalize_paths_to_workspace_root()` with multiple paths
- [x] Test normalization filters out paths outside workspace (not error)
- [x] Test normalization handles deleted files (canonicalization fails gracefully)
- [ ] Test JJ error handling (JJ not found)
- [ ] Test JJ error handling (not a JJ repo)

### Unit Tests (cli/src/jj/workspace.rs)
- [x] Test `find()` from workspace root
- [x] Test `find()` from subdirectory
- [x] Test `find()` error when not in JJ workspace
- [x] Test `workspace_root()` getter

### Integration Tests
- [ ] Shell `rm file.txt` → JJ shows `D file.txt` → emits `change.completed(Delete)` with correct path
- [ ] Shell `mv file.txt renamed.txt` → JJ shows `R file.txt => renamed.txt` → emits `change.completed(Move)` with correct paths
- [ ] Shell `mv file.txt existing_dir/` → JJ shows `R file.txt => existing_dir/file.txt` → emits `change.completed(Move)` with correct destination
- [ ] Shell `mv a b c dir/` (multiple files) → JJ shows multiple R lines → emits `change.completed(Move)` with all moves
- [ ] Shell command that fails → JJ shows no changes → emits normal `shell.completed`
- [ ] Multiple deletes in same working copy change → only emit paths matching current command
- [ ] Command from subdirectory → JJ runs from workspace root → paths resolved correctly
- [ ] JJ not initialized → graceful fallback to syntactic detection

### Manual Testing with AIKI_DEBUG=1
```bash
# Test delete with accurate paths
cd /tmp/test-aiki && jj git init
echo "test" > file.txt && jj new
AIKI_DEBUG=1 aiki acp &
rm file.txt
# Should see: "JJ detected delete: file.txt" → change.completed(Delete)

# Test move with directory detection
mkdir dir
echo "test" > file.txt && jj new
AIKI_DEBUG=1 aiki acp &
mv file.txt dir/
# Should see: "JJ detected move: file.txt => dir/file.txt" → change.completed(Move)

# Test move with spaces in filename
touch "spaced file.txt" && jj new
AIKI_DEBUG=1 aiki acp &
mv "spaced file.txt" dir/
# Should see: "JJ detected move: spaced file.txt => dir/spaced file.txt"

# Test delete from subdirectory (path normalization)
mkdir -p src/nested
echo "test" > src/nested/file.txt && jj new
cd src/nested
AIKI_DEBUG=1 aiki acp &
rm file.txt
# Should see: "JJ detected delete: src/nested/file.txt" → change.completed(Delete)
# (path is normalized to workspace root)

# Test move from subdirectory (path normalization)
cd /tmp/test-aiki
mkdir -p src/old src/new
echo "test" > src/old/file.txt && jj new
cd src/old
AIKI_DEBUG=1 aiki acp &
mv file.txt ../new/
# Should see: "JJ detected move: src/old/file.txt => src/new/file.txt"
# (both paths normalized to workspace root)
```

---

## Files to Modify

| File | Changes |
|------|---------|
| **NEW:** `cli/src/jj/` | Create jj module directory |
| **NEW:** `cli/src/jj/mod.rs` | Re-export workspace and diff submodules |
| **NEW:** `cli/src/jj/workspace.rs` | Move `JJWorkspace` from `cli/src/jj.rs` |
| **NEW:** `cli/src/jj/diff.rs` | JJ diff path extraction functions with glob support |
| **DELETE:** `cli/src/jj.rs` | Replaced by jj/ module |
| `cli/Cargo.toml` | Add `globset = "0.4"` dependency for glob pattern matching |
| `cli/src/event_bus.rs` | Replace `transform_shell_*_to_change_completed` functions with JJ-based versions, remove `MOVE_DIR_CACHE`, simplify `permission_asked` move transformation |

---

## Migration Strategy

1. **Restructure JJ module** (convert `jj.rs` → `jj/` directory with `mod.rs`, `workspace.rs`, `diff.rs`)
2. **Update transformation functions** to use JJ and add command_args parameter
3. **Remove MOVE_DIR_CACHE**
4. **Update tests**

---

## Open Questions

1. **Multiple moves in single command:** Should `mv a b c dir/` emit one `change.completed` with all moves, or separate events per file?
   - **Answer:** One event with all moves (matches shell command granularity)

2. **Performance:** Is running `jj diff -r @ --summary` on every shell delete/move acceptable?
   - Likely yes - only runs when shell command parsing detects delete/move
   - JJ is fast (reads from disk-based state)
   - Can measure and optimize later if needed

3. **Mixed operations:** What if shell command does both move and delete? (`mv a b && rm c`)
   - Shell command parsing only detects **first** operation (`mv`)
   - But JJ will see **all** filesystem changes (both move and delete)
   - Current implementation: Only first operation detected via shell parsing
   - **Improvement:** Could detect all operations if we query JJ for any shell command, not just detected file operations
   - For MVP: Accept that only explicitly detected operations (first command) are tracked

---

## Definition of Done

- [ ] JJ module restructured (`jj.rs` → `jj/` with `mod.rs`, `workspace.rs`, `diff.rs`)
- [ ] `cli/src/jj/diff.rs` implemented with path extraction functions
- [ ] `shell.completed` for delete uses JJ to get accurate paths
- [ ] `shell.completed` for move uses JJ to get accurate source/destination paths
- [ ] `MOVE_DIR_CACHE` removed entirely
- [ ] Move/delete operations have accurate paths regardless of edge cases
- [ ] All existing tests pass
- [ ] New JJ path extraction tests pass
- [ ] Manual testing confirms `mv file existing_dir` works correctly
- [ ] Performance is acceptable

---

## Summary of Review Finding Fixes

### Finding 1: Path rebasing issue ✅ FIXED

**Problem:** JJ returns workspace-root-relative paths (e.g., `src/foo.rs`), but downstream consumers expect paths relative to `event.cwd`. When a command runs from `src/`, joining `src/foo.rs` onto `/repo/src` creates `/repo/src/src/foo.rs` (invalid).

**Solution:** Added `rebase_paths_to_cwd()` function that converts workspace-relative paths to cwd-relative paths:
- Strips the cwd prefix when path is inside cwd (e.g., `src/foo.rs` → `foo.rs` when cwd=`/repo/src`)
- Uses `../` notation when path is outside cwd (e.g., `lib.rs` → `../lib.rs` when cwd=`/repo/src`)
- Handles nested subdirectories correctly (e.g., `../../lib.rs` when cwd=`/repo/src/nested`)

**Changed functions:**
- `get_deleted_paths()`: Now calls `rebase_paths_to_cwd()` before returning
- `get_move_operations()`: Rebases both source and destination paths before returning

**Added tests:**
- `test_rebase_paths_to_cwd_from_root()` - Paths stay unchanged when command runs from workspace root
- `test_rebase_paths_to_cwd_from_subdir()` - Strips cwd prefix correctly
- `test_rebase_paths_to_cwd_outside_cwd()` - Uses `../` for paths outside cwd
- `test_rebase_paths_to_cwd_nested_subdir()` - Handles multiple directory levels

### Finding 2: Glob pattern support ✅ FIXED

**Problem:** Shell commands like `rm *.tmp` or `mv src/{a,b}.rs dest/` pass unexpanded glob patterns to the parser. The literal string `"*.tmp"` never matches actual filenames like `"foo.tmp"` from JJ, so all paths are filtered out and no event is emitted.

**Solution:** Implemented proper glob matching using the `globset` crate:
- Detects glob metacharacters (`*`, `?`, `[`, `{`) in command arguments
- Builds a `GlobSet` from the patterns and filters JJ paths using glob matching
- Supports all standard glob patterns: `*.txt`, `src/{a,b}.rs`, `[abc].rs`, `?.txt`
- Also adds `pattern/**` for directory matching (e.g., `src/*` matches `src/foo/bar.rs`)
- Falls back to syntactic detection if glob compilation fails

**Why this works:**
- `globset` provides Unix shell-compatible glob matching
- Each shell command is processed independently, so filtering by glob prevents re-emitting files from previous operations
- Example: `rm foo.rs` then `rm *.tmp` → second event filters JJ paths using `*.tmp` pattern, excludes `foo.rs`

**Added dependency:**
- `globset = "0.4"` in `cli/Cargo.toml`

**Added tests:**
- `test_filter_paths_with_glob_patterns()` - Tests all glob metacharacters with proper matching
- Verifies `*.txt` matches only `.txt` files
- Verifies `src/{c,d}.rs` matches specific files
- Verifies `[ab].txt` matches bracket patterns
- Verifies literal arguments still filter normally

### Finding 3: Windows path separator handling ✅ FIXED

**Problem:** `path_matches_command()` used string-based prefix checks with hardcoded `/` separator. On Windows:
- JJ might output forward slashes: `src/foo.rs`
- Command args use backslashes: `src\foo.rs`
- String comparison `"src/foo.rs".starts_with("src\")` fails → no matches on Windows

**Solution:** Rewrote `path_matches_command()` to use `Path` semantics instead of string manipulation:
- `Path::new(path)` and `Path::new(arg)` normalize separators automatically
- `path.starts_with(arg_path)` uses platform-aware comparison
- Works correctly on both Unix (forward slashes) and Windows (backslashes)

**Changed implementation:**
```rust
// Old (string-based, broken on Windows):
path.starts_with(&format!("{}/", arg.trim_end_matches('/')))

// New (Path-based, cross-platform):
let path = Path::new(path);
let arg_path = Path::new(arg.trim_end_matches('/').trim_end_matches('\\'));
path.starts_with(arg_path)
```

**Added tests:**
- `test_path_matches_command_windows_separators()` - Verifies backslash handling
- Tests exact matches and directory prefix matches with Windows paths

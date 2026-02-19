//! Workspace isolation for concurrent agent sessions
//!
//! When multiple agent sessions run concurrently in the same repo, they share
//! the same JJ workspace. This module provides isolated JJ workspaces per
//! session, with lazy creation (only when concurrent), automatic merge-back
//! at session end, and crash recovery.
//!
//! Workspace paths follow: `~/.aiki/workspaces/<repo-id>/<session-uuid>/`

use crate::cache::debug_log;
use crate::error::{AikiError, Result};
use crate::global;
use crate::jj::{jj_cmd, JJWorkspace};
use crate::repo_id;
use std::fs;
use std::path::{Path, PathBuf};

/// An isolated JJ workspace for a specific session/repo pair
#[derive(Debug, Clone)]
pub struct IsolatedWorkspace {
    /// Workspace name: "aiki-<session-uuid>"
    pub name: String,
    /// Workspace path: ~/.aiki/workspaces/<repo-id>/<session-uuid>/
    pub path: PathBuf,
    /// Project root this workspace belongs to
    pub repo_root: PathBuf,
    /// Session UUID that owns this workspace
    pub session_uuid: String,
}

/// Walk up from path looking for `.jj/` directory. Returns repo root or None.
///
/// Delegates to `JJWorkspace::find()` — does not reimplement the walk.
pub fn find_jj_root(path: &Path) -> Option<PathBuf> {
    JJWorkspace::find(path)
        .ok()
        .map(|ws| ws.workspace_root().to_path_buf())
}

/// Create an isolated JJ workspace for a repo/session pair.
///
/// Idempotent: returns existing workspace if directory already exists.
///
/// - workspace_name: "aiki-<session-uuid>"
/// - workspace_path: ~/.aiki/workspaces/<repo-id>/<session-uuid>/
/// - Forks from repo's main workspace @- (parent of working copy, starts clean)
pub fn create_isolated_workspace(
    repo_root: &Path,
    session_uuid: &str,
) -> Result<IsolatedWorkspace> {
    let repo_id = repo_id::read_repo_id(repo_root)?
        .ok_or_else(|| {
            AikiError::WorkspaceCreationFailed(format!(
                "No repo-id found at {}",
                repo_root.display()
            ))
        })?;

    let workspace_path = global::global_aiki_dir()
        .join("workspaces")
        .join(&repo_id)
        .join(session_uuid);
    let workspace_name = format!("aiki-{}", session_uuid);

    let workspace = IsolatedWorkspace {
        name: workspace_name.clone(),
        path: workspace_path.clone(),
        repo_root: repo_root.to_path_buf(),
        session_uuid: session_uuid.to_string(),
    };

    // Idempotent: if workspace directory already exists, return it
    if workspace_path.exists() {
        debug_log(|| {
            format!(
                "Workspace already exists at {}, reusing",
                workspace_path.display()
            )
        });
        return Ok(workspace);
    }

    // Create parent directories
    if let Some(parent) = workspace_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AikiError::WorkspaceCreationFailed(format!(
                "Failed to create workspace parent dirs: {}",
                e
            ))
        })?;
    }

    // Resolve default workspace parent explicitly to avoid ambiguous @
    // in multi-workspace contexts (where --ignore-working-copy can cause @- to
    // resolve to root() instead of the actual parent)
    let parent_output = jj_cmd()
        .current_dir(repo_root)
        .args([
            "log",
            "-r",
            "@-",
            "-T",
            "change_id",
            "--no-graph",
            "--limit",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceCreationFailed(format!("Failed to resolve @-: {}", e))
        })?;

    let parent_change_id = if parent_output.status.success() {
        let id = String::from_utf8_lossy(&parent_output.stdout)
            .trim()
            .to_string();
        if id.is_empty() {
            "@-".to_string()
        } else {
            id
        }
    } else {
        "@-".to_string()
    };

    // Create workspace forked from the resolved parent
    let output = jj_cmd()
        .current_dir(repo_root)
        .args([
            "workspace",
            "add",
            &workspace_path.to_string_lossy(),
            "--name",
            &workspace_name,
            "-r",
            &parent_change_id,
        ])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceCreationFailed(format!("Failed to run jj workspace add: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::WorkspaceCreationFailed(format!(
            "jj workspace add failed: {}",
            stderr.trim()
        )));
    }

    debug_log(|| {
        format!(
            "Created isolated workspace '{}' at {}",
            workspace_name,
            workspace_path.display()
        )
    });

    Ok(workspace)
}

/// Absorb workspace changes into the target workspace.
///
/// Target is parent session's workspace if it exists, otherwise main.
///
/// 1. Resolve workspace head via JJ
/// 2. If no changes, return early
/// 3. Rebase main/parent onto workspace head
pub fn absorb_workspace(
    repo_root: &Path,
    workspace: &IsolatedWorkspace,
    parent_session_uuid: Option<&str>,
) -> Result<()> {
    // Get workspace working copy change ID by parsing `jj workspace list`
    // (workspace_id() revset doesn't exist in JJ 0.38)
    let ws_change_id = find_workspace_change_id(repo_root, &workspace.name)?;
    let ws_change_id = match ws_change_id {
        Some(id) => id,
        None => {
            debug_log(|| {
                format!(
                    "Workspace '{}' not found in jj workspace list, skipping absorb",
                    workspace.name
                )
            });
            return Ok(());
        }
    };

    // Get the parent of the workspace's working copy (the last real commit)
    let parent_revset = format!("{}-", ws_change_id);
    let output = jj_cmd()
        .current_dir(repo_root)
        .args([
            "log",
            "-r",
            &parent_revset,
            "-T",
            "change_id",
            "--no-graph",
            "-l",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceAbsorbFailed(format!("Failed to query workspace head: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::WorkspaceAbsorbFailed(format!(
            "Failed to query workspace head: {}",
            stderr.trim()
        )));
    }

    let ws_head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ws_head.is_empty() {
        debug_log(|| "No changes in workspace (empty output), skipping absorb");
        return Ok(());
    }

    // Guard against root/empty change heads — these indicate no real commits
    // were made in the workspace. JJ's root change ID is all zeros.
    if ws_head.chars().all(|c| c == '0') {
        debug_log(|| "Workspace head is root change, skipping absorb");
        return Ok(());
    }

    // Note: we skip a separate "verify changes exist" jj log call — jj rebase
    // is a no-op when there's nothing to rebase, and the subprocess overhead
    // of the check (~30ms) exceeds the cost of a no-op rebase.

    // Determine absorb target directory
    let target_dir = if let Some(parent_uuid) = parent_session_uuid {
        let repo_id = repo_id::read_repo_id(repo_root)?
            .unwrap_or_default();
        let parent_ws_path = global::global_aiki_dir()
            .join("workspaces")
            .join(&repo_id)
            .join(parent_uuid);
        if parent_ws_path.exists() {
            parent_ws_path
        } else {
            repo_root.to_path_buf()
        }
    } else {
        repo_root.to_path_buf()
    };

    // Rebase: jj rebase -b @ -d <ws_head>
    let output = jj_cmd()
        .current_dir(&target_dir)
        .args(["rebase", "-b", "@", "-d", &ws_head, "--ignore-working-copy"])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceAbsorbFailed(format!("Failed to run jj rebase: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::WorkspaceAbsorbFailed(format!(
            "jj rebase failed: {}",
            stderr.trim()
        )));
    }

    // Log any divergent-operation warning from stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        debug_log(|| format!("jj rebase stderr: {}", stderr.trim()));
    }

    debug_log(|| {
        format!(
            "Absorbed workspace '{}' into {}",
            workspace.name,
            target_dir.display()
        )
    });

    Ok(())
}

/// Forget workspace in JJ and delete its directory.
pub fn cleanup_workspace(
    repo_root: &Path,
    workspace: &IsolatedWorkspace,
) -> Result<()> {
    // Forget the workspace in JJ
    let output = jj_cmd()
        .current_dir(repo_root)
        .args(["workspace", "forget", &workspace.name, "--ignore-working-copy"])
        .output()
        .map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to forget workspace '{}': {}",
                workspace.name,
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug_log(|| format!("jj workspace forget warning: {}", stderr.trim()));
        // Don't fail — workspace might already be forgotten
    }

    // Remove the directory
    match fs::remove_dir_all(&workspace.path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            debug_log(|| {
                format!(
                    "Warning: failed to remove workspace dir {}: {}",
                    workspace.path.display(),
                    e
                )
            });
        }
    }

    debug_log(|| format!("Cleaned up workspace '{}'", workspace.name));
    Ok(())
}

/// Find and recover all workspaces for a dead session across all repos.
///
/// Scans `~/.aiki/workspaces/*/<session-uuid>/` (where * is repo-id).
/// For each: absorb into main, then cleanup.
/// If absorb fails, creates a recovery bookmark and warns.
pub fn recover_orphaned_workspaces(session_uuid: &str) -> Result<u32> {
    let workspaces_dir = global::global_aiki_dir().join("workspaces");
    if !workspaces_dir.exists() {
        return Ok(0);
    }

    let mut recovered = 0u32;

    // Scan repo-id directories
    let entries = fs::read_dir(&workspaces_dir).map_err(|e| {
        AikiError::Other(anyhow::anyhow!(
            "Failed to read workspaces dir: {}",
            e
        ))
    })?;

    for entry in entries.flatten() {
        let repo_id_dir = entry.path();
        if !repo_id_dir.is_dir() {
            continue;
        }

        let session_ws_dir = repo_id_dir.join(session_uuid);
        if !session_ws_dir.exists() {
            continue;
        }

        let workspace_name = format!("aiki-{}", session_uuid);

        // Try to find the repo root from the workspace
        // The workspace contains a .jj/ that links back to the repo
        let repo_root = match find_repo_root_from_workspace(&session_ws_dir) {
            Some(root) => root,
            None => {
                eprintln!(
                    "[aiki] Warning: could not determine repo root for orphaned workspace at {}",
                    session_ws_dir.display()
                );
                // Clean up the directory even if we can't absorb
                let _ = fs::remove_dir_all(&session_ws_dir);
                continue;
            }
        };

        let workspace = IsolatedWorkspace {
            name: workspace_name.clone(),
            path: session_ws_dir,
            repo_root: repo_root.clone(),
            session_uuid: session_uuid.to_string(),
        };

        // Try to absorb into main
        match absorb_workspace(&repo_root, &workspace, None) {
            Ok(()) => {
                recovered += 1;
            }
            Err(e) => {
                // Fallback: create recovery bookmark
                eprintln!(
                    "[aiki] Warning: failed to absorb orphaned workspace '{}': {}",
                    workspace_name, e
                );
                // Try to create a recovery bookmark using workspace list parsing
                let bookmark_name = format!("aiki/recovered/{}", workspace_name);
                if let Ok(Some(ws_cid)) =
                    find_workspace_change_id(&repo_root, &workspace_name)
                {
                    let parent_rev = format!("{}-", ws_cid);
                    let _ = jj_cmd()
                        .current_dir(&repo_root)
                        .args([
                            "bookmark",
                            "create",
                            &bookmark_name,
                            "-r",
                            &parent_rev,
                            "--ignore-working-copy",
                        ])
                        .output();
                    eprintln!(
                        "[aiki] Orphaned workspace had untagged changes at {}",
                        bookmark_name
                    );
                } else {
                    eprintln!(
                        "[aiki] Warning: could not find workspace '{}' in jj workspace list for recovery bookmark",
                        workspace_name
                    );
                }
            }
        }

        // Always clean up
        let _ = cleanup_workspace(&repo_root, &workspace);
    }

    Ok(recovered)
}

/// Clean up orphaned JJ workspaces that no longer have active sessions.
///
/// Scans `jj workspace list` for `aiki-*` entries, checks if each session
/// is still registered in by-repo/, and forgets workspaces for dead sessions.
/// This prevents the JJ workspace list from growing unbounded.
pub fn cleanup_orphaned_workspaces(repo_root: &Path) -> Result<u32> {
    let output = jj_cmd()
        .current_dir(repo_root)
        .args(["workspace", "list", "--ignore-working-copy"])
        .output()
        .map_err(|e| {
            AikiError::Other(anyhow::anyhow!("Failed to list workspaces: {}", e))
        })?;

    if !output.status.success() {
        return Ok(0);
    }

    let list_str = String::from_utf8_lossy(&output.stdout);
    let mut cleaned = 0u32;

    for line in list_str.lines() {
        // Match lines like "aiki-<uuid>: ..."
        let ws_name = match line.split(':').next() {
            Some(name) if name.starts_with("aiki-") => name.trim(),
            _ => continue,
        };

        // Extract UUID from workspace name "aiki-<uuid>"
        let uuid = &ws_name["aiki-".len()..];

        // Check if this session is still active (has a by-repo sidecar)
        if find_session_repo(uuid).is_some() {
            continue; // Session is still active, skip
        }

        // Session is dead — forget the workspace
        debug_log(|| {
            format!(
                "Forgetting orphaned workspace '{}' (no active session)",
                ws_name
            )
        });

        let forget_output = jj_cmd()
            .current_dir(repo_root)
            .args(["workspace", "forget", ws_name, "--ignore-working-copy"])
            .output();

        if let Ok(out) = forget_output {
            if out.status.success() {
                cleaned += 1;
            }
        }

        // Also clean up workspace directory if it exists
        if let Ok(Some(repo_id)) = crate::repo_id::read_repo_id(repo_root) {
            let ws_dir = global::global_aiki_dir()
                .join("workspaces")
                .join(&repo_id)
                .join(uuid);
            if ws_dir.exists() {
                let _ = fs::remove_dir_all(&ws_dir);
            }
        }
    }

    if cleaned > 0 {
        debug_log(|| format!("Cleaned up {} orphaned workspace(s)", cleaned));
    }

    Ok(cleaned)
}

/// Count how many active sessions are using a specific repo.
///
/// Counts entries in `~/.aiki/sessions/by-repo/<repo-id>/` — O(1) directory listing
/// instead of scanning and parsing every session file.
pub fn count_sessions_in_repo(repo_id: &str) -> usize {
    let repo_dir = by_repo_dir().join(repo_id);
    match fs::read_dir(&repo_dir) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    }
}

/// Path to the by-repo sidecar directory.
fn by_repo_dir() -> PathBuf {
    global::global_aiki_dir()
        .join("sessions")
        .join("by-repo")
}

/// Register a session as active in a specific repo.
///
/// Creates an empty marker file at `~/.aiki/sessions/by-repo/<repo-id>/<session-uuid>`.
/// Idempotent — no-op if already registered.
pub fn register_session_in_repo(repo_id: &str, session_uuid: &str) {
    let sidecar = by_repo_dir().join(repo_id).join(session_uuid);
    if sidecar.exists() {
        return;
    }
    if let Some(parent) = sidecar.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&sidecar, "");
}

/// Unregister a session from a specific repo.
///
/// Removes `~/.aiki/sessions/by-repo/<repo-id>/<session-uuid>`.
/// Cleans up the repo-id directory if empty.
/// Idempotent — no-op if not registered.
pub fn unregister_session_from_repo(repo_id: &str, session_uuid: &str) {
    let repo_dir = by_repo_dir().join(repo_id);
    let sidecar = repo_dir.join(session_uuid);
    let _ = fs::remove_file(&sidecar);
    // Clean up empty repo directory
    if let Ok(entries) = fs::read_dir(&repo_dir) {
        if entries.count() == 0 {
            let _ = fs::remove_dir(&repo_dir);
        }
    }
}

/// Find which repo a session is currently registered in.
///
/// Scans `~/.aiki/sessions/by-repo/*/` for a file named `<session-uuid>`.
/// Returns the repo-id if found. Typically 1-2 directories to scan.
pub fn find_session_repo(session_uuid: &str) -> Option<String> {
    let base = by_repo_dir();
    let entries = fs::read_dir(&base).ok()?;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        if entry.path().join(session_uuid).exists() {
            return entry.file_name().to_str().map(String::from);
        }
    }
    None
}

/// Find the change ID for a named workspace by parsing `jj workspace list`.
///
/// Returns the short change ID of the workspace's working copy, or None if
/// the workspace is not found. This avoids using the `workspace_id()` revset
/// function which doesn't exist in JJ 0.38.
///
/// Output format: `workspace_name: <short_change_id> <commit_hash> ...`
pub fn find_workspace_change_id(repo_root: &Path, workspace_name: &str) -> Result<Option<String>> {
    let output = jj_cmd()
        .current_dir(repo_root)
        .args(["workspace", "list", "--ignore-working-copy"])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceAbsorbFailed(format!("Failed to list workspaces: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::WorkspaceAbsorbFailed(format!(
            "jj workspace list failed: {}",
            stderr.trim()
        )));
    }

    let list_str = String::from_utf8_lossy(&output.stdout);
    let prefix = format!("{}: ", workspace_name);

    let change_id = list_str
        .lines()
        .find(|line| line.starts_with(&prefix))
        .and_then(|line| {
            // After "workspace_name: ", first token is the short change ID
            line[prefix.len()..].trim().split_whitespace().next().map(String::from)
        });

    Ok(change_id)
}

/// Try to determine the repo root from a workspace directory.
///
/// Reads the JJ workspace config to find the original repo location.
fn find_repo_root_from_workspace(workspace_path: &Path) -> Option<PathBuf> {
    // JJ workspaces store their repo location in .jj/repo -> symlink or path
    let repo_link = workspace_path.join(".jj").join("repo");
    if let Ok(target) = fs::read_link(&repo_link) {
        // The repo link points to <original_repo>/.jj/repo
        // Walk up to find the actual repo root
        if let Some(jj_dir) = target.parent() {
            if let Some(repo_root) = jj_dir.parent() {
                return Some(repo_root.to_path_buf());
            }
        }
    }

    // Alternative: try reading .jj/working_copy/checkout — less reliable
    // Fall back to None
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify AIKI_HOME env var
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to run a test with a temporary AIKI_HOME value.
    /// Serializes access to prevent parallel test interference.
    fn with_aiki_home<F, R>(temp_dir: &std::path::Path, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var("AIKI_HOME").ok();
        std::env::set_var("AIKI_HOME", temp_dir);
        let result = f();
        match original {
            Some(v) => std::env::set_var("AIKI_HOME", v),
            None => std::env::remove_var("AIKI_HOME"),
        }
        result
    }

    #[test]
    fn test_find_jj_root_with_jj_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let jj_dir = temp_dir.path().join(".jj");
        fs::create_dir(&jj_dir).unwrap();

        let nested = temp_dir.path().join("src").join("nested");
        fs::create_dir_all(&nested).unwrap();

        let result = find_jj_root(&nested);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().canonicalize().unwrap(),
            temp_dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_jj_root_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = find_jj_root(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_count_sessions_in_repo_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        with_aiki_home(temp_dir.path(), || {
            let count = count_sessions_in_repo("test-repo-id");
            assert_eq!(count, 0);
        });
    }

    #[test]
    fn test_count_sessions_in_repo_with_sessions() {
        let temp_dir = tempfile::tempdir().unwrap();
        with_aiki_home(temp_dir.path(), || {
            register_session_in_repo("test-repo-id", "session-1");
            register_session_in_repo("test-repo-id", "session-2");
            register_session_in_repo("other-repo", "session-3");

            assert_eq!(count_sessions_in_repo("test-repo-id"), 2);
            assert_eq!(count_sessions_in_repo("other-repo"), 1);
        });
    }

    #[test]
    fn test_register_session_in_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        with_aiki_home(temp_dir.path(), || {
            register_session_in_repo("repo-1", "session-abc");

            let sidecar = temp_dir.path().join("sessions/by-repo/repo-1/session-abc");
            assert!(sidecar.exists());

            // Idempotent — second call is a no-op
            register_session_in_repo("repo-1", "session-abc");
            assert!(sidecar.exists());
        });
    }

    #[test]
    fn test_unregister_session_from_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        with_aiki_home(temp_dir.path(), || {
            register_session_in_repo("repo-1", "session-abc");
            let sidecar = temp_dir.path().join("sessions/by-repo/repo-1/session-abc");
            assert!(sidecar.exists());

            unregister_session_from_repo("repo-1", "session-abc");
            assert!(!sidecar.exists());
            // Empty repo dir should be cleaned up
            assert!(!temp_dir.path().join("sessions/by-repo/repo-1").exists());

            // Idempotent — second call is a no-op
            unregister_session_from_repo("repo-1", "session-abc");
        });
    }

    #[test]
    fn test_find_session_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        with_aiki_home(temp_dir.path(), || {
            // No sidecars → None
            assert_eq!(find_session_repo("session-abc"), None);

            // Register and find
            register_session_in_repo("repo-1", "session-abc");
            assert_eq!(find_session_repo("session-abc"), Some("repo-1".to_string()));

            // Different session → None
            assert_eq!(find_session_repo("session-xyz"), None);

            // Unregister → None
            unregister_session_from_repo("repo-1", "session-abc");
            assert_eq!(find_session_repo("session-abc"), None);
        });
    }
}

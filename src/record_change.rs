use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::{self, Read};
use std::thread;

use crate::provenance::{
    AgentInfo, AgentType, AttributionConfidence, DetectionMethod, ProvenanceRecord,
};

/// Input data structure from Claude Code hook
#[derive(Deserialize, Debug)]
struct HookInput {
    session_id: String,
    #[allow(dead_code)]
    transcript_path: Option<String>,
    cwd: String,
    #[allow(dead_code)]
    hook_event_name: String,
    tool_name: String,
    tool_input: ToolInput,
    #[allow(dead_code)]
    tool_output: Option<String>,
}

/// Tool input data from Claude Code
#[derive(Deserialize, Debug)]
struct ToolInput {
    file_path: String,
}

/// Handle the record-change command (called by AI editor hooks)
pub fn record_change(agent_type: AgentType) -> Result<()> {
    eprintln!("=== AIKI HOOK CALLED ===");
    eprintln!("Agent type: {:?}", agent_type);

    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("Failed to read JSON from stdin")?;

    eprintln!("Hook input JSON length: {} bytes", buffer.len());

    // Parse hook data
    let hook_data: HookInput =
        serde_json::from_str(&buffer).context("Failed to parse hook JSON")?;

    eprintln!("Hook data parsed successfully");
    eprintln!("  Session ID: {}", hook_data.session_id);
    eprintln!("  CWD: {}", hook_data.cwd);
    eprintln!("  Tool: {}", hook_data.tool_name);
    eprintln!("  File: {}", hook_data.tool_input.file_path);

    // Get change_id from the working copy
    eprintln!("Getting working copy change ID...");
    let change_id = get_working_copy_change_id(&hook_data.cwd)
        .context("Failed to get working copy change ID. Is the repository initialized with JJ?")?;
    eprintln!("  Change ID: {}", change_id);

    // Build provenance record (only metadata, jj knows the rest)
    eprintln!("Building provenance record...");
    let provenance = ProvenanceRecord {
        agent: AgentInfo {
            agent_type,
            version: None,
            detected_at: chrono::Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        session_id: hook_data.session_id.clone(),
        tool_name: hook_data.tool_name.clone(),
    };

    eprintln!("Provenance record built");
    eprintln!("  Agent: {:?}", provenance.agent.agent_type);
    eprintln!("  Session: {}", provenance.session_id);
    eprintln!("  Tool: {}", provenance.tool_name);

    // Spawn background thread to set the change description
    // This keeps the hook response time under 10ms
    let repo_path = hook_data.cwd.clone();
    let change_id_for_thread = change_id.clone();
    let provenance_for_thread = provenance.clone();

    eprintln!("Spawning background thread to set change description...");
    thread::spawn(move || {
        eprintln!("Background thread: Setting change description...");
        // Set the change description with full provenance metadata
        if let Err(e) =
            set_change_description(&repo_path, &change_id_for_thread, &provenance_for_thread)
        {
            eprintln!(
                "Warning: Failed to set change description in background: {}",
                e
            );
        } else {
            eprintln!("Background thread: Successfully set change description");
        }
    });

    eprintln!("=== AIKI HOOK COMPLETED SUCCESSFULLY ===");
    // Return immediately - the hook is now fast!
    Ok(())
}

/// Get the working copy change ID from JJ using jj-lib
///
/// The change_id is a stable identifier that persists across rewrites,
/// unlike commit_id which changes every time the commit content changes.
///
/// NOTE: This reads the current working copy change without snapshotting.
/// In the typical Claude Code workflow, files are already tracked by git/jj,
/// so the working copy should reflect recent changes.
fn get_working_copy_change_id(repo_path: &str) -> Result<String> {
    use jj_lib::object_id::ObjectId;
    use jj_lib::repo::{Repo, StoreFactories};
    use jj_lib::workspace::{default_working_copy_factories, Workspace};
    use std::path::Path;

    // Create settings
    let settings = crate::jj::JJWorkspace::create_user_settings()?;

    // Get default factories
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    // Load workspace
    let workspace = Workspace::load(
        &settings,
        Path::new(repo_path),
        &store_factories,
        &working_copy_factories,
    )
    .context("Failed to load JJ workspace. Is the repository initialized with JJ?")?;

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head operation")?;

    // Get working copy commit
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_id)
        .context("No working copy commit found for workspace")?;

    // Load the commit to get its change_id
    let commit = repo
        .store()
        .get_commit(wc_commit_id)
        .context("Failed to load working copy commit")?;

    // Return the change_id (stable identifier)
    Ok(commit.change_id().hex())
}

/// Set the description on a change to embed provenance metadata
///
/// This embeds the full provenance metadata in the commit description using
/// the [aiki]...[/aiki] format. Since change_id is stable, this metadata
/// persists even as the commit is rewritten.
fn set_change_description(
    repo_path: &str,
    change_id_str: &str,
    provenance: &ProvenanceRecord,
) -> Result<()> {
    use jj_lib::backend::ChangeId;
    use jj_lib::object_id::ObjectId;
    use jj_lib::repo::{Repo, StoreFactories};
    use jj_lib::workspace::{default_working_copy_factories, Workspace};
    use std::path::Path;

    // Create settings
    let settings = crate::jj::JJWorkspace::create_user_settings()?;

    // Get default factories
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    // Load workspace
    let workspace = Workspace::load(
        &settings,
        Path::new(repo_path),
        &store_factories,
        &working_copy_factories,
    )
    .context("Failed to load JJ workspace")?;

    // Parse change ID from hex string
    let change_id_bytes = hex::decode(change_id_str).context("Invalid change ID: not valid hex")?;
    let change_id = ChangeId::new(change_id_bytes);

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head")?;

    // Start transaction
    let mut tx = repo.start_transaction();

    // Find the commit with this change_id
    // Get the working copy commit (it should have this change_id)
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = tx
        .repo()
        .view()
        .get_wc_commit_id(workspace_id)
        .context("No working copy commit found")?;

    let commit = tx
        .repo()
        .store()
        .get_commit(wc_commit_id)
        .context("Failed to load working copy commit")?;

    // Verify this is the right change
    if commit.change_id() != &change_id {
        anyhow::bail!(
            "Change ID mismatch: expected {}, got {}",
            change_id_str,
            commit.change_id().hex()
        );
    }

    // Rewrite commit with provenance metadata in description
    let description = provenance.to_description();
    eprintln!("Setting description:\n{}", description);

    let new_commit = tx
        .repo_mut()
        .rewrite_commit(&commit)
        .set_description(description)
        .write()
        .context("Failed to write rewritten commit")?;

    // Rebase descendants if any
    let num_rebased = tx.repo_mut().rebase_descendants()?;
    if num_rebased > 0 {
        eprintln!("Rebased {} descendant commits", num_rebased);
    }

    // Update working copy pointer
    tx.repo_mut()
        .set_wc_commit(workspace_id.into(), new_commit.id().clone())?;

    // Commit transaction
    tx.commit("aiki: embed provenance metadata")
        .context("Failed to commit transaction")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Helper to check if jj is available for testing
    fn jj_available() -> bool {
        std::process::Command::new("jj")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Helper to initialize a JJ workspace for testing
    fn init_jj_workspace(path: &std::path::Path) -> Result<()> {
        let output = std::process::Command::new("jj")
            .arg("git")
            .arg("init")
            .arg("--colocate")
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to initialize JJ workspace");
        }

        Ok(())
    }

    #[test]
    fn test_get_working_copy_change_id_not_initialized() {
        // Test error when JJ is not initialized
        let temp_dir = tempdir().unwrap();

        let result = get_working_copy_change_id(temp_dir.path().to_str().unwrap());

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to load JJ workspace") || err_msg.contains("not initialized"),
            "Expected error about uninitialized workspace, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_get_working_copy_change_id_invalid_path() {
        // Test error with non-existent path
        let result = get_working_copy_change_id("/nonexistent/path/to/repo");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to load JJ workspace"),
            "Expected workspace load error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_get_working_copy_change_id_success() {
        if !jj_available() {
            eprintln!("Skipping test: jj binary not found in PATH");
            return;
        }

        let temp_dir = tempdir().unwrap();

        // Initialize JJ workspace
        if let Err(e) = init_jj_workspace(temp_dir.path()) {
            eprintln!("Skipping test: Failed to initialize JJ workspace: {}", e);
            return;
        }

        // Should successfully get change ID
        let result = get_working_copy_change_id(temp_dir.path().to_str().unwrap());

        assert!(
            result.is_ok(),
            "Expected success, got error: {:?}",
            result.err()
        );
        let change_id = result.unwrap();

        // Change ID should be a hex string
        assert!(!change_id.is_empty());
        assert!(change_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_set_change_description_invalid_change_id() {
        if !jj_available() {
            eprintln!("Skipping test: jj binary not found in PATH");
            return;
        }

        let temp_dir = tempdir().unwrap();

        // Initialize JJ workspace
        if let Err(e) = init_jj_workspace(temp_dir.path()) {
            eprintln!("Skipping test: Failed to initialize JJ workspace: {}", e);
            return;
        }

        let provenance = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: chrono::Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
        };

        // Test with invalid hex string
        let result = set_change_description(
            temp_dir.path().to_str().unwrap(),
            "not-valid-hex",
            &provenance,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid change ID") || err_msg.contains("not valid hex"),
            "Expected invalid change ID error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_set_change_description_wrong_change_id() {
        if !jj_available() {
            eprintln!("Skipping test: jj binary not found in PATH");
            return;
        }

        let temp_dir = tempdir().unwrap();

        // Initialize JJ workspace
        if let Err(e) = init_jj_workspace(temp_dir.path()) {
            eprintln!("Skipping test: Failed to initialize JJ workspace: {}", e);
            return;
        }

        let provenance = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: chrono::Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
        };

        // Use a valid hex string but wrong change ID (all zeros)
        let fake_change_id = "0".repeat(64);

        let result = set_change_description(
            temp_dir.path().to_str().unwrap(),
            &fake_change_id,
            &provenance,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Change ID mismatch"),
            "Expected change ID mismatch error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_set_change_description_success() {
        if !jj_available() {
            eprintln!("Skipping test: jj binary not found in PATH");
            return;
        }

        let temp_dir = tempdir().unwrap();

        // Initialize JJ workspace
        if let Err(e) = init_jj_workspace(temp_dir.path()) {
            eprintln!("Skipping test: Failed to initialize JJ workspace: {}", e);
            return;
        }

        // Get the actual change ID
        let change_id = match get_working_copy_change_id(temp_dir.path().to_str().unwrap()) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Skipping test: Failed to get change ID: {}", e);
                return;
            }
        };

        let provenance = ProvenanceRecord {
            agent: AgentInfo {
                agent_type: AgentType::ClaudeCode,
                version: None,
                detected_at: chrono::Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: "test-session-success".to_string(),
            tool_name: "Write".to_string(),
        };

        // Should successfully set description
        let result =
            set_change_description(temp_dir.path().to_str().unwrap(), &change_id, &provenance);

        assert!(
            result.is_ok(),
            "Expected success, got error: {:?}",
            result.err()
        );

        // Verify the description was actually set
        let output = std::process::Command::new("jj")
            .arg("log")
            .arg("-r")
            .arg("@")
            .arg("-T")
            .arg("description")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let description = String::from_utf8_lossy(&output.stdout);
        assert!(description.contains("[aiki]"));
        assert!(description.contains("agent=claude-code"));
        assert!(description.contains("session=test-session-success"));
        assert!(description.contains("tool=Write"));
    }

    #[test]
    fn test_hook_input_parsing_valid() {
        let json = r#"{
            "session_id": "test-123",
            "transcript_path": "/path/to/transcript",
            "cwd": "/tmp/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/tmp/repo/file.txt"
            },
            "tool_output": "Success"
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let hook_input = result.unwrap();
        assert_eq!(hook_input.session_id, "test-123");
        assert_eq!(hook_input.cwd, "/tmp/repo");
        assert_eq!(hook_input.tool_name, "Edit");
        assert_eq!(hook_input.tool_input.file_path, "/tmp/repo/file.txt");
    }

    #[test]
    fn test_hook_input_parsing_missing_required_field() {
        // Missing session_id
        let json = r#"{
            "cwd": "/tmp/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/tmp/repo/file.txt"
            }
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_input_parsing_optional_fields() {
        // transcript_path and tool_output are optional
        let json = r#"{
            "session_id": "test-456",
            "cwd": "/tmp/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/repo/new_file.txt"
            }
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let hook_input = result.unwrap();
        assert_eq!(hook_input.session_id, "test-456");
        assert!(hook_input.transcript_path.is_none());
        assert!(hook_input.tool_output.is_none());
    }
}

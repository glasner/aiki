use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

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
///
/// This command runs synchronously and executes:
/// 1. Parse JSON input from stdin
/// 2. Build provenance metadata
/// 3. Run `jj describe -m` to add metadata to current change
/// 4. Run `jj new` to create a new change for the next edit
pub fn record_change(agent_type: AgentType, _sync: bool) -> Result<()> {
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

    // Convert provenance to description format
    let description = provenance.to_description();
    eprintln!("Setting description:\n{}", description);

    // Run jj describe to add metadata to current change
    run_jj_describe(Path::new(&hook_data.cwd), &description)?;

    // Run jj new to create a new change for the next edit
    run_jj_new(Path::new(&hook_data.cwd))?;

    eprintln!("=== AIKI HOOK COMPLETED SUCCESSFULLY ===");
    Ok(())
}

/// Run `jj describe -m` to set the change description
///
/// Logs a warning if the command fails, but doesn't fail the hook.
fn run_jj_describe(cwd: &Path, description: &str) -> Result<()> {
    eprintln!("Running: jj describe -m [metadata]");

    let output = Command::new("jj")
        .args(["describe", "-m", description])
        .current_dir(cwd)
        .output()
        .context("Failed to execute jj describe command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: jj describe failed: {}", stderr);
        eprintln!("  Status: {}", output.status);
    } else {
        eprintln!("Successfully set change description");
    }

    Ok(())
}

/// Run `jj new` to create a new change
///
/// Logs a warning if the command fails, but doesn't fail the hook.
fn run_jj_new(cwd: &Path) -> Result<()> {
    eprintln!("Running: jj new");

    let output = Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .output()
        .context("Failed to execute jj new command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: jj new failed: {}", stderr);
        eprintln!("  Status: {}", output.status);
    } else {
        eprintln!("Successfully created new change");
    }

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
    fn test_run_jj_describe_success() {
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

        let description = "[aiki]\nagent=claude-code\nsession=test\ntool=Edit\n[/aiki]";

        // Should succeed
        let result = run_jj_describe(temp_dir.path(), description);
        assert!(
            result.is_ok(),
            "Expected success, got error: {:?}",
            result.err()
        );

        // Verify description was set
        let output = std::process::Command::new("jj")
            .arg("log")
            .arg("-r")
            .arg("@")
            .arg("-T")
            .arg("description")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let desc = String::from_utf8_lossy(&output.stdout);
        assert!(desc.contains("[aiki]"));
        assert!(desc.contains("agent=claude-code"));
    }

    #[test]
    fn test_run_jj_new_success() {
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

        // Get initial change count
        let output_before = std::process::Command::new("jj")
            .arg("log")
            .arg("-T")
            .arg("change_id")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        let changes_before = String::from_utf8_lossy(&output_before.stdout);
        let count_before = changes_before.lines().count();

        // Should succeed
        let result = run_jj_new(temp_dir.path());
        assert!(
            result.is_ok(),
            "Expected success, got error: {:?}",
            result.err()
        );

        // Verify new change was created
        let output_after = std::process::Command::new("jj")
            .arg("log")
            .arg("-T")
            .arg("change_id")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();
        let changes_after = String::from_utf8_lossy(&output_after.stdout);
        let count_after = changes_after.lines().count();

        assert_eq!(count_after, count_before + 1, "Expected one new change");
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

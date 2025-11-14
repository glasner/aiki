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
    tool_name: String,
}

/// Record a change with provenance metadata
///
/// This is the core provenance recording function, called from event handlers.
/// It executes:
/// 1. Build provenance metadata from parameters
/// 2. Run `jj describe -m` to add metadata to current change
/// 3. Run `jj new` to create a new change for the next edit
///
/// # Arguments
/// * `agent_type` - The AI agent that made the change
/// * `session_id` - Session identifier for grouping related changes
/// * `tool_name` - Name of the tool that made the change (e.g., "Edit", "Write")
pub fn record_change(agent_type: AgentType, session_id: &str, tool_name: &str) -> Result<()> {
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("=== RECORDING CHANGE ===");
        eprintln!("Agent type: {:?}", agent_type);
        eprintln!("Session ID: {}", session_id);
        eprintln!("Tool: {}", tool_name);
    }

    // Get current working directory
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Build provenance record (only metadata, jj knows the rest)
    let provenance = ProvenanceRecord {
        agent: AgentInfo {
            agent_type,
            version: None,
            detected_at: chrono::Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        session_id: session_id.to_string(),
        tool_name: tool_name.to_string(),
    };

    // Convert provenance to description format
    let description = provenance.to_description();

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("Description:\n{}", description);
    }

    // Run jj describe to add metadata to current change
    run_jj_describe(&cwd, &description)?;

    // Run jj new to create a new change for the next edit
    run_jj_new(&cwd)?;

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("=== CHANGE RECORDED SUCCESSFULLY ===");
    }

    Ok(())
}

/// Legacy function for backward compatibility with old record-change command
///
/// This reads JSON from stdin and calls the new record_change function.
/// Used only by the deprecated `aiki record-change` command.
#[allow(dead_code)]
pub fn record_change_legacy(agent_type: AgentType, _sync: bool) -> Result<()> {
    eprintln!("=== AIKI HOOK CALLED (LEGACY) ===");
    eprintln!("Agent type: {:?}", agent_type);

    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("Failed to read JSON from stdin")?;

    // Parse hook data
    let hook_data: HookInput =
        serde_json::from_str(&buffer).context("Failed to parse hook JSON")?;

    eprintln!("Hook data parsed successfully");
    eprintln!("  Session ID: {}", hook_data.session_id);
    eprintln!("  Tool: {}", hook_data.tool_name);

    // Call new record_change function
    record_change(agent_type, &hook_data.session_id, &hook_data.tool_name)?;

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
            "tool_name": "Edit"
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let hook_input = result.unwrap();
        assert_eq!(hook_input.session_id, "test-123");
        assert_eq!(hook_input.tool_name, "Edit");
    }

    #[test]
    fn test_hook_input_parsing_missing_required_field() {
        // Missing session_id
        let json = r#"{
            "tool_name": "Edit"
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_input_parsing_optional_fields() {
        // Extra fields should be ignored by serde
        let json = r#"{
            "session_id": "test-456",
            "tool_name": "Write",
            "extra_field": "ignored",
            "another_extra": 123
        }"#;

        let result: Result<HookInput, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let hook_input = result.unwrap();
        assert_eq!(hook_input.session_id, "test-456");
        assert_eq!(hook_input.tool_name, "Write");
    }
}

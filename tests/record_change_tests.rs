use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

// Helper to check if jj is available
fn jj_available() -> bool {
    std::process::Command::new("jj")
        .arg("--version")
        .output()
        .is_ok()
}

// Helper to initialize a JJ workspace
fn init_jj_workspace(path: &std::path::Path) -> anyhow::Result<()> {
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
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_record_change_with_valid_json() {
    // Skip if jj not available
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

    // Create a test file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "original content").unwrap();

    // Create mock hook input
    let hook_input = serde_json::json!({
        "session_id": "test-session-123",
        "transcript_path": "/path/to/transcript.json",
        "cwd": temp_dir.path().to_string_lossy(),
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": test_file.to_string_lossy(),
            "old_string": "original",
            "new_string": "modified"
        },
        "tool_output": "Successfully edited file"
    });

    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.arg("record-change")
        .arg("--claude-code")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Give background thread time to complete
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify commit description contains aiki metadata
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
    assert!(
        description.contains("[aiki]"),
        "Description should contain [aiki] marker"
    );
    assert!(
        description.contains("agent=claude-code"),
        "Description should contain agent=claude-code"
    );
    assert!(
        description.contains("session=test-session-123"),
        "Description should contain session ID"
    );
    assert!(
        description.contains("tool=Edit"),
        "Description should contain tool=Edit"
    );
}

#[test]
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_record_change_fails_with_invalid_json() {
    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.arg("record-change")
        .arg("--claude-code")
        .write_stdin("not valid json")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse"));
}

#[test]
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_record_change_requires_agent_flag() {
    let hook_input = serde_json::json!({
        "session_id": "test",
        "cwd": "/tmp",
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/tmp/test.txt",
            "old_string": "old",
            "new_string": "new"
        }
    });

    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.arg("record-change")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Agent type flag required"));
}

#[test]
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_record_change_handles_write_tool() {
    // Skip if jj not available
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

    let test_file = temp_dir.path().join("new_file.txt");

    // Create mock hook input for Write tool
    let hook_input = serde_json::json!({
        "session_id": "test-session-456",
        "cwd": temp_dir.path().to_string_lossy(),
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "tool_input": {
            "file_path": test_file.to_string_lossy(),
            "new_string": "new content"
        },
        "tool_output": "Successfully wrote file"
    });

    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.arg("record-change")
        .arg("--claude-code")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Give background thread time to complete
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify commit description contains aiki metadata
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
    assert!(
        description.contains("[aiki]"),
        "Description should contain [aiki] marker"
    );
    assert!(
        description.contains("tool=Write"),
        "Description should contain tool=Write"
    );
}

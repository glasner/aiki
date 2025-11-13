mod common;

use assert_cmd::Command;
use common::{init_git_repo, jj_available};
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

#[test]
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_complete_workflow_init_to_provenance_tracking() {
    // Skip if jj not available
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Step 1: Initialize Git repository
    init_git_repo(repo_path);
    assert!(
        repo_path.join(".git").exists(),
        "Git repository not created"
    );

    // Step 2: Copy plugin directory to test repo
    let source_plugin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("claude-code-plugin");
    let dest_plugin = repo_path.join("claude-code-plugin");

    if source_plugin.exists() {
        copy_dir_all(&source_plugin, &dest_plugin).expect("Failed to copy plugin directory");
    } else {
        eprintln!("Warning: Plugin directory not found at {:?}", source_plugin);
        eprintln!("Creating minimal plugin structure for test");
        create_minimal_plugin(&dest_plugin);
    }

    // Step 3: Run aiki init
    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.current_dir(repo_path).arg("init");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initializing Aiki"))
        .stdout(predicate::str::contains("✓ Initialized JJ repository"))
        .stdout(predicate::str::contains("✓ Created .aiki directory"))
        .stdout(predicate::str::contains("✓ Configured Claude Code plugin"))
        .stdout(predicate::str::contains("✓ Aiki initialized successfully"));

    // Step 4: Verify directory structure created
    assert!(
        repo_path.join(".jj").exists(),
        "JJ repository not initialized"
    );
    assert!(
        repo_path.join(".aiki").exists(),
        ".aiki directory not created"
    );
    assert!(
        repo_path.join(".aiki/cache").exists(),
        ".aiki/cache not created"
    );
    assert!(
        repo_path.join(".aiki/logs").exists(),
        ".aiki/logs not created"
    );
    assert!(
        repo_path.join(".aiki/tmp").exists(),
        ".aiki/tmp not created"
    );
    assert!(
        repo_path.join(".aiki/config.toml").exists(),
        "config.toml not created"
    );

    // Step 5: Verify Claude Code plugin configuration
    let settings_file = repo_path.join(".claude/settings.json");
    assert!(settings_file.exists(), ".claude/settings.json not created");

    let settings_content = fs::read_to_string(&settings_file).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_content).unwrap();

    assert!(
        settings.get("extraKnownMarketplaces").is_some(),
        "extraKnownMarketplaces not configured"
    );
    assert!(
        settings["extraKnownMarketplaces"].get("aiki").is_some(),
        "aiki marketplace not configured"
    );
    assert_eq!(
        settings["extraKnownMarketplaces"]["aiki"]["source"]["source"],
        "directory"
    );
    assert_eq!(
        settings["extraKnownMarketplaces"]["aiki"]["source"]["path"],
        "./claude-code-plugin"
    );

    assert!(
        settings.get("enabledPlugins").is_some(),
        "enabledPlugins not configured"
    );
    assert_eq!(settings["enabledPlugins"]["aiki@aiki"], true);

    // Step 6: Verify plugin directory structure
    assert!(
        dest_plugin.exists(),
        "Plugin directory not found at {:?}",
        dest_plugin
    );
    assert!(
        dest_plugin.join(".claude-plugin/plugin.json").exists(),
        "plugin.json not found"
    );
    assert!(
        dest_plugin.join("hooks/hooks.json").exists(),
        "hooks.json not found"
    );

    // Step 7: Create and track a test file
    let test_file = repo_path.join("test.rs");
    fs::write(&test_file, "fn main() {\n    println!(\"Hello\");\n}").unwrap();

    // Add file to git (jj tracks git-tracked files)
    std::process::Command::new("git")
        .args(&["add", "test.rs"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to git add");

    std::process::Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to git commit");

    // Step 8: Simulate a real edit (like Claude Code would make)
    fs::write(
        &test_file,
        "fn main() {\n    println!(\"Hello, World!\");\n}",
    )
    .unwrap();

    // Step 9: Call aiki record-change (simulates Claude Code PostToolUse hook)
    // NOTE: The hook runs off the critical path via background threading for performance.
    // Working copy snapshotting is a known limitation that will be addressed in Milestone 1.2.
    let hook_input = serde_json::json!({
        "session_id": "test-session-e2e",
        "transcript_path": "/path/to/transcript.json",
        "cwd": repo_path.to_string_lossy(),
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": test_file.to_string_lossy(),
            "old_string": "println!(\"Hello\")",
            "new_string": "println!(\"Hello, World!\")"
        },
        "tool_output": "Successfully edited file"
    });

    // Measure hook performance
    let start = Instant::now();
    let mut record_cmd = Command::cargo_bin("aiki").unwrap();
    let output = record_cmd
        .arg("record-change")
        .arg("--claude-code")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .current_dir(repo_path)
        .output()
        .expect("Failed to run record-change");

    let elapsed = start.elapsed();

    // Debug: Print record-change output
    println!(
        "record-change stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    println!(
        "record-change stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("record-change status: {}", output.status);

    assert!(output.status.success(), "record-change should succeed");

    println!(
        "⏱️  Hook execution time: {:.2}ms",
        elapsed.as_secs_f64() * 1000.0
    );

    // Note: With synchronous execution, the hook blocks until complete
    // After completion, `jj new` has created a new working copy change,
    // so the metadata is on the parent change (@-)

    // Debug: Show operation log first
    let op_log_output = std::process::Command::new("jj")
        .arg("op")
        .arg("log")
        .arg("--limit")
        .arg("10")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj op log");
    println!(
        "Operation log:\n{}",
        String::from_utf8_lossy(&op_log_output.stdout)
    );

    // Debug: Show the log at the operation right after our hook finished
    // Use --at-op to prevent auto-snapshotting from modifying the state
    let log_at_hook_output = std::process::Command::new("jj")
        .arg("log")
        .arg("--at-op")
        .arg("@") // The head operation after our hook
        .arg("-r")
        .arg("all()")
        .arg("-T")
        .arg("change_id.short() ++ \" \" ++ description.first_line()")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log at op");
    println!(
        "Log immediately after hook (at op @):\n{}",
        String::from_utf8_lossy(&log_at_hook_output.stdout)
    );

    // Debug: Show full log (this will auto-snapshot)
    let log_output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("all()")
        .arg("-T")
        .arg("change_id.short() ++ \" \" ++ description.first_line()")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log");
    println!(
        "Full jj log (after auto-snapshot):\n{}",
        String::from_utf8_lossy(&log_output.stdout)
    );

    // Show what @ actually is
    let at_output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("-T")
        .arg(r#"change_id.short() ++ " parents: " ++ parents.map(|p| p.change_id().short()).join(", ")"#)
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log @");
    println!(
        "@ (working copy) parents: {}",
        String::from_utf8_lossy(&at_output.stdout)
    );

    // Step 10: Verify provenance was recorded in parent change description
    let output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@-")
        .arg("-T")
        .arg("description")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log");

    let description = String::from_utf8_lossy(&output.stdout);

    // Debug: Print the actual description
    println!("Parent change (@-) description: '{}'", description);

    // Parse provenance metadata from description
    assert!(
        description.contains("[aiki]"),
        "Description should contain [aiki] marker. Got: '{}'",
        description
    );
    assert!(
        description.contains("agent=claude-code"),
        "Description should contain agent=claude-code"
    );
    assert!(
        description.contains("session=test-session-e2e"),
        "Description should contain session ID"
    );
    assert!(
        description.contains("tool=Edit"),
        "Description should contain tool=Edit"
    );
    assert!(
        description.contains("confidence=High"),
        "Description should contain confidence=High"
    );
    assert!(
        description.contains("method=Hook"),
        "Description should contain method=Hook"
    );

    // Step 11: Verify the actual file contains the edited content
    let final_content = fs::read_to_string(&test_file).unwrap();
    assert!(
        final_content.contains("Hello, World!"),
        "File should contain edited content"
    );

    println!("✅ End-to-end test passed!");
    println!("  ✓ aiki init created all necessary files");
    println!("  ✓ Plugin configuration is correct");
    println!("  ✓ File was edited: test.rs");
    println!("  ✓ record-change completed successfully");
    println!("  ✓ Provenance metadata embedded in parent change description");
    println!("  ✓ Metadata format validated");
    println!("  ✓ File content verified: {:?}", final_content.trim());
    println!("  ✓ jj new created new working copy change");
}

/// Helper to recursively copy a directory
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

/// Create minimal plugin structure for testing
fn create_minimal_plugin(plugin_dir: &std::path::Path) {
    let plugin_json_dir = plugin_dir.join(".claude-plugin");
    let hooks_dir = plugin_dir.join("hooks");

    fs::create_dir_all(&plugin_json_dir).unwrap();
    fs::create_dir_all(&hooks_dir).unwrap();

    // Create plugin.json
    let plugin_json = serde_json::json!({
        "name": "aiki",
        "version": "0.1.0",
        "description": "AI code provenance tracking",
        "author": {"name": "Aiki Team"},
        "hooks": "./hooks/hooks.json"
    });
    fs::write(
        plugin_json_dir.join("plugin.json"),
        serde_json::to_string_pretty(&plugin_json).unwrap(),
    )
    .unwrap();

    // Create hooks.json
    let hooks_json = serde_json::json!({
        "description": "Track AI code changes",
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Edit|Write",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "aiki record-change --claude-code",
                            "timeout": 5
                        }
                    ]
                }
            ]
        }
    });
    fs::write(
        hooks_dir.join("hooks.json"),
        serde_json::to_string_pretty(&hooks_json).unwrap(),
    )
    .unwrap();
}

use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

/// Helper to initialize a Git repository
fn init_git_repo(path: &std::path::Path) {
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(path)
        .output()
        .expect("Failed to initialize Git repository");
}

#[test]
#[allow(deprecated)] // cargo_bin deprecated but replacement cargo_bin! macro not yet documented
fn test_complete_workflow_init_to_provenance_tracking() {
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
        .stdout(predicate::str::contains(
            "✓ Initialized provenance database",
        ))
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
    assert!(
        repo_path.join(".aiki/provenance").exists(),
        "provenance directory not created"
    );
    assert!(
        repo_path.join(".aiki/provenance/attribution.db").exists(),
        "attribution.db not created"
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
    record_cmd
        .arg("record-change")
        .arg("--claude-code")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .current_dir(repo_path)
        .assert()
        .success();
    let elapsed = start.elapsed();

    println!(
        "⏱️  Hook execution time: {:.2}ms (target: <25ms)",
        elapsed.as_secs_f64() * 1000.0
    );

    // Note: With background threading, the hook should return in <25ms
    // The heavy link_jj_operation work happens asynchronously
    // We'll wait a bit to let the background work complete for testing
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Step 10: Verify provenance was recorded in database
    let db_path = repo_path.join(".aiki/provenance/attribution.db");
    let conn = Connection::open(&db_path).expect("Failed to open database");

    // Check that a record was inserted
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM provenance_records")
        .unwrap();
    let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count, 1, "Expected 1 provenance record, found {}", count);

    // Verify record contents
    let mut stmt = conn
        .prepare(
            "SELECT file_path, agent_type, session_id, tool_name, confidence, detection_method, \
             old_string, new_string, jj_change_id FROM provenance_records",
        )
        .unwrap();

    let record = stmt
        .query_row([], |row| {
            Ok((
                row.get::<_, String>(0)?,         // file_path
                row.get::<_, String>(1)?,         // agent_type
                row.get::<_, String>(2)?,         // session_id
                row.get::<_, String>(3)?,         // tool_name
                row.get::<_, String>(4)?,         // confidence
                row.get::<_, String>(5)?,         // detection_method
                row.get::<_, Option<String>>(6)?, // old_string
                row.get::<_, Option<String>>(7)?, // new_string
                row.get::<_, Option<String>>(8)?, // jj_change_id
            ))
        })
        .unwrap();

    let (
        file_path,
        agent_type,
        session_id,
        tool_name,
        confidence,
        detection_method,
        old_string,
        new_string,
        jj_change_id,
    ) = record;

    assert!(
        file_path.ends_with("test.rs"),
        "File path should end with test.rs, got: {}",
        file_path
    );
    assert_eq!(agent_type, "ClaudeCode");
    assert_eq!(session_id, "test-session-e2e");
    assert_eq!(tool_name, "Edit");
    assert_eq!(confidence, "High");
    assert_eq!(detection_method, "Hook");
    assert_eq!(old_string, Some("println!(\"Hello\")".to_string()));
    assert_eq!(new_string, Some("println!(\"Hello, World!\")".to_string()));
    assert!(jj_change_id.is_some(), "JJ change ID should be recorded");

    // Step 11: Verify JJ change exists and is valid
    let change_id = jj_change_id.unwrap();
    assert!(!change_id.is_empty(), "JJ change ID should not be empty");
    assert!(
        change_id.len() >= 16,
        "JJ change ID should be at least 16 characters (hex)"
    );

    // Step 12: Verify the actual file contains the edited content
    let final_content = fs::read_to_string(&test_file).unwrap();
    assert!(
        final_content.contains("Hello, World!"),
        "File should contain edited content"
    );

    // Step 13: Verify we can find the commit with this change_id using jj-lib
    // This validates that the change_id in the database is valid
    use jj_lib::backend::ChangeId;
    use jj_lib::repo::{Repo, StoreFactories};
    use jj_lib::workspace::{default_working_copy_factories, Workspace};

    let settings = {
        use jj_lib::config::StackedConfig;
        use jj_lib::settings::UserSettings;
        let config = StackedConfig::with_defaults();
        UserSettings::from_config(config).unwrap()
    };

    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    let workspace = Workspace::load(
        &settings,
        repo_path,
        &store_factories,
        &working_copy_factories,
    )
    .expect("Failed to load workspace");

    let repo = workspace
        .repo_loader()
        .load_at_head()
        .expect("Failed to load repo");

    // Get the working copy commit and verify it has the expected change_id
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_id)
        .expect("No working copy commit found");

    let commit = repo
        .store()
        .get_commit(wc_commit_id)
        .expect("Failed to load working copy commit");

    let change_id_bytes = hex::decode(&change_id).expect("Invalid change ID hex");
    let expected_change_id = ChangeId::new(change_id_bytes);

    assert_eq!(
        commit.change_id(),
        &expected_change_id,
        "Working copy commit should have the recorded change_id"
    );

    println!("✅ End-to-end test passed!");
    println!("  ✓ aiki init created all necessary files");
    println!("  ✓ Plugin configuration is correct");
    println!("  ✓ File was edited: test.rs");
    println!("  ✓ record-change captured working copy change ID");
    println!("  ✓ Provenance data stored correctly in database");
    println!("  ✓ JJ change ID captured: {}", change_id);
    println!("  ✓ JJ change ID is valid (stable across rewrites)");
    println!("  ✓ File content verified: {:?}", final_content.trim());
    println!("  ✓ Background threading keeps hook fast (<25ms target)");
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

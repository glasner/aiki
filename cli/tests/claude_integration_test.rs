/// Integration test that uses REAL Claude Code to make edits
///
/// This test requires:
/// - Claude Code CLI installed (`npm install -g @anthropic-ai/claude-code`)
/// - Active Claude Pro/Max subscription
/// - Set env var: CLAUDE_INTEGRATION_TEST=1 to enable
///
/// Run with: CLAUDE_INTEGRATION_TEST=1 cargo test test_real_claude_code_integration -- --nocapture
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Helper to check if Claude Code CLI is available
fn claude_available() -> bool {
    Command::new("claude").arg("--version").output().is_ok()
}

/// Helper to check if integration tests are enabled
fn integration_tests_enabled() -> bool {
    std::env::var("CLAUDE_INTEGRATION_TEST").is_ok()
}

/// Helper to initialize a Git repository
fn init_git_repo(path: &std::path::Path) {
    Command::new("git")
        .args(&["init"])
        .current_dir(path)
        .output()
        .expect("Failed to initialize Git repository");

    // Configure git user for commits
    Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .expect("Failed to set git user.name");

    Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .expect("Failed to set git user.email");
}

#[test]
fn test_real_claude_code_integration() {
    // Skip if Claude Code CLI not available or integration tests not enabled
    if !integration_tests_enabled() {
        eprintln!("Skipping test: Set CLAUDE_INTEGRATION_TEST=1 to enable");
        return;
    }

    if !claude_available() {
        eprintln!("Skipping test: Claude Code CLI not installed");
        eprintln!("Install with: npm install -g @anthropic-ai/claude-code");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    println!("🧪 Starting real Claude Code integration test");
    println!("📁 Test directory: {}", repo_path.display());

    // Step 1: Initialize Git repository
    init_git_repo(repo_path);
    assert!(
        repo_path.join(".git").exists(),
        "Git repository not created"
    );
    println!("✓ Git repository initialized");

    // Step 2: Copy plugin directory
    let source_plugin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("claude-code-plugin");
    let dest_plugin = repo_path.join("claude-code-plugin");

    if source_plugin.exists() {
        copy_dir_all(&source_plugin, &dest_plugin).expect("Failed to copy plugin");
    } else {
        create_minimal_plugin(&dest_plugin);
    }
    println!("✓ Plugin directory copied");

    // Step 3: Run aiki init
    let init_output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run aiki init");

    assert!(init_output.status.success(), "aiki init failed");
    println!("✓ aiki init completed");

    // Step 3.5: Also configure hooks directly in .claude/hooks/ (not just via plugin)
    // This may work better in print mode
    let claude_hooks_dir = repo_path.join(".claude/hooks");
    fs::create_dir_all(&claude_hooks_dir).expect("Failed to create .claude/hooks");

    let aiki_binary = env!("CARGO_BIN_EXE_aiki");
    let workspace_hooks = serde_json::json!({
        "PostToolUse": [
            {
                "matcher": "Edit|Write",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{} record-change --claude-code", aiki_binary),
                        "timeout": 5
                    }
                ]
            }
        ]
    });

    fs::write(
        claude_hooks_dir.join("hooks.json"),
        serde_json::to_string_pretty(&workspace_hooks).unwrap(),
    )
    .expect("Failed to write workspace hooks.json");
    println!("✓ Configured workspace-level hooks in .claude/hooks/");

    // Step 4: Create an initial file for Claude to edit
    let test_file = repo_path.join("calculator.py");
    fs::write(
        &test_file,
        "# Calculator module\n\ndef add(a, b):\n    return a + b\n",
    )
    .unwrap();

    Command::new("git")
        .args(&["add", "calculator.py"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to git add");

    Command::new("git")
        .args(&["commit", "-m", "Initial calculator"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to git commit");

    println!("✓ Initial file created: calculator.py");

    // Step 5: Use Claude Code CLI to make a REAL edit
    println!("🤖 Invoking Claude Code to edit calculator.py...");

    let claude_output = Command::new("claude")
        .arg("-p") // Print mode (non-interactive)
        .arg("Add a subtract function to calculator.py that takes two numbers and returns their difference")
        .arg("--output-format")
        .arg("json")
        .arg("--dangerously-skip-permissions") // Auto-accept edits for testing
        .arg("--debug")
        .arg("hooks") // Enable hooks debugging
        .current_dir(repo_path)
        .output()
        .expect("Failed to run Claude Code");

    // Print Claude Code output for debugging
    let stdout = String::from_utf8_lossy(&claude_output.stdout);
    let stderr = String::from_utf8_lossy(&claude_output.stderr);

    println!("\n📋 Claude Code stdout:");
    println!("{}", stdout);

    println!("\n📋 Claude Code stderr (including debug output):");
    println!("{}", stderr);
    println!("--- End stderr ---");

    if !claude_output.status.success() {
        eprintln!("Claude Code failed:");
        panic!("Claude Code execution failed");
    }

    println!("✓ Claude Code executed successfully");

    // Step 6: Verify the file was actually edited by Claude
    let edited_content = fs::read_to_string(&test_file).unwrap();
    println!("📝 Edited file content:");
    println!("{}", edited_content);

    assert!(
        edited_content.contains("subtract"),
        "Claude should have added a subtract function"
    );
    println!("✓ File contains 'subtract' function");

    // Step 6.5: Debug - Check settings and plugin configuration
    let settings_file = repo_path.join(".claude/settings.json");
    if settings_file.exists() {
        let settings = fs::read_to_string(&settings_file).unwrap();
        println!("\n📋 Settings.json content:");
        println!("{}", settings);
    } else {
        println!("⚠️  No .claude/settings.json found!");
    }

    let hooks_json = repo_path.join("claude-code-plugin/hooks/hooks.json");
    if hooks_json.exists() {
        let hooks = fs::read_to_string(&hooks_json).unwrap();
        println!("\n📋 Hooks.json content:");
        println!("{}", hooks);
    } else {
        println!("⚠️  No hooks.json found!");
    }

    // Check what aiki binary path the hook will use
    let which_output = Command::new("which")
        .arg("aiki")
        .output()
        .expect("Failed to run which");
    println!(
        "\n📋 aiki binary path: {}",
        String::from_utf8_lossy(&which_output.stdout).trim()
    );

    // Step 7: Check that provenance was recorded
    let db_path = repo_path.join(".aiki/provenance/attribution.db");

    if !db_path.exists() {
        eprintln!("⚠️  Database not found. Hook may not have triggered.");
        eprintln!("This could mean:");
        eprintln!("  - Claude Code didn't use Edit/Write tool");
        eprintln!("  - Hook wasn't properly configured");
        eprintln!("  - Plugin wasn't loaded by Claude Code");

        panic!("Provenance database not created");
    }

    println!("✓ Provenance database exists");

    // Step 8: Query the database to verify provenance was recorded
    use rusqlite::Connection;
    let conn = Connection::open(&db_path).expect("Failed to open database");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM provenance_records", [], |row| {
            row.get(0)
        })
        .expect("Failed to query database");

    if count == 0 {
        eprintln!("⚠️  No provenance records found!");
        eprintln!("The hook may not have been triggered by Claude Code.");
        panic!("No provenance records in database");
    }

    println!("✓ Found {} provenance record(s)", count);

    // Query the actual record
    let mut stmt = conn
        .prepare("SELECT file_path, agent_type, tool_name, confidence FROM provenance_records")
        .unwrap();

    let records: Vec<(String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?, // file_path
                row.get(1)?, // agent_type
                row.get(2)?, // tool_name
                row.get(3)?, // confidence
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    for (file_path, agent_type, tool_name, confidence) in &records {
        println!("📊 Provenance record:");
        println!("   File: {}", file_path);
        println!("   Agent: {}", agent_type);
        println!("   Tool: {}", tool_name);
        println!("   Confidence: {}", confidence);

        assert!(
            file_path.contains("calculator.py"),
            "Should track calculator.py"
        );
        assert_eq!(agent_type, "ClaudeCode");
        assert_eq!(confidence, "High");
    }

    println!("\n✅ Real Claude Code integration test passed!");
    println!("   ✓ Claude Code CLI invoked successfully");
    println!("   ✓ File was edited by Claude");
    println!("   ✓ PostToolUse hook triggered");
    println!("   ✓ Provenance recorded in database");
    println!("   ✓ Attribution is 100% accurate");
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

    // Use absolute path to the aiki binary for testing
    // This ensures the hook can find the binary even when it's not in PATH
    let aiki_binary = env!("CARGO_BIN_EXE_aiki");
    let hook_command = format!("{} record-change --claude-code", aiki_binary);

    let hooks_json = serde_json::json!({
        "description": "Track AI code changes",
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Edit|Write",
                    "hooks": [
                        {
                            "type": "command",
                            "command": hook_command,
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

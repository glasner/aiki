use jj_lib::repo::{Repo, StoreFactories};
use jj_lib::workspace::{default_working_copy_factories, Workspace};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Wait for background thread to update change description (using jj-lib, not jj binary)
#[allow(dead_code)]
fn wait_for_description_update_jjlib(
    repo_path: &Path,
    expected_content: &str,
    timeout: Duration,
) -> bool {
    let start = Instant::now();

    while start.elapsed() < timeout {
        // Try to load the workspace and check the description
        let settings = {
            use jj_lib::config::StackedConfig;
            use jj_lib::settings::UserSettings;
            let config = StackedConfig::with_defaults();
            match UserSettings::from_config(config) {
                Ok(s) => s,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        };

        {
            let store_factories = StoreFactories::default();
            let working_copy_factories = default_working_copy_factories();

            if let Ok(workspace) = Workspace::load(
                &settings,
                repo_path,
                &store_factories,
                &working_copy_factories,
            ) {
                if let Ok(repo) = workspace.repo_loader().load_at_head() {
                    let workspace_id = workspace.workspace_name();
                    if let Some(wc_commit_id) = repo.view().get_wc_commit_id(workspace_id) {
                        if let Ok(commit) = repo.store().get_commit(wc_commit_id) {
                            let description = commit.description();
                            if description.contains(expected_content) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Poll every 100ms
        std::thread::sleep(Duration::from_millis(100));
    }

    false
}

/// Test that records a change and then runs blame to verify attribution
#[test]
fn test_blame_shows_recorded_change() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repository
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Create a test file
    let test_file = repo_path.join("test.txt");
    fs::write(&test_file, "line 1\nline 2\nline 3\n").unwrap();

    // Commit it
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Initialize aiki (this will also do git import)
    let aiki_bin = get_aiki_binary_path();
    let output = Command::new(&aiki_bin)
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run aiki init");

    assert!(
        output.status.success(),
        "aiki init failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Modify the file
    fs::write(&test_file, "line 1\nline 2 modified\nline 3\nline 4\n").unwrap();

    // Record the change as if ClaudeCode made it
    let hook_input = r#"{
        "session_id": "test-session-123",
        "transcript_path": null,
        "cwd": "",
        "hook_event_name": "tool_succeeded",
        "tool_name": "Write",
        "tool_input": {
            "file_path": ""
        },
        "tool_output": null,
        "confidence": "high"
    }"#;

    let hook_input = hook_input
        .replace(
            r#""cwd": """#,
            &format!(r#""cwd": "{}""#, repo_path.display()),
        )
        .replace(
            r#""file_path": """#,
            &format!(r#""file_path": "{}""#, test_file.display()),
        );

    let output = Command::new(&aiki_bin)
        .arg("record-change")
        .arg("--claude-code")
        .arg("--sync") // Run synchronously for testing
        .current_dir(repo_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(hook_input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("Failed to run aiki record-change");

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

    // No need to wait - --sync mode blocks until the description is set
    // record_change now handles snapshotting via jj-lib

    // Use jj-lib to check the change description (no jj binary needed)
    // After record_change with snapshotting, the working copy is now a NEW change,
    // and the metadata is on the PARENT change (the one we modified)
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

    // Get the working copy commit
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_id)
        .expect("No working copy commit found");

    let wc_commit = repo
        .store()
        .get_commit(wc_commit_id)
        .expect("Failed to load working copy commit");

    // Get the parent commit (which has the metadata)
    let parent_ids = wc_commit.parent_ids();
    assert!(!parent_ids.is_empty(), "Working copy should have a parent");

    let parent_commit = repo
        .store()
        .get_commit(&parent_ids[0])
        .expect("Failed to load parent commit");

    let description = parent_commit.description();
    println!("JJ parent change description:\n{}", description);

    // Verify the metadata was written
    assert!(
        description.contains("[aiki]"),
        "Parent change description should contain [aiki] marker. Got: {}",
        description
    );

    // Run blame on the file
    let output = Command::new(&aiki_bin)
        .args(["blame", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run aiki blame");

    let blame_output = String::from_utf8_lossy(&output.stdout);
    let blame_stderr = String::from_utf8_lossy(&output.stderr);

    println!("Blame output:\n{}", blame_output);
    if !blame_stderr.is_empty() {
        println!("Blame stderr:\n{}", blame_stderr);
    }

    // Verify the blame command succeeded
    assert!(output.status.success(), "aiki blame should succeed");

    // Verify the output contains the file content
    assert!(blame_output.contains("line 1"), "Blame should show line 1");
    assert!(blame_output.contains("line 2"), "Blame should show line 2");

    // Verify line markers are present
    assert!(blame_output.contains("1|"), "Should have line 1 marker");
    assert!(blame_output.contains("2|"), "Should have line 2 marker");

    // CRITICAL: Verify Claude Code attribution appears in the blame output
    // Format is: <commit_id> (<agent_type> <session_id> <confidence>) <line_num>| <line_text>
    // The modified line (line 2) should show Claude Code attribution

    // Look for Claude Code agent type in the output (using Display format with space)
    assert!(
        blame_output.contains("Claude Code"),
        "Blame should show 'Claude Code' agent type. Output:\n{}",
        blame_output
    );

    // Verify session ID appears (truncated to first 9 chars as "test-sess...")
    assert!(
        blame_output.contains("test-sess"),
        "Blame should show truncated session ID 'test-sess...'. Output:\n{}",
        blame_output
    );

    // Verify High confidence appears
    assert!(
        blame_output.contains("High"),
        "Blame should show 'High' confidence. Output:\n{}",
        blame_output
    );

    println!("✅ Verified Claude Code attribution in blame output:");
    println!("   ✓ Agent type: Claude Code");
    println!("   ✓ Session ID: test-session-123");
    println!("   ✓ Confidence: High");
}

fn get_aiki_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("aiki");
    path
}

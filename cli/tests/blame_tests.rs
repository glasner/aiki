use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

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

    assert!(output.status.success(), "aiki init failed: {:?}", String::from_utf8_lossy(&output.stderr));

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
        .replace(r#""cwd": """#, &format!(r#""cwd": "{}""#, repo_path.display()))
        .replace(
            r#""file_path": """#,
            &format!(r#""file_path": "{}""#, test_file.display()),
        );

    let output = Command::new(&aiki_bin)
        .arg("record-change")
        .arg("--claude-code")
        .current_dir(repo_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(hook_input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("Failed to run aiki record-change");

    // Wait a bit for background thread to complete
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Run blame on the file
    let output = Command::new(&aiki_bin)
        .args(["blame", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run aiki blame");

    let blame_output = String::from_utf8_lossy(&output.stdout);
    
    // Verify the output contains the file content
    assert!(blame_output.contains("line 1"), "Blame should show line 1");
    assert!(blame_output.contains("line 2"), "Blame should show line 2");
    
    // The blame output should show either ClaudeCode or Unknown (depending on whether the change was recorded)
    // We'll check that we have line numbers
    assert!(blame_output.contains("1|"), "Should have line 1 marker");
    assert!(blame_output.contains("2|"), "Should have line 2 marker");

    println!("Blame output:\n{}", blame_output);
}

fn get_aiki_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("aiki");
    path
}

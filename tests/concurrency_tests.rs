mod common;

use assert_cmd::Command;
use common::{init_jj_workspace, jj_available, wait_for_description_update};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
#[allow(deprecated)]
fn test_concurrent_record_change_calls() {
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

    // Create test files
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    let file3 = temp_dir.path().join("file3.txt");

    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    fs::write(&file3, "content3").unwrap();

    // Use a barrier to ensure all threads start at the same time
    let barrier = Arc::new(Barrier::new(3));
    let repo_path = temp_dir.path().to_path_buf();

    // Spawn 3 threads that will call record-change simultaneously
    let handles: Vec<_> = (1..=3)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            let repo_path = repo_path.clone();
            let file_path = match i {
                1 => file1.clone(),
                2 => file2.clone(),
                _ => file3.clone(),
            };

            thread::spawn(move || {
                // Wait for all threads to be ready
                barrier.wait();

                // Create hook input for this thread
                let hook_input = serde_json::json!({
                    "session_id": format!("concurrent-session-{}", i),
                    "cwd": repo_path.to_string_lossy(),
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Edit",
                    "tool_input": {
                        "file_path": file_path.to_string_lossy(),
                    },
                    "tool_output": "Success"
                });

                // Execute record-change
                let mut cmd = Command::cargo_bin("aiki").unwrap();
                let result = cmd
                    .arg("record-change")
                    .arg("--claude-code")
                    .write_stdin(serde_json::to_string(&hook_input).unwrap())
                    .current_dir(&repo_path)
                    .assert()
                    .try_success();

                (i, result.is_ok())
            })
        })
        .collect();

    // Wait for all threads to complete
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all commands succeeded
    for (thread_id, success) in &results {
        assert!(
            success,
            "Thread {} failed to execute record-change",
            thread_id
        );
    }

    // Wait for at least one background thread to complete
    // Note: With concurrent calls, only the last one will win
    assert!(
        wait_for_description_update(&repo_path, "[aiki]", Duration::from_secs(10)),
        "No background thread completed within 10 seconds"
    );

    // Verify the final description contains aiki metadata
    let output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("-T")
        .arg("description")
        .current_dir(&repo_path)
        .output()
        .unwrap();

    let description = String::from_utf8_lossy(&output.stdout);

    // Should have aiki metadata from one of the sessions
    assert!(description.contains("[aiki]"));
    assert!(description.contains("agent=claude-code"));

    // Should contain one of the session IDs (the last one to complete wins)
    let has_session = description.contains("concurrent-session-1")
        || description.contains("concurrent-session-2")
        || description.contains("concurrent-session-3");
    assert!(
        has_session,
        "Description should contain one of the session IDs"
    );
}

#[test]
#[allow(deprecated)]
fn test_rapid_sequential_record_change_calls() {
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
    let test_file = temp_dir.path().join("rapid.txt");
    fs::write(&test_file, "initial").unwrap();

    // Make 5 rapid sequential calls
    for i in 1..=5 {
        let hook_input = serde_json::json!({
            "session_id": format!("rapid-session-{}", i),
            "cwd": temp_dir.path().to_string_lossy(),
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": test_file.to_string_lossy(),
            },
            "tool_output": "Success"
        });

        let mut cmd = Command::cargo_bin("aiki").unwrap();
        cmd.arg("record-change")
            .arg("--claude-code")
            .write_stdin(serde_json::to_string(&hook_input).unwrap())
            .current_dir(temp_dir.path())
            .assert()
            .success();

        // Small delay to avoid overwhelming the system
        thread::sleep(Duration::from_millis(10));
    }

    // Wait for background threads to settle
    // The last call should eventually win
    assert!(
        wait_for_description_update(temp_dir.path(), "rapid-session-", Duration::from_secs(10)),
        "Background threads did not complete within 10 seconds"
    );

    // Verify the description was updated
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
    // Should contain one of the session IDs (likely the last one)
    assert!(description.contains("rapid-session-"));
}

#[test]
#[allow(deprecated)]
fn test_record_change_does_not_block() {
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

    let test_file = temp_dir.path().join("nonblocking.txt");
    fs::write(&test_file, "test").unwrap();

    let hook_input = serde_json::json!({
        "session_id": "nonblocking-session",
        "cwd": temp_dir.path().to_string_lossy(),
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": test_file.to_string_lossy(),
        },
        "tool_output": "Success"
    });

    // Measure execution time
    let start = Instant::now();

    let mut cmd = Command::cargo_bin("aiki").unwrap();
    cmd.arg("record-change")
        .arg("--claude-code")
        .write_stdin(serde_json::to_string(&hook_input).unwrap())
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let elapsed = start.elapsed();

    // The command should return quickly (< 100ms) because background threading
    // Note: This is more lenient than the <10ms target to account for CI variability
    assert!(
        elapsed < Duration::from_millis(100),
        "record-change took {:.2}ms, expected <100ms (target: <10ms)",
        elapsed.as_secs_f64() * 1000.0
    );

    println!(
        "✓ record-change returned in {:.2}ms (target: <10ms)",
        elapsed.as_secs_f64() * 1000.0
    );

    // Verify background thread eventually completes
    assert!(
        wait_for_description_update(temp_dir.path(), "[aiki]", Duration::from_secs(5)),
        "Background thread did not complete within 5 seconds"
    );
}

#[test]
#[allow(deprecated)]
fn test_concurrent_different_sessions() {
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

    // Simulate multiple Claude Code sessions working on different files
    let file_a = temp_dir.path().join("file_a.txt");
    let file_b = temp_dir.path().join("file_b.txt");

    fs::write(&file_a, "session A work").unwrap();
    fs::write(&file_b, "session B work").unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let repo_path = temp_dir.path().to_path_buf();

    let handles: Vec<_> = vec![
        (file_a.clone(), "session-alice"),
        (file_b.clone(), "session-bob"),
    ]
    .into_iter()
    .map(|(file_path, session_id)| {
        let barrier = Arc::clone(&barrier);
        let repo_path = repo_path.clone();
        let session_id = session_id.to_string();

        thread::spawn(move || {
            barrier.wait();

            let hook_input = serde_json::json!({
                "session_id": session_id,
                "cwd": repo_path.to_string_lossy(),
                "hook_event_name": "PostToolUse",
                "tool_name": "Write",
                "tool_input": {
                    "file_path": file_path.to_string_lossy(),
                },
                "tool_output": "Success"
            });

            let mut cmd = Command::cargo_bin("aiki").unwrap();
            cmd.arg("record-change")
                .arg("--claude-code")
                .write_stdin(serde_json::to_string(&hook_input).unwrap())
                .current_dir(&repo_path)
                .assert()
                .success();
        })
    })
    .collect();

    // Wait for threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Wait for at least one to complete
    assert!(
        wait_for_description_update(&repo_path, "[aiki]", Duration::from_secs(10)),
        "Background threads did not complete within 10 seconds"
    );

    // Just verify the system didn't crash and metadata was written
    let output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("-T")
        .arg("description")
        .current_dir(&repo_path)
        .output()
        .unwrap();

    let description = String::from_utf8_lossy(&output.stdout);
    assert!(description.contains("[aiki]"));
}

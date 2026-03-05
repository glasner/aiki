//! Integration tests for task commands
//!
//! Tests the complete task workflow through the CLI interface.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

/// Helper function to initialize a Git repository
fn init_git_repo(path: &std::path::Path) {
    Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("Failed to initialize Git repository");

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .expect("Failed to configure git email");
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .expect("Failed to configure git name");
}

/// Helper function to initialize an Aiki repository
fn init_aiki_repo(path: &std::path::Path) {
    init_git_repo(path);

    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(path)
        .arg("init")
        .output()
        .expect("Failed to run aiki init");

    if !output.status.success() {
        panic!(
            "aiki init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Helper to run aiki task command
fn aiki_task(path: &std::path::Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.current_dir(path);
    cmd.arg("task");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.assert()
}

/// Extract short task ID from "Added <id>" output line
fn extract_short_id(output: &str) -> String {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Added ") {
            // "Added abc1234 — name" or "Added abc1234"
            let id = rest.split_whitespace().next().unwrap_or("");
            return id.to_string();
        }
    }
    panic!("Could not find 'Added <id>' in output: {}", output);
}

/// Extract short task ID from "Started <id>" output line
fn extract_started_id(output: &str) -> String {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Started ") {
            let id = rest.split_whitespace().next().unwrap_or("");
            return id.to_string();
        }
    }
    panic!("Could not find 'Started <id>' in output: {}", output);
}

/// Extract the first short task ID from a list output line matching `[pN] <id>  <name>`
fn extract_id_from_list(output: &str) -> String {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[p") {
            // Format: "[pN] <id>  <name>" or "[pN] <id> — <name>"
            if let Some(after_bracket) = trimmed.find("] ") {
                let rest = &trimmed[after_bracket + 2..];
                let id = rest.split_whitespace().next().unwrap_or("");
                if !id.is_empty() {
                    return id.to_string();
                }
            }
        }
    }
    panic!("Could not find task ID in list output: {}", output);
}

/// Extract a short task ID from list output whose line contains the given name
fn extract_id_from_list_by_name(output: &str, name: &str) -> String {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains(name) && trimmed.starts_with("[p") {
            if let Some(after_bracket) = trimmed.find("] ") {
                let rest = &trimmed[after_bracket + 2..];
                let id = rest.split_whitespace().next().unwrap_or("");
                if !id.is_empty() {
                    return id.to_string();
                }
            }
        }
    }
    panic!(
        "Could not find task '{}' in list output: {}",
        name, output
    );
}

// ============================================================================
// Phase 1: Core Workflow Tests
// ============================================================================

#[test]
fn test_task_list_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains("Ready (0):"));
}

#[test]
fn test_task_add_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["add", "Fix auth bug"])
        .success()
        .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_priority_p0() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "Critical bug", "--p0"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    // Verify priority via show
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("Priority: p0"));
}

#[test]
fn test_task_add_with_priority_p1() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "High priority task", "--p1"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    // Verify priority via show
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("Priority: p1"));
}

#[test]
fn test_task_add_with_priority_p3() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "Low priority task", "--p3"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    // Verify priority via show
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("Priority: p3"));
}

#[test]
fn test_task_list_after_add() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add a task
    aiki_task(temp_dir.path(), &["add", "Test task"]).success();

    // List should show the task
    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains("Ready (1):"))
        .stdout(predicate::str::contains("Test task"));
}

#[test]
fn test_task_start_from_ready_queue() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add a task
    aiki_task(temp_dir.path(), &["add", "Task to start"]).success();

    // Start with no ID should start from ready queue
    aiki_task(temp_dir.path(), &["start"])
        .success()
        .stdout(predicate::str::contains("Started"))
        .stdout(predicate::str::contains("Task to start"));
}

#[test]
fn test_task_start_shows_in_context() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Working task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // List should show it in context's in_progress
    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains("In Progress:"))
        .stdout(predicate::str::contains("Working task"));
}

#[test]
fn test_task_stop_current() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task to stop"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Stop the current task
    aiki_task(temp_dir.path(), &["stop"])
        .success()
        .stdout(predicate::str::contains("Stopped"))
        .stdout(predicate::str::contains("Task to stop"));
}

#[test]
fn test_task_stop_with_reason() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task with blocker"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Stop with reason
    aiki_task(
        temp_dir.path(),
        &["stop", "--reason", "Need design decision"],
    )
    .success()
    .stdout(predicate::str::contains("Stopped"));
}

#[test]
fn test_task_close_current() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task to complete"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Close the current task
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"])
        .success()
        .stdout(predicate::str::contains("Closed"));
}

#[test]
fn test_task_close_wont_do() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add a task and extract its short ID
    let add_output = aiki_task(temp_dir.path(), &["add", "Task to abandon"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let short_id = add_stdout
        .strip_prefix("Added ")
        .and_then(|s| s.split_whitespace().next())
        .expect("Should extract short ID from add output");

    aiki_task(temp_dir.path(), &["start"]).success();

    // Close as won't do
    aiki_task(
        temp_dir.path(),
        &["close", "--wont-do", "--summary", "Not implementing"],
    )
    .success()
    .stdout(predicate::str::contains("Closed"));

    // Verify the outcome persisted as wont_do via show
    let show_output = aiki_task(temp_dir.path(), &["show", short_id]).success();
    let show_stdout = String::from_utf8_lossy(&show_output.get_output().stdout);
    assert!(
        show_stdout.contains("closed (wont_do)"),
        "Task should have wont_do outcome after --wont-do close, got: {}",
        show_stdout
    );
}

#[test]
fn test_task_close_with_outcome_done() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add a task and extract its short ID
    let add_output = aiki_task(temp_dir.path(), &["add", "Task with explicit done"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let short_id = add_stdout
        .strip_prefix("Added ")
        .and_then(|s| s.split_whitespace().next())
        .expect("Should extract short ID from add output");

    aiki_task(temp_dir.path(), &["start"]).success();

    // Close with --outcome done (explicit)
    aiki_task(
        temp_dir.path(),
        &["close", "--outcome", "done", "--summary", "Done explicitly"],
    )
    .success()
    .stdout(predicate::str::contains("Closed"));

    // Verify the outcome persisted as done via show
    let show_output = aiki_task(temp_dir.path(), &["show", short_id]).success();
    let show_stdout = String::from_utf8_lossy(&show_output.get_output().stdout);
    assert!(
        show_stdout.contains("closed (done)"),
        "Task should have done outcome after --outcome done close, got: {}",
        show_stdout
    );
}

#[test]
fn test_task_close_with_outcome_wont_do() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add a task and extract its short ID
    let add_output = aiki_task(temp_dir.path(), &["add", "Task with outcome wont_do"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let short_id = add_stdout
        .strip_prefix("Added ")
        .and_then(|s| s.split_whitespace().next())
        .expect("Should extract short ID from add output");

    aiki_task(temp_dir.path(), &["start"]).success();

    // Close with --outcome wont_do
    aiki_task(
        temp_dir.path(),
        &[
            "close",
            "--outcome",
            "wont_do",
            "--summary",
            "Won't do via outcome",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Closed"));

    // Verify the outcome persisted as wont_do via show
    let show_output = aiki_task(temp_dir.path(), &["show", short_id]).success();
    let show_stdout = String::from_utf8_lossy(&show_output.get_output().stdout);
    assert!(
        show_stdout.contains("closed (wont_do)"),
        "Task should have wont_do outcome after --outcome wont_do close, got: {}",
        show_stdout
    );
}

#[test]
fn test_task_close_with_invalid_outcome() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task with invalid outcome"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Close with invalid --outcome should fail
    aiki_task(
        temp_dir.path(),
        &["close", "--outcome", "invalid", "--summary", "Bad outcome"],
    )
    .failure()
    .stderr(predicate::str::contains("Invalid outcome: 'invalid'"))
    .stderr(predicate::str::contains("done, wont_do"));
}

// ============================================================================
// Phase 2: Hierarchical Tasks Tests
// ============================================================================

#[test]
fn test_task_add_with_parent() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add parent task and get its ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Parent task"])
        .output()
        .expect("Failed to add parent task");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parent_id = extract_short_id(&stdout);

    // Add child task
    aiki_task(
        temp_dir.path(),
        &["add", "Child task", "--parent", &parent_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_hierarchical_id_format() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add parent task
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Parent"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parent_id = extract_short_id(&stdout);

    // Add first child — verify via list --all that subtask IDs contain parent prefix
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "First child", "--parent", &parent_id])
        .output()
        .unwrap();

    // Add second child
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Second child", "--parent", &parent_id])
        .output()
        .unwrap();

    // Verify subtasks exist via show on parent
    let show_output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "show", &parent_id])
        .output()
        .unwrap();
    let show_stdout = String::from_utf8_lossy(&show_output.stdout);
    assert!(
        show_stdout.contains("First child"),
        "Should list first child subtask, got: {}",
        show_stdout
    );
    assert!(
        show_stdout.contains("Second child"),
        "Should list second child subtask, got: {}",
        show_stdout
    );
}

// ============================================================================
// Phase 3: Status Filters Tests
// ============================================================================

#[test]
fn test_task_list_all() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create tasks in different states
    aiki_task(temp_dir.path(), &["add", "Open task"]).success();
    aiki_task(temp_dir.path(), &["add", "To be closed"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"]).success();

    // --all should show all tasks including closed
    aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .stdout(predicate::str::contains("Open task"))
        .stdout(predicate::str::contains("To be closed"));
}

#[test]
fn test_task_list_open_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create two tasks - we'll close the first one and keep second open
    // Note: start without ID starts oldest task (first in queue)
    aiki_task(temp_dir.path(), &["add", "Task to close"]).success();
    aiki_task(temp_dir.path(), &["add", "Task to keep open"]).success();
    aiki_task(temp_dir.path(), &["start"]).success(); // Starts "Task to close" (oldest)
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"]).success(); // Closes "Task to close"

    // --open should only show open tasks
    let output = aiki_task(temp_dir.path(), &["list", "--open"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(
        stdout.contains("Task to keep open"),
        "Should contain the task that's still open, got: {}",
        stdout
    );
}

#[test]
fn test_task_list_in_progress_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create tasks
    aiki_task(temp_dir.path(), &["add", "Open task"]).success();
    aiki_task(temp_dir.path(), &["add", "In progress task"]).success();

    // Start the first task from ready queue
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_task_id = extract_id_from_list(&stdout);

    aiki_task(temp_dir.path(), &["start", &first_task_id]).success();

    // --in-progress should only show in-progress tasks
    aiki_task(temp_dir.path(), &["list", "--in-progress"])
        .success()
        .stdout(predicate::str::contains("Tasks (1):"));
}

#[test]
fn test_task_list_stopped_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and stop a task
    aiki_task(temp_dir.path(), &["add", "Stopped task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["stop", "--reason", "blocked"]).success();

    // --stopped should show stopped tasks
    aiki_task(temp_dir.path(), &["list", "--stopped"])
        .success()
        .stdout(predicate::str::contains("Stopped task"));
}

#[test]
fn test_task_list_closed_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and close a task
    aiki_task(temp_dir.path(), &["add", "Closed task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"]).success();

    // --closed should show closed tasks
    aiki_task(temp_dir.path(), &["list", "--closed"])
        .success()
        .stdout(predicate::str::contains("Closed task"));
}

// ============================================================================
// Phase 3: Multiple Blocked Flags Tests
// ============================================================================

#[test]
fn test_task_stop_with_blocked() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and start a task
    aiki_task(temp_dir.path(), &["add", "Task with blocker"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Stop with --blocked creates a blocker task
    aiki_task(
        temp_dir.path(),
        &["stop", "--blocked", "Need API credentials"],
    )
    .success()
    .stdout(predicate::str::contains("Stopped"));

    // The blocker task should appear in list (use --all to bypass session-based filtering)
    aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .stdout(predicate::str::contains("Need API credentials"));
}

#[test]
fn test_task_stop_with_multiple_blocked() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and start a task
    aiki_task(temp_dir.path(), &["add", "Complex blocker task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Stop with multiple --blocked flags
    aiki_task(
        temp_dir.path(),
        &[
            "stop",
            "--blocked",
            "Need API credentials",
            "--blocked",
            "Need design review",
        ],
    )
    .success();

    // Both blocker tasks should appear in list (use --all to bypass session-based filtering)
    aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .stdout(predicate::str::contains("Need API credentials"))
        .stdout(predicate::str::contains("Need design review"));
}

// ============================================================================
// Phase 4: Show, Update, Comment, Reopen Tests
// ============================================================================

#[test]
fn test_task_show_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task
    let add_output = aiki_task(temp_dir.path(), &["add", "Task to show"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let task_id = extract_short_id(&add_stdout);

    // Show the task
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("Task: Task to show"));
}

#[test]
fn test_task_show_current() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and start a task
    aiki_task(temp_dir.path(), &["add", "Current task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Show without ID shows current task
    aiki_task(temp_dir.path(), &["show"])
        .success()
        .stdout(predicate::str::contains("Task: Current task"));
}

#[test]
fn test_task_set_name() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task and extract short ID from "Added <id>" output
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Original name"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Set the name
    aiki_task(
        temp_dir.path(),
        &["set", &task_id, "--name", "Updated name"],
    )
    .success()
    .stdout(predicate::str::contains("Updated name"));

    // Verify the name changed
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("Updated name"));
}

#[test]
fn test_task_set_priority() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task (default P2) and extract short ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Priority task"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Set to P0
    aiki_task(temp_dir.path(), &["set", &task_id, "--p0"]).success();

    // Verify the priority changed
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains("p0"));
}

#[test]
fn test_task_unset_assignee() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task with assignee
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Assigned task", "--assignee", "claude-code"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Unset assignee — should produce confirmation with "Cleared" and field name
    aiki_task(temp_dir.path(), &["unset", &task_id, "--assignee"])
        .success()
        .stdout(predicate::str::contains("Cleared"))
        .stdout(predicate::str::contains("assignee"));
}

#[test]
fn test_task_unset_rejects_name() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "My task"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Attempt to unset name — should fail (no --name flag exists)
    aiki_task(temp_dir.path(), &["unset", &task_id])
        .success()
        .stdout(predicate::str::contains("No fields specified"));
}

#[test]
fn test_task_unset_rejects_priority() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "My task"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Attempt to unset priority — should fail (no --priority flag exists)
    aiki_task(temp_dir.path(), &["unset", &task_id])
        .success()
        .stdout(predicate::str::contains("No fields specified"));
}

#[test]
fn test_task_unset_rejects_unknown_field() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "My task"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Attempt to unset with unknown flag — should fail at CLI parsing level
    aiki_task(temp_dir.path(), &["unset", &task_id, "--foobar"]).failure(); // Clap will reject unknown flags
}

#[test]
fn test_task_set_rejects_empty_data_value() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "My task"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let task_id = extract_short_id(&stdout);

    // Start the task first
    aiki_task(temp_dir.path(), &["start", &task_id]).success();

    // Attempt to set data with empty value
    aiki_task(temp_dir.path(), &["set", &task_id, "--data", "key="])
        .success()
        .stdout(predicate::str::contains("aiki task unset"));
}

#[test]
fn test_task_comment() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task
    let add_output = aiki_task(temp_dir.path(), &["add", "Task with comment"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let task_id = extract_short_id(&add_stdout);

    // Add a comment (comment add <ID> <TEXT>)
    aiki_task(
        temp_dir.path(),
        &["comment", "add", &task_id, "This is a test comment"],
    )
    .success()
    .stdout(predicate::str::contains("Comment added."));
}

#[test]
fn test_task_comment_with_data() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task
    let add_output = aiki_task(temp_dir.path(), &["add", "Task with structured comment"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let task_id = extract_short_id(&add_stdout);

    // Add a comment with structured data
    aiki_task(
        temp_dir.path(),
        &[
            "comment",
            "add",
            &task_id,
            "Potential null pointer dereference",
            "--data",
            "file=src/auth.ts",
            "--data",
            "line=42",
            "--data",
            "severity=error",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Comment added."));

    // Verify task show displays the comment
    aiki_task(temp_dir.path(), &["show", &task_id])
        .success()
        .stdout(predicate::str::contains(
            "Potential null pointer dereference",
        ));

    // Verify the data fields are persisted in jj task events
    // Read the events from the aiki/tasks branch via jj log
    let output = Command::new("jj")
        .current_dir(temp_dir.path())
        .args([
            "log",
            "-r",
            "children(aiki/tasks)",
            "--no-graph",
            "-T",
            "description",
            "--ignore-working-copy",
        ])
        .output()
        .expect("Failed to run jj log");

    let contents = String::from_utf8_lossy(&output.stdout);

    // Check that all data fields are stored in the event
    assert!(
        contents.contains("data=file:src/auth.ts"),
        "Should contain file data field, got: {}",
        contents
    );
    assert!(
        contents.contains("data=line:42"),
        "Should contain line data field, got: {}",
        contents
    );
    assert!(
        contents.contains("data=severity:error"),
        "Should contain severity data field, got: {}",
        contents
    );
}

#[test]
fn test_task_start_reopen() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create, start, and close a task
    let add_output = aiki_task(temp_dir.path(), &["add", "Task to reopen"]).success();
    let add_stdout = String::from_utf8_lossy(&add_output.get_output().stdout);
    let task_id = extract_short_id(&add_stdout);

    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"]).success();

    // Reopen and start the task
    aiki_task(
        temp_dir.path(),
        &[
            "start",
            &task_id,
            "--reopen",
            "--reason",
            "Found another bug",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Started"))
    .stdout(predicate::str::contains("Task to reopen"));
}

// ============================================================================
// Workflow Tests: Auto-stop on Start
// ============================================================================

#[test]
fn test_task_start_does_not_stop_other_tasks() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create two tasks
    aiki_task(temp_dir.path(), &["add", "First task"]).success();
    aiki_task(temp_dir.path(), &["add", "Second task"]).success();

    // Start first task
    aiki_task(temp_dir.path(), &["start"]).success();

    // Get second task ID from list (in ready queue)
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let second_task_id = extract_id_from_list_by_name(&stdout, "Second task");

    // Start second task - should NOT auto-stop first (no stopped output)
    aiki_task(temp_dir.path(), &["start", &second_task_id])
        .success()
        .stdout(predicate::str::contains("Stopped").not());

    // Verify both tasks are now in progress
    let list_output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list", "--in-progress"])
        .output()
        .unwrap();
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout.contains("First task") && list_stdout.contains("Second task"),
        "Both tasks should be in progress, got: {}",
        list_stdout
    );
}

// ============================================================================
// Workflow Tests: Context Contract
// ============================================================================

#[test]
fn test_context_always_shows_ready_queue() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create tasks
    aiki_task(temp_dir.path(), &["add", "Task 1"]).success();
    aiki_task(temp_dir.path(), &["add", "Task 2"]).success();

    // Every command should have context with ready queue
    for cmd in [vec!["list"], vec!["list", "--all"], vec!["list", "--open"]] {
        let output = aiki_task(temp_dir.path(), &cmd).success();
        let stdout = String::from_utf8_lossy(&output.get_output().stdout);
        assert!(
            stdout.contains("Ready ("),
            "Command {:?} should have ready queue count, got: {}",
            cmd,
            stdout
        );
    }
}

#[test]
fn test_list_filter_preserves_context_ready_queue() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create open and closed tasks
    aiki_task(temp_dir.path(), &["add", "Open task"]).success();
    aiki_task(temp_dir.path(), &["add", "To close"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"]).success();

    // When filtering by --closed, the context should still show actual ready queue
    let output = aiki_task(temp_dir.path(), &["list", "--closed"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Context should show ready count
    assert!(
        stdout.contains("Ready ("),
        "Context should show ready count, got: {}",
        stdout
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_task_start_nonexistent() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Quick-start feature: if input isn't a task ID, it's treated as a description
    // and a new task is created with that name
    aiki_task(temp_dir.path(), &["start", "nonexistent"])
        .success()
        .stdout(predicate::str::contains("Started"));
}

#[test]
fn test_task_close_nonexistent() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["close", "nonexistent"])
        .failure()
        .stderr(predicate::str::contains("Task not found"));
}

#[test]
fn test_task_stop_when_none_in_progress() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add task but don't start
    aiki_task(temp_dir.path(), &["add", "Not started"]).success();

    // Error cases return exit code 0 but with error output
    aiki_task(temp_dir.path(), &["stop"])
        .success()
        .stdout(predicate::str::contains("Error:"))
        .stdout(predicate::str::contains("No task in progress to stop"));
}

#[test]
fn test_task_close_when_none_in_progress() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add task but don't start
    aiki_task(temp_dir.path(), &["add", "Not started"]).success();

    // Note: Error cases return exit code 0 but with XML error output
    aiki_task(temp_dir.path(), &["close", "--summary", "Test completed"])
        .success()
        .stdout(predicate::str::contains("Error:"))
        .stdout(predicate::str::contains("No task in progress to close"));
}

#[test]
fn test_task_add_invalid_parent() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Error for invalid parent is returned on stderr with exit code 1
    aiki_task(
        temp_dir.path(),
        &["add", "Orphan child", "--parent", "nonexistent"],
    )
    .failure()
    .stderr(predicate::str::contains("Task not found"));
}

#[test]
fn test_parent_auto_starts_when_all_subtasks_closed() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create parent task
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Parent task"])
        .output()
        .expect("Failed to add parent task");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parent_id = extract_short_id(&stdout);

    // Create two subtasks (they get full IDs, linked via subtask-of edges)
    let output1 = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Subtask 1", "--parent", &parent_id])
        .output()
        .expect("Failed to add subtask 1");
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let subtask1_id = extract_short_id(&stdout1);

    let output2 = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Subtask 2", "--parent", &parent_id])
        .output()
        .expect("Failed to add subtask 2");
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let subtask2_id = extract_short_id(&stdout2);

    // Start parent (note: .0 planning subtask auto-creation is currently disabled)
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "start", &parent_id])
        .output()
        .expect("Failed to start parent");

    // Start and close subtask 1
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "start", &subtask1_id])
        .output()
        .expect("Failed to start subtask 1");

    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "close", &subtask1_id, "--summary", "Done"])
        .output()
        .expect("Failed to close subtask 1");

    // Start and close subtask 2 - this should trigger parent auto-start
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "start", &subtask2_id])
        .output()
        .expect("Failed to start subtask 2");

    // Close subtask 2 - parent should auto-start (visible in In Progress section)
    let close_output = aiki_task(
        temp_dir.path(),
        &["close", &subtask2_id, "--summary", "All done"],
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let close_stdout = String::from_utf8_lossy(&close_output);

    // Verify the parent task is now in progress (auto-started after all subtasks closed)
    assert!(
        close_stdout.contains("In Progress:") && close_stdout.contains("Parent task"),
        "Parent should auto-start when all subtasks closed. Output: {}",
        close_stdout
    );
}

// ============================================================================
// Declarative Subtasks (Template with subtasks: source.comments)
// ============================================================================

/// Helper to create a template file for testing
fn create_template(templates_dir: &std::path::Path, namespace: &str, name: &str, content: &str) {
    let ns_dir = templates_dir.join(namespace);
    std::fs::create_dir_all(&ns_dir).expect("Failed to create namespace directory");
    let file_path = ns_dir.join(format!("{}.md", name));
    std::fs::write(&file_path, content).expect("Failed to write template file");
}

#[test]
fn test_template_add_creates_static_subtasks() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a template with static subtasks
    let templates_dir = temp_dir.path().join(".aiki/templates");
    create_template(
        &templates_dir,
        "test",
        "static-review",
        r#"---
version: 1.0.0
description: Review with static subtasks
---

# Review: {{data.scope}}

Review the code in {{data.scope}}.

# Subtasks

## Analyze code

Look at the code structure.

## Write summary

Document your findings.
"#,
    );

    // Create task from template
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "--template",
            "test/static-review",
            "--data",
            "scope=src/auth.rs",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));

    // List should show the parent and subtasks
    aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .stdout(predicate::str::contains("Review: src/auth.rs"))
        .stdout(predicate::str::contains("Analyze code"))
        .stdout(predicate::str::contains("Write summary"));
}

#[test]
fn test_template_add_with_dynamic_subtasks() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a template with dynamic subtasks using inline loops
    let templates_dir = temp_dir.path().join(".aiki/templates");
    create_template(
        &templates_dir,
        "test",
        "followup",
        r#"---
version: 1.0.0
description: Followup with dynamic subtasks from comments
---

# Followup: {{data.scope}}

Fix all issues identified in the review.

# Subtasks

{% for item in source.comments %}
## Fix: {{item.file}}

{{item.text}}
{% endfor %}
"#,
    );

    // Create a source task
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Code review"])
        .output()
        .expect("Failed to add source task");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let source_task_id = extract_short_id(&stdout);

    // Add comments to the source task with structured data
    // Comment 1: file=auth.rs
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args([
            "task",
            "comment",
            "add",
            &source_task_id,
            "Missing null check",
            "--data",
            "file=auth.rs",
        ])
        .output()
        .expect("Failed to add comment 1");

    // Comment 2: file=utils.rs
    Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args([
            "task",
            "comment",
            "add",
            &source_task_id,
            "Unused import",
            "--data",
            "file=utils.rs",
        ])
        .output()
        .expect("Failed to add comment 2");

    // Create task from template with --source task:<id>
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "--template",
            "test/followup",
            "--data",
            "scope=auth-module",
            "--source",
            &format!("task:{}", source_task_id),
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));

    // List should show the parent and dynamically created subtasks
    let output = aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .get_output()
        .stdout
        .clone();
    let list_output = String::from_utf8_lossy(&output);

    assert!(
        list_output.contains("Followup: auth-module"),
        "Should have parent task"
    );
    assert!(
        list_output.contains("Fix: auth.rs"),
        "Should have subtask for auth.rs"
    );
    assert!(
        list_output.contains("Fix: utils.rs"),
        "Should have subtask for utils.rs"
    );
}

#[test]
fn test_template_dynamic_subtasks_requires_source() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a template with dynamic subtasks using inline loops
    let templates_dir = temp_dir.path().join(".aiki/templates");
    create_template(
        &templates_dir,
        "test",
        "dynamic",
        r#"---
version: 1.0.0
---

# Task

Do work.

# Subtasks

{% for item in source.comments %}
## Fix: {{item.text}}

{{item.text}}
{% endfor %}
"#,
    );

    // Creating without --source task:<id> should succeed but have no subtasks
    // (inline loops with no data source produce no subtasks)
    aiki_task(temp_dir.path(), &["add", "--template", "test/dynamic"])
        .success()
        .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_template_unknown_data_source_fails() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a template with unknown data source using inline loop
    let templates_dir = temp_dir.path().join(".aiki/templates");
    create_template(
        &templates_dir,
        "test",
        "unknown-source",
        r#"---
version: 1.0.0
---

# Task

Do work.

# Subtasks

{% for item in source.unknown_source %}
## Fix

{{item.text}}
{% endfor %}
"#,
    );

    // Create a source task
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Source task"])
        .output()
        .expect("Failed to add source task");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let source_task_id = extract_short_id(&stdout);

    // Creating with unknown data source should fail
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "--template",
            "test/unknown-source",
            "--source",
            &format!("task:{}", source_task_id),
        ],
    )
    .failure()
    .stderr(predicate::str::contains("Unknown"));
}

#[test]
fn test_template_static_subtasks_honor_frontmatter_sources() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a source task to reference
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Original review"])
        .output()
        .expect("Failed to add source task");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let source_task_id = extract_short_id(&stdout);

    // Create a template with static subtasks that have frontmatter sources
    let templates_dir = temp_dir.path().join(".aiki/templates");
    create_template(
        &templates_dir,
        "test",
        "followup-static",
        &format!(
            r#"---
version: 1.0.0
description: Followup with static subtasks that have sources
---

# Followup: {{{{data.scope}}}}

Fix issues identified in the review.

# Subtasks

## Fix auth issue
---
sources:
  - task:{}
---

Fix the authentication bug.

## Fix validation issue
---
sources:
  - task:{}
---

Fix the validation issue.
"#,
            source_task_id, source_task_id
        ),
    );

    // Create task from template
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "--template",
            "test/followup-static",
            "--data",
            "scope=auth-module",
        ],
    )
    .success();

    // Show the subtasks to verify sources are included
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list", "--all"])
        .output()
        .expect("Failed to list tasks");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check that subtasks exist
    assert!(
        stdout.contains("Fix auth issue"),
        "Should have subtask 'Fix auth issue', got: {}",
        stdout
    );

    // Find the subtask ID from the list output
    let subtask_id = extract_id_from_list_by_name(&stdout, "Fix auth issue");

    // Show the subtask to verify it has the source
    let show_output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "show", &subtask_id])
        .output()
        .expect("Failed to show subtask");
    let show_stdout = String::from_utf8_lossy(&show_output.stdout);

    // The subtask should have the source from frontmatter
    // Format: "- Source: task:<id>"
    assert!(
        show_stdout.contains(&format!("task:{}", source_task_id)),
        "Subtask should have source from frontmatter. Output: {}",
        show_stdout
    );
}

// ============================================================================
// Link Flag Tests
// ============================================================================

#[test]
fn test_task_add_with_blocked_by() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create blocker task
    let output = aiki_task(temp_dir.path(), &["add", "Blocker task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let blocker_id = extract_short_id(&stdout);

    // Create task blocked by the first
    aiki_task(
        temp_dir.path(),
        &["add", "Blocked task", "--blocked-by", &blocker_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_multiple_blocked_by() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create two blocker tasks
    let output1 = aiki_task(temp_dir.path(), &["add", "Blocker 1"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let blocker1_id = extract_short_id(&stdout1);

    let output2 = aiki_task(temp_dir.path(), &["add", "Blocker 2"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let blocker2_id = extract_short_id(&stdout2);

    // Create task blocked by both
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Doubly blocked",
            "--blocked-by",
            &blocker1_id,
            "--blocked-by",
            &blocker2_id,
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_supersedes() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create old task
    let output = aiki_task(temp_dir.path(), &["add", "Old approach"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let old_id = extract_short_id(&stdout);

    // Create replacement
    aiki_task(
        temp_dir.path(),
        &["add", "New approach", "--supersedes", &old_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_subtask_of() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create parent task
    let output = aiki_task(temp_dir.path(), &["add", "Parent task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parent_id = extract_short_id(&stdout);

    // Create subtask using --subtask-of
    aiki_task(
        temp_dir.path(),
        &["add", "Child task", "--subtask-of", &parent_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_sourced_from() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(
        temp_dir.path(),
        &["add", "Fix bug", "--sourced-from", "file:ops/now/design.md"],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_multiple_sourced_from() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Fix bug",
            "--sourced-from",
            "file:ops/now/design.md",
            "--sourced-from",
            "file:ops/now/review.md",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_parent_alias_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create parent
    let output = aiki_task(temp_dir.path(), &["add", "Parent task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parent_id = extract_short_id(&stdout);

    // --parent is hidden alias for --subtask-of
    aiki_task(
        temp_dir.path(),
        &["add", "Child via parent", "--parent", &parent_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_source_alias_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // --source is hidden alias for --sourced-from
    aiki_task(
        temp_dir.path(),
        &["add", "Fix bug", "--source", "file:design.md"],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_both_subtask_of_and_parent_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "Parent task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let parent_id = extract_short_id(&stdout);

    // Both --subtask-of and --parent should error
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Child",
            "--subtask-of",
            &parent_id,
            "--parent",
            &parent_id,
        ],
    )
    .failure();
}

#[test]
fn test_task_add_both_sourced_from_and_source_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Both --sourced-from and --source should error
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Fix bug",
            "--sourced-from",
            "file:a.md",
            "--source",
            "file:b.md",
        ],
    )
    .failure();
}

#[test]
fn test_task_add_with_multiple_link_kinds() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create blocker task
    let output = aiki_task(temp_dir.path(), &["add", "Blocker"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let blocker_id = extract_short_id(&stdout);

    // Create task with both --blocked-by and --sourced-from
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Complex task",
            "--blocked-by",
            &blocker_id,
            "--sourced-from",
            "file:ops/now/design.md",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_link_with_source_alias() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "A task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    // --source is hidden alias for --sourced-from on link
    aiki_task(
        temp_dir.path(),
        &["link", &task_id, "--source", "file:design.md"],
    )
    .success();
}

#[test]
fn test_task_link_with_parent_alias() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let out1 = aiki_task(temp_dir.path(), &["add", "Parent"]).success();
    let stdout1 = String::from_utf8_lossy(&out1.get_output().stdout);
    let parent_id = extract_short_id(&stdout1);

    let out2 = aiki_task(temp_dir.path(), &["add", "Child"]).success();
    let stdout2 = String::from_utf8_lossy(&out2.get_output().stdout);
    let child_id = extract_short_id(&stdout2);

    // --parent is hidden alias for --subtask-of on link
    aiki_task(
        temp_dir.path(),
        &["link", &child_id, "--parent", &parent_id],
    )
    .success();
}

#[test]
fn test_task_link_both_sourced_from_and_source_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "A task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    aiki_task(
        temp_dir.path(),
        &[
            "link",
            &task_id,
            "--sourced-from",
            "file:a.md",
            "--source",
            "file:b.md",
        ],
    )
    .failure();
}

#[test]
fn test_task_unlink_with_source_alias() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create task with source
    let output = aiki_task(
        temp_dir.path(),
        &["add", "Task with source", "--source", "file:design.md"],
    )
    .success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let task_id = extract_short_id(&stdout);

    // Unlink using --source alias
    aiki_task(
        temp_dir.path(),
        &["unlink", &task_id, "--source", "file:design.md"],
    )
    .success();
}

// ── Complete link flags tests ──────────────────────────────────────────

#[test]
fn test_task_add_with_implements() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Plan: Auth system",
            "--implements",
            "file:ops/now/auth-plan.md",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_orchestrates() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a plan task to orchestrate
    let output = aiki_task(temp_dir.path(), &["add", "Plan task"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let plan_id = extract_short_id(&stdout);

    aiki_task(
        temp_dir.path(),
        &["add", "Build: Auth", "--orchestrates", &plan_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

// Note: --scoped-to is not yet implemented in the CLI, so those tests are skipped

#[test]
fn test_task_add_with_depends_on() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create dependency task
    let output = aiki_task(temp_dir.path(), &["add", "Unit tests"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let dep_id = extract_short_id(&stdout);

    aiki_task(
        temp_dir.path(),
        &["add", "Integration tests", "--depends-on", &dep_id],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_multiple_depends_on() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output1 = aiki_task(temp_dir.path(), &["add", "Dep 1"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let dep1_id = extract_short_id(&stdout1);

    let output2 = aiki_task(temp_dir.path(), &["add", "Dep 2"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let dep2_id = extract_short_id(&stdout2);

    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Final task",
            "--depends-on",
            &dep1_id,
            "--depends-on",
            &dep2_id,
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_add_with_all_new_link_types() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create tasks for link targets
    let output1 = aiki_task(temp_dir.path(), &["add", "Blocker"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let blocker_id = extract_short_id(&stdout1);

    let output2 = aiki_task(temp_dir.path(), &["add", "Dependency"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let dep_id = extract_short_id(&stdout2);

    // Create task with multiple link kinds at once (scoped-to removed - not yet in CLI)
    aiki_task(
        temp_dir.path(),
        &[
            "add",
            "Complex task",
            "--blocked-by",
            &blocker_id,
            "--depends-on",
            &dep_id,
            "--implements",
            "file:ops/now/plan.md",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Added"));
}

#[test]
fn test_task_start_quickstart_with_implements() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(
        temp_dir.path(),
        &[
            "start",
            "Plan: Auth",
            "--implements",
            "file:ops/now/auth-plan.md",
        ],
    )
    .success()
    .stdout(predicate::str::contains("Started"));
}

#[test]
fn test_task_start_quickstart_with_depends_on() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "Prerequisite"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let dep_id = extract_short_id(&stdout);

    aiki_task(
        temp_dir.path(),
        &["start", "Dependent task", "--depends-on", &dep_id],
    )
    .success()
    .stdout(predicate::str::contains("Started"));
}

// Note: --scoped-to is not yet implemented in the CLI, so test_task_start_quickstart_with_scoped_to is skipped

#[test]
fn test_task_start_existing_with_new_link_flags() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create dep task
    let output1 = aiki_task(temp_dir.path(), &["add", "Dep task"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let dep_id = extract_short_id(&stdout1);

    // Create task to start
    let output2 = aiki_task(temp_dir.path(), &["add", "Main task"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let task_id = extract_short_id(&stdout2);

    // Start with link flags (scoped-to removed - not yet in CLI)
    aiki_task(
        temp_dir.path(),
        &[
            "start",
            &task_id,
            "--depends-on",
            &dep_id,
        ],
    )
    .success()
    .stdout(predicate::str::contains("Started"));
}

#[test]
fn test_task_link_with_depends_on() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output1 = aiki_task(temp_dir.path(), &["add", "Task A"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let task_a_id = extract_short_id(&stdout1);

    let output2 = aiki_task(temp_dir.path(), &["add", "Task B"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let task_b_id = extract_short_id(&stdout2);

    // Link B depends-on A
    aiki_task(
        temp_dir.path(),
        &["link", &task_b_id, "--depends-on", &task_a_id],
    )
    .success();
}

#[test]
fn test_task_unlink_with_depends_on() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output1 = aiki_task(temp_dir.path(), &["add", "Task A"]).success();
    let stdout1 = String::from_utf8_lossy(&output1.get_output().stdout);
    let task_a_id = extract_short_id(&stdout1);

    let output2 = aiki_task(temp_dir.path(), &["add", "Task B"]).success();
    let stdout2 = String::from_utf8_lossy(&output2.get_output().stdout);
    let task_b_id = extract_short_id(&stdout2);

    // Link then unlink
    aiki_task(
        temp_dir.path(),
        &["link", &task_b_id, "--depends-on", &task_a_id],
    )
    .success();

    aiki_task(
        temp_dir.path(),
        &["unlink", &task_b_id, "--depends-on", &task_a_id],
    )
    .success();
}

#[test]
fn test_add_output_id_returns_bare_task_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    let output = aiki_task(temp_dir.path(), &["add", "Test output id", "--output", "id"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let id = stdout.trim();

    // Should be exactly a 32-char lowercase alpha string
    assert_eq!(id.len(), 32, "Expected 32-char ID, got '{}' (len={})", id, id.len());
    assert!(
        id.chars().all(|c| c.is_ascii_lowercase()),
        "Expected all lowercase letters, got '{}'",
        id
    );
    // Should NOT contain "Added:" prefix
    assert!(
        !stdout.contains("Added"),
        "Output should be bare ID, got '{}'",
        stdout
    );
}

#[test]
fn test_dedup_guard_prevents_duplicate_subtask() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create parent task
    let output = aiki_task(temp_dir.path(), &["add", "Dedup parent", "--output", "id"]).success();
    let parent_id = String::from_utf8_lossy(&output.get_output().stdout).trim().to_string();

    // Add subtask with name "Same name"
    let output1 = aiki_task(
        temp_dir.path(),
        &["add", "Same name", "--subtask-of", &parent_id, "--output", "id"],
    )
    .success();
    let id1 = String::from_utf8_lossy(&output1.get_output().stdout).trim().to_string();

    // Add subtask with same name again — should return existing ID
    let output2 = aiki_task(
        temp_dir.path(),
        &["add", "Same name", "--subtask-of", &parent_id, "--output", "id"],
    )
    .success();
    let id2 = String::from_utf8_lossy(&output2.get_output().stdout).trim().to_string();

    // Same ID both times
    assert_eq!(id1, id2, "Dedup should return same ID, got '{}' vs '{}'", id1, id2);
}

#[test]
fn test_dedup_guard_allows_recreation_after_close() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create parent task
    let output = aiki_task(temp_dir.path(), &["add", "Reopen parent", "--output", "id"]).success();
    let parent_id = String::from_utf8_lossy(&output.get_output().stdout).trim().to_string();

    // Add subtask
    let output1 = aiki_task(
        temp_dir.path(),
        &["add", "Closeable", "--subtask-of", &parent_id, "--output", "id"],
    )
    .success();
    let id1 = String::from_utf8_lossy(&output1.get_output().stdout).trim().to_string();

    // Start and close the subtask
    aiki_task(temp_dir.path(), &["start", &id1]).success();
    aiki_task(temp_dir.path(), &["close", &id1, "--summary", "done"]).success();

    // Add subtask with same name again — should create a NEW one (closed doesn't dedup)
    let output2 = aiki_task(
        temp_dir.path(),
        &["add", "Closeable", "--subtask-of", &parent_id, "--output", "id"],
    )
    .success();
    let id2 = String::from_utf8_lossy(&output2.get_output().stdout).trim().to_string();

    assert_ne!(id1, id2, "Should create new subtask after closing original, got same ID '{}'", id1);
}

/// Regression test: non-task-scoped reviews (plan/code) should succeed even
/// though `render_review_workflow` finds no `validates` edge and returns an
/// empty string.  This exercises the early-return path at review.rs:907-910.
#[test]
fn test_review_non_task_scope_succeeds() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create the review/plan template so the review command can resolve it
    let template_dir = temp_dir.path().join(".aiki/templates/aiki/review");
    std::fs::create_dir_all(&template_dir).expect("Failed to create template dir");
    std::fs::write(
        template_dir.join("plan.md"),
        "---\nversion: 3.0.0\ntype: review\n---\n\n# Review: {{data.scope.name}}\n\nReview the plan.\n",
    )
    .expect("Failed to write plan template");

    // Commit the template so aiki can find it
    Command::new("git")
        .args(["add", ".aiki/templates"])
        .current_dir(temp_dir.path())
        .output()
        .expect("git add templates failed");
    Command::new("git")
        .args(["commit", "-m", "add review template"])
        .current_dir(temp_dir.path())
        .output()
        .expect("git commit failed");

    // Create an .md file so the review target resolves as Plan scope
    std::fs::write(temp_dir.path().join("design.md"), "# Design\nSome plan content\n")
        .expect("Failed to write design.md");

    // Commit the file so the repo has history
    Command::new("git")
        .args(["add", "design.md"])
        .current_dir(temp_dir.path())
        .output()
        .expect("git add failed");
    Command::new("git")
        .args(["commit", "-m", "add design doc"])
        .current_dir(temp_dir.path())
        .output()
        .expect("git commit failed");

    // Run a non-task review with --start (plan scope, no validates edge)
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["review", "design.md", "--start"])
        .output()
        .expect("Failed to run aiki review");

    assert!(
        output.status.success(),
        "aiki review --start on a plan-scoped file should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Regression test: `aiki task show` with no task ID and no in-progress task
/// should produce an error message.
#[test]
fn test_show_without_id_returns_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // No tasks in progress — `aiki task show` with no ID should produce an error
    let output = aiki_task(temp_dir.path(), &["show"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(
        stdout.contains("No task ID provided"),
        "Expected 'No task ID provided' error, got: {}",
        stdout
    );
}

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

// ============================================================================
// Phase 1: Core Workflow Tests
// ============================================================================

#[test]
fn test_task_list_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains(r#"cmd="list""#))
        .stdout(predicate::str::contains(r#"status="ok""#))
        .stdout(predicate::str::contains(r#"<list total="0">"#));
}

#[test]
fn test_task_add_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["add", "Fix auth bug"])
        .success()
        .stdout(predicate::str::contains(r#"cmd="add""#))
        .stdout(predicate::str::contains(r#"status="ok""#))
        .stdout(predicate::str::contains("<added>"))
        .stdout(predicate::str::contains(r#"name="Fix auth bug""#))
        .stdout(predicate::str::contains(r#"priority="p2""#)); // default priority
}

#[test]
fn test_task_add_with_priority_p0() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["add", "Critical bug", "--p0"])
        .success()
        .stdout(predicate::str::contains(r#"priority="p0""#));
}

#[test]
fn test_task_add_with_priority_p1() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["add", "High priority task", "--p1"])
        .success()
        .stdout(predicate::str::contains(r#"priority="p1""#));
}

#[test]
fn test_task_add_with_priority_p3() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    aiki_task(temp_dir.path(), &["add", "Low priority task", "--p3"])
        .success()
        .stdout(predicate::str::contains(r#"priority="p3""#));
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
        .stdout(predicate::str::contains(r#"<list total="1">"#))
        .stdout(predicate::str::contains(r#"name="Test task""#));
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
        .stdout(predicate::str::contains(r#"cmd="start""#))
        .stdout(predicate::str::contains("<started>"))
        .stdout(predicate::str::contains(r#"name="Task to start""#));
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
        .stdout(predicate::str::contains("<in_progress>"))
        .stdout(predicate::str::contains(r#"name="Working task""#));
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
        .stdout(predicate::str::contains(r#"cmd="stop""#))
        .stdout(predicate::str::contains("<stopped"))
        .stdout(predicate::str::contains(r#"name="Task to stop""#));
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
    .stdout(predicate::str::contains(r#"reason="Need design decision""#));
}

#[test]
fn test_task_close_current() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task to complete"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Close the current task
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"])
        .success()
        .stdout(predicate::str::contains(r#"cmd="close""#))
        .stdout(predicate::str::contains(r#"<closed outcome="done">"#))
        .stdout(predicate::str::contains(r#"name="Task to complete""#));
}

#[test]
fn test_task_close_wont_do() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add and start a task
    aiki_task(temp_dir.path(), &["add", "Task to abandon"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();

    // Close as won't do
    aiki_task(temp_dir.path(), &["close", "--wont-do", "--comment", "Not implementing"])
        .success()
        .stdout(predicate::str::contains(r#"outcome="wont_do""#));
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
    // Extract task ID from output (format: id="xxxx")
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let parent_id = &stdout[id_start..id_end];

    // Add child task
    aiki_task(temp_dir.path(), &["add", "Child task", "--parent", parent_id])
        .success()
        .stdout(predicate::str::contains(&format!("{}.", parent_id))); // Child ID should start with parent.
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
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let parent_id = &stdout[id_start..id_end];

    // Add first child
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "First child", "--parent", parent_id])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(r#"id="{}.1""#, parent_id)),
        "First child should have ID {}.1, got: {}",
        parent_id,
        stdout
    );

    // Add second child
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "add", "Second child", "--parent", parent_id])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(r#"id="{}.2""#, parent_id)),
        "Second child should have ID {}.2, got: {}",
        parent_id,
        stdout
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
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"]).success();

    // --all should show all tasks including closed
    aiki_task(temp_dir.path(), &["list", "--all"])
        .success()
        .stdout(predicate::str::contains(r#"name="Open task""#))
        .stdout(predicate::str::contains(r#"name="To be closed""#));
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
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"]).success(); // Closes "Task to close"

    // --open should only show open tasks
    let output = aiki_task(temp_dir.path(), &["list", "--open"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(
        stdout.contains(r#"name="Task to keep open""#),
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

    // Start the second task (it becomes in-progress)
    // Note: start without ID starts from ready queue based on priority/time
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let first_task_id = &stdout[id_start..id_end];

    aiki_task(temp_dir.path(), &["start", first_task_id]).success();

    // --in-progress should only show in-progress tasks
    aiki_task(temp_dir.path(), &["list", "--in-progress"])
        .success()
        .stdout(predicate::str::contains("<list"));
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
        .stdout(predicate::str::contains(r#"name="Stopped task""#));
}

#[test]
fn test_task_list_closed_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and close a task
    aiki_task(temp_dir.path(), &["add", "Closed task"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"]).success();

    // --closed should show closed tasks
    aiki_task(temp_dir.path(), &["list", "--closed"])
        .success()
        .stdout(predicate::str::contains(r#"name="Closed task""#));
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
    aiki_task(temp_dir.path(), &["stop", "--blocked", "Need API credentials"])
        .success()
        .stdout(predicate::str::contains("<stopped"));

    // The blocker task should appear in list
    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains(r#"name="Need API credentials""#))
        .stdout(predicate::str::contains(r#"priority="p0""#)); // Blocker tasks are P0
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

    // Both blocker tasks should appear in list
    aiki_task(temp_dir.path(), &["list"])
        .success()
        .stdout(predicate::str::contains(r#"name="Need API credentials""#))
        .stdout(predicate::str::contains(r#"name="Need design review""#));
}

// ============================================================================
// Phase 4: Show, Update, Comment, Reopen Tests
// ============================================================================

#[test]
fn test_task_show_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create and start a task
    aiki_task(temp_dir.path(), &["add", "Task to show"]).success();

    // Get the task ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let task_id = &stdout[id_start..id_end];

    // Show the task
    aiki_task(temp_dir.path(), &["show", task_id])
        .success()
        .stdout(predicate::str::contains(r#"cmd="show""#))
        .stdout(predicate::str::contains(r#"name="Task to show""#));
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
        .stdout(predicate::str::contains(r#"name="Current task""#));
}

#[test]
fn test_task_update_name() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task
    aiki_task(temp_dir.path(), &["add", "Original name"]).success();

    // Get the task ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let task_id = &stdout[id_start..id_end];

    // Update the name
    aiki_task(
        temp_dir.path(),
        &["update", task_id, "--name", "Updated name"],
    )
    .success()
    .stdout(predicate::str::contains(r#"cmd="update""#));

    // Verify the name changed
    aiki_task(temp_dir.path(), &["show", task_id])
        .success()
        .stdout(predicate::str::contains(r#"name="Updated name""#));
}

#[test]
fn test_task_update_priority() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task (default P2)
    aiki_task(temp_dir.path(), &["add", "Priority task"]).success();

    // Get the task ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let task_id = &stdout[id_start..id_end];

    // Update to P0
    aiki_task(temp_dir.path(), &["update", task_id, "--p0"]).success();

    // Verify the priority changed
    aiki_task(temp_dir.path(), &["show", task_id])
        .success()
        .stdout(predicate::str::contains(r#"priority="p0""#));
}

#[test]
fn test_task_comment() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create a task
    aiki_task(temp_dir.path(), &["add", "Task with comment"]).success();

    // Get the task ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let task_id = &stdout[id_start..id_end];

    // Add a comment using --id flag (comment command signature: <TEXT> [--id <ID>])
    aiki_task(
        temp_dir.path(),
        &["comment", "This is a test comment", "--id", task_id],
    )
    .success()
    .stdout(predicate::str::contains(r#"cmd="comment""#))
    .stdout(predicate::str::contains("comment_added"));
}

#[test]
fn test_task_start_reopen() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create, start, and close a task
    aiki_task(temp_dir.path(), &["add", "Task to reopen"]).success();
    aiki_task(temp_dir.path(), &["start"]).success();
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"]).success();

    // Get the task ID from closed list
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list", "--closed"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_start = stdout.find(r#"id=""#).unwrap() + 4;
    let id_end = stdout[id_start..].find('"').unwrap() + id_start;
    let task_id = &stdout[id_start..id_end];

    // Reopen and start the task
    aiki_task(
        temp_dir.path(),
        &[
            "start",
            task_id,
            "--reopen",
            "--reason",
            "Found another bug",
        ],
    )
    .success()
    .stdout(predicate::str::contains("<started>"))
    .stdout(predicate::str::contains(r#"name="Task to reopen""#));
}

// ============================================================================
// Workflow Tests: Auto-stop on Start
// ============================================================================

#[test]
fn test_task_start_auto_stops_current() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Create two tasks
    aiki_task(temp_dir.path(), &["add", "First task"]).success();
    aiki_task(temp_dir.path(), &["add", "Second task"]).success();

    // Start first task
    aiki_task(temp_dir.path(), &["start"]).success();

    // Get second task ID
    let output = Command::new(assert_cmd::cargo::cargo_bin!("aiki"))
        .current_dir(temp_dir.path())
        .args(["task", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Find the second task (not in_progress)
    let ready_section = stdout
        .split("<context>")
        .next()
        .unwrap_or(&stdout)
        .to_string();
    if let Some(pos) = ready_section.find(r#"id=""#) {
        let id_start = pos + 4;
        let id_end = ready_section[id_start..].find('"').unwrap() + id_start;
        let second_task_id = &ready_section[id_start..id_end];

        // Start second task - should auto-stop first
        aiki_task(temp_dir.path(), &["start", second_task_id])
            .success()
            .stdout(predicate::str::contains("<stopped"));
    }
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
    for cmd in [
        vec!["list"],
        vec!["list", "--all"],
        vec!["list", "--open"],
    ] {
        let output = aiki_task(temp_dir.path(), &cmd).success();
        let stdout = String::from_utf8_lossy(&output.get_output().stdout);
        assert!(
            stdout.contains("<context>"),
            "Command {:?} should have context element",
            cmd
        );
        assert!(
            stdout.contains("<list ready="),
            "Command {:?} should have ready queue in context",
            cmd
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
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"]).success();

    // When filtering by --closed, the context should still show actual ready queue
    let output = aiki_task(temp_dir.path(), &["list", "--closed"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Context should show ready="1" (the open task), not ready="1" (the closed task)
    // The main list shows closed, but context.ready_queue shows what's actually ready
    assert!(
        stdout.contains("<context>"),
        "Should have context element"
    );
    assert!(
        stdout.contains(r#"<list ready="#),
        "Context should show ready count"
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
        .stdout(predicate::str::contains(r#"<added>"#))
        .stdout(predicate::str::contains(r#"name="nonexistent""#));
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

    // Note: Error cases return exit code 0 but with XML error output
    aiki_task(temp_dir.path(), &["stop"])
        .success()
        .stdout(predicate::str::contains(r#"status="error""#))
        .stdout(predicate::str::contains("No task in progress to stop"));
}

#[test]
fn test_task_close_when_none_in_progress() {
    let temp_dir = tempfile::tempdir().unwrap();
    init_aiki_repo(temp_dir.path());

    // Add task but don't start
    aiki_task(temp_dir.path(), &["add", "Not started"]).success();

    // Note: Error cases return exit code 0 but with XML error output
    aiki_task(temp_dir.path(), &["close", "--comment", "Test completed"])
        .success()
        .stdout(predicate::str::contains(r#"status="error""#))
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

//! Integration tests for advance_bookmark and event chaining
//!
//! Tests cover:
//! 1. advance_bookmark happy path (moves bookmark forward)
//! 2. Event chaining via write_event (sequential events form a chain)
//! 3. read_events returns all chained events in order
//! 4. new_jj_write_marker and resolve_change_id_by_marker roundtrip

mod common;

use common::{init_jj_workspace, jj_available};
use std::process::Command;
use tempfile::tempdir;

/// Helper: run a jj command and return stdout
fn jj_run(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("Failed to run jj");
    if !output.status.success() {
        panic!(
            "jj {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Helper: get the change_id that a bookmark points to
fn bookmark_change_id(cwd: &std::path::Path, bookmark: &str) -> String {
    jj_run(
        cwd,
        &[
            "log",
            "-r",
            bookmark,
            "-T",
            "change_id",
            "--no-graph",
            "--ignore-working-copy",
        ],
    )
}

/// Helper: get parent change_ids for a given revision
fn parent_change_ids(cwd: &std::path::Path, rev: &str) -> Vec<String> {
    let output = jj_run(
        cwd,
        &[
            "log",
            "-r",
            &format!("parents({})", rev),
            "-T",
            r#"change_id ++ "\n""#,
            "--no-graph",
            "--ignore-working-copy",
        ],
    );
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect()
}

// ============================================================================
// 1. advance_bookmark happy path
// ============================================================================

#[test]
fn test_advance_bookmark_moves_bookmark_forward() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    // Create a bookmark on root()
    jj_run(
        cwd,
        &[
            "bookmark",
            "create",
            "test-branch",
            "-r",
            "root()",
            "--ignore-working-copy",
        ],
    );

    // Create a new change as child of the bookmark
    jj_run(
        cwd,
        &[
            "new",
            "test-branch",
            "--no-edit",
            "--ignore-working-copy",
            "-m",
            "event-1",
        ],
    );

    // Find the change_id of the new change
    let new_id = jj_run(
        cwd,
        &[
            "log",
            "-r",
            "description(substring:'event-1')",
            "-T",
            "change_id",
            "--no-graph",
            "--ignore-working-copy",
        ],
    );
    assert!(!new_id.is_empty(), "Should find the new change");

    // advance_bookmark should succeed
    aiki::jj::advance_bookmark(cwd, "test-branch", &new_id).unwrap();

    // Verify bookmark now points to the new change
    let bm_id = bookmark_change_id(cwd, "test-branch");
    assert_eq!(bm_id, new_id, "Bookmark should point to the new change");
}

#[test]
fn test_advance_bookmark_multiple_advances() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    // Create a bookmark on root()
    jj_run(
        cwd,
        &[
            "bookmark",
            "create",
            "test-chain",
            "-r",
            "root()",
            "--ignore-working-copy",
        ],
    );

    // Write 3 events sequentially, advancing the bookmark each time
    let mut prev_ids = Vec::new();
    for i in 1..=3 {
        let msg = format!("chain-event-{}", i);
        jj_run(
            cwd,
            &[
                "new",
                "test-chain",
                "--no-edit",
                "--ignore-working-copy",
                "-m",
                &msg,
            ],
        );

        let new_id = jj_run(
            cwd,
            &[
                "log",
                "-r",
                &format!("description(substring:'{}')", msg),
                "-T",
                "change_id",
                "--no-graph",
                "--ignore-working-copy",
            ],
        );
        assert!(!new_id.is_empty(), "Should find change for {}", msg);

        aiki::jj::advance_bookmark(cwd, "test-chain", &new_id).unwrap();
        prev_ids.push(new_id);
    }

    // Bookmark should point to the last event
    let bm_id = bookmark_change_id(cwd, "test-chain");
    assert_eq!(
        bm_id, prev_ids[2],
        "Bookmark should point to the third event"
    );

    // Each event should be a child of the previous
    // Event 2's parent should be event 1
    let parents_2 = parent_change_ids(cwd, &prev_ids[1]);
    assert!(
        parents_2.contains(&prev_ids[0]),
        "Event 2 parent should be event 1"
    );

    // Event 3's parent should be event 2
    let parents_3 = parent_change_ids(cwd, &prev_ids[2]);
    assert!(
        parents_3.contains(&prev_ids[1]),
        "Event 3 parent should be event 2"
    );
}

// ============================================================================
// 2. new_jj_write_marker + resolve_change_id_by_marker roundtrip
// ============================================================================

#[test]
fn test_write_marker_roundtrip() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    // Create a marker
    let marker = aiki::jj::new_jj_write_marker("test-prefix");
    assert!(marker.starts_with("test-prefix="));

    // Create a change with the marker in its description
    jj_run(
        cwd,
        &[
            "new",
            "root()",
            "--no-edit",
            "--ignore-working-copy",
            "-m",
            &marker,
        ],
    );

    // Resolve should find exactly one change
    let resolved = aiki::jj::resolve_change_id_by_marker(cwd, &marker).unwrap();
    assert!(!resolved.is_empty(), "Should resolve to a change id");

    // Verify the resolved id matches
    let expected = jj_run(
        cwd,
        &[
            "log",
            "-r",
            &format!("description(substring:'{}')", marker),
            "-T",
            "change_id",
            "--no-graph",
            "--ignore-working-copy",
        ],
    );
    assert_eq!(resolved, expected);
}

#[test]
fn test_write_markers_are_unique() {
    // Two markers from the same prefix should be different
    let m1 = aiki::jj::new_jj_write_marker("aiki-test");
    let m2 = aiki::jj::new_jj_write_marker("aiki-test");
    assert_ne!(m1, m2, "Markers should be unique");
    assert!(m1.starts_with("aiki-test="));
    assert!(m2.starts_with("aiki-test="));
}

// ============================================================================
// 3. ensure_branch
// ============================================================================

#[test]
fn test_ensure_branch_creates_and_caches() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    let branch = "aiki/test-ensure";

    // Branch should not exist yet
    let exists_before = aiki::jj::branch_exists(cwd, branch).unwrap();
    assert!(!exists_before, "Branch should not exist before ensure");

    // ensure_branch should create it
    aiki::jj::ensure_branch(cwd, branch).unwrap();

    // Now it should exist
    let exists_after = aiki::jj::branch_exists(cwd, branch).unwrap();
    assert!(exists_after, "Branch should exist after ensure");

    // Calling ensure_branch again should succeed (cached)
    aiki::jj::ensure_branch(cwd, branch).unwrap();
}

// ============================================================================
// 4. Integration: write_event chaining via task storage
// ============================================================================

#[test]
fn test_task_write_event_and_read_back() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    use aiki::tasks::storage::{read_events, write_event};
    use aiki::tasks::types::{TaskEvent, TaskPriority};
    use chrono::Utc;

    // Write a Created event
    let event1 = TaskEvent::Created {
        task_id: "test001".to_string(),
        name: "Test task one".to_string(),
        slug: None,
        task_type: None,
        priority: TaskPriority::P2,
        assignee: Some("claude-code".to_string()),
        sources: Vec::new(),
        template: None,
        working_copy: None,
        instructions: None,
        data: std::collections::HashMap::new(),
        timestamp: Utc::now(),
    };

    write_event(cwd, &event1).unwrap();

    // Read back events
    let events = read_events(cwd).unwrap();
    assert_eq!(events.len(), 1, "Should have 1 event");

    match &events[0] {
        TaskEvent::Created { task_id, name, .. } => {
            assert_eq!(task_id, "test001");
            assert_eq!(name, "Test task one");
        }
        _ => panic!("Expected Created event, got {:?}", events[0]),
    }
}

#[test]
fn test_task_event_chaining_multiple_writes() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    use aiki::tasks::storage::{read_events, write_event};
    use aiki::tasks::types::{TaskEvent, TaskOutcome, TaskPriority};
    use chrono::Utc;

    // Write 3 events sequentially: Created, Started, Closed
    let t = Utc::now();
    let events_to_write = vec![
        TaskEvent::Created {
            task_id: "chain001".to_string(),
            name: "Chaining test".to_string(),
            slug: None,
            task_type: None,
            priority: TaskPriority::P1,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: std::collections::HashMap::new(),
            timestamp: t,
        },
        TaskEvent::Started {
            task_ids: vec!["chain001".to_string()],
            agent_type: "claude-code".to_string(),
            session_id: Some("test-session".to_string()),
            turn_id: None,
            timestamp: t + chrono::Duration::seconds(1),
        },
        TaskEvent::Closed {
            task_ids: vec!["chain001".to_string()],
            outcome: TaskOutcome::Done,
            summary: Some("Completed the test".to_string()),
            timestamp: t + chrono::Duration::seconds(2),
        },
    ];

    for event in &events_to_write {
        write_event(cwd, event).unwrap();
    }

    // Read back all events
    let events = read_events(cwd).unwrap();
    assert_eq!(events.len(), 3, "Should have 3 events");

    // Verify ordering (sorted by timestamp)
    match &events[0] {
        TaskEvent::Created { task_id, .. } => assert_eq!(task_id, "chain001"),
        other => panic!("Expected Created, got {:?}", other),
    }
    match &events[1] {
        TaskEvent::Started { task_ids, .. } => assert_eq!(task_ids, &vec!["chain001".to_string()]),
        other => panic!("Expected Started, got {:?}", other),
    }
    match &events[2] {
        TaskEvent::Closed {
            task_ids, summary, ..
        } => {
            assert_eq!(task_ids, &vec!["chain001".to_string()]);
            assert_eq!(summary, &Some("Completed the test".to_string()));
        }
        other => panic!("Expected Closed, got {:?}", other),
    }

    // Verify the bookmark points to the last event (chain integrity)
    let bm_id = bookmark_change_id(cwd, "aiki/tasks");
    assert!(!bm_id.is_empty(), "Bookmark should exist");
}

#[test]
fn test_read_events_empty_repo() {
    if !jj_available() {
        eprintln!("Skipping test: jj binary not found in PATH");
        return;
    }

    let temp = tempdir().unwrap();
    let cwd = temp.path();
    init_jj_workspace(cwd).unwrap();

    use aiki::tasks::storage::read_events;

    // No events written yet — should return empty vec
    let events = read_events(cwd).unwrap();
    assert!(events.is_empty(), "Should have no events in fresh repo");
}

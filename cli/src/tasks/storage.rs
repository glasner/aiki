//! Event storage on aiki/tasks branch
//!
//! Tasks are stored as fileless JJ changes on the `aiki/tasks` branch.
//! Each event is a JJ change with metadata in the description.

use crate::error::{AikiError, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;

use super::types::{TaskEvent, TaskOutcome, TaskPriority};

const TASKS_BRANCH: &str = "aiki/tasks";
const METADATA_START: &str = "[aiki-task]";
const METADATA_END: &str = "[/aiki-task]";

/// Ensure the aiki/tasks branch exists
pub fn ensure_tasks_branch(cwd: &Path) -> Result<()> {
    // Check if branch exists by listing bookmarks
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "list", "--all"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);

    if !bookmarks.contains(TASKS_BRANCH) {
        // Create the branch as an orphan (no parent) starting from root()
        let result = Command::new("jj")
            .current_dir(cwd)
            .args(["bookmark", "create", TASKS_BRANCH, "-r", "root()"])
            .output()
            .map_err(|e| {
                AikiError::TaskBranchInitFailed(format!("Failed to create bookmark: {}", e))
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(AikiError::TaskBranchInitFailed(stderr.to_string()));
        }
    }
    Ok(())
}

/// Write a task event to the aiki/tasks branch
///
/// Uses `jj new --no-edit` to create the event change without affecting the working copy.
pub fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;

    let metadata = event_to_metadata_block(event);

    // Create a new change as child of aiki/tasks WITHOUT switching working copy
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "-m", &metadata])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to create task event: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to write task event: {}",
            stderr
        )));
    }

    // Move the bookmark forward to point at the newly created change
    // Filter to only the task change (has [aiki-task] in description), not the working copy
    let result = Command::new("jj")
        .current_dir(cwd)
        .args([
            "bookmark",
            "set",
            TASKS_BRANCH,
            "-r",
            &format!(
                "children({}) & description(substring:\"{}\")",
                TASKS_BRANCH, METADATA_START
            ),
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to update bookmark: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to update task bookmark: {}",
            stderr
        )));
    }

    Ok(())
}

/// Read all task events from the aiki/tasks branch
pub fn read_events(cwd: &Path) -> Result<Vec<TaskEvent>> {
    // Check if branch exists first
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "list", "--all"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(TASKS_BRANCH) {
        // Branch doesn't exist yet, return empty list
        return Ok(Vec::new());
    }

    // Read all changes on the branch, oldest first
    // Using `root()..aiki/tasks` to get ancestors of bookmark (excluding root)
    // This gives us the linear chain of task events
    let output = Command::new("jj")
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!("root()..{}", TASKS_BRANCH),
            "--no-graph",
            "-T",
            "description ++ \"\\n---EVENT-SEPARATOR---\\n\"",
            "--reversed",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to read task events: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to read task events: {}",
            stderr
        )));
    }

    let descriptions = String::from_utf8_lossy(&output.stdout);
    let mut events = Vec::new();

    // Split by our separator and parse each description
    for desc in descriptions.split("---EVENT-SEPARATOR---") {
        let desc = desc.trim();
        if desc.is_empty() {
            continue;
        }

        // Look for metadata block
        if let Some(start_idx) = desc.find(METADATA_START) {
            if let Some(end_idx) = desc.find(METADATA_END) {
                let block = &desc[start_idx + METADATA_START.len()..end_idx];
                if let Some(event) = parse_metadata_block(block) {
                    events.push(event);
                }
            }
        }
    }

    Ok(events)
}

/// Escape a string value for metadata storage
/// Encodes characters that would break key=value parsing: %, =, \n, \r
fn escape_metadata_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => result.push_str("%25"),
            '=' => result.push_str("%3D"),
            '\n' => result.push_str("%0A"),
            '\r' => result.push_str("%0D"),
            _ => result.push(c),
        }
    }
    result
}

/// Unescape a metadata value
fn unescape_metadata_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            // Read two hex characters
            let hex: String = chars.by_ref().take(2).collect();
            match hex.as_str() {
                "25" => result.push('%'),
                "3D" | "3d" => result.push('='),
                "0A" | "0a" => result.push('\n'),
                "0D" | "0d" => result.push('\r'),
                _ => {
                    // Unknown escape, keep as-is
                    result.push('%');
                    result.push_str(&hex);
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Helper to add metadata field (for safe values like task_id, event type)
fn add_metadata(key: &str, value: impl std::fmt::Display, lines: &mut Vec<String>) {
    lines.push(format!("{}={}", key, value));
}

/// Helper to add metadata field with escaping (for user-provided text)
fn add_metadata_escaped(key: &str, value: &str, lines: &mut Vec<String>) {
    lines.push(format!("{}={}", key, escape_metadata_value(value)));
}

/// Helper to add timestamp metadata field
fn add_metadata_timestamp(timestamp: &chrono::DateTime<chrono::Utc>, lines: &mut Vec<String>) {
    add_metadata("timestamp", timestamp.to_rfc3339(), lines);
}

/// Convert a TaskEvent to a metadata block string
fn event_to_metadata_block(event: &TaskEvent) -> String {
    let mut lines = vec![METADATA_START.to_string()];

    match event {
        TaskEvent::Created {
            task_id,
            name,
            priority,
            assignee,
            sources,
            timestamp,
        } => {
            add_metadata("event", "created", &mut lines);
            add_metadata("task_id", task_id, &mut lines);
            add_metadata_escaped("name", name, &mut lines);
            add_metadata("priority", priority, &mut lines);
            if let Some(assignee) = assignee {
                add_metadata("assignee", assignee, &mut lines);
            }
            // Add source= lines (one per source)
            for source in sources {
                add_metadata("source", source, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::Started {
            task_ids,
            agent_type,
            session_id,
            timestamp,
            stopped,
        } => {
            add_metadata("event", "started", &mut lines);
            for task_id in task_ids {
                add_metadata("task_id", task_id, &mut lines);
            }
            add_metadata("agent_type", agent_type, &mut lines);
            if let Some(sid) = session_id {
                add_metadata("session_id", sid, &mut lines);
            }
            for stopped_id in stopped {
                add_metadata("stopped_task", stopped_id, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::Stopped {
            task_ids,
            reason,
            blocked_reason,
            timestamp,
        } => {
            add_metadata("event", "stopped", &mut lines);
            for task_id in task_ids {
                add_metadata("task_id", task_id, &mut lines);
            }
            if let Some(reason) = reason {
                add_metadata_escaped("reason", reason, &mut lines);
            }
            if let Some(blocked) = blocked_reason {
                add_metadata_escaped("blocked_reason", blocked, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::Closed {
            task_ids,
            outcome,
            timestamp,
        } => {
            add_metadata("event", "closed", &mut lines);
            for task_id in task_ids {
                add_metadata("task_id", task_id, &mut lines);
            }
            add_metadata("outcome", outcome, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::Reopened {
            task_id,
            reason,
            timestamp,
        } => {
            add_metadata("event", "reopened", &mut lines);
            add_metadata("task_id", task_id, &mut lines);
            add_metadata_escaped("reason", reason, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::CommentAdded {
            task_ids,
            text,
            timestamp,
        } => {
            add_metadata("event", "comment_added", &mut lines);
            for task_id in task_ids {
                add_metadata("task_id", task_id, &mut lines);
            }
            add_metadata_escaped("text", text, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        TaskEvent::Updated {
            task_id,
            name,
            priority,
            assignee,
            timestamp,
        } => {
            add_metadata("event", "updated", &mut lines);
            add_metadata("task_id", task_id, &mut lines);
            if let Some(name) = name {
                add_metadata_escaped("name", name, &mut lines);
            }
            if let Some(priority) = priority {
                add_metadata("priority", priority, &mut lines);
            }
            // Serialize assignee: Some(Some(a)) = "assignee=<value>", Some(None) = "assignee="
            if let Some(assignee_value) = assignee {
                if let Some(ref a) = assignee_value {
                    add_metadata("assignee", a, &mut lines);
                } else {
                    add_metadata("assignee", "", &mut lines); // Explicit unassign
                }
            }
            // If assignee is None, we don't write anything (no change)
            add_metadata_timestamp(timestamp, &mut lines);
        }
    }

    lines.push(METADATA_END.to_string());
    lines.join("\n")
}

/// Parse a metadata block into a TaskEvent
fn parse_metadata_block(block: &str) -> Option<TaskEvent> {
    let mut fields: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();

    // Collect all values for each key (to handle multiple task_id= lines)
    for line in block.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            fields
                .entry(key.trim())
                .or_insert_with(Vec::new)
                .push(value.trim());
        }
    }

    let event_type = fields.get("event")?.first()?;
    let timestamp = fields
        .get("timestamp")
        .and_then(|v| v.first())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    match *event_type {
        "created" => {
            let task_id = fields.get("task_id")?.first()?.to_string();
            let name = unescape_metadata_value(fields.get("name")?.first()?);
            let priority = fields
                .get("priority")
                .and_then(|v| v.first())
                .and_then(|s| TaskPriority::from_str(s))
                .unwrap_or_default();
            let assignee = fields
                .get("assignee")
                .and_then(|v| v.first())
                .map(|s| s.to_string());
            // Parse sources (multiple source= lines)
            let sources = fields
                .get("source")
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_else(Vec::new);

            Some(TaskEvent::Created {
                task_id,
                name,
                priority,
                assignee,
                sources,
                timestamp,
            })
        }
        "started" => {
            let task_ids = fields
                .get("task_id")?
                .iter()
                .map(|s| s.to_string())
                .collect();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .unwrap_or(&"unknown")
                .to_string();
            let session_id = fields
                .get("session_id")
                .and_then(|v| v.first())
                .map(|s| s.to_string());
            let stopped = fields
                .get("stopped_task")
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_else(Vec::new);

            Some(TaskEvent::Started {
                task_ids,
                agent_type,
                session_id,
                timestamp,
                stopped,
            })
        }
        "stopped" => {
            let task_ids = fields
                .get("task_id")?
                .iter()
                .map(|s| s.to_string())
                .collect();
            let reason = fields
                .get("reason")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));
            let blocked_reason = fields
                .get("blocked_reason")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));

            Some(TaskEvent::Stopped {
                task_ids,
                reason,
                blocked_reason,
                timestamp,
            })
        }
        "closed" => {
            let task_ids = fields
                .get("task_id")?
                .iter()
                .map(|s| s.to_string())
                .collect();
            let outcome = fields
                .get("outcome")
                .and_then(|v| v.first())
                .and_then(|s| TaskOutcome::from_str(s))
                .unwrap_or(TaskOutcome::Done);

            Some(TaskEvent::Closed {
                task_ids,
                outcome,
                timestamp,
            })
        }
        "reopened" => {
            let task_id = fields.get("task_id")?.first()?.to_string();
            let reason = unescape_metadata_value(fields.get("reason")?.first()?);

            Some(TaskEvent::Reopened {
                task_id,
                reason,
                timestamp,
            })
        }
        "comment_added" => {
            let task_ids = fields
                .get("task_id")?
                .iter()
                .map(|s| s.to_string())
                .collect();
            let text = unescape_metadata_value(fields.get("text")?.first()?);

            Some(TaskEvent::CommentAdded {
                task_ids,
                text,
                timestamp,
            })
        }
        "updated" => {
            let task_id = fields.get("task_id")?.first()?.to_string();
            let name = fields
                .get("name")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));
            let priority = fields
                .get("priority")
                .and_then(|v| v.first())
                .and_then(|s| TaskPriority::from_str(s));
            // Parse assignee: absent=None, empty=Some(None), value=Some(Some(value))
            let assignee = fields.get("assignee").map(|v| {
                let value = v.first().map(|s| *s).unwrap_or("");
                if value.is_empty() {
                    None  // Unassign
                } else {
                    Some(value.to_string())  // Assign
                }
            });

            Some(TaskEvent::Updated {
                task_id,
                name,
                priority,
                assignee,
                timestamp,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_to_metadata_block_created() {
        let event = TaskEvent::Created {
            task_id: "a1b2".to_string(),
            name: "Fix auth bug".to_string(),
            priority: TaskPriority::P2,
            assignee: Some("claude-code".to_string()),
            sources: Vec::new(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-task]"));
        assert!(block.contains("event=created"));
        assert!(block.contains("task_id=a1b2"));
        assert!(block.contains("name=Fix auth bug"));
        assert!(block.contains("priority=p2"));
        assert!(block.contains("assignee=claude-code"));
        assert!(block.contains("[/aiki-task]"));
    }

    #[test]
    fn test_parse_metadata_block_created() {
        let block = r#"
event=created
task_id=a1b2
name=Fix auth bug
priority=p2
assignee=claude-code
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Created {
                task_id,
                name,
                priority,
                assignee,
                ..
            } => {
                assert_eq!(task_id, "a1b2");
                assert_eq!(name, "Fix auth bug");
                assert_eq!(priority, TaskPriority::P2);
                assert_eq!(assignee, Some("claude-code".to_string()));
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_started() {
        let block = r#"
event=started
task_id=a1b2
task_id=c3d4
agent_type=claude-code
stopped_task=e5f6
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Started {
                task_ids,
                agent_type,
                stopped,
                ..
            } => {
                assert_eq!(task_ids, vec!["a1b2", "c3d4"]);
                assert_eq!(agent_type, "claude-code");
                assert_eq!(stopped, vec!["e5f6"]);
            }
            _ => panic!("Expected Started event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_stopped() {
        let block = r#"
event=stopped
task_id=a1b2
task_id=c3d4
reason=Need design decision
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Stopped {
                task_ids, reason, ..
            } => {
                assert_eq!(task_ids, vec!["a1b2", "c3d4"]);
                assert_eq!(reason, Some("Need design decision".to_string()));
            }
            _ => panic!("Expected Stopped event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_closed() {
        let block = r#"
event=closed
task_id=a1b2
task_id=c3d4
outcome=done
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Closed {
                task_ids, outcome, ..
            } => {
                assert_eq!(task_ids, vec!["a1b2", "c3d4"]);
                assert_eq!(outcome, TaskOutcome::Done);
            }
            _ => panic!("Expected Closed event"),
        }
    }

    #[test]
    fn test_roundtrip_created() {
        let original = TaskEvent::Created {
            task_id: "test".to_string(),
            name: "Test task".to_string(),
            priority: TaskPriority::P1,
            assignee: None,
            sources: Vec::new(),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        // Extract the content between markers
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Created {
                    task_id: id1,
                    name: name1,
                    priority: p1,
                    ..
                },
                TaskEvent::Created {
                    task_id: id2,
                    name: name2,
                    priority: p2,
                    ..
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(name1, name2);
                assert_eq!(p1, p2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_started() {
        let original = TaskEvent::Started {
            task_ids: vec!["task1".to_string(), "task2".to_string()],
            agent_type: "claude-code".to_string(),
            session_id: Some("test-session-uuid".to_string()),
            timestamp: Utc::now(),
            stopped: vec!["stopped1".to_string()],
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Started {
                    task_ids: ids1,
                    agent_type: agent1,
                    stopped: stopped1,
                    ..
                },
                TaskEvent::Started {
                    task_ids: ids2,
                    agent_type: agent2,
                    stopped: stopped2,
                    ..
                },
            ) => {
                assert_eq!(ids1, ids2);
                assert_eq!(agent1, agent2);
                assert_eq!(stopped1, stopped2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_stopped() {
        let original = TaskEvent::Stopped {
            task_ids: vec!["task1".to_string()],
            reason: Some("Need more info".to_string()),
            blocked_reason: Some("Waiting for API".to_string()),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Stopped {
                    task_ids: ids1,
                    reason: reason1,
                    blocked_reason: blocked1,
                    ..
                },
                TaskEvent::Stopped {
                    task_ids: ids2,
                    reason: reason2,
                    blocked_reason: blocked2,
                    ..
                },
            ) => {
                assert_eq!(ids1, ids2);
                assert_eq!(reason1, reason2);
                assert_eq!(blocked1, blocked2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_closed() {
        let original = TaskEvent::Closed {
            task_ids: vec!["task1".to_string(), "task2".to_string()],
            outcome: TaskOutcome::WontDo,
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Closed {
                    task_ids: ids1,
                    outcome: outcome1,
                    ..
                },
                TaskEvent::Closed {
                    task_ids: ids2,
                    outcome: outcome2,
                    ..
                },
            ) => {
                assert_eq!(ids1, ids2);
                assert_eq!(outcome1, outcome2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    // Edge case tests

    #[test]
    fn test_parse_missing_event_type() {
        let block = r#"
task_id=a1b2
name=Some task
"#;
        assert!(parse_metadata_block(block).is_none());
    }

    #[test]
    fn test_parse_unknown_event_type() {
        let block = r#"
event=unknown
task_id=a1b2
"#;
        assert!(parse_metadata_block(block).is_none());
    }

    #[test]
    fn test_parse_missing_required_fields_created() {
        // Missing task_id
        let block = r#"
event=created
name=Some task
"#;
        assert!(parse_metadata_block(block).is_none());

        // Missing name
        let block = r#"
event=created
task_id=a1b2
"#;
        assert!(parse_metadata_block(block).is_none());
    }

    #[test]
    fn test_parse_missing_timestamp_uses_default() {
        let block = r#"
event=created
task_id=a1b2
name=Some task
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Created { timestamp, .. } => {
                // Should use current time as default (within last second)
                let now = Utc::now();
                let diff = (now - timestamp).num_seconds().abs();
                assert!(diff < 2, "Timestamp should be recent");
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_invalid_priority_uses_default() {
        let block = r#"
event=created
task_id=a1b2
name=Some task
priority=invalid
timestamp=2026-01-09T10:30:00Z
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Created { priority, .. } => {
                assert_eq!(priority, TaskPriority::default()); // P2
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_whitespace_handling() {
        let block = r#"
  event = created
  task_id = a1b2
  name = Fix auth bug
  timestamp = 2026-01-09T10:30:00Z
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Created { task_id, name, .. } => {
                assert_eq!(task_id, "a1b2");
                assert_eq!(name, "Fix auth bug");
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_special_characters_in_name() {
        let block = r#"
event=created
task_id=a1b2
name=Fix <bug> & "error" 'handling'
timestamp=2026-01-09T10:30:00Z
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Created { name, .. } => {
                assert_eq!(name, r#"Fix <bug> & "error" 'handling'"#);
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_empty_block() {
        let block = "";
        assert!(parse_metadata_block(block).is_none());

        let block = "   \n\n   ";
        assert!(parse_metadata_block(block).is_none());
    }

    #[test]
    fn test_parse_started_with_no_stopped_tasks() {
        let block = r#"
event=started
task_id=a1b2
agent_type=claude-code
timestamp=2026-01-09T10:30:00Z
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Started { stopped, .. } => {
                assert!(stopped.is_empty());
            }
            _ => panic!("Expected Started event"),
        }
    }

    #[test]
    fn test_parse_stopped_with_no_reason() {
        let block = r#"
event=stopped
task_id=a1b2
timestamp=2026-01-09T10:30:00Z
"#;
        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Stopped {
                reason,
                blocked_reason,
                ..
            } => {
                assert!(reason.is_none());
                assert!(blocked_reason.is_none());
            }
            _ => panic!("Expected Stopped event"),
        }
    }

    #[test]
    fn test_escape_unescape_roundtrip() {
        let test_cases = [
            "simple text",
            "with=equals",
            "with\nnewline",
            "with\r\nwindows newline",
            "with%percent",
            "complex=value\nwith%all=special\rchars",
            "",
            "===",
            "\n\n\n",
            "100% done = success\nNext line",
        ];

        for original in &test_cases {
            let escaped = escape_metadata_value(original);
            let unescaped = unescape_metadata_value(&escaped);
            assert_eq!(
                original, &unescaped,
                "Roundtrip failed for: {:?}",
                original
            );
        }
    }

    #[test]
    fn test_escape_produces_safe_output() {
        // Escaped output should not contain newlines or unescaped equals
        let input = "key=value\nwith\rnewlines";
        let escaped = escape_metadata_value(input);

        assert!(!escaped.contains('\n'), "Should not contain newline");
        assert!(!escaped.contains('\r'), "Should not contain carriage return");
        assert!(!escaped.contains('='), "Should not contain unescaped equals");
    }

    #[test]
    fn test_roundtrip_created_with_special_chars() {
        let original = TaskEvent::Created {
            task_id: "test".to_string(),
            name: "Fix bug = critical\nSee issue #123".to_string(),
            priority: TaskPriority::P1,
            assignee: None,
            sources: Vec::new(),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        // Extract the content between markers
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Created {
                    name: name1,
                    ..
                },
                TaskEvent::Created {
                    name: name2,
                    ..
                },
            ) => {
                assert_eq!(name1, name2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_comment_with_special_chars() {
        let original = TaskEvent::CommentAdded {
            task_ids: vec!["a1b2".to_string()],
            text: "This is a comment with\nmultiple lines\nand = signs".to_string(),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::CommentAdded { text: text1, .. },
                TaskEvent::CommentAdded { text: text2, .. },
            ) => {
                assert_eq!(text1, text2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_reopened() {
        let timestamp = DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let original = TaskEvent::Reopened {
            task_id: "a1b2".to_string(),
            reason: "Found new info".to_string(),
            timestamp,
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Reopened {
                    task_id: id1,
                    reason: r1,
                    timestamp: t1,
                },
                TaskEvent::Reopened {
                    task_id: id2,
                    reason: r2,
                    timestamp: t2,
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(r1, r2);
                assert_eq!(t1, t2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_reopened_with_special_chars() {
        let original = TaskEvent::Reopened {
            task_id: "a1b2".to_string(),
            reason: "Need to fix = sign\nand newline".to_string(),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Reopened { reason: r1, .. },
                TaskEvent::Reopened { reason: r2, .. },
            ) => {
                assert_eq!(r1, r2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_updated_name_only() {
        let timestamp = DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let original = TaskEvent::Updated {
            task_id: "a1b2".to_string(),
            name: Some("New name".to_string()),
            priority: None,
            assignee: None,
            timestamp,
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Updated {
                    task_id: id1,
                    name: n1,
                    priority: p1,
                    assignee: a1,
                    timestamp: t1,
                },
                TaskEvent::Updated {
                    task_id: id2,
                    name: n2,
                    priority: p2,
                    assignee: a2,
                    timestamp: t2,
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(n1, n2);
                assert_eq!(p1, p2);
                assert_eq!(a1, a2);
                assert_eq!(t1, t2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_updated_priority_only() {
        let timestamp = DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let original = TaskEvent::Updated {
            task_id: "a1b2".to_string(),
            name: None,
            priority: Some(TaskPriority::P0),
            assignee: None,
            timestamp,
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Updated {
                    name: n1,
                    priority: p1,
                    ..
                },
                TaskEvent::Updated {
                    name: n2,
                    priority: p2,
                    ..
                },
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(p1, p2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_updated_both_fields() {
        let original = TaskEvent::Updated {
            task_id: "a1b2".to_string(),
            name: Some("Updated name".to_string()),
            priority: Some(TaskPriority::P1),
            assignee: None,
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Updated {
                    name: n1,
                    priority: p1,
                    ..
                },
                TaskEvent::Updated {
                    name: n2,
                    priority: p2,
                    ..
                },
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(p1, p2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_updated_with_special_chars() {
        let original = TaskEvent::Updated {
            task_id: "a1b2".to_string(),
            name: Some("Name = special\nwith newlines".to_string()),
            priority: None,
            assignee: None,
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find("[aiki-task]").unwrap() + "[aiki-task]".len();
        let end = block.find("[/aiki-task]").unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                TaskEvent::Updated { name: n1, .. },
                TaskEvent::Updated { name: n2, .. },
            ) => {
                assert_eq!(n1, n2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_parse_metadata_block_reopened() {
        let block = r#"
event=reopened
task_id=a1b2
reason=Found new info
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Reopened {
                task_id,
                reason,
                ..
            } => {
                assert_eq!(task_id, "a1b2");
                assert_eq!(reason, "Found new info");
            }
            _ => panic!("Expected Reopened event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_comment_added() {
        let block = r#"
event=comment_added
task_id=a1b2
text=This is a comment
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::CommentAdded { task_ids, text, .. } => {
                assert_eq!(task_ids, vec!["a1b2"]);
                assert_eq!(text, "This is a comment");
            }
            _ => panic!("Expected CommentAdded event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_updated() {
        let block = r#"
event=updated
task_id=a1b2
name=New name
priority=p0
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Updated {
                task_id,
                name,
                priority,
                ..
            } => {
                assert_eq!(task_id, "a1b2");
                assert_eq!(name, Some("New name".to_string()));
                assert_eq!(priority, Some(TaskPriority::P0));
            }
            _ => panic!("Expected Updated event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_updated_partial() {
        // Test with only name, no priority
        let block = r#"
event=updated
task_id=a1b2
name=New name only
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            TaskEvent::Updated {
                name, priority, ..
            } => {
                assert_eq!(name, Some("New name only".to_string()));
                assert_eq!(priority, None);
            }
            _ => panic!("Expected Updated event"),
        }
    }

    #[test]
    fn test_event_to_metadata_block_reopened() {
        let event = TaskEvent::Reopened {
            task_id: "a1b2".to_string(),
            reason: "New information found".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-task]"));
        assert!(block.contains("event=reopened"));
        assert!(block.contains("task_id=a1b2"));
        assert!(block.contains("reason=New information found"));
        assert!(block.contains("[/aiki-task]"));
    }

    #[test]
    fn test_event_to_metadata_block_comment_added() {
        let event = TaskEvent::CommentAdded {
            task_ids: vec!["a1b2".to_string()],
            text: "This is a comment".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-task]"));
        assert!(block.contains("event=comment_added"));
        assert!(block.contains("task_id=a1b2"));
        assert!(block.contains("text=This is a comment"));
        assert!(block.contains("[/aiki-task]"));
    }

    #[test]
    fn test_event_to_metadata_block_updated() {
        let event = TaskEvent::Updated {
            task_id: "a1b2".to_string(),
            name: Some("New task name".to_string()),
            priority: Some(TaskPriority::P1),
            assignee: None,
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-task]"));
        assert!(block.contains("event=updated"));
        assert!(block.contains("task_id=a1b2"));
        assert!(block.contains("name=New task name"));
        assert!(block.contains("priority=p1"));
        assert!(block.contains("[/aiki-task]"));
    }
}

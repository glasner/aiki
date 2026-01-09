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
pub fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;

    let metadata = event_to_metadata_block(event);

    // Generate a unique marker to identify this specific change
    let unique_marker = format!("__aiki_event_{}__", std::process::id());
    let temp_message = format!("{}\n{}", unique_marker, metadata);

    // Step 1: Create the change with our unique marker in the message
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["new", "--no-edit", TASKS_BRANCH, "-m", &temp_message])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to create task event: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to write task event: {}",
            stderr
        )));
    }

    // Step 2: Find the new change by its unique marker
    let log_output = Command::new("jj")
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            "all()",
            "--no-graph",
            "-T",
            &format!(
                "if(description.contains(\"{}\"), change_id, \"\")",
                unique_marker
            ),
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to find new change: {}", e)))?;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to find new task change: {}",
            stderr
        )));
    }

    let output = String::from_utf8_lossy(&log_output.stdout);
    let new_change_id = output.lines().find(|l| !l.is_empty());

    let new_change_id = match new_change_id {
        Some(id) => id.trim().to_string(),
        None => {
            return Err(AikiError::JjCommandFailed(
                "Could not find newly created task change".to_string(),
            ));
        }
    };

    // Step 3: Update the description to remove the unique marker
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["describe", &new_change_id, "-m", &metadata])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to describe change: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to set task event description: {}",
            stderr
        )));
    }

    // Step 4: Move the bookmark to the new change
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", &new_change_id])
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

/// Convert a TaskEvent to a metadata block string
fn event_to_metadata_block(event: &TaskEvent) -> String {
    let mut lines = vec![METADATA_START.to_string()];

    match event {
        TaskEvent::Created {
            task_id,
            name,
            priority,
            assignee,
            timestamp,
        } => {
            lines.push("event=created".to_string());
            lines.push(format!("task_id={}", task_id));
            lines.push(format!("name={}", name));
            lines.push(format!("priority={}", priority));
            if let Some(assignee) = assignee {
                lines.push(format!("assignee={}", assignee));
            }
            lines.push(format!("timestamp={}", timestamp.to_rfc3339()));
        }
        TaskEvent::Started {
            task_ids,
            agent_type,
            timestamp,
            stopped_tasks,
        } => {
            lines.push("event=started".to_string());
            for task_id in task_ids {
                lines.push(format!("task_id={}", task_id));
            }
            lines.push(format!("agent_type={}", agent_type));
            for stopped in stopped_tasks {
                lines.push(format!("stopped_task={}", stopped));
            }
            lines.push(format!("timestamp={}", timestamp.to_rfc3339()));
        }
        TaskEvent::Stopped {
            task_ids,
            reason,
            blocked_reason,
            timestamp,
        } => {
            lines.push("event=stopped".to_string());
            for task_id in task_ids {
                lines.push(format!("task_id={}", task_id));
            }
            if let Some(reason) = reason {
                lines.push(format!("reason={}", reason));
            }
            if let Some(blocked) = blocked_reason {
                lines.push(format!("blocked_reason={}", blocked));
            }
            lines.push(format!("timestamp={}", timestamp.to_rfc3339()));
        }
        TaskEvent::Closed {
            task_ids,
            outcome,
            timestamp,
        } => {
            lines.push("event=closed".to_string());
            for task_id in task_ids {
                lines.push(format!("task_id={}", task_id));
            }
            lines.push(format!("outcome={}", outcome));
            lines.push(format!("timestamp={}", timestamp.to_rfc3339()));
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
            let name = fields.get("name")?.first()?.to_string();
            let priority = fields
                .get("priority")
                .and_then(|v| v.first())
                .and_then(|s| TaskPriority::from_str(s))
                .unwrap_or_default();
            let assignee = fields
                .get("assignee")
                .and_then(|v| v.first())
                .map(|s| s.to_string());

            Some(TaskEvent::Created {
                task_id,
                name,
                priority,
                assignee,
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
            let stopped_tasks = fields
                .get("stopped_task")
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_else(Vec::new);

            Some(TaskEvent::Started {
                task_ids,
                agent_type,
                timestamp,
                stopped_tasks,
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
                .map(|s| s.to_string());
            let blocked_reason = fields
                .get("blocked_reason")
                .and_then(|v| v.first())
                .map(|s| s.to_string());

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
                stopped_tasks,
                ..
            } => {
                assert_eq!(task_ids, vec!["a1b2", "c3d4"]);
                assert_eq!(agent_type, "claude-code");
                assert_eq!(stopped_tasks, vec!["e5f6"]);
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
}

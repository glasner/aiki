//! Task engine for materializing views and calculating ready queue

use std::collections::HashMap;

use super::types::{Task, TaskEvent, TaskOutcome, TaskStatus};

/// Materialize task views from an event stream
///
/// Processes events in order and builds up the current state of each task.
#[must_use]
pub fn materialize_tasks(events: &[TaskEvent]) -> HashMap<String, Task> {
    let mut tasks: HashMap<String, Task> = HashMap::new();

    for event in events {
        match event {
            TaskEvent::Created {
                task_id,
                name,
                priority,
                assignee,
                timestamp,
            } => {
                tasks.insert(
                    task_id.clone(),
                    Task {
                        id: task_id.clone(),
                        name: name.clone(),
                        status: TaskStatus::Open,
                        priority: *priority,
                        assignee: assignee.clone(),
                        created_at: *timestamp,
                        stopped_reason: None,
                        closed_outcome: None,
                    },
                );
            }
            TaskEvent::Started { task_ids, .. } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.status = TaskStatus::InProgress;
                        task.stopped_reason = None;
                    }
                }
            }
            TaskEvent::Stopped {
                task_ids, reason, ..
            } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.status = TaskStatus::Stopped;
                        task.stopped_reason = reason.clone();
                    }
                }
            }
            TaskEvent::Closed {
                task_ids, outcome, ..
            } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.status = TaskStatus::Closed;
                        task.closed_outcome = Some(*outcome);
                    }
                }
            }
        }
    }

    tasks
}

/// Get the ready queue (open tasks sorted by priority)
///
/// Ready queue contains:
/// - Open status tasks
/// - Sorted by priority (P0 first, then P1, P2, P3)
/// - Then by creation time (oldest first)
#[must_use]
pub fn get_ready_queue(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    let mut ready: Vec<&Task> = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Open)
        .collect();

    // Sort by priority (P0 < P1 < P2 < P3), then by creation time (oldest first)
    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    ready
}

/// Get tasks currently in progress
#[must_use]
pub fn get_in_progress(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks
        .values()
        .filter(|t| t.status == TaskStatus::InProgress)
        .collect()
}

/// Get stopped tasks
#[must_use]
pub fn get_stopped(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks
        .values()
        .filter(|t| t.status == TaskStatus::Stopped)
        .collect()
}

/// Get closed tasks
#[must_use]
pub fn get_closed(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks
        .values()
        .filter(|t| t.status == TaskStatus::Closed)
        .collect()
}

/// Find a task by ID
#[must_use]
pub fn find_task<'a>(tasks: &'a HashMap<String, Task>, id: &str) -> Option<&'a Task> {
    tasks.get(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::TaskPriority;
    use chrono::{TimeZone, Utc};

    fn make_created_event(
        task_id: &str,
        name: &str,
        priority: TaskPriority,
        hours_ago: i64,
    ) -> TaskEvent {
        TaskEvent::Created {
            task_id: task_id.to_string(),
            name: name.to_string(),
            priority,
            assignee: None,
            timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
        }
    }

    fn make_started_event(task_id: &str) -> TaskEvent {
        TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: "claude-code".to_string(),
            timestamp: Utc::now(),
            stopped_tasks: Vec::new(),
        }
    }

    fn make_stopped_event(task_id: &str, reason: Option<&str>) -> TaskEvent {
        TaskEvent::Stopped {
            task_ids: vec![task_id.to_string()],
            reason: reason.map(|s| s.to_string()),
            blocked_reason: None,
            timestamp: Utc::now(),
        }
    }

    fn make_closed_event(task_id: &str, outcome: TaskOutcome) -> TaskEvent {
        TaskEvent::Closed {
            task_ids: vec![task_id.to_string()],
            outcome,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_materialize_single_task() {
        let events = vec![make_created_event("a1b2", "Test task", TaskPriority::P2, 1)];

        let tasks = materialize_tasks(&events);

        assert_eq!(tasks.len(), 1);
        let task = tasks.get("a1b2").unwrap();
        assert_eq!(task.name, "Test task");
        assert_eq!(task.status, TaskStatus::Open);
        assert_eq!(task.priority, TaskPriority::P2);
    }

    #[test]
    fn test_materialize_task_lifecycle() {
        let events = vec![
            make_created_event("a1b2", "Test task", TaskPriority::P2, 1),
            make_started_event("a1b2"),
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);

        // Add stop event
        let events = vec![
            make_created_event("a1b2", "Test task", TaskPriority::P2, 1),
            make_started_event("a1b2"),
            make_stopped_event("a1b2", Some("Need info")),
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").unwrap();
        assert_eq!(task.status, TaskStatus::Stopped);
        assert_eq!(task.stopped_reason, Some("Need info".to_string()));

        // Add close event
        let events = vec![
            make_created_event("a1b2", "Test task", TaskPriority::P2, 1),
            make_started_event("a1b2"),
            make_stopped_event("a1b2", Some("Need info")),
            make_started_event("a1b2"),
            make_closed_event("a1b2", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").unwrap();
        assert_eq!(task.status, TaskStatus::Closed);
        assert_eq!(task.closed_outcome, Some(TaskOutcome::Done));
    }

    #[test]
    fn test_ready_queue_priority_sorting() {
        let events = vec![
            make_created_event("low", "Low priority", TaskPriority::P3, 1),
            make_created_event("high", "High priority", TaskPriority::P1, 1),
            make_created_event("critical", "Critical", TaskPriority::P0, 1),
            make_created_event("normal", "Normal", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_ready_queue(&tasks);

        assert_eq!(ready.len(), 4);
        assert_eq!(ready[0].id, "critical"); // P0 first
        assert_eq!(ready[1].id, "high"); // P1 second
        assert_eq!(ready[2].id, "normal"); // P2 third
        assert_eq!(ready[3].id, "low"); // P3 last
    }

    #[test]
    fn test_ready_queue_time_sorting_same_priority() {
        let events = vec![
            make_created_event("newer", "Newer task", TaskPriority::P2, 1),
            make_created_event("older", "Older task", TaskPriority::P2, 3),
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_ready_queue(&tasks);

        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].id, "older"); // Older first (created 3 hours ago)
        assert_eq!(ready[1].id, "newer"); // Newer second (created 1 hour ago)
    }

    #[test]
    fn test_ready_queue_excludes_non_open() {
        let events = vec![
            make_created_event("open", "Open task", TaskPriority::P2, 1),
            make_created_event("started", "Started task", TaskPriority::P2, 1),
            make_started_event("started"),
            make_created_event("closed", "Closed task", TaskPriority::P2, 1),
            make_closed_event("closed", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_ready_queue(&tasks);

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "open");
    }

    #[test]
    fn test_get_in_progress() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 1),
            make_created_event("task2", "Task 2", TaskPriority::P2, 1),
            make_started_event("task1"),
        ];

        let tasks = materialize_tasks(&events);
        let in_progress = get_in_progress(&tasks);

        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0].id, "task1");
    }

    #[test]
    fn test_find_task() {
        let events = vec![make_created_event("a1b2", "Test", TaskPriority::P2, 1)];

        let tasks = materialize_tasks(&events);

        assert!(find_task(&tasks, "a1b2").is_some());
        assert!(find_task(&tasks, "nonexistent").is_none());
    }
}

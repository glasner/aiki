//! Task manager for materializing views and calculating ready queue

use std::collections::HashMap;

use super::id::{get_parent_id, is_direct_child_of};
use super::types::{Task, TaskEvent, TaskStatus};

/// Represents the set of active scopes based on in-progress tasks
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeSet {
    /// Whether root-level tasks should be included (a root task is in-progress)
    pub include_root: bool,
    /// Parent IDs of in-progress child tasks (sorted, deduplicated)
    pub scopes: Vec<String>,
}

impl ScopeSet {
    /// Check if this scope set is empty (no scopes and root not included)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.include_root && self.scopes.is_empty()
    }
}

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

/// Check if a task has any children
#[must_use]
pub fn has_children(tasks: &HashMap<String, Task>, parent_id: &str) -> bool {
    tasks.keys().any(|id| is_direct_child_of(id, parent_id))
}

/// Get direct children of a task
#[must_use]
pub fn get_children<'a>(tasks: &'a HashMap<String, Task>, parent_id: &str) -> Vec<&'a Task> {
    tasks
        .values()
        .filter(|t| is_direct_child_of(&t.id, parent_id))
        .collect()
}

/// Get the ready queue filtered by scope
///
/// When scope is None, returns only root-level tasks (no parent).
/// When scope is Some(parent_id), returns only direct children of that parent.
#[must_use]
pub fn get_scoped_ready_queue<'a>(
    tasks: &'a HashMap<String, Task>,
    scope: Option<&str>,
) -> Vec<&'a Task> {
    let mut ready: Vec<&Task> = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| match scope {
            None => get_parent_id(&t.id).is_none(), // Root-level tasks only
            Some(parent_id) => is_direct_child_of(&t.id, parent_id),
        })
        .collect();

    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    ready
}

/// Determine the current scope set based on in-progress tasks
///
/// Returns a `ScopeSet` containing:
/// - `include_root`: true if any root task is in-progress
/// - `scopes`: unique parent IDs of in-progress child tasks
#[must_use]
pub fn get_current_scope_set(tasks: &HashMap<String, Task>) -> ScopeSet {
    let in_progress = get_in_progress(tasks);

    let mut include_root = false;
    let mut scopes: Vec<String> = Vec::new();

    for task in in_progress {
        if let Some(parent_id) = get_parent_id(&task.id) {
            scopes.push(parent_id.to_string());
        } else {
            // This is a root task
            include_root = true;
        }
    }

    // Remove duplicates and sort for deterministic output
    scopes.sort();
    scopes.dedup();

    ScopeSet {
        include_root,
        scopes,
    }
}

/// Get current scopes as a Vec (for backward compatibility)
#[must_use]
pub fn get_current_scopes(tasks: &HashMap<String, Task>) -> Vec<String> {
    get_current_scope_set(tasks).scopes
}

/// Check if all children of a parent are closed
#[must_use]
pub fn all_children_closed(tasks: &HashMap<String, Task>, parent_id: &str) -> bool {
    let children = get_children(tasks, parent_id);
    !children.is_empty() && children.iter().all(|t| t.status == TaskStatus::Closed)
}

/// Get unclosed children of a parent
#[must_use]
pub fn get_unclosed_children<'a>(
    tasks: &'a HashMap<String, Task>,
    parent_id: &str,
) -> Vec<&'a Task> {
    get_children(tasks, parent_id)
        .into_iter()
        .filter(|t| t.status != TaskStatus::Closed)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskOutcome, TaskPriority};
    use chrono::Utc;

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
            stopped: Vec::new(),
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

    #[test]
    fn test_has_children() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("other", "Other", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);

        assert!(has_children(&tasks, "parent"));
        assert!(!has_children(&tasks, "parent.1"));
        assert!(!has_children(&tasks, "other"));
        assert!(!has_children(&tasks, "nonexistent"));
    }

    #[test]
    fn test_get_children() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("parent.1.1", "Grandchild", TaskPriority::P2, 1),
            make_created_event("other", "Other", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);
        let children = get_children(&tasks, "parent");

        // Should only get direct children, not grandchildren
        assert_eq!(children.len(), 2);
        let child_ids: Vec<_> = children.iter().map(|t| t.id.as_str()).collect();
        assert!(child_ids.contains(&"parent.1"));
        assert!(child_ids.contains(&"parent.2"));
        assert!(!child_ids.contains(&"parent.1.1"));
    }

    #[test]
    fn test_get_scoped_ready_queue_no_scope() {
        let events = vec![
            make_created_event("root1", "Root 1", TaskPriority::P2, 1),
            make_created_event("root2", "Root 2", TaskPriority::P1, 1),
            make_created_event("root1.1", "Child", TaskPriority::P0, 1),
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_scoped_ready_queue(&tasks, None);

        // Should only get root-level tasks, not children
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].id, "root2"); // P1 first
        assert_eq!(ready[1].id, "root1"); // P2 second
    }

    #[test]
    fn test_get_scoped_ready_queue_with_scope() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P0, 1),
            make_created_event("parent.1.1", "Grandchild", TaskPriority::P0, 1),
            make_created_event("other", "Other root", TaskPriority::P0, 1),
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_scoped_ready_queue(&tasks, Some("parent"));

        // Should only get direct children of parent
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].id, "parent.2"); // P0 first
        assert_eq!(ready[1].id, "parent.1"); // P2 second
    }

    #[test]
    fn test_get_current_scopes() {
        // No in-progress tasks -> no scopes
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child", TaskPriority::P2, 1),
        ];
        let tasks = materialize_tasks(&events);
        assert!(get_current_scopes(&tasks).is_empty());

        // In-progress root task -> no scopes
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_started_event("parent"),
        ];
        let tasks = materialize_tasks(&events);
        assert!(get_current_scopes(&tasks).is_empty());

        // In-progress child task -> scope is parent
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child", TaskPriority::P2, 1),
            make_started_event("parent.1"),
        ];
        let tasks = materialize_tasks(&events);
        assert_eq!(get_current_scopes(&tasks), vec!["parent".to_string()]);
    }

    #[test]
    fn test_get_current_scopes_multi_parent() {
        // Two in-progress children from different parents -> two scopes
        let events = vec![
            make_created_event("parent1", "Parent 1", TaskPriority::P2, 1),
            make_created_event("parent2", "Parent 2", TaskPriority::P2, 1),
            make_created_event("parent1.1", "Child 1.1", TaskPriority::P2, 1),
            make_created_event("parent2.1", "Child 2.1", TaskPriority::P2, 1),
            TaskEvent::Started {
                task_ids: vec!["parent1.1".to_string(), "parent2.1".to_string()],
                agent_type: "claude-code".to_string(),
                timestamp: Utc::now(),
                stopped: Vec::new(),
            },
        ];
        let tasks = materialize_tasks(&events);
        let scopes = get_current_scopes(&tasks);
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&"parent1".to_string()));
        assert!(scopes.contains(&"parent2".to_string()));
    }

    #[test]
    fn test_get_current_scopes_same_parent() {
        // Two in-progress children from same parent -> one scope (deduplicated)
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            TaskEvent::Started {
                task_ids: vec!["parent.1".to_string(), "parent.2".to_string()],
                agent_type: "claude-code".to_string(),
                timestamp: Utc::now(),
                stopped: Vec::new(),
            },
        ];
        let tasks = materialize_tasks(&events);
        let scopes = get_current_scopes(&tasks);
        assert_eq!(scopes, vec!["parent".to_string()]);
    }

    #[test]
    fn test_all_children_closed() {
        // No children -> returns false
        let events = vec![make_created_event("parent", "Parent", TaskPriority::P2, 1)];
        let tasks = materialize_tasks(&events);
        assert!(!all_children_closed(&tasks, "parent"));

        // Some children open -> returns false
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);
        assert!(!all_children_closed(&tasks, "parent"));

        // All children closed -> returns true
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
            make_closed_event("parent.2", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);
        assert!(all_children_closed(&tasks, "parent"));
    }

    #[test]
    fn test_get_unclosed_children() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("parent.3", "Child 3", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let unclosed = get_unclosed_children(&tasks, "parent");

        assert_eq!(unclosed.len(), 2);
        let ids: Vec<_> = unclosed.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"parent.2"));
        assert!(ids.contains(&"parent.3"));
    }

    #[test]
    fn test_scope_set_only_root() {
        // Root task in-progress → include_root=true, scopes=[]
        let events = vec![
            make_created_event("root1", "Root task", TaskPriority::P2, 1),
            make_started_event("root1"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(scope_set.include_root);
        assert!(scope_set.scopes.is_empty());
    }

    #[test]
    fn test_scope_set_root_and_child() {
        // Root task + child task both in-progress
        // → include_root=true, scopes=[parent_of_child]
        let events = vec![
            make_created_event("root1", "Root task", TaskPriority::P2, 1),
            make_created_event("parent", "Parent task", TaskPriority::P2, 2),
            make_created_event("parent.1", "Child task", TaskPriority::P2, 3),
            make_started_event("root1"),
            make_started_event("parent.1"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(scope_set.include_root);
        assert_eq!(scope_set.scopes, vec!["parent".to_string()]);
    }

    #[test]
    fn test_scope_set_only_child() {
        // Only child task in-progress → include_root=false, scopes=[parent]
        let events = vec![
            make_created_event("parent", "Parent task", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child task", TaskPriority::P2, 2),
            make_started_event("parent.1"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(!scope_set.include_root);
        assert_eq!(scope_set.scopes, vec!["parent".to_string()]);
    }

    #[test]
    fn test_scope_set_is_empty() {
        // No in-progress tasks → is_empty() = true
        let events = vec![make_created_event("task1", "Task 1", TaskPriority::P2, 1)];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(!scope_set.include_root);
        assert!(scope_set.scopes.is_empty());
        assert!(scope_set.is_empty());
    }
}

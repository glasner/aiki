//! Task manager for materializing views and calculating ready queue

use std::collections::HashMap;

use super::id::{get_parent_id, is_direct_child_of};
use super::types::{Task, TaskComment, TaskEvent, TaskStatus};
use crate::agents::{AgentType, Assignee};

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

    /// Get scope list for XML output, including "root" when include_root=true
    #[must_use]
    pub fn to_xml_scopes(&self) -> Vec<String> {
        let mut result = Vec::new();
        if self.include_root {
            result.push("root".to_string());
        }
        result.extend(self.scopes.iter().cloned());
        result
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
                task_type,
                priority,
                assignee,
                sources,
                template,
                working_copy,
                instructions,
                data,
                timestamp,
            } => {
                tasks.insert(
                    task_id.clone(),
                    Task {
                        id: task_id.clone(),
                        name: name.clone(),
                        task_type: task_type.clone(),
                        status: TaskStatus::Open,
                        priority: *priority,
                        assignee: assignee.clone(),
                        sources: sources.clone(),
                        template: template.clone(),
                        working_copy: working_copy.clone(),
                        instructions: instructions.clone(),
                        data: data.clone(),
                        created_at: *timestamp,
                        started_at: None,
                        claimed_by_session: None,
                        last_session_id: None,
                        stopped_reason: None,
                        closed_outcome: None,
                        comments: Vec::new(),
                    },
                );
            }
            TaskEvent::Started { task_ids, session_id, timestamp, .. } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.status = TaskStatus::InProgress;
                        task.stopped_reason = None;
                        task.claimed_by_session = session_id.clone();
                        task.last_session_id = session_id.clone();
                        task.started_at = Some(*timestamp);
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
                        task.claimed_by_session = None; // Release claim so task is visible to all
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
                        task.claimed_by_session = None; // Release claim
                    }
                }
            }
            TaskEvent::Reopened { task_id, .. } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.status = TaskStatus::Open;
                    task.closed_outcome = None;
                    task.claimed_by_session = None; // Release claim so task is visible to all
                }
            }
            TaskEvent::CommentAdded {
                task_ids,
                text,
                data,
                timestamp,
            } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.comments.push(TaskComment {
                            id: None, // Change ID not available in this code path
                            text: text.clone(),
                            data: data.clone(),
                            timestamp: *timestamp,
                        });
                    }
                }
            }
            TaskEvent::Updated {
                task_id,
                name,
                priority,
                assignee,
                ..
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if let Some(new_name) = name {
                        task.name = new_name.clone();
                    }
                    if let Some(new_priority) = priority {
                        task.priority = *new_priority;
                    }
                    // Handle assignee: Some(Some(a)) = assign, Some(None) = unassign
                    if let Some(new_assignee) = assignee {
                        task.assignee = new_assignee.clone();
                    }
                }
            }
        }
    }

    tasks
}

/// Materialize task views from an event stream with change IDs
///
/// Like `materialize_tasks`, but accepts `EventWithId` to populate comment IDs.
/// This is needed when generating followup tasks that need to reference specific comments
/// via `source: comment:<change_id>`.
#[must_use]
pub fn materialize_tasks_with_ids(
    events: &[super::storage::EventWithId],
) -> HashMap<String, Task> {
    let mut tasks: HashMap<String, Task> = HashMap::new();

    for event_with_id in events {
        let super::storage::EventWithId { change_id, event } = event_with_id;

        match event {
            TaskEvent::Created {
                task_id,
                name,
                task_type,
                priority,
                assignee,
                sources,
                template,
                working_copy,
                instructions,
                data,
                timestamp,
            } => {
                tasks.insert(
                    task_id.clone(),
                    Task {
                        id: task_id.clone(),
                        name: name.clone(),
                        task_type: task_type.clone(),
                        status: TaskStatus::Open,
                        priority: *priority,
                        assignee: assignee.clone(),
                        sources: sources.clone(),
                        template: template.clone(),
                        working_copy: working_copy.clone(),
                        instructions: instructions.clone(),
                        data: data.clone(),
                        created_at: *timestamp,
                        started_at: None,
                        claimed_by_session: None,
                        last_session_id: None,
                        stopped_reason: None,
                        closed_outcome: None,
                        comments: Vec::new(),
                    },
                );
            }
            TaskEvent::Started {
                task_ids,
                session_id,
                timestamp,
                stopped,
                ..
            } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.status = TaskStatus::InProgress;
                        task.stopped_reason = None;
                        task.claimed_by_session = session_id.clone();
                        task.last_session_id = session_id.clone();
                        task.started_at = Some(*timestamp);
                    }
                }
                for stopped_id in stopped {
                    if let Some(task) = tasks.get_mut(stopped_id) {
                        task.status = TaskStatus::Stopped;
                        task.stopped_reason =
                            Some(format!("Preempted by task {}", task_ids.join(", ")));
                        task.claimed_by_session = None;
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
                        task.claimed_by_session = None;
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
                        task.claimed_by_session = None;
                    }
                }
            }
            TaskEvent::Reopened { task_id, .. } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.status = TaskStatus::Open;
                    task.closed_outcome = None;
                    task.claimed_by_session = None;
                }
            }
            TaskEvent::CommentAdded {
                task_ids,
                text,
                data,
                timestamp,
            } => {
                for task_id in task_ids {
                    if let Some(task) = tasks.get_mut(task_id) {
                        task.comments.push(TaskComment {
                            id: Some(change_id.clone()), // Include the change_id as comment ID
                            text: text.clone(),
                            data: data.clone(),
                            timestamp: *timestamp,
                        });
                    }
                }
            }
            TaskEvent::Updated {
                task_id,
                name,
                priority,
                assignee,
                ..
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if let Some(new_name) = name {
                        task.name = new_name.clone();
                    }
                    if let Some(new_priority) = priority {
                        task.priority = *new_priority;
                    }
                    if let Some(new_assignee) = assignee {
                        task.assignee = new_assignee.clone();
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

/// Get task IDs that are in-progress and claimed by a specific session
///
/// This is used to include task context in provenance records when
/// changes are made during a task.
///
/// Returns task IDs ordered by most recently started first.
#[must_use]
pub fn get_in_progress_task_ids_for_session(
    tasks: &HashMap<String, Task>,
    session_id: &str,
) -> Vec<String> {
    let mut result: Vec<&Task> = tasks
        .values()
        .filter(|t| {
            t.status == TaskStatus::InProgress
                && t.claimed_by_session.as_deref() == Some(session_id)
        })
        .collect();

    // Sort by start time descending (most recently started first)
    // Falls back to creation time if started_at is not set (shouldn't happen for in-progress tasks)
    result.sort_by(|a, b| {
        let a_time = a.started_at.unwrap_or(a.created_at);
        let b_time = b.started_at.unwrap_or(b.created_at);
        b_time.cmp(&a_time)
    });

    result.into_iter().map(|t| t.id.clone()).collect()
}

/// Get stopped tasks
#[must_use]
#[allow(dead_code)] // Part of task manager API
pub fn get_stopped(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks
        .values()
        .filter(|t| t.status == TaskStatus::Stopped)
        .collect()
}

/// Get closed tasks
#[must_use]
#[allow(dead_code)] // Part of task manager API
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

/// Check if a task has any subtasks
#[must_use]
pub fn has_subtasks(tasks: &HashMap<String, Task>, parent_id: &str) -> bool {
    tasks.keys().any(|id| is_direct_child_of(id, parent_id))
}

/// Get direct subtasks of a task
#[must_use]
pub fn get_subtasks<'a>(tasks: &'a HashMap<String, Task>, parent_id: &str) -> Vec<&'a Task> {
    tasks
        .values()
        .filter(|t| is_direct_child_of(&t.id, parent_id))
        .collect()
}

/// Get the ready queue filtered by scope
///
/// When scope is None, returns only root-level tasks (no parent).
/// When scope is Some(parent_id), returns only direct subtasks of that parent.
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
#[allow(dead_code)] // Part of task manager API
pub fn get_current_scopes(tasks: &HashMap<String, Task>) -> Vec<String> {
    get_current_scope_set(tasks).scopes
}

/// Check if all subtasks of a parent are closed
#[must_use]
pub fn all_subtasks_closed(tasks: &HashMap<String, Task>, parent_id: &str) -> bool {
    let subtasks = get_subtasks(tasks, parent_id);
    !subtasks.is_empty() && subtasks.iter().all(|t| t.status == TaskStatus::Closed)
}

/// Get unclosed subtasks of a parent
#[must_use]
#[allow(dead_code)] // Part of task manager API
pub fn get_unclosed_subtasks<'a>(
    tasks: &'a HashMap<String, Task>,
    parent_id: &str,
) -> Vec<&'a Task> {
    get_subtasks(tasks, parent_id)
        .into_iter()
        .filter(|t| t.status != TaskStatus::Closed)
        .collect()
}

/// Get all unclosed descendants of a parent (recursive)
///
/// Returns all descendants (subtasks, grandsubtasks, etc.) that are not closed,
/// in depth-first order (deepest first, so they can be closed bottom-up).
#[must_use]
pub fn get_all_unclosed_descendants<'a>(
    tasks: &'a HashMap<String, Task>,
    parent_id: &str,
) -> Vec<&'a Task> {
    let mut result = Vec::new();
    collect_unclosed_descendants(tasks, parent_id, &mut result);
    result
}

/// Helper for recursive descent - collects descendants depth-first
fn collect_unclosed_descendants<'a>(
    tasks: &'a HashMap<String, Task>,
    parent_id: &str,
    result: &mut Vec<&'a Task>,
) {
    for subtask in get_subtasks(tasks, parent_id) {
        // First recurse to get grandsubtasks (depth-first)
        collect_unclosed_descendants(tasks, &subtask.id, result);
        // Then add this subtask if unclosed
        if subtask.status != TaskStatus::Closed {
            result.push(subtask);
        }
    }
}

/// Get ready queue based on a ScopeSet
///
/// When include_root is true, includes root-level tasks.
/// When scopes has entries, includes tasks from those scopes.
/// When scope_set is empty (no in-progress tasks), defaults to root-level tasks.
/// Merges and deduplicates when multiple sources are active.
#[must_use]
pub fn get_ready_queue_for_scope_set<'a>(
    tasks: &'a HashMap<String, Task>,
    scope_set: &ScopeSet,
) -> Vec<&'a Task> {
    use std::collections::HashSet;

    let mut seen: HashSet<&str> = HashSet::new();
    let mut ready: Vec<&Task> = Vec::new();

    // Include root-level tasks if requested OR if scope set is empty (no in-progress tasks)
    // This ensures `task list` shows root tasks when nothing is in progress
    if scope_set.include_root || scope_set.is_empty() {
        for task in get_scoped_ready_queue(tasks, None) {
            if seen.insert(&task.id) {
                ready.push(task);
            }
        }
    }

    // Include tasks from each scope
    for scope in &scope_set.scopes {
        for task in get_scoped_ready_queue(tasks, Some(scope)) {
            if seen.insert(&task.id) {
                ready.push(task);
            }
        }
    }

    // Sort by priority then creation time
    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    ready
}

/// Get ready queue filtered for a specific agent
///
/// Returns open tasks that are visible to the given agent:
/// - Unassigned tasks (visible to all)
/// - Tasks assigned to this specific agent
///
/// Excludes:
/// - Tasks assigned to "human"
/// - Tasks assigned to other agents
#[must_use]
pub fn get_ready_queue_for_agent<'a>(tasks: &'a HashMap<String, Task>, agent: &AgentType) -> Vec<&'a Task> {
    let mut ready: Vec<&Task> = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| {
            let assignee = t
                .assignee
                .as_ref()
                .and_then(|s| Assignee::from_str(s))
                .unwrap_or(Assignee::Unassigned);
            assignee.is_visible_to(agent)
        })
        .collect();

    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    ready
}

/// Get ready queue filtered for human visibility
///
/// Returns open tasks that are visible to humans:
/// - Unassigned tasks
/// - Tasks assigned to "human"
///
/// Excludes:
/// - Tasks assigned to any agent
#[must_use]
#[allow(dead_code)] // Part of task manager API
pub fn get_ready_queue_for_human(tasks: &HashMap<String, Task>) -> Vec<&'_ Task> {
    let mut ready: Vec<&Task> = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| {
            let assignee = t
                .assignee
                .as_ref()
                .and_then(|s| Assignee::from_str(s))
                .unwrap_or(Assignee::Unassigned);
            assignee.is_visible_to_human()
        })
        .collect();

    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    ready
}

/// Get ready queue for a scope set, filtered for a specific agent
///
/// Combines scope filtering with agent visibility filtering.
#[must_use]
pub fn get_ready_queue_for_agent_scoped<'a>(
    tasks: &'a HashMap<String, Task>,
    scope_set: &ScopeSet,
    agent: &AgentType,
) -> Vec<&'a Task> {
    let scoped = get_ready_queue_for_scope_set(tasks, scope_set);
    scoped
        .into_iter()
        .filter(|t| {
            let assignee = t
                .assignee
                .as_ref()
                .and_then(|s| Assignee::from_str(s))
                .unwrap_or(Assignee::Unassigned);
            assignee.is_visible_to(agent)
        })
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
            task_type: None,
            priority,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: std::collections::HashMap::new(),
            timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
        }
    }

    fn make_started_event(task_id: &str) -> TaskEvent {
        TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: "claude-code".to_string(),
            session_id: None,
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
    fn test_has_subtasks() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("other", "Other", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);

        assert!(has_subtasks(&tasks, "parent"));
        assert!(!has_subtasks(&tasks, "parent.1"));
        assert!(!has_subtasks(&tasks, "other"));
        assert!(!has_subtasks(&tasks, "nonexistent"));
    }

    #[test]
    fn test_get_subtasks() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("parent.1.1", "Grandsubtask", TaskPriority::P2, 1),
            make_created_event("other", "Other", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);
        let subtasks = get_subtasks(&tasks, "parent");

        // Should only get direct subtasks, not grandsubtasks
        assert_eq!(subtasks.len(), 2);
        let subtask_ids: Vec<_> = subtasks.iter().map(|t| t.id.as_str()).collect();
        assert!(subtask_ids.contains(&"parent.1"));
        assert!(subtask_ids.contains(&"parent.2"));
        assert!(!subtask_ids.contains(&"parent.1.1"));
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

        // Should only get root-level tasks, not subtasks
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

        // Should only get direct subtasks of parent
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
        // Two in-progress subtasks from different parents -> two scopes
        let events = vec![
            make_created_event("parent1", "Parent 1", TaskPriority::P2, 1),
            make_created_event("parent2", "Parent 2", TaskPriority::P2, 1),
            make_created_event("parent1.1", "Child 1.1", TaskPriority::P2, 1),
            make_created_event("parent2.1", "Child 2.1", TaskPriority::P2, 1),
            TaskEvent::Started {
                task_ids: vec!["parent1.1".to_string(), "parent2.1".to_string()],
                agent_type: "claude-code".to_string(),
                session_id: None,
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
        // Two in-progress subtasks from same parent -> one scope (deduplicated)
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            TaskEvent::Started {
                task_ids: vec!["parent.1".to_string(), "parent.2".to_string()],
                agent_type: "claude-code".to_string(),
                session_id: None,
                timestamp: Utc::now(),
                stopped: Vec::new(),
            },
        ];
        let tasks = materialize_tasks(&events);
        let scopes = get_current_scopes(&tasks);
        assert_eq!(scopes, vec!["parent".to_string()]);
    }

    #[test]
    fn test_all_subtasks_closed() {
        // No subtasks -> returns false
        let events = vec![make_created_event("parent", "Parent", TaskPriority::P2, 1)];
        let tasks = materialize_tasks(&events);
        assert!(!all_subtasks_closed(&tasks, "parent"));

        // Some subtasks open -> returns false
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);
        assert!(!all_subtasks_closed(&tasks, "parent"));

        // All subtasks closed -> returns true
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
            make_closed_event("parent.2", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);
        assert!(all_subtasks_closed(&tasks, "parent"));
    }

    #[test]
    fn test_get_unclosed_subtasks() {
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 1),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
            make_created_event("parent.3", "Child 3", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let unclosed = get_unclosed_subtasks(&tasks, "parent");

        assert_eq!(unclosed.len(), 2);
        let ids: Vec<_> = unclosed.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"parent.2"));
        assert!(ids.contains(&"parent.3"));
    }

    #[test]
    fn test_get_all_unclosed_descendants_flat() {
        // Test with only direct subtasks (no grandsubtasks)
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 4),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 3),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 2),
            make_created_event("parent.3", "Child 3", TaskPriority::P2, 1),
            make_closed_event("parent.1", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let descendants = get_all_unclosed_descendants(&tasks, "parent");

        assert_eq!(descendants.len(), 2);
        let ids: Vec<_> = descendants.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"parent.2"));
        assert!(ids.contains(&"parent.3"));
    }

    #[test]
    fn test_get_all_unclosed_descendants_nested() {
        // Test with grandsubtasks - should return depth-first (deepest first)
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 6),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 5),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 4),
            make_created_event("parent.1.1", "Grandchild 1.1", TaskPriority::P2, 3),
            make_created_event("parent.1.2", "Grandchild 1.2", TaskPriority::P2, 2),
            make_created_event("parent.2.1", "Grandchild 2.1", TaskPriority::P2, 1),
        ];

        let tasks = materialize_tasks(&events);
        let descendants = get_all_unclosed_descendants(&tasks, "parent");

        // Should have all 5 descendants
        assert_eq!(descendants.len(), 5);
        let ids: Vec<_> = descendants.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"parent.1"));
        assert!(ids.contains(&"parent.2"));
        assert!(ids.contains(&"parent.1.1"));
        assert!(ids.contains(&"parent.1.2"));
        assert!(ids.contains(&"parent.2.1"));

        // Grandsubtasks should come before their parents (depth-first)
        let pos_1 = ids.iter().position(|id| *id == "parent.1").unwrap();
        let pos_1_1 = ids.iter().position(|id| *id == "parent.1.1").unwrap();
        let pos_1_2 = ids.iter().position(|id| *id == "parent.1.2").unwrap();
        assert!(pos_1_1 < pos_1, "Grandsubtask 1.1 should come before Child 1");
        assert!(pos_1_2 < pos_1, "Grandsubtask 1.2 should come before Child 1");
    }

    #[test]
    fn test_get_all_unclosed_descendants_some_closed() {
        // Test with some descendants already closed
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 5),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 4),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 3),
            make_created_event("parent.1.1", "Grandchild 1.1", TaskPriority::P2, 2),
            make_closed_event("parent.1", TaskOutcome::Done),
            make_closed_event("parent.1.1", TaskOutcome::Done),
        ];

        let tasks = materialize_tasks(&events);
        let descendants = get_all_unclosed_descendants(&tasks, "parent");

        // Only parent.2 should be unclosed
        assert_eq!(descendants.len(), 1);
        assert_eq!(descendants[0].id, "parent.2");
    }

    #[test]
    fn test_get_all_unclosed_descendants_empty() {
        // Test with no subtasks
        let events = vec![make_created_event("parent", "Parent", TaskPriority::P2, 1)];

        let tasks = materialize_tasks(&events);
        let descendants = get_all_unclosed_descendants(&tasks, "parent");

        assert!(descendants.is_empty());
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

    #[test]
    fn test_scope_set_deeply_nested() {
        // Deeply nested task in-progress (3 levels: parent.1.1)
        // Should set scope to "parent.1", not "parent"
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 4),
            make_created_event("parent.1", "Child", TaskPriority::P2, 3),
            make_created_event("parent.1.1", "Grandchild", TaskPriority::P2, 2),
            make_started_event("parent.1.1"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(!scope_set.include_root);
        // Scope should be the DIRECT parent of the in-progress task
        assert_eq!(scope_set.scopes, vec!["parent.1".to_string()]);
    }

    #[test]
    fn test_scope_set_multiple_depths() {
        // Multiple in-progress tasks at different hierarchy depths
        // root task + grandchild task
        let events = vec![
            make_created_event("root", "Root", TaskPriority::P2, 5),
            make_created_event("parent", "Parent", TaskPriority::P2, 4),
            make_created_event("parent.1", "Child", TaskPriority::P2, 3),
            make_created_event("parent.1.1", "Grandchild", TaskPriority::P2, 2),
            make_started_event("root"),
            make_started_event("parent.1.1"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(scope_set.include_root); // root task is in-progress
        assert_eq!(scope_set.scopes, vec!["parent.1".to_string()]); // grandchild's parent
    }

    #[test]
    fn test_scope_set_deduplication() {
        // Multiple subtasks of same parent in-progress → only one scope entry
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 4),
            make_created_event("parent.1", "Child 1", TaskPriority::P2, 3),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 2),
            make_created_event("parent.3", "Child 3", TaskPriority::P2, 1),
            make_started_event("parent.1"),
            make_started_event("parent.2"),
            make_started_event("parent.3"),
        ];
        let tasks = materialize_tasks(&events);
        let scope_set = get_current_scope_set(&tasks);

        assert!(!scope_set.include_root);
        // Should deduplicate to single "parent" scope
        assert_eq!(scope_set.scopes, vec!["parent".to_string()]);
    }

    #[test]
    fn test_materialize_empty_events() {
        let events: Vec<TaskEvent> = vec![];
        let tasks = materialize_tasks(&events);
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_ready_queue_empty_tasks() {
        let tasks: HashMap<String, Task> = HashMap::new();
        let ready = get_ready_queue(&tasks);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_scoped_ready_queue_empty_tasks() {
        let tasks: HashMap<String, Task> = HashMap::new();
        let ready = get_scoped_ready_queue(&tasks, None);
        assert!(ready.is_empty());

        let ready = get_scoped_ready_queue(&tasks, Some("parent"));
        assert!(ready.is_empty());
    }

    #[test]
    fn test_ready_queue_same_priority_same_time() {
        // All tasks with same priority and creation time
        // Should return in deterministic order (by ID as tiebreaker)
        let now = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "task_c".to_string(),
                name: "Task C".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: now,
            },
            TaskEvent::Created {
                task_type: None,
                task_id: "task_a".to_string(),
                name: "Task A".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: now,
            },
            TaskEvent::Created {
                task_type: None,
                task_id: "task_b".to_string(),
                name: "Task B".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: now,
            },
        ];
        let tasks = materialize_tasks(&events);
        let ready = get_ready_queue(&tasks);

        assert_eq!(ready.len(), 3);
        // With same priority and time, order is deterministic (HashMap iteration order + sort stability)
        // The key thing is that all 3 appear and sorting doesn't crash
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"task_a"));
        assert!(ids.contains(&"task_b"));
        assert!(ids.contains(&"task_c"));
    }

    #[test]
    fn test_task_lifecycle_complete() {
        // Test complete lifecycle: Open → InProgress → Stopped → InProgress → Closed
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 5),
            make_started_event("task1"),
            make_stopped_event("task1", Some("need info")),
            make_started_event("task1"),
            make_closed_event("task1", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);

        let task = tasks.get("task1").unwrap();
        assert_eq!(task.status, TaskStatus::Closed);
        assert_eq!(task.closed_outcome, Some(TaskOutcome::Done));
    }

    #[test]
    fn test_find_task_nonexistent() {
        let events = vec![make_created_event("task1", "Task 1", TaskPriority::P2, 1)];
        let tasks = materialize_tasks(&events);

        assert!(find_task(&tasks, "nonexistent").is_none());
        assert!(find_task(&tasks, "").is_none());
    }

    #[test]
    fn test_get_subtasks_excludes_grandsubtasks() {
        // Verify get_subtasks only returns direct subtasks, not grandsubtasks
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 4),
            make_created_event("parent.1", "Child", TaskPriority::P2, 3),
            make_created_event("parent.1.1", "Grandsubtask", TaskPriority::P2, 2),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
        ];
        let tasks = materialize_tasks(&events);

        let subtasks = get_subtasks(&tasks, "parent");
        let ids: Vec<_> = subtasks.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"parent.1"));
        assert!(ids.contains(&"parent.2"));
        assert!(!ids.contains(&"parent.1.1")); // Grandsubtask excluded
    }

    #[test]
    fn test_has_subtasks_with_only_grandsubtasks() {
        // Parent has grandsubtasks but no direct subtasks
        // This is an edge case - should return false
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 3),
            make_created_event("parent.1", "Child", TaskPriority::P2, 2),
            make_created_event("parent.1.1", "Grandsubtask", TaskPriority::P2, 1),
        ];
        let tasks = materialize_tasks(&events);

        // parent.1 has subtasks (grandsubtask of parent)
        assert!(has_subtasks(&tasks, "parent.1"));
        // parent has subtasks (direct subtask parent.1)
        assert!(has_subtasks(&tasks, "parent"));
        // parent.1.1 has no subtasks
        assert!(!has_subtasks(&tasks, "parent.1.1"));
    }

    #[test]
    fn test_in_progress_excludes_stopped() {
        // Stopped tasks should not be in in_progress list
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 3),
            make_created_event("task2", "Task 2", TaskPriority::P2, 2),
            make_started_event("task1"),
            make_started_event("task2"),
            make_stopped_event("task1", None),
        ];
        let tasks = materialize_tasks(&events);

        let in_progress = get_in_progress(&tasks);
        let ids: Vec<_> = in_progress.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&"task2"));
        assert!(!ids.contains(&"task1"));
    }

    // Tests for get_ready_queue_for_scope_set

    #[test]
    fn test_scope_set_queue_empty_scope_set_defaults_to_root() {
        // Empty ScopeSet (no in-progress tasks) should default to root-level tasks
        // This ensures `task list` works when nothing is in progress
        let events = vec![
            make_created_event("root1", "Root 1", TaskPriority::P2, 2),
            make_created_event("root2", "Root 2", TaskPriority::P1, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P0, 1),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec![],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        // Should return root-level tasks only (not subtasks)
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"root1"));
        assert!(ids.contains(&"root2"));
        assert!(!ids.contains(&"parent.1")); // Child excluded
    }

    #[test]
    fn test_scope_set_queue_root_only() {
        // include_root=true should return root-level tasks only
        let events = vec![
            make_created_event("root1", "Root 1", TaskPriority::P2, 2),
            make_created_event("root2", "Root 2", TaskPriority::P1, 1),
            make_created_event("parent.1", "Child 1", TaskPriority::P0, 1),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: true,
            scopes: vec![],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"root1"));
        assert!(ids.contains(&"root2"));
        assert!(!ids.contains(&"parent.1")); // Child not included
    }

    #[test]
    fn test_scope_set_queue_scoped_only() {
        // Single scope should return only that scope's subtasks
        let events = vec![
            make_created_event("root1", "Root 1", TaskPriority::P2, 3),
            make_created_event("parent", "Parent", TaskPriority::P2, 3),
            make_created_event("parent.1", "Child 1", TaskPriority::P0, 2),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec!["parent".to_string()],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"parent.1"));
        assert!(ids.contains(&"parent.2"));
        assert!(!ids.contains(&"root1")); // Root not included
        // Verify priority sorting (P0 first)
        assert_eq!(ready[0].id, "parent.1");
    }

    #[test]
    fn test_scope_set_queue_root_and_scopes() {
        // Both root and scopes should merge results
        // Note: "parent" is also a root-level task so gets included with include_root=true
        let events = vec![
            make_created_event("root1", "Root 1", TaskPriority::P1, 4),
            make_created_event("parent", "Parent", TaskPriority::P3, 3), // P3 so it's last
            make_created_event("parent.1", "Child 1", TaskPriority::P0, 2),
            make_created_event("parent.2", "Child 2", TaskPriority::P2, 1),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: true,
            scopes: vec!["parent".to_string()],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        // 4 tasks: root1, parent (root), parent.1, parent.2
        assert_eq!(ids.len(), 4);
        assert!(ids.contains(&"root1"));
        assert!(ids.contains(&"parent"));
        assert!(ids.contains(&"parent.1"));
        assert!(ids.contains(&"parent.2"));
        // Verify priority sorting (P0 first, then P1, then P2, then P3)
        assert_eq!(ready[0].id, "parent.1"); // P0
        assert_eq!(ready[1].id, "root1"); // P1
        assert_eq!(ready[2].id, "parent.2"); // P2
        assert_eq!(ready[3].id, "parent"); // P3
    }

    #[test]
    fn test_scope_set_queue_multiple_scopes() {
        // Multiple scopes should merge and deduplicate
        let events = vec![
            make_created_event("p1", "Parent 1", TaskPriority::P2, 5),
            make_created_event("p2", "Parent 2", TaskPriority::P2, 4),
            make_created_event("p1.1", "P1 Child", TaskPriority::P1, 3),
            make_created_event("p2.1", "P2 Child", TaskPriority::P0, 2),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec!["p1".to_string(), "p2".to_string()],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"p1.1"));
        assert!(ids.contains(&"p2.1"));
        // Verify priority sorting
        assert_eq!(ready[0].id, "p2.1"); // P0 first
        assert_eq!(ready[1].id, "p1.1"); // P1 second
    }

    #[test]
    fn test_scope_set_queue_excludes_non_open() {
        // Should only include Open tasks, not InProgress/Stopped/Closed
        let events = vec![
            make_created_event("parent", "Parent", TaskPriority::P2, 5),
            make_created_event("parent.1", "Open", TaskPriority::P2, 4),
            make_created_event("parent.2", "InProgress", TaskPriority::P2, 3),
            make_created_event("parent.3", "Closed", TaskPriority::P2, 2),
            make_started_event("parent.2"),
            make_closed_event("parent.3", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec!["parent".to_string()],
        };

        let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
        let ids: Vec<_> = ready.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&"parent.1"));
        assert!(!ids.contains(&"parent.2")); // InProgress excluded
        assert!(!ids.contains(&"parent.3")); // Closed excluded
    }

    // Tests for ScopeSet::to_xml_scopes

    #[test]
    fn test_to_xml_scopes_empty() {
        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec![],
        };
        assert!(scope_set.to_xml_scopes().is_empty());
    }

    #[test]
    fn test_to_xml_scopes_root_only() {
        let scope_set = ScopeSet {
            include_root: true,
            scopes: vec![],
        };
        assert_eq!(scope_set.to_xml_scopes(), vec!["root".to_string()]);
    }

    #[test]
    fn test_to_xml_scopes_scopes_only() {
        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec!["parent1".to_string(), "parent2".to_string()],
        };
        assert_eq!(
            scope_set.to_xml_scopes(),
            vec!["parent1".to_string(), "parent2".to_string()]
        );
    }

    #[test]
    fn test_to_xml_scopes_root_and_scopes() {
        let scope_set = ScopeSet {
            include_root: true,
            scopes: vec!["parent1".to_string()],
        };
        assert_eq!(
            scope_set.to_xml_scopes(),
            vec!["root".to_string(), "parent1".to_string()]
        );
    }

    #[test]
    fn test_materialize_reopened_event() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Closed {
                task_ids: vec!["a1b2".to_string()],
                outcome: TaskOutcome::Done,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
            TaskEvent::Reopened {
                task_id: "a1b2".to_string(),
                reason: "Found another bug".to_string(),
                timestamp: base_time + chrono::Duration::seconds(2),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.status, TaskStatus::Open);
        // Closed outcome should be cleared when reopened
        assert!(task.closed_outcome.is_none());
    }

    #[test]
    fn test_materialize_comment_added_event() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::CommentAdded {
                task_ids: vec!["a1b2".to_string()],
                text: "First comment".to_string(),
                data: std::collections::HashMap::new(),
                timestamp: base_time + chrono::Duration::seconds(1),
            },
            TaskEvent::CommentAdded {
                task_ids: vec!["a1b2".to_string()],
                text: "Second comment".to_string(),
                data: std::collections::HashMap::new(),
                timestamp: base_time + chrono::Duration::seconds(2),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.comments.len(), 2);
        assert_eq!(task.comments[0].text, "First comment");
        assert_eq!(task.comments[1].text, "Second comment");
    }

    #[test]
    fn test_materialize_updated_event_name() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Original name".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Updated {
                task_id: "a1b2".to_string(),
                name: Some("Updated name".to_string()),
                priority: None,
                assignee: None,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.name, "Updated name");
        assert_eq!(task.priority, TaskPriority::P2); // Priority unchanged
    }

    #[test]
    fn test_materialize_updated_event_priority() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Updated {
                task_id: "a1b2".to_string(),
                name: None,
                priority: Some(TaskPriority::P0),
                assignee: None,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.name, "Test task"); // Name unchanged
        assert_eq!(task.priority, TaskPriority::P0);
    }

    #[test]
    fn test_materialize_updated_event_both_fields() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Original".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Updated {
                task_id: "a1b2".to_string(),
                name: Some("New name".to_string()),
                priority: Some(TaskPriority::P1),
                assignee: None,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.name, "New name");
        assert_eq!(task.priority, TaskPriority::P1);
    }

    #[test]
    fn test_materialize_full_task_lifecycle_with_reopen() {
        let base_time = Utc::now();
        let events = vec![
            // Create task
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            // Start task
            TaskEvent::Started {
                task_ids: vec!["a1b2".to_string()],
                agent_type: "claude-code".to_string(),
                session_id: None,
                timestamp: base_time + chrono::Duration::seconds(1),
                stopped: vec![],
            },
            // Add comment
            TaskEvent::CommentAdded {
                task_ids: vec!["a1b2".to_string()],
                text: "Working on this".to_string(),
                data: std::collections::HashMap::new(),
                timestamp: base_time + chrono::Duration::seconds(2),
            },
            // Close task
            TaskEvent::Closed {
                task_ids: vec!["a1b2".to_string()],
                outcome: TaskOutcome::Done,
                timestamp: base_time + chrono::Duration::seconds(3),
            },
            // Reopen task
            TaskEvent::Reopened {
                task_id: "a1b2".to_string(),
                reason: "Bug still exists".to_string(),
                timestamp: base_time + chrono::Duration::seconds(4),
            },
            // Update task
            TaskEvent::Updated {
                task_id: "a1b2".to_string(),
                name: Some("Fix critical bug".to_string()),
                priority: Some(TaskPriority::P0),
                assignee: None,
                timestamp: base_time + chrono::Duration::seconds(5),
            },
        ];

        let tasks = materialize_tasks(&events);
        let task = tasks.get("a1b2").expect("Task should exist");

        assert_eq!(task.status, TaskStatus::Open);
        assert_eq!(task.name, "Fix critical bug");
        assert_eq!(task.priority, TaskPriority::P0);
        assert_eq!(task.comments.len(), 1);
        assert!(task.closed_outcome.is_none());
    }

    #[test]
    fn test_reopened_task_appears_in_ready_queue() {
        let base_time = Utc::now();
        let events = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Closed {
                task_ids: vec!["a1b2".to_string()],
                outcome: TaskOutcome::Done,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
        ];

        let tasks = materialize_tasks(&events);
        let ready = get_ready_queue(&tasks);
        assert!(ready.is_empty(), "Closed task should not be in ready queue");

        // Now add reopened event
        let events_with_reopen = vec![
            TaskEvent::Created {
                task_type: None,
                task_id: "a1b2".to_string(),
                name: "Test task".to_string(),
                priority: TaskPriority::P2,
                assignee: None,
                sources: Vec::new(),
                template: None,
                working_copy: None,
                instructions: None,
                data: std::collections::HashMap::new(),
                timestamp: base_time,
            },
            TaskEvent::Closed {
                task_ids: vec!["a1b2".to_string()],
                outcome: TaskOutcome::Done,
                timestamp: base_time + chrono::Duration::seconds(1),
            },
            TaskEvent::Reopened {
                task_id: "a1b2".to_string(),
                reason: "Found more issues".to_string(),
                timestamp: base_time + chrono::Duration::seconds(2),
            },
        ];

        let tasks = materialize_tasks(&events_with_reopen);
        let ready = get_ready_queue(&tasks);
        assert_eq!(ready.len(), 1, "Reopened task should be in ready queue");
        assert_eq!(ready[0].id, "a1b2");
    }

    #[test]
    fn test_update_nonexistent_task_ignored() {
        let events = vec![TaskEvent::Updated {
            task_id: "nonexistent".to_string(),
            name: Some("New name".to_string()),
            priority: None,
            assignee: None,
            timestamp: Utc::now(),
        }];

        let tasks = materialize_tasks(&events);
        assert!(tasks.is_empty(), "No task should be created from Update event alone");
    }

    #[test]
    fn test_comment_on_nonexistent_task_ignored() {
        let events = vec![TaskEvent::CommentAdded {
            task_ids: vec!["nonexistent".to_string()],
            text: "Comment".to_string(),
            data: std::collections::HashMap::new(),
            timestamp: Utc::now(),
        }];

        let tasks = materialize_tasks(&events);
        assert!(tasks.is_empty(), "No task should be created from CommentAdded event alone");
    }

    #[test]
    fn test_reopen_nonexistent_task_ignored() {
        let events = vec![TaskEvent::Reopened {
            task_id: "nonexistent".to_string(),
            reason: "Reason".to_string(),
            timestamp: Utc::now(),
        }];

        let tasks = materialize_tasks(&events);
        assert!(tasks.is_empty(), "No task should be created from Reopened event alone");
    }

    // Tests for assignee-based filtering

    fn make_created_event_with_assignee(
        task_id: &str,
        name: &str,
        priority: TaskPriority,
        hours_ago: i64,
        assignee: Option<&str>,
    ) -> TaskEvent {
        TaskEvent::Created {
            task_id: task_id.to_string(),
            name: name.to_string(),
            task_type: None,
            priority,
            assignee: assignee.map(|s| s.to_string()),
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: std::collections::HashMap::new(),
            timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
        }
    }

    #[test]
    fn test_get_ready_queue_for_agent_unassigned() {
        // Unassigned tasks are visible to all agents
        let events = vec![
            make_created_event_with_assignee("task1", "Unassigned", TaskPriority::P2, 1, None),
        ];
        let tasks = materialize_tasks(&events);

        let claude_queue = get_ready_queue_for_agent(&tasks, &AgentType::ClaudeCode);
        let codex_queue = get_ready_queue_for_agent(&tasks, &AgentType::Codex);

        assert_eq!(claude_queue.len(), 1);
        assert_eq!(codex_queue.len(), 1);
    }

    #[test]
    fn test_get_ready_queue_for_agent_assigned_to_self() {
        // Tasks assigned to an agent are visible only to that agent
        let events = vec![
            make_created_event_with_assignee("task1", "For Claude", TaskPriority::P2, 1, Some("claude-code")),
        ];
        let tasks = materialize_tasks(&events);

        let claude_queue = get_ready_queue_for_agent(&tasks, &AgentType::ClaudeCode);
        let codex_queue = get_ready_queue_for_agent(&tasks, &AgentType::Codex);

        assert_eq!(claude_queue.len(), 1);
        assert_eq!(codex_queue.len(), 0); // Not visible to Codex
    }

    #[test]
    fn test_get_ready_queue_for_agent_human_task() {
        // Tasks assigned to human are not visible to agents
        let events = vec![
            make_created_event_with_assignee("task1", "For Human", TaskPriority::P2, 1, Some("human")),
        ];
        let tasks = materialize_tasks(&events);

        let claude_queue = get_ready_queue_for_agent(&tasks, &AgentType::ClaudeCode);
        let codex_queue = get_ready_queue_for_agent(&tasks, &AgentType::Codex);

        assert_eq!(claude_queue.len(), 0);
        assert_eq!(codex_queue.len(), 0);
    }

    #[test]
    fn test_get_ready_queue_for_agent_mixed() {
        // Mixed assignees - agent should see only relevant tasks
        let events = vec![
            make_created_event_with_assignee("unassigned", "Unassigned", TaskPriority::P3, 4, None),
            make_created_event_with_assignee("for_claude", "For Claude", TaskPriority::P2, 3, Some("claude-code")),
            make_created_event_with_assignee("for_codex", "For Codex", TaskPriority::P1, 2, Some("codex")),
            make_created_event_with_assignee("for_human", "For Human", TaskPriority::P0, 1, Some("human")),
        ];
        let tasks = materialize_tasks(&events);

        let claude_queue = get_ready_queue_for_agent(&tasks, &AgentType::ClaudeCode);
        let codex_queue = get_ready_queue_for_agent(&tasks, &AgentType::Codex);

        // Claude sees: unassigned, for_claude (sorted by priority)
        assert_eq!(claude_queue.len(), 2);
        assert_eq!(claude_queue[0].id, "for_claude"); // P2
        assert_eq!(claude_queue[1].id, "unassigned"); // P3

        // Codex sees: unassigned, for_codex (sorted by priority)
        assert_eq!(codex_queue.len(), 2);
        assert_eq!(codex_queue[0].id, "for_codex"); // P1
        assert_eq!(codex_queue[1].id, "unassigned"); // P3
    }

    #[test]
    fn test_get_ready_queue_for_human() {
        let events = vec![
            make_created_event_with_assignee("unassigned", "Unassigned", TaskPriority::P3, 4, None),
            make_created_event_with_assignee("for_claude", "For Claude", TaskPriority::P2, 3, Some("claude-code")),
            make_created_event_with_assignee("for_human", "For Human", TaskPriority::P0, 1, Some("human")),
        ];
        let tasks = materialize_tasks(&events);

        let human_queue = get_ready_queue_for_human(&tasks);

        // Human sees: unassigned, for_human (sorted by priority)
        assert_eq!(human_queue.len(), 2);
        assert_eq!(human_queue[0].id, "for_human"); // P0
        assert_eq!(human_queue[1].id, "unassigned"); // P3
    }

    #[test]
    fn test_get_ready_queue_for_agent_scoped() {
        let events = vec![
            make_created_event_with_assignee("parent", "Parent", TaskPriority::P2, 5, None),
            make_created_event_with_assignee("parent.1", "Child Unassigned", TaskPriority::P2, 4, None),
            make_created_event_with_assignee("parent.2", "Child For Claude", TaskPriority::P1, 3, Some("claude-code")),
            make_created_event_with_assignee("parent.3", "Child For Human", TaskPriority::P0, 2, Some("human")),
        ];
        let tasks = materialize_tasks(&events);

        let scope_set = ScopeSet {
            include_root: false,
            scopes: vec!["parent".to_string()],
        };

        let scoped_queue = get_ready_queue_for_agent_scoped(&tasks, &scope_set, &AgentType::ClaudeCode);

        // Claude sees subtasks: unassigned (P2), for_claude (P1)
        // Sorted by priority: P1 first
        assert_eq!(scoped_queue.len(), 2);
        assert_eq!(scoped_queue[0].id, "parent.2"); // P1 for claude
        assert_eq!(scoped_queue[1].id, "parent.1"); // P2 unassigned
    }

    // Tests for get_in_progress_task_ids_for_session

    fn make_started_event_with_session(task_id: &str, session_id: &str) -> TaskEvent {
        TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: "claude-code".to_string(),
            session_id: Some(session_id.to_string()),
            timestamp: Utc::now(),
            stopped: Vec::new(),
        }
    }

    fn make_started_event_with_time(
        task_id: &str,
        session_id: &str,
        hours_ago: i64,
    ) -> TaskEvent {
        TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: "claude-code".to_string(),
            session_id: Some(session_id.to_string()),
            timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
            stopped: Vec::new(),
        }
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_empty() {
        let events = vec![make_created_event("task1", "Task 1", TaskPriority::P2, 1)];
        let tasks = materialize_tasks(&events);

        let task_ids = get_in_progress_task_ids_for_session(&tasks, "session-123");
        assert!(task_ids.is_empty());
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_single() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 1),
            make_started_event_with_session("task1", "session-123"),
        ];
        let tasks = materialize_tasks(&events);

        let task_ids = get_in_progress_task_ids_for_session(&tasks, "session-123");
        assert_eq!(task_ids, vec!["task1"]);
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_multiple() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 3),
            make_created_event("task2", "Task 2", TaskPriority::P2, 2),
            make_created_event("task3", "Task 3", TaskPriority::P2, 1),
            // task1 started 3 hours ago (oldest start)
            make_started_event_with_time("task1", "session-123", 3),
            // task2 started 2 hours ago
            make_started_event_with_time("task2", "session-123", 2),
            // task3 started 1 hour ago (most recent start)
            make_started_event_with_time("task3", "session-123", 1),
        ];
        let tasks = materialize_tasks(&events);

        let task_ids = get_in_progress_task_ids_for_session(&tasks, "session-123");
        // Should be sorted by start time descending (most recently started first)
        assert_eq!(task_ids.len(), 3);
        assert_eq!(task_ids[0], "task3"); // Started 1 hour ago (most recent)
        assert_eq!(task_ids[1], "task2"); // Started 2 hours ago
        assert_eq!(task_ids[2], "task1"); // Started 3 hours ago (oldest)
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_different_sessions() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 2),
            make_created_event("task2", "Task 2", TaskPriority::P2, 1),
            make_started_event_with_session("task1", "session-123"),
            make_started_event_with_session("task2", "session-456"),
        ];
        let tasks = materialize_tasks(&events);

        let session_123_tasks = get_in_progress_task_ids_for_session(&tasks, "session-123");
        let session_456_tasks = get_in_progress_task_ids_for_session(&tasks, "session-456");

        assert_eq!(session_123_tasks, vec!["task1"]);
        assert_eq!(session_456_tasks, vec!["task2"]);
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_excludes_stopped() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 2),
            make_created_event("task2", "Task 2", TaskPriority::P2, 1),
            make_started_event_with_session("task1", "session-123"),
            make_started_event_with_session("task2", "session-123"),
            make_stopped_event("task1", None),
        ];
        let tasks = materialize_tasks(&events);

        let task_ids = get_in_progress_task_ids_for_session(&tasks, "session-123");
        // task1 was stopped, so only task2 remains in-progress
        assert_eq!(task_ids, vec!["task2"]);
    }

    #[test]
    fn test_get_in_progress_task_ids_for_session_excludes_closed() {
        let events = vec![
            make_created_event("task1", "Task 1", TaskPriority::P2, 2),
            make_created_event("task2", "Task 2", TaskPriority::P2, 1),
            make_started_event_with_session("task1", "session-123"),
            make_started_event_with_session("task2", "session-123"),
            make_closed_event("task1", TaskOutcome::Done),
        ];
        let tasks = materialize_tasks(&events);

        let task_ids = get_in_progress_task_ids_for_session(&tasks, "session-123");
        // task1 was closed, so only task2 remains in-progress
        assert_eq!(task_ids, vec!["task2"]);
    }
}

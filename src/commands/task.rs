//! Task management CLI commands
//!
//! Provides the `aiki task` command with subcommands:
//! - `add` - Create a new task
//! - `list` - Show ready queue (default)
//! - `start` - Start working on task(s)
//! - `stop` - Stop current task
//! - `close` - Close task(s) as done

use clap::Subcommand;
use std::path::Path;

use crate::error::{AikiError, Result};
use std::collections::{HashMap, HashSet};

use crate::tasks::{
    generate_child_id, generate_task_id, get_next_child_number,
    manager::{
        find_task, get_current_scope_set, get_in_progress, get_ready_queue, get_scoped_ready_queue,
        has_children, materialize_tasks, ScopeSet,
    },
    storage::{read_events, write_event},
    types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus},
    xml::{format_added, format_closed, format_started, format_stopped, format_task_list},
    XmlBuilder,
};

/// Get ready queue based on a ScopeSet
///
/// When include_root is true, includes root-level tasks.
/// When scopes has entries, includes tasks from those scopes.
/// Merges and deduplicates when multiple sources are active.
fn get_ready_queue_for_scope_set<'a>(
    tasks: &'a HashMap<String, Task>,
    scope_set: &ScopeSet,
) -> Vec<&'a Task> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut ready: Vec<&Task> = Vec::new();

    // Include root-level tasks if requested
    if scope_set.include_root {
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

/// Task subcommands
#[derive(Subcommand)]
pub enum TaskCommands {
    /// Show ready queue (default when no subcommand given)
    List,

    /// Create a new task
    Add {
        /// Task name
        name: String,

        /// Create as child of existing task
        #[arg(long)]
        parent: Option<String>,
    },

    /// Start working on task(s)
    Start {
        /// Task ID(s) to start (defaults to first from ready queue)
        #[arg(value_name = "ID")]
        ids: Vec<String>,
    },

    /// Stop the current task
    Stop {
        /// Task ID to stop (defaults to current in-progress task)
        id: Option<String>,

        /// Reason for stopping
        #[arg(long)]
        reason: Option<String>,

        /// Create blocker task (assigned to human)
        #[arg(long)]
        blocked: Option<String>,
    },

    /// Close task(s) as done
    Close {
        /// Task ID(s) to close (defaults to current in-progress task)
        #[arg(value_name = "ID")]
        ids: Vec<String>,

        /// Mark as won't do instead of done
        #[arg(long)]
        wont_do: bool,
    },
}

/// Main entry point for `aiki task` command
///
/// If no subcommand is provided, defaults to `list`.
pub fn run(command: Option<TaskCommands>) -> Result<()> {
    let cwd = std::env::current_dir()?;

    // Default to list if no subcommand provided
    let cmd = command.unwrap_or(TaskCommands::List);

    match cmd {
        TaskCommands::List => run_list(&cwd, None),
        TaskCommands::Add { name, parent } => run_add(&cwd, name, parent),
        TaskCommands::Start { ids } => run_start(&cwd, ids),
        TaskCommands::Stop {
            id,
            reason,
            blocked,
        } => run_stop(&cwd, id, reason, blocked),
        TaskCommands::Close { ids, wont_do } => run_close(&cwd, ids, wont_do),
    }
}

/// List tasks in the ready queue
fn run_list(cwd: &Path, scope_override: Option<&str>) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Determine scope set from override or current in-progress tasks
    let scope_set = if let Some(s) = scope_override {
        ScopeSet {
            include_root: false,
            scopes: vec![s.to_string()],
        }
    } else {
        get_current_scope_set(&tasks)
    };

    // Get ready queue filtered by scope set
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
    let in_progress = get_in_progress(&tasks);

    let content = format_task_list(&ready);

    let mut builder = XmlBuilder::new("list");
    if !scope_set.scopes.is_empty() {
        builder = builder.with_scopes(&scope_set.scopes);
    }
    let xml = builder.build(&content, &in_progress, &ready);

    println!("{}", xml);
    Ok(())
}

/// Add a new task
fn run_add(cwd: &Path, name: String, parent: Option<String>) -> Result<()> {
    // Read current state first (needed for context)
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine task ID based on whether this is a child task
    let task_id = if let Some(ref parent_id) = parent {
        // Validate parent exists and is not closed
        if let Some(parent_task) = find_task(&tasks, parent_id) {
            if parent_task.status == TaskStatus::Closed {
                return Err(AikiError::ParentTaskClosed(parent_id.clone()));
            }
        } else {
            return Err(AikiError::TaskNotFound(parent_id.clone()));
        }

        // Generate child ID (parent.N where N is next available)
        let task_ids: Vec<&str> = tasks.keys().map(|s| s.as_str()).collect();
        let child_num = get_next_child_number(parent_id, task_ids.into_iter());
        generate_child_id(parent_id, child_num)
    } else {
        // Root-level task with new JJ-style ID
        generate_task_id(&name)
    };

    let timestamp = chrono::Utc::now();

    let event = TaskEvent::Created {
        task_id: task_id.clone(),
        name: name.clone(),
        priority: TaskPriority::default(),
        assignee: None, // Phase 3: auto-detect from agent
        timestamp,
    };

    write_event(cwd, &event)?;

    // Build new task from event (avoid re-reading)
    let new_task = Task {
        id: task_id,
        name,
        priority: TaskPriority::default(),
        status: TaskStatus::Open,
        assignee: None,
        created_at: timestamp,
        stopped_reason: None,
        closed_outcome: None,
    };

    // Determine current scope set for context
    let scope_set = get_current_scope_set(&tasks);

    // Update ready queue based on scope set
    let mut ready: Vec<Task> = get_ready_queue_for_scope_set(&tasks, &scope_set)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();

    // Add new task if it's in the current scope
    let new_task_in_scope = match (&parent, &scope_set) {
        // New root task is in scope if root is included or no scopes active
        (None, ss) if ss.include_root || ss.is_empty() => true,
        // New child task is in scope if its parent is one of the active scopes
        (Some(p), ss) => ss.scopes.contains(p),
        // New root task when only child scopes active - not in scope
        (None, _) => false,
    };

    if new_task_in_scope {
        ready.push(new_task.clone());
        ready.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    let content = format_added(&[&new_task]);

    let ready_refs: Vec<_> = ready.iter().collect();
    let mut builder = XmlBuilder::new("add");
    if !scope_set.scopes.is_empty() {
        builder = builder.with_scopes(&scope_set.scopes);
    }
    let xml = builder.build(&content, &in_progress, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Start working on task(s)
fn run_start(cwd: &Path, ids: Vec<String>) -> Result<()> {
    let events = read_events(cwd)?;
    let mut tasks = materialize_tasks(&events);

    // Get in-progress task IDs first (to avoid borrow issues)
    let current_in_progress_ids: Vec<String> = get_in_progress(&tasks)
        .iter()
        .map(|t| t.id.clone())
        .collect();

    // Determine current scope set for ready queue
    let current_scope_set = get_current_scope_set(&tasks);
    let ready = get_ready_queue_for_scope_set(&tasks, &current_scope_set);

    // Determine which task(s) to start
    let ids_to_start = if ids.is_empty() {
        // Default: start first from ready queue
        if let Some(first) = ready.first() {
            vec![first.id.clone()]
        } else {
            return Err(AikiError::NoTasksReady);
        }
    } else {
        // Validate all IDs exist
        for id in &ids {
            if find_task(&tasks, id).is_none() {
                return Err(AikiError::TaskNotFound(id.clone()));
            }
        }
        ids
    };

    // Check if we're starting a parent task with children
    // If so, auto-create a planning task (.0) and start that instead
    let mut new_scope: Option<String> = None;
    let mut actual_ids_to_start = ids_to_start.clone();

    if ids_to_start.len() == 1 {
        let task_id = ids_to_start[0].clone();
        if has_children(&tasks, &task_id) {
            // Starting a parent task - create planning task if needed
            let planning_id = generate_child_id(&task_id, 0);

            // Check if planning task already exists
            if find_task(&tasks, &planning_id).is_none() {
                // Create the planning task
                let timestamp = chrono::Utc::now();
                let planning_event = TaskEvent::Created {
                    task_id: planning_id.clone(),
                    name: "Review all subtasks and start first batch".to_string(),
                    priority: TaskPriority::default(),
                    assignee: None,
                    timestamp,
                };
                write_event(cwd, &planning_event)?;

                // Add to local tasks map for output
                let task = Task {
                    id: planning_id.clone(),
                    name: "Review all subtasks and start first batch".to_string(),
                    status: TaskStatus::Open,
                    priority: TaskPriority::default(),
                    assignee: None,
                    created_at: timestamp,
                    stopped_reason: None,
                    closed_outcome: None,
                };
                tasks.insert(planning_id.clone(), task);
            }

            // Start the planning task instead of the parent
            actual_ids_to_start = vec![generate_child_id(&task_id, 0)];
            new_scope = Some(task_id);
        }
    }

    // Get tasks before state changes (for output)
    let mut stopped_tasks: Vec<Task> = current_in_progress_ids
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();
    let mut started_tasks: Vec<Task> = actual_ids_to_start
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();

    // Auto-stop current in-progress tasks (batch operation)
    let stop_reason = format!("Started {}", actual_ids_to_start.join(", "));

    if !current_in_progress_ids.is_empty() {
        let stop_event = TaskEvent::Stopped {
            task_ids: current_in_progress_ids.clone(),
            reason: Some(stop_reason.clone()),
            blocked_reason: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &stop_event)?;
    }

    // Start new tasks (batch operation)
    let timestamp = chrono::Utc::now();
    let start_event = TaskEvent::Started {
        task_ids: actual_ids_to_start.clone(),
        agent_type: "claude-code".to_string(), // TODO: get from context
        timestamp,
        stopped: current_in_progress_ids.clone(),
    };
    write_event(cwd, &start_event)?;

    // Update task statuses
    for task in &mut stopped_tasks {
        task.status = TaskStatus::Stopped;
    }
    for task in &mut started_tasks {
        task.status = TaskStatus::InProgress;
        task.stopped_reason = None;
    }

    // Determine output scope set (new scope if starting parent, or scope set from started tasks)
    let output_scope_set: ScopeSet = if let Some(ref s) = new_scope {
        ScopeSet {
            include_root: false,
            scopes: vec![s.clone()],
        }
    } else {
        // Build scope set from started tasks
        let mut include_root = false;
        let mut scopes: Vec<String> = Vec::new();
        for task in &started_tasks {
            if let Some(parent_id) = crate::tasks::id::get_parent_id(&task.id) {
                scopes.push(parent_id.to_string());
            } else {
                include_root = true;
            }
        }
        scopes.sort();
        scopes.dedup();
        ScopeSet {
            include_root,
            scopes,
        }
    };

    // Update context: started tasks are now in progress
    let updated_in_progress = started_tasks.clone();

    // Update ready queue based on new scope set
    let mut updated_ready: Vec<Task> = get_ready_queue_for_scope_set(&tasks, &output_scope_set)
        .into_iter()
        .filter(|t| !actual_ids_to_start.contains(&t.id))
        .map(|t| (*t).clone())
        .collect();

    // Add stopped tasks back to ready if they're in scope
    for task in &stopped_tasks {
        let task_parent = crate::tasks::id::get_parent_id(&task.id);
        let task_in_scope = match task_parent {
            None => output_scope_set.include_root || output_scope_set.is_empty(),
            Some(parent) => output_scope_set.scopes.iter().any(|s| s == parent),
        };
        if task_in_scope {
            updated_ready.push(task.clone());
        }
    }
    updated_ready.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Build output
    let mut content = String::new();

    // Show stopped tasks if any
    if !current_in_progress_ids.is_empty() {
        let stopped_task_refs: Vec<_> = stopped_tasks.iter().collect();
        content.push_str(&format_stopped(&stopped_task_refs, Some(&stop_reason)));
        content.push('\n');
    }

    // Show started tasks
    let started_task_refs: Vec<_> = started_tasks.iter().collect();
    content.push_str(&format_started(&started_task_refs));

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let updated_ready_refs: Vec<_> = updated_ready.iter().collect();

    let mut builder = XmlBuilder::new("start");
    if !output_scope_set.scopes.is_empty() {
        builder = builder.with_scopes(&output_scope_set.scopes);
    }
    let xml = builder.build(&content, &updated_in_progress_refs, &updated_ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Stop the current task
fn run_stop(
    cwd: &Path,
    id: Option<String>,
    reason: Option<String>,
    blocked: Option<String>,
) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine which task to stop
    let task_id = if let Some(id) = id {
        // Verify task exists and is in progress
        if let Some(task) = find_task(&tasks, &id) {
            if task.status != TaskStatus::InProgress {
                // Task exists but isn't in progress - still allow stopping if it's open
                if task.status != TaskStatus::Open {
                    return Err(AikiError::TaskNotFound(format!(
                        "Task '{}' is not in progress",
                        id
                    )));
                }
            }
            id
        } else {
            return Err(AikiError::TaskNotFound(id));
        }
    } else {
        // Default to first in-progress task
        if let Some(task) = in_progress.first() {
            task.id.clone()
        } else {
            // Try to print an error response
            let xml = XmlBuilder::new("stop")
                .error()
                .build_error("No task in progress to stop");
            println!("{}", xml);
            return Ok(());
        }
    };

    // Get the task before stopping (for output)
    let mut stopped_task = tasks.get(&task_id).expect("Task should exist").clone();

    // Stop the task (batch operation with single task)
    let stop_event = TaskEvent::Stopped {
        task_ids: vec![task_id.clone()],
        reason: reason.clone(),
        blocked_reason: blocked.clone(),
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &stop_event)?;

    // If blocked reason provided, create a blocker task
    if let Some(blocked_reason) = &blocked {
        let blocker_id = generate_task_id(blocked_reason);
        let blocker_event = TaskEvent::Created {
            task_id: blocker_id,
            name: blocked_reason.clone(),
            priority: TaskPriority::P0, // Blockers are high priority
            assignee: Some("human".to_string()),
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &blocker_event)?;
    }

    // Update stopped task status
    stopped_task.status = TaskStatus::Stopped;

    // Update context: remove from in_progress
    let updated_in_progress: Vec<Task> = in_progress
        .into_iter()
        .filter(|t| t.id != task_id)
        .map(|t| (*t).clone())
        .collect();

    // Determine scope set based on remaining in-progress tasks
    let mut include_root = false;
    let mut scopes: Vec<String> = Vec::new();
    for task in &updated_in_progress {
        if let Some(parent_id) = crate::tasks::id::get_parent_id(&task.id) {
            scopes.push(parent_id.to_string());
        } else {
            include_root = true;
        }
    }
    scopes.sort();
    scopes.dedup();
    let scope_set = ScopeSet { include_root, scopes };

    // Get scoped ready queue
    let mut ready: Vec<Task> = get_ready_queue_for_scope_set(&tasks, &scope_set)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();

    // Add stopped task if it's in scope
    let stopped_in_scope = match (crate::tasks::id::get_parent_id(&stopped_task.id), &scope_set) {
        // Root task in scope if root included or no scopes
        (None, ss) => ss.include_root || ss.is_empty(),
        // Child task in scope if parent is in scopes
        (Some(parent), ss) => ss.scopes.iter().any(|s| s == parent),
    };
    if stopped_in_scope {
        ready.push(stopped_task.clone());
    }
    ready.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Build output
    let content = format_stopped(&[&stopped_task], reason.as_deref());

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let ready_refs: Vec<_> = ready.iter().collect();

    let mut builder = XmlBuilder::new("stop");
    if !scope_set.scopes.is_empty() {
        builder = builder.with_scopes(&scope_set.scopes);
    }
    let xml = builder.build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Close task(s) as done
fn run_close(cwd: &Path, ids: Vec<String>, wont_do: bool) -> Result<()> {
    use crate::tasks::manager::{all_children_closed, get_unclosed_children};

    let events = read_events(cwd)?;
    let mut tasks = materialize_tasks(&events);

    // Get in-progress task IDs first (to avoid borrow issues)
    let in_progress_ids: Vec<String> = get_in_progress(&tasks)
        .iter()
        .map(|t| t.id.clone())
        .collect();

    // Determine which task(s) to close
    let ids_to_close = if ids.is_empty() {
        // Default to current in-progress tasks
        if in_progress_ids.is_empty() {
            let xml = XmlBuilder::new("close")
                .error()
                .build_error("No task in progress to close");
            println!("{}", xml);
            return Ok(());
        }
        in_progress_ids.clone()
    } else {
        // Validate all IDs exist
        for id in &ids {
            if find_task(&tasks, id).is_none() {
                return Err(AikiError::TaskNotFound(id.clone()));
            }
        }
        ids
    };

    // Check if any task being closed is a parent with unclosed children
    for id in &ids_to_close {
        if has_children(&tasks, id) {
            let unclosed = get_unclosed_children(&tasks, id);
            if !unclosed.is_empty() {
                let unclosed_ids: Vec<_> = unclosed.iter().map(|t| t.id.as_str()).collect();
                let xml = XmlBuilder::new("close").error().build_error(&format!(
                    "Cannot close {}, children still open: {}",
                    id,
                    unclosed_ids.join(", ")
                ));
                println!("{}", xml);
                return Ok(());
            }
        }
    }

    let outcome = if wont_do {
        TaskOutcome::WontDo
    } else {
        TaskOutcome::Done
    };

    // Get tasks before closing (for output)
    let mut closed_tasks: Vec<_> = ids_to_close
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();

    // Close the tasks (batch operation)
    let close_event = TaskEvent::Closed {
        task_ids: ids_to_close.clone(),
        outcome,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &close_event)?;

    // Update closed tasks status in local state
    for task in &mut closed_tasks {
        task.status = TaskStatus::Closed;
        task.closed_outcome = Some(outcome);
    }
    for id in &ids_to_close {
        if let Some(task) = tasks.get_mut(id) {
            task.status = TaskStatus::Closed;
            task.closed_outcome = Some(outcome);
        }
    }

    // Collect all unique parent IDs from closed tasks for auto-start check
    let unique_parent_ids: HashSet<String> = ids_to_close
        .iter()
        .filter_map(|id| crate::tasks::id::get_parent_id(id).map(|s| s.to_string()))
        .collect();

    // Check each parent for auto-start eligibility
    let mut auto_started_parents: Vec<Task> = Vec::new();
    let mut notices: Vec<String> = Vec::new();

    for parent_id in &unique_parent_ids {
        // Check if all children are now closed
        if all_children_closed(&tasks, parent_id) {
            if let Some(parent) = tasks.get_mut(parent_id) {
                // Guard: skip if already closed or in-progress
                if parent.status == TaskStatus::Closed {
                    continue;
                }
                if parent.status == TaskStatus::InProgress {
                    continue;
                }

                // Auto-start the parent for review/finalization
                let start_event = TaskEvent::Started {
                    task_ids: vec![parent_id.clone()],
                    agent_type: "claude-code".to_string(),
                    timestamp: chrono::Utc::now(),
                    stopped: Vec::new(),
                };
                write_event(cwd, &start_event)?;

                parent.status = TaskStatus::InProgress;
                auto_started_parents.push(parent.clone());
                notices.push(format!(
                    "All subtasks complete. Parent task (id: {}) auto-started for review/finalization.",
                    parent_id
                ));
            }
        }
    }

    // Update context: remove closed tasks from in_progress
    let mut updated_in_progress: Vec<Task> = in_progress_ids
        .iter()
        .filter(|id| !ids_to_close.contains(id))
        .filter_map(|id| tasks.get(id).cloned())
        .collect();

    // Add auto-started parents to in_progress
    for parent in &auto_started_parents {
        updated_in_progress.push(parent.clone());
    }

    // Determine output scope set based on updated in-progress tasks
    let mut include_root = false;
    let mut output_scopes: Vec<String> = Vec::new();
    for task in &updated_in_progress {
        if let Some(parent_id) = crate::tasks::id::get_parent_id(&task.id) {
            output_scopes.push(parent_id.to_string());
        } else {
            include_root = true;
        }
    }
    output_scopes.sort();
    output_scopes.dedup();
    let scope_set = ScopeSet {
        include_root,
        scopes: output_scopes,
    };

    // Get scoped ready queue
    let ready: Vec<Task> = get_ready_queue_for_scope_set(&tasks, &scope_set)
        .into_iter()
        .filter(|t| !ids_to_close.contains(&t.id))
        .map(|t| (*t).clone())
        .collect();

    // Build output
    let mut content = String::new();

    let closed_task_refs: Vec<_> = closed_tasks.iter().collect();
    content.push_str(&format_closed(&closed_task_refs, &outcome.to_string()));

    // Add auto-started parents to output
    if !auto_started_parents.is_empty() {
        content.push('\n');
        let parent_refs: Vec<_> = auto_started_parents.iter().collect();
        content.push_str(&format_started(&parent_refs));
    }

    // Add notices if present
    for notice in &notices {
        content.push_str(&format!(
            "\n  <notice>{}</notice>",
            crate::tasks::xml::escape_xml(notice)
        ));
    }

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let ready_refs: Vec<_> = ready.iter().collect();

    let mut builder = XmlBuilder::new("close");
    if !scope_set.scopes.is_empty() {
        builder = builder.with_scopes(&scope_set.scopes);
    }
    let xml = builder.build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

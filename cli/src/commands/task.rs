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
use crate::tasks::{
    id::generate_task_id,
    manager::{find_task, get_in_progress, get_ready_queue, materialize_tasks},
    storage::{read_events, write_event},
    types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus},
    xml::{format_added, format_closed, format_started, format_stopped, format_task_list},
    XmlBuilder,
};

/// Task subcommands
#[derive(Subcommand)]
pub enum TaskCommands {
    /// Show ready queue (default when no subcommand given)
    List,

    /// Create a new task
    Add {
        /// Task name
        name: String,
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
        TaskCommands::List => run_list(&cwd),
        TaskCommands::Add { name } => run_add(&cwd, name),
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
fn run_list(cwd: &Path) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let ready = get_ready_queue(&tasks);
    let in_progress = get_in_progress(&tasks);

    let content = format_task_list(&ready);

    let xml = XmlBuilder::new("list").build(&content, &in_progress, &ready);

    println!("{}", xml);
    Ok(())
}

/// Add a new task
fn run_add(cwd: &Path, name: String) -> Result<()> {
    // Read current state first (needed for context)
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    let task_id = generate_task_id(&name);
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
    use crate::tasks::types::Task;
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

    // Update ready queue (new task is now ready)
    let mut ready: Vec<Task> = get_ready_queue(&tasks)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();
    ready.push(new_task.clone());
    ready.sort_by(|a, b| a.priority.cmp(&b.priority));

    let content = format_added(&[&new_task]);

    let ready_refs: Vec<_> = ready.iter().collect();
    let xml = XmlBuilder::new("add").build(&content, &in_progress, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Start working on task(s)
fn run_start(cwd: &Path, ids: Vec<String>) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let current_in_progress = get_in_progress(&tasks);
    let ready = get_ready_queue(&tasks);

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

    // Get tasks before state changes (for output)
    let mut stopped_tasks: Vec<Task> = current_in_progress.iter().map(|t| (*t).clone()).collect();
    let mut started_tasks: Vec<Task> = ids_to_start
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();

    // Auto-stop current in-progress tasks (batch operation)
    let stop_reason = format!("Started {}", ids_to_start.join(", "));
    let stopped_ids: Vec<String> = current_in_progress.iter().map(|t| t.id.clone()).collect();

    if !stopped_ids.is_empty() {
        let stop_event = TaskEvent::Stopped {
            task_ids: stopped_ids.clone(),
            reason: Some(stop_reason.clone()),
            blocked_reason: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &stop_event)?;
    }

    // Start new tasks (batch operation)
    let timestamp = chrono::Utc::now();
    let start_event = TaskEvent::Started {
        task_ids: ids_to_start.clone(),
        agent_type: "claude-code".to_string(), // TODO: get from context
        timestamp,
        stopped_tasks: stopped_ids.clone(),
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

    // Update context: started tasks are now in progress
    let updated_in_progress = started_tasks.clone();

    // Update ready queue: remove started tasks, add stopped tasks
    let mut updated_ready: Vec<Task> = ready
        .into_iter()
        .filter(|t| !ids_to_start.contains(&t.id))
        .map(|t| (*t).clone())
        .collect();
    updated_ready.extend(stopped_tasks.clone());
    updated_ready.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Build output
    let mut content = String::new();

    // Show stopped tasks if any
    if !stopped_ids.is_empty() {
        let stopped_task_refs: Vec<_> = stopped_tasks.iter().collect();
        content.push_str(&format_stopped(&stopped_task_refs, Some(&stop_reason)));
        content.push('\n');
    }

    // Show started tasks
    let started_task_refs: Vec<_> = started_tasks.iter().collect();
    content.push_str(&format_started(&started_task_refs));

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let updated_ready_refs: Vec<_> = updated_ready.iter().collect();
    let xml =
        XmlBuilder::new("start").build(&content, &updated_in_progress_refs, &updated_ready_refs);

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

    // Update context: remove from in_progress, add to ready
    let updated_in_progress: Vec<Task> = in_progress
        .into_iter()
        .filter(|t| t.id != task_id)
        .map(|t| (*t).clone())
        .collect();
    let mut ready: Vec<Task> = get_ready_queue(&tasks)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();
    ready.push(stopped_task.clone());
    ready.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Build output
    let content = format_stopped(&[&stopped_task], reason.as_deref());

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let ready_refs: Vec<_> = ready.iter().collect();
    let xml = XmlBuilder::new("stop").build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Close task(s) as done
fn run_close(cwd: &Path, ids: Vec<String>, wont_do: bool) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine which task(s) to close
    let ids_to_close = if ids.is_empty() {
        // Default to current in-progress tasks
        if in_progress.is_empty() {
            let xml = XmlBuilder::new("close")
                .error()
                .build_error("No task in progress to close");
            println!("{}", xml);
            return Ok(());
        }
        in_progress.iter().map(|t| t.id.clone()).collect()
    } else {
        // Validate all IDs exist
        for id in &ids {
            if find_task(&tasks, id).is_none() {
                return Err(AikiError::TaskNotFound(id.clone()));
            }
        }
        ids
    };

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

    // Update closed tasks status
    for task in &mut closed_tasks {
        task.status = TaskStatus::Closed;
        task.closed_outcome = Some(outcome);
    }

    // Update context: remove closed tasks from in_progress
    let updated_in_progress: Vec<Task> = in_progress
        .into_iter()
        .filter(|t| !ids_to_close.contains(&t.id))
        .map(|t| (*t).clone())
        .collect();
    let ready: Vec<Task> = get_ready_queue(&tasks)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();

    // Build output
    let closed_task_refs: Vec<_> = closed_tasks.iter().collect();
    let content = format_closed(&closed_task_refs, &outcome.to_string());

    let updated_in_progress_refs: Vec<_> = updated_in_progress.iter().collect();
    let ready_refs: Vec<_> = ready.iter().collect();
    let xml = XmlBuilder::new("close").build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

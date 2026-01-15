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
use std::collections::HashSet;

use crate::tasks::{
    generate_child_id, generate_task_id, get_next_child_number,
    manager::{
        find_task, get_current_scope_set, get_in_progress, get_ready_queue_for_scope_set,
        has_children, materialize_tasks, ScopeSet,
    },
    storage::{read_events, write_event},
    types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus},
    xml::{format_added, format_closed, format_started, format_stopped, format_task_list},
    XmlBuilder,
};

/// Task subcommands
#[derive(Subcommand)]
pub enum TaskCommands {
    /// Show ready queue (default when no subcommand given)
    List {
        /// Show all tasks (not just ready queue)
        #[arg(long)]
        all: bool,

        /// Filter to open tasks only
        #[arg(long)]
        open: bool,

        /// Filter to in-progress tasks only
        #[arg(long)]
        in_progress: bool,

        /// Filter to stopped tasks only
        #[arg(long)]
        stopped: bool,

        /// Filter to closed tasks only
        #[arg(long)]
        closed: bool,
    },

    /// Create a new task
    Add {
        /// Task name
        name: String,

        /// Create as child of existing task
        #[arg(long)]
        parent: Option<String>,

        /// Set priority to P0 (critical/urgent)
        #[arg(long, group = "priority")]
        p0: bool,

        /// Set priority to P1 (high)
        #[arg(long, group = "priority")]
        p1: bool,

        /// Set priority to P2 (normal, default)
        #[arg(long, group = "priority")]
        p2: bool,

        /// Set priority to P3 (low)
        #[arg(long, group = "priority")]
        p3: bool,
    },

    /// Start working on task(s)
    Start {
        /// Task ID(s) to start (defaults to first from ready queue)
        #[arg(value_name = "ID")]
        ids: Vec<String>,

        /// Reopen a closed task before starting
        #[arg(long)]
        reopen: bool,

        /// Reason for reopening (required with --reopen)
        #[arg(long, requires = "reopen")]
        reason: Option<String>,
    },

    /// Stop the current task
    Stop {
        /// Task ID to stop (defaults to current in-progress task)
        id: Option<String>,

        /// Reason for stopping
        #[arg(long)]
        reason: Option<String>,

        /// Create blocker task(s) (assigned to human). Can be specified multiple times.
        #[arg(long, action = clap::ArgAction::Append)]
        blocked: Vec<String>,
    },

    /// Close task(s) as done
    Close {
        /// Task ID(s) to close (defaults to current in-progress task)
        #[arg(value_name = "ID")]
        ids: Vec<String>,

        /// Mark as won't do instead of done
        #[arg(long)]
        wont_do: bool,

        /// Comment to add before closing (use "-" for stdin/heredoc)
        #[arg(long)]
        comment: Option<String>,
    },

    /// Show task details (including children for parent tasks)
    Show {
        /// Task ID to show (defaults to current in-progress task)
        id: Option<String>,
    },

    /// Update task details
    Update {
        /// Task ID to update (defaults to current in-progress task)
        id: Option<String>,

        /// Set priority to P0 (critical/urgent)
        #[arg(long, group = "priority")]
        p0: bool,

        /// Set priority to P1 (high)
        #[arg(long, group = "priority")]
        p1: bool,

        /// Set priority to P2 (normal)
        #[arg(long, group = "priority")]
        p2: bool,

        /// Set priority to P3 (low)
        #[arg(long, group = "priority")]
        p3: bool,

        /// Update task name
        #[arg(long)]
        name: Option<String>,
    },

    /// Add a comment to a task
    Comment {
        /// Comment text (required)
        text: String,

        /// Task ID to comment on (defaults to current in-progress task)
        #[arg(long)]
        id: Option<String>,
    },
}

/// Main entry point for `aiki task` command
///
/// If no subcommand is provided, defaults to `list`.
pub fn run(command: Option<TaskCommands>) -> Result<()> {
    let cwd = std::env::current_dir()?;

    // Default to list if no subcommand provided
    let cmd = command.unwrap_or(TaskCommands::List {
        all: false,
        open: false,
        in_progress: false,
        stopped: false,
        closed: false,
    });

    match cmd {
        TaskCommands::List {
            all,
            open,
            in_progress,
            stopped,
            closed,
        } => run_list(&cwd, None, all, open, in_progress, stopped, closed),
        TaskCommands::Add {
            name,
            parent,
            p0,
            p1,
            p2,
            p3,
        } => run_add(&cwd, name, parent, p0, p1, p2, p3),
        TaskCommands::Start { ids, reopen, reason } => run_start(&cwd, ids, reopen, reason),
        TaskCommands::Stop {
            id,
            reason,
            blocked,
        } => run_stop(&cwd, id, reason, blocked),
        TaskCommands::Close { ids, wont_do, comment } => run_close(&cwd, ids, wont_do, comment),
        TaskCommands::Show { id } => run_show(&cwd, id),
        TaskCommands::Update {
            id,
            p0,
            p1,
            p2,
            p3,
            name,
        } => run_update(&cwd, id, p0, p1, p2, p3, name),
        TaskCommands::Comment { text, id } => run_comment(&cwd, text, id),
    }
}

/// List tasks in the ready queue
fn run_list(
    cwd: &Path,
    scope_override: Option<&str>,
    all: bool,
    filter_open: bool,
    filter_in_progress: bool,
    filter_stopped: bool,
    filter_closed: bool,
) -> Result<()> {
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

    // Collect active status filters
    let has_status_filters = filter_open || filter_in_progress || filter_stopped || filter_closed;

    // Always compute the actual ready queue for context (maintains contract)
    let ready_queue = get_ready_queue_for_scope_set(&tasks, &scope_set);

    // Get list of tasks based on filters (for display in content)
    let list_tasks: Vec<&Task> = if all || has_status_filters {
        // Show all tasks (or filtered by status)
        let mut all_tasks: Vec<_> = tasks.values().collect();
        all_tasks.sort_by(|a, b| a.priority.cmp(&b.priority));

        if has_status_filters {
            // Filter by status
            all_tasks
                .into_iter()
                .filter(|t| {
                    (filter_open && t.status == TaskStatus::Open)
                        || (filter_in_progress && t.status == TaskStatus::InProgress)
                        || (filter_stopped && t.status == TaskStatus::Stopped)
                        || (filter_closed && t.status == TaskStatus::Closed)
                })
                .collect()
        } else {
            all_tasks
        }
    } else {
        // Default: show ready queue (same as context)
        ready_queue.clone()
    };

    let in_progress = get_in_progress(&tasks);

    let content = format_task_list(&list_tasks);

    let mut builder = XmlBuilder::new("list");
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    // Context always uses the actual ready queue, not the filtered list
    let xml = builder.build(&content, &in_progress, &ready_queue);

    println!("{}", xml);
    Ok(())
}

/// Add a new task
fn run_add(
    cwd: &Path,
    name: String,
    parent: Option<String>,
    p0: bool,
    p1: bool,
    p2: bool,
    p3: bool,
) -> Result<()> {
    // Determine priority from flags (default P2)
    let priority = if p0 {
        TaskPriority::P0
    } else if p1 {
        TaskPriority::P1
    } else if p3 {
        TaskPriority::P3
    } else {
        TaskPriority::P2 // Default, also covers explicit --p2
    };

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
        priority,
        assignee: None, // Phase 3: auto-detect from agent
        timestamp,
    };

    write_event(cwd, &event)?;

    // Build new task from event (avoid re-reading)
    let new_task = Task {
        id: task_id,
        name,
        priority,
        status: TaskStatus::Open,
        assignee: None,
        created_at: timestamp,
        stopped_reason: None,
        closed_outcome: None,
        comments: Vec::new(),
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
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &in_progress, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Start working on task(s)
fn run_start(cwd: &Path, ids: Vec<String>, reopen: bool, reopen_reason: Option<String>) -> Result<()> {
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
        // Validate all IDs exist and check reopen requirements
        for id in &ids {
            if let Some(task) = find_task(&tasks, id) {
                if task.status == TaskStatus::Closed {
                    if !reopen {
                        let xml = XmlBuilder::new("start")
                            .error()
                            .build_error(&format!(
                                "Task '{}' is closed. Use --reopen --reason to reopen it.",
                                id
                            ));
                        println!("{}", xml);
                        return Ok(());
                    }
                    // Reopen requires a reason
                    if reopen_reason.is_none() {
                        let xml = XmlBuilder::new("start")
                            .error()
                            .build_error("--reopen requires --reason");
                        println!("{}", xml);
                        return Ok(());
                    }
                }
            } else {
                return Err(AikiError::TaskNotFound(id.clone()));
            }
        }
        ids
    };

    // Reopen closed tasks if --reopen was specified
    if reopen {
        if let Some(reason) = &reopen_reason {
            for id in &ids_to_start {
                if let Some(task) = find_task(&tasks, id) {
                    if task.status == TaskStatus::Closed {
                        let reopen_event = TaskEvent::Reopened {
                            task_id: id.clone(),
                            reason: reason.clone(),
                            timestamp: chrono::Utc::now(),
                        };
                        write_event(cwd, &reopen_event)?;

                        // Update local task state
                        if let Some(t) = tasks.get_mut(id) {
                            t.status = TaskStatus::Open;
                            t.closed_outcome = None;
                        }
                    }
                }
            }
        }
    }

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
                    comments: Vec::new(),
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
    let xml_scopes = output_scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
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
    blocked: Vec<String>,
) -> Result<()> {
    let events = read_events(cwd)?;
    let mut tasks = materialize_tasks(&events);

    // Get in-progress task IDs first (to avoid borrow conflicts)
    let in_progress_ids: Vec<String> = get_in_progress(&tasks)
        .iter()
        .map(|t| t.id.clone())
        .collect();

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
        if let Some(first_id) = in_progress_ids.first() {
            first_id.clone()
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
    // Store first blocked reason in event (for backward compatibility)
    let stop_event = TaskEvent::Stopped {
        task_ids: vec![task_id.clone()],
        reason: reason.clone(),
        blocked_reason: blocked.first().cloned(),
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &stop_event)?;

    // Create blocker tasks for each --blocked flag and add to in-memory map
    let timestamp = chrono::Utc::now();
    for blocked_reason in &blocked {
        let blocker_id = generate_task_id(blocked_reason);
        let blocker_event = TaskEvent::Created {
            task_id: blocker_id.clone(),
            name: blocked_reason.clone(),
            priority: TaskPriority::P0, // Blockers are high priority
            assignee: Some("human".to_string()),
            timestamp,
        };
        write_event(cwd, &blocker_event)?;

        // Add blocker task to in-memory map so it appears in ready queue
        tasks.insert(
            blocker_id.clone(),
            Task {
                id: blocker_id,
                name: blocked_reason.clone(),
                status: TaskStatus::Open,
                priority: TaskPriority::P0,
                assignee: Some("human".to_string()),
                created_at: timestamp,
                stopped_reason: None,
                closed_outcome: None,
                comments: Vec::new(),
            },
        );
    }

    // Update stopped task status
    stopped_task.status = TaskStatus::Stopped;

    // Update context: get in-progress tasks minus the stopped one
    let updated_in_progress: Vec<Task> = in_progress_ids
        .iter()
        .filter(|id| *id != &task_id)
        .filter_map(|id| tasks.get(id).cloned())
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
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Close task(s) as done
fn run_close(cwd: &Path, ids: Vec<String>, wont_do: bool, comment: Option<String>) -> Result<()> {
    use crate::tasks::manager::{all_children_closed, get_unclosed_children};
    use std::io::Read;

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

    // Handle stdin for --comment -
    let comment_text = if comment.as_deref() == Some("-") {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Some(buffer.trim().to_string())
    } else {
        comment
    };

    // Check if agent session is active - require comment for agent-initiated closes
    if comment_text.is_none() {
        let sessions_dir = cwd.join(".aiki/sessions");
        if sessions_dir.exists() {
            let session_count = std::fs::read_dir(&sessions_dir)
                .ok()
                .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
                .unwrap_or(0);

            if session_count > 0 {
                return Err(AikiError::TaskCommentRequired(
                    "Closing tasks requires a comment when running in an agent session. Please summarize your work with --comment.".to_string()
                ));
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

    // If comment provided, emit comment events first (1ms before close for chronological order)
    let close_timestamp = chrono::Utc::now();
    if let Some(ref comment) = comment_text {
        let comment_timestamp = close_timestamp - chrono::Duration::milliseconds(1);
        for task_id in &ids_to_close {
            let comment_event = TaskEvent::CommentAdded {
                task_id: task_id.clone(),
                text: comment.clone(),
                timestamp: comment_timestamp,
            };
            write_event(cwd, &comment_event)?;
        }
    }

    // Close the tasks (batch operation)
    let close_event = TaskEvent::Closed {
        task_ids: ids_to_close.clone(),
        outcome,
        timestamp: close_timestamp,
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
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &updated_in_progress_refs, &ready_refs);

    println!("{}", xml);
    Ok(())
}

/// Show task details (including children for parent tasks)
fn run_show(cwd: &Path, id: Option<String>) -> Result<()> {
    use crate::tasks::manager::get_children;
    use crate::tasks::xml::escape_xml;

    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine which task to show
    let task_id = if let Some(id) = id {
        if find_task(&tasks, &id).is_none() {
            return Err(AikiError::TaskNotFound(id));
        }
        id
    } else {
        // Default to first in-progress task
        if let Some(task) = in_progress.first() {
            task.id.clone()
        } else {
            let xml = XmlBuilder::new("show")
                .error()
                .build_error("No task in progress to show");
            println!("{}", xml);
            return Ok(());
        }
    };

    let task = tasks.get(&task_id).expect("Task should exist");

    // Get children if this is a parent task
    let children = get_children(&tasks, &task_id);
    let has_children = !children.is_empty();

    // Calculate progress if has children
    let (completed, total) = if has_children {
        let total = children.len();
        let completed = children
            .iter()
            .filter(|t| t.status == TaskStatus::Closed)
            .count();
        (completed, total)
    } else {
        (0, 0)
    };

    // Build task XML content
    let mut content = format!(
        "  <task id=\"{}\" name=\"{}\" status=\"{}\" priority=\"{}\">",
        escape_xml(&task.id),
        escape_xml(&task.name),
        task.status,
        task.priority
    );

    // Add children section if this is a parent
    if has_children {
        content.push_str("\n    <children>");
        for child in &children {
            content.push_str(&format!(
                "\n      <task id=\"{}\" status=\"{}\" name=\"{}\"/>",
                escape_xml(&child.id),
                child.status,
                escape_xml(&child.name)
            ));
        }
        content.push_str("\n    </children>");

        // Add progress element
        let percentage = if total > 0 {
            (completed * 100) / total
        } else {
            0
        };
        content.push_str(&format!(
            "\n    <progress completed=\"{}\" total=\"{}\" percentage=\"{}\"/>",
            completed, total, percentage
        ));
    }

    // Add comments if any
    if !task.comments.is_empty() {
        content.push_str("\n    <comments>");
        for comment in &task.comments {
            content.push_str(&format!(
                "\n      <comment timestamp=\"{}\">{}</comment>",
                comment.timestamp.to_rfc3339(),
                escape_xml(&comment.text)
            ));
        }
        content.push_str("\n    </comments>");
    }

    content.push_str("\n  </task>");

    // Get scope set and ready queue for context
    let scope_set = get_current_scope_set(&tasks);
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);

    let in_progress_refs: Vec<_> = in_progress.iter().map(|t| *t).collect();

    let mut builder = XmlBuilder::new("show");
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &in_progress_refs, &ready);

    println!("{}", xml);
    Ok(())
}

/// Update task details
fn run_update(
    cwd: &Path,
    id: Option<String>,
    p0: bool,
    p1: bool,
    p2: bool,
    p3: bool,
    name: Option<String>,
) -> Result<()> {
    use crate::tasks::xml::escape_xml;

    let events = read_events(cwd)?;
    let mut tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine which task to update
    let task_id = if let Some(id) = id {
        if find_task(&tasks, &id).is_none() {
            return Err(AikiError::TaskNotFound(id));
        }
        id
    } else {
        // Default to first in-progress task
        if let Some(task) = in_progress.first() {
            task.id.clone()
        } else {
            let xml = XmlBuilder::new("update")
                .error()
                .build_error("No task in progress to update");
            println!("{}", xml);
            return Ok(());
        }
    };

    // Determine new priority if any flag is set
    let new_priority = if p0 {
        Some(TaskPriority::P0)
    } else if p1 {
        Some(TaskPriority::P1)
    } else if p2 {
        Some(TaskPriority::P2)
    } else if p3 {
        Some(TaskPriority::P3)
    } else {
        None
    };

    // Check if there's anything to update
    if new_priority.is_none() && name.is_none() {
        let xml = XmlBuilder::new("update")
            .error()
            .build_error("No updates specified. Use --name or --p0/--p1/--p2/--p3");
        println!("{}", xml);
        return Ok(());
    }

    // Write the update event
    let event = TaskEvent::Updated {
        task_id: task_id.clone(),
        name: name.clone(),
        priority: new_priority,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &event)?;

    // Update the in-memory task and insert back into map
    {
        let task = tasks.get_mut(&task_id).expect("Task should exist");
        if let Some(ref new_name) = name {
            task.name = new_name.clone();
        }
        if let Some(new_p) = new_priority {
            task.priority = new_p;
        }
    }

    // Get updated task for output
    let updated_task = tasks.get(&task_id).expect("Task should exist");

    // Build output
    let content = format!(
        "  <updated>\n    <task id=\"{}\" name=\"{}\" priority=\"{}\"/>\n  </updated>",
        escape_xml(&updated_task.id),
        escape_xml(&updated_task.name),
        updated_task.priority
    );

    // Get scope set and ready queue for context (now uses updated tasks map)
    let scope_set = get_current_scope_set(&tasks);
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
    // Re-calculate in_progress since it may have changed
    let updated_in_progress = get_in_progress(&tasks);
    let in_progress_refs: Vec<_> = updated_in_progress.iter().map(|t| *t).collect();

    let mut builder = XmlBuilder::new("update");
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &in_progress_refs, &ready);

    println!("{}", xml);
    Ok(())
}

/// Add a comment to a task
fn run_comment(cwd: &Path, text: String, id: Option<String>) -> Result<()> {
    use crate::tasks::xml::escape_xml;

    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let in_progress = get_in_progress(&tasks);

    // Determine which task to comment on
    let task_id = if let Some(id) = id {
        if find_task(&tasks, &id).is_none() {
            return Err(AikiError::TaskNotFound(id));
        }
        id
    } else {
        // Default to first in-progress task
        if let Some(task) = in_progress.first() {
            task.id.clone()
        } else {
            let xml = XmlBuilder::new("comment")
                .error()
                .build_error("No task in progress to comment on");
            println!("{}", xml);
            return Ok(());
        }
    };

    let timestamp = chrono::Utc::now();

    // Write the comment event
    let event = TaskEvent::CommentAdded {
        task_id: task_id.clone(),
        text: text.clone(),
        timestamp,
    };
    write_event(cwd, &event)?;

    // Build output
    let content = format!(
        "  <comment_added task_id=\"{}\" timestamp=\"{}\">\n    <text>{}</text>\n  </comment_added>",
        escape_xml(&task_id),
        timestamp.to_rfc3339(),
        escape_xml(&text)
    );

    // Get scope set and ready queue for context
    let scope_set = get_current_scope_set(&tasks);
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);
    let in_progress_refs: Vec<_> = in_progress.iter().map(|t| *t).collect();

    let mut builder = XmlBuilder::new("comment");
    let xml_scopes = scope_set.to_xml_scopes();
    if !xml_scopes.is_empty() {
        builder = builder.with_scopes(&xml_scopes);
    }
    let xml = builder.build(&content, &in_progress_refs, &ready);

    println!("{}", xml);
    Ok(())
}

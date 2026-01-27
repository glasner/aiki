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

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use std::collections::HashSet;

use crate::tasks::{
    generate_child_id, generate_task_id, get_next_subtask_number, is_task_id,
    manager::{
        find_task, get_current_scope_set, get_in_progress, get_ready_queue_for_agent_scoped,
        get_ready_queue_for_scope_set, has_subtasks, materialize_tasks, ScopeSet,
    },
    runner::{run_task_with_xml, TaskRunOptions},
    storage::{read_events, write_event},
    types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus},
    xml::{format_added, format_closed, format_started, format_stopped, format_task_list},
    XmlBuilder,
};

/// Valid prefixes for task sources
const VALID_SOURCE_PREFIXES: &[&str] = &["file:", "task:", "comment:", "issue:", "prompt:"];

/// Validate that all sources have valid prefixes
///
/// Sources must start with one of: file:, task:, comment:, issue:, prompt:
/// The special source "prompt" (without colon) is also valid and will be resolved
/// to the latest prompt's change_id for the current session.
/// Returns an error with the first invalid source if validation fails.
fn validate_sources(sources: &[String]) -> Result<()> {
    for source in sources {
        // "prompt" without colon is special - will be resolved later
        if source == "prompt" {
            continue;
        }
        let has_valid_prefix = VALID_SOURCE_PREFIXES
            .iter()
            .any(|prefix| source.starts_with(prefix));
        if !has_valid_prefix {
            return Err(AikiError::InvalidTaskSource(source.clone()));
        }
    }
    Ok(())
}

/// Resolve "prompt" source to the actual prompt change_id
///
/// If `--source prompt` is used (without an explicit ID), this function resolves it
/// to `prompt:<change_id>` using the latest prompt from the current session.
///
/// Returns the sources with "prompt" replaced, or an error if resolution fails.
fn resolve_prompt_sources(cwd: &Path, mut sources: Vec<String>) -> Result<Vec<String>> {
    use crate::global;
    use crate::history::get_latest_prompt_change_id;
    use crate::session::find_active_session;

    // Check if any source is the special "prompt" (without ID)
    let has_bare_prompt = sources.iter().any(|s| s == "prompt");
    if !has_bare_prompt {
        return Ok(sources);
    }

    // Find the current session via PID or agent-type detection
    let session =
        find_active_session(cwd).ok_or(AikiError::NoActiveSessionForPromptSource)?;

    // Get the latest prompt's change_id for this session.
    // Conversation history is stored in the global JJ repo at ~/.aiki/, not the project repo.
    // The prompt event may not be written yet (hook fires concurrently with event recording),
    // so retry a few times with backoff before giving up.
    let global_dir = global::global_aiki_dir();
    let mut prompt_change_id = None;
    for attempt in 0..10 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(50 * attempt));
        }
        if let Some(id) = get_latest_prompt_change_id(&global_dir, &session.session_id)? {
            prompt_change_id = Some(id);
            break;
        }
    }

    let change_id = prompt_change_id.ok_or(AikiError::NoPromptEventsForSession)?;

    // Replace "prompt" with "prompt:<change_id>"
    for source in &mut sources {
        if source == "prompt" {
            *source = format!("prompt:{}", change_id);
        }
    }

    Ok(sources)
}

/// Template subcommands for `aiki task template`
#[derive(Subcommand)]
pub enum TemplateCommands {
    /// List all available templates
    List,

    /// Show details of a specific template
    Show {
        /// Template name (e.g., "aiki/review")
        name: String,
    },
}

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

        /// Filter to tasks assigned to specific agent or human
        #[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
        assignee: Option<String>,

        /// Filter to unassigned tasks only
        #[arg(long)]
        unassigned: bool,

        /// Filter to tasks from a specific source (supports partial matching)
        #[arg(long)]
        source: Option<String>,

        /// Filter to tasks created from a specific template (e.g., "aiki/review", "myorg/build@1.0")
        #[arg(long)]
        template: Option<String>,
    },

    /// List or show templates
    Template {
        #[command(subcommand)]
        command: TemplateCommands,
    },

    /// Create a new task
    ///
    /// Create a task either by name or from a template.
    ///
    /// Examples:
    ///   aiki task add "Implement user auth"
    ///   aiki task add --template aiki/review --data scope="@"
    ///   aiki task add --template myorg/build --source file:ops/now/feature.md
    Add {
        /// Task name (required unless --template is provided)
        name: Option<String>,

        /// Create from a template (e.g., "aiki/review", "myorg/refactor-cleanup")
        #[arg(long)]
        template: Option<String>,

        /// Set task data (for template-based tasks). Can be specified multiple times.
        #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        data: Vec<String>,

        /// Create as child of existing task
        #[arg(long)]
        parent: Option<String>,

        /// Assign to specific agent or human (claude-code, codex, cursor, gemini, human)
        #[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
        assignee: Option<String>,

        /// Source that spawned this task (e.g., "file:ops/now/design.md", "task:abc123")
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        source: Vec<String>,

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
    ///
    /// Accepts either task ID(s), a description, or --template for template-based tasks.
    ///
    /// Examples:
    ///   aiki task start "Implement user auth"  # Quick-start: create and start
    ///   aiki task start xmryrzwl...           # Start existing task by ID
    ///   aiki task start --template aiki/review --data scope="@"  # Create from template and start
    Start {
        /// Task ID(s) or description to start
        ///
        /// If a description (not a task ID), creates and starts a new task.
        #[arg(value_name = "ID_OR_DESCRIPTION")]
        ids: Vec<String>,

        /// Create from template and start (quick-start pattern for templates)
        #[arg(long)]
        template: Option<String>,

        /// Set task data (for template-based tasks). Can be specified multiple times.
        #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        data: Vec<String>,

        /// Reopen a closed task before starting
        #[arg(long)]
        reopen: bool,

        /// Reason for reopening (required with --reopen)
        #[arg(long, requires = "reopen")]
        reason: Option<String>,

        /// Set priority to P0 (critical/urgent) for new task
        #[arg(long, group = "priority")]
        p0: bool,

        /// Set priority to P1 (high) for new task
        #[arg(long, group = "priority")]
        p1: bool,

        /// Set priority to P2 (normal, default) for new task
        #[arg(long, group = "priority")]
        p2: bool,

        /// Set priority to P3 (low) for new task
        #[arg(long, group = "priority")]
        p3: bool,

        /// Source that spawned this task (for quick-start)
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        source: Vec<String>,

        /// Override template assignee
        #[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
        assignee: Option<String>,
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

    /// Show task details (including subtasks for parent tasks)
    Show {
        /// Task ID to show (defaults to current in-progress task)
        id: Option<String>,

        /// Show full diffs for all changes made during this task
        #[arg(long)]
        diff: bool,
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

        /// Reassign to specific agent or human (claude-code, codex, cursor, gemini, human)
        #[arg(
            long = "for",
            visible_alias = "assignee",
            value_name = "AGENT",
            group = "assign"
        )]
        assignee: Option<String>,

        /// Remove assignee (make task unassigned)
        #[arg(long, group = "assign")]
        unassign: bool,
    },

    /// Add a comment to a task
    Comment {
        /// Comment text (required)
        text: String,

        /// Task ID to comment on (defaults to current in-progress task)
        #[arg(long)]
        id: Option<String>,
    },

    /// Run a task by spawning an agent session
    ///
    /// Spawns an agent session to work on the specified task. The agent will:
    /// 1. Claim the task via `aiki task start`
    /// 2. Execute the task (following instructions/subtasks)
    /// 3. Close the task when complete
    Run {
        /// Task ID to run
        id: String,

        /// Override assignee agent (claude-code, codex)
        #[arg(long)]
        agent: Option<String>,
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
        assignee: None,
        unassigned: false,
        source: None,
        template: None,
    });

    match cmd {
        TaskCommands::List {
            all,
            open,
            in_progress,
            stopped,
            closed,
            assignee,
            unassigned,
            source,
            template,
        } => run_list(
            &cwd,
            None,
            all,
            open,
            in_progress,
            stopped,
            closed,
            assignee,
            unassigned,
            source,
            template,
        ),
        TaskCommands::Template { command } => run_template(&cwd, command),
        TaskCommands::Add {
            name,
            template,
            data,
            parent,
            assignee,
            source,
            p0,
            p1,
            p2,
            p3,
        } => run_add(&cwd, name, template, data, parent, assignee, source, p0, p1, p2, p3),
        TaskCommands::Start {
            ids,
            template,
            data,
            reopen,
            reason,
            p0,
            p1,
            p2,
            p3,
            source,
            assignee,
        } => run_start(&cwd, ids, template, data, reopen, reason, p0, p1, p2, p3, source, assignee),
        TaskCommands::Stop {
            id,
            reason,
            blocked,
        } => run_stop(&cwd, id, reason, blocked),
        TaskCommands::Close {
            ids,
            wont_do,
            comment,
        } => run_close(&cwd, ids, wont_do, comment),
        TaskCommands::Show { id, diff } => run_show(&cwd, id, diff),
        TaskCommands::Update {
            id,
            p0,
            p1,
            p2,
            p3,
            name,
            assignee,
            unassign,
        } => run_update(&cwd, id, p0, p1, p2, p3, name, assignee, unassign),
        TaskCommands::Comment { text, id } => run_comment(&cwd, text, id),
        TaskCommands::Run { id, agent } => run_run(&cwd, id, agent),
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
    filter_assignee: Option<String>,
    filter_unassigned: bool,
    filter_source: Option<String>,
    filter_template: Option<String>,
) -> Result<()> {
    use crate::agents::{AgentType, Assignee};
    use crate::session::find_active_session;

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
    let has_explicit_assignee_filters = filter_assignee.is_some() || filter_unassigned;

    // Validate and normalize assignee filter if provided
    // Converts "claude" → "claude-code", "me" → "human"
    let normalized_filter_assignee = if let Some(ref a) = filter_assignee {
        match Assignee::from_str(a) {
            Some(parsed) => parsed.as_str().map(|s| s.to_string()),
            None => return Err(AikiError::UnknownAssignee(a.clone())),
        }
    } else {
        None
    };

    // Session detection: find session by PID matching or agent-type fallback
    // This automatically finds our session without needing --session flag
    let session_match = find_active_session(cwd);
    let detected_agent: Option<AgentType> = session_match.as_ref().map(|m| m.agent_type);
    let our_session_uuid: Option<String> = session_match.map(|m| m.session_id);

    // Determine automatic assignee filtering based on session context
    // If no explicit filter is set and not --all, apply visibility rules:
    // - Agent detected: show tasks assigned to that agent + unassigned
    // - No agent detected: show tasks assigned to human + unassigned (human terminal mode)
    let auto_agent_filter: Option<AgentType> = if !all && !has_explicit_assignee_filters {
        detected_agent
    } else {
        None
    };

    // Whether we should apply human visibility filtering (no agent detected)
    let apply_human_filter = !all && !has_explicit_assignee_filters && detected_agent.is_none();

    // Helper closure to check if a task is visible based on auto-filtering
    let is_auto_visible = |task: &Task| -> bool {
        if all {
            return true; // --all bypasses all auto-filtering
        }

        let assignee = task
            .assignee
            .as_ref()
            .and_then(|s| Assignee::from_str(s))
            .unwrap_or(Assignee::Unassigned);

        if let Some(ref agent) = auto_agent_filter {
            assignee.is_visible_to(agent)
        } else if apply_human_filter {
            // Human mode: show human-assigned + unassigned
            assignee.is_visible_to_human()
        } else {
            true // No auto-filtering
        }
    };

    // Helper closure to check explicit assignee filter
    let matches_explicit_filter = |task: &Task| -> bool {
        if !has_explicit_assignee_filters {
            return true;
        }
        if filter_unassigned {
            task.assignee.is_none()
        } else if let Some(ref target) = normalized_filter_assignee {
            task.assignee.as_ref().map(|a| a == target).unwrap_or(false)
        } else {
            true
        }
    };

    // Helper closure to check session ownership
    // Task is visible if: unclaimed OR claimed by our session
    let matches_session = |task: &Task| -> bool {
        if all {
            return true; // --all bypasses session filtering
        }
        match (&task.claimed_by_session, &our_session_uuid) {
            (None, _) => true,                              // Unclaimed tasks visible to all
            (Some(claimed), Some(ours)) => claimed == ours, // Claimed tasks visible only to owner
            (Some(_), None) => false, // Claimed tasks not visible without session
        }
    };

    // Helper closure to check source filter
    // Supports partial matching: "ops/now/assign.md" matches "file:ops/now/assign.md"
    let matches_source = |task: &Task| -> bool {
        match &filter_source {
            None => true, // No filter applied
            Some(query) => {
                // Match if any source in the task's sources list matches the query
                task.sources.iter().any(|source| {
                    // Exact match
                    source == query ||
                    // Partial match: query without prefix matches source
                    source.ends_with(query) ||
                    // Partial match: source without prefix matches query
                    source.split(':').nth(1).map_or(false, |suffix| suffix == query)
                })
            }
        }
    };

    // Helper closure to check template filter
    // Supports exact match and version-agnostic matching:
    // - "aiki/review" matches "aiki/review" and "aiki/review@1.0.0"
    // - "aiki/review@1.0.0" only matches "aiki/review@1.0.0"
    let matches_template = |task: &Task| -> bool {
        match (&filter_template, &task.template) {
            (None, _) => true, // No filter applied
            (Some(_), None) => false, // Filter applied but task has no template
            (Some(query), Some(task_template)) => {
                // Exact match
                task_template == query ||
                // Version-agnostic match: query without version matches task_template with version
                task_template.split('@').next() == Some(query)
            }
        }
    };

    // Always compute the actual ready queue for context (maintains contract)
    // Apply agent/human filtering AND session filtering
    let ready_queue: Vec<&Task> = if let Some(ref agent) = auto_agent_filter {
        get_ready_queue_for_agent_scoped(&tasks, &scope_set, agent)
            .into_iter()
            .filter(|t| matches_session(t))
            .collect()
    } else if apply_human_filter {
        // Human mode: filter to human-visible tasks
        get_ready_queue_for_scope_set(&tasks, &scope_set)
            .into_iter()
            .filter(|t| is_auto_visible(t) && matches_session(t))
            .collect()
    } else {
        get_ready_queue_for_scope_set(&tasks, &scope_set)
            .into_iter()
            .filter(|t| matches_session(t))
            .collect()
    };

    // Get list of tasks based on filters (for display in content)
    let list_tasks: Vec<&Task> =
        if all || has_status_filters || has_explicit_assignee_filters || filter_source.is_some() || filter_template.is_some() {
            // Show tasks with filters applied
            let mut all_tasks: Vec<_> = tasks.values().collect();
            all_tasks.sort_by(|a, b| a.priority.cmp(&b.priority));

            // Apply status filters if active
            let filtered_by_status: Vec<_> = if has_status_filters {
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
            };

            // Apply explicit assignee filters if active
            let filtered_by_assignee: Vec<_> = if has_explicit_assignee_filters {
                filtered_by_status
                    .into_iter()
                    .filter(|t| matches_explicit_filter(t))
                    .collect()
            } else {
                filtered_by_status
            };

            // Apply source filter if active
            let filtered_by_source: Vec<_> = if filter_source.is_some() {
                filtered_by_assignee
                    .into_iter()
                    .filter(|t| matches_source(t))
                    .collect()
            } else {
                filtered_by_assignee
            };

            // Apply template filter if active
            let filtered_by_template: Vec<_> = if filter_template.is_some() {
                filtered_by_source
                    .into_iter()
                    .filter(|t| matches_template(t))
                    .collect()
            } else {
                filtered_by_source
            };

            // Apply auto visibility filter (unless --all is specified or explicit filter is used)
            // This ensures status filters still respect assignee visibility
            // Also apply session filtering
            let filtered_by_visibility: Vec<_> = if !all && !has_explicit_assignee_filters {
                filtered_by_template
                    .into_iter()
                    .filter(|t| is_auto_visible(t))
                    .collect()
            } else {
                filtered_by_template
            };

            // Apply session filtering
            filtered_by_visibility
                .into_iter()
                .filter(|t| matches_session(t))
                .collect()
        } else {
            // Default: show ready queue (same as context)
            ready_queue.clone()
        };

    // Get in-progress tasks, filtered by:
    // 1. Explicit assignee filter (--for/--unassigned) if specified
    // 2. Otherwise, auto visibility filter based on session context
    // 3. Session ownership filter
    let in_progress: Vec<&Task> = get_in_progress(&tasks)
        .into_iter()
        .filter(|t| {
            let assignee_visible = if has_explicit_assignee_filters {
                matches_explicit_filter(t)
            } else {
                is_auto_visible(t)
            };
            assignee_visible && matches_session(t)
        })
        .collect();

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
    name: Option<String>,
    template_name: Option<String>,
    data_args: Vec<String>,
    parent: Option<String>,
    assignee_arg: Option<String>,
    sources: Vec<String>,
    p0: bool,
    p1: bool,
    _p2: bool,
    p3: bool,
) -> Result<()> {
    use crate::agents::Assignee;

    // If --template is provided, delegate to template-based creation
    if let Some(ref template) = template_name {
        // Template-based creation doesn't support --parent (templates define their own structure)
        if parent.is_some() {
            return Err(AikiError::InvalidArgument(
                "--parent cannot be used with --template (templates define their own task structure)".to_string()
            ));
        }

        let task_id = create_from_template(cwd, template, &data_args, &sources, assignee_arg.as_deref(), p0, p1, false, p3)?;

        // Read events to get the task we just created
        let events = read_events(cwd)?;
        let tasks = materialize_tasks(&events);
        let in_progress = get_in_progress(&tasks);

        let task = tasks.get(&task_id).ok_or_else(|| AikiError::TaskNotFound(task_id.clone()))?;

        // Get scope set and ready queue
        let scope_set = get_current_scope_set(&tasks);
        let ready: Vec<_> = get_ready_queue_for_scope_set(&tasks, &scope_set)
            .into_iter()
            .cloned()
            .collect();

        // Build output
        let content = format_added(&[task]);

        let in_progress_refs: Vec<_> = in_progress.iter().map(|t| *t).collect();
        let ready_refs: Vec<_> = ready.iter().collect();

        let mut builder = XmlBuilder::new("add");
        let xml_scopes = scope_set.to_xml_scopes();
        if !xml_scopes.is_empty() {
            builder = builder.with_scopes(&xml_scopes);
        }
        let xml = builder.build(&content, &in_progress_refs, &ready_refs);

        println!("{}", xml);
        return Ok(());
    }

    // Manual task creation requires a name
    let name = name.ok_or_else(|| AikiError::InvalidArgument(
        "Task name required. Either provide a name or use --template".to_string()
    ))?;

    // Validate and normalize assignee if provided
    // This converts aliases like "claude" → "claude-code", "me" → "human"
    let assignee = if let Some(ref a) = assignee_arg {
        match Assignee::from_str(a) {
            Some(parsed) => parsed.as_str().map(|s| s.to_string()),
            None => return Err(AikiError::UnknownAssignee(a.clone())),
        }
    } else {
        None
    };

    // Validate source prefixes
    validate_sources(&sources)?;

    // Resolve "prompt" source to actual prompt change_id
    let sources = resolve_prompt_sources(cwd, sources)?;

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

    // Determine task ID and possibly inherit parent's assignee
    let (task_id, effective_assignee) = if let Some(ref parent_id) = parent {
        // Validate parent exists and is not closed
        let parent_task = if let Some(pt) = find_task(&tasks, parent_id) {
            if pt.status == TaskStatus::Closed {
                return Err(AikiError::ParentTaskClosed(parent_id.clone()));
            }
            pt
        } else {
            return Err(AikiError::TaskNotFound(parent_id.clone()));
        };

        // Generate subtask ID (parent.N where N is next available)
        let task_ids: Vec<&str> = tasks.keys().map(|s| s.as_str()).collect();
        let subtask_num = get_next_subtask_number(parent_id, task_ids.into_iter());
        let child_id = generate_child_id(parent_id, subtask_num);

        // Inherit parent's assignee if none specified
        let final_assignee = if assignee.is_some() {
            assignee.clone()
        } else {
            parent_task.assignee.clone()
        };

        (child_id, final_assignee)
    } else {
        // Root-level task with new JJ-style ID
        (generate_task_id(&name), assignee.clone())
    };

    let timestamp = chrono::Utc::now();

    let working_copy = get_working_copy_change_id(cwd);

    let event = TaskEvent::Created {
        task_id: task_id.clone(),
        name: name.clone(),
        priority,
        assignee: effective_assignee.clone(),
        sources: sources.clone(),
        template: None,
        working_copy: working_copy.clone(),
        instructions: None,
        data: std::collections::HashMap::new(),
        timestamp,
    };

    write_event(cwd, &event)?;

    // Build new task from event (avoid re-reading)
    let new_task = Task {
        id: task_id,
        name,
        priority,
        status: TaskStatus::Open,
        assignee: effective_assignee,
        sources,
        template: None,
        working_copy,
        instructions: None,
        data: std::collections::HashMap::new(),
        created_at: timestamp,
        started_at: None,
        claimed_by_session: None,
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
fn run_start(
    cwd: &Path,
    ids: Vec<String>,
    template_name: Option<String>,
    data_args: Vec<String>,
    reopen: bool,
    reopen_reason: Option<String>,
    p0: bool,
    p1: bool,
    _p2: bool,
    p3: bool,
    sources: Vec<String>,
    assignee_arg: Option<String>,
) -> Result<()> {
    use crate::session::find_active_session;
    use crate::agents::Assignee;

    // If --template is provided, create from template and start
    if let Some(ref template) = template_name {
        // Create task from template first
        let task_id = create_from_template(cwd, template, &data_args, &sources, assignee_arg.as_deref(), p0, p1, false, p3)?;
        // Now start that task - recursive call with just the task ID
        return run_start(cwd, vec![task_id], None, Vec::new(), false, None, false, false, false, false, Vec::new(), None);
    }

    // Validate source prefixes (if any sources provided for quick-start)
    validate_sources(&sources)?;

    // Resolve "prompt" source to actual prompt change_id
    let sources = resolve_prompt_sources(cwd, sources)?;

    // Determine priority for new task (if quick-start is used)
    let priority = if p0 {
        TaskPriority::P0
    } else if p1 {
        TaskPriority::P1
    } else if p3 {
        TaskPriority::P3
    } else {
        TaskPriority::default() // P2
    };
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

    // Track if we created a new task (for output formatting)
    let mut created_new_task: Option<Task> = None;

    // Determine which task(s) to start
    let ids_to_start = if ids.is_empty() {
        // Default: start first from ready queue
        if let Some(first) = ready.first() {
            vec![first.id.clone()]
        } else {
            return Err(AikiError::NoTasksReady);
        }
    } else if ids.len() == 1 && !is_task_id(&ids[0]) {
        // Quick-start: input is a description, not a task ID
        // Create a new task and start it atomically
        let description = &ids[0];
        let task_id = generate_task_id(description);
        let timestamp = chrono::Utc::now();
        let working_copy = get_working_copy_change_id(cwd);

        // Create the task
        let create_event = TaskEvent::Created {
            task_id: task_id.clone(),
            name: description.clone(),
            priority,
            assignee: None,
            sources: sources.clone(),
            template: None,
            working_copy: working_copy.clone(),
            instructions: None,
            data: std::collections::HashMap::new(),
            timestamp,
        };
        write_event(cwd, &create_event)?;

        // Add to local tasks map for output
        let new_task = Task {
            id: task_id.clone(),
            name: description.clone(),
            status: TaskStatus::Open,
            priority,
            assignee: None,
            sources: sources.clone(),
            template: None,
            working_copy,
            instructions: None,
            data: std::collections::HashMap::new(),
            created_at: timestamp,
            started_at: None,
            claimed_by_session: None,
            stopped_reason: None,
            closed_outcome: None,
            comments: Vec::new(),
        };
        tasks.insert(task_id.clone(), new_task.clone());
        created_new_task = Some(new_task);

        vec![task_id]
    } else {
        // Validate all IDs exist and check reopen requirements
        for id in &ids {
            if let Some(task) = find_task(&tasks, id) {
                if task.status == TaskStatus::Closed {
                    if !reopen {
                        let xml = XmlBuilder::new("start").error().build_error(&format!(
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

    // Check if we're starting a parent task with subtasks
    // If so, auto-create a planning task (.0) and start that instead
    let mut new_scope: Option<String> = None;
    let mut actual_ids_to_start = ids_to_start.clone();

    if ids_to_start.len() == 1 {
        let task_id = ids_to_start[0].clone();
        if has_subtasks(&tasks, &task_id) {
            // Starting a parent task - create planning task if needed
            let planning_id = generate_child_id(&task_id, 0);

            // Check if planning task already exists
            if find_task(&tasks, &planning_id).is_none() {
                // Create the planning task
                let timestamp = chrono::Utc::now();
                let working_copy = get_working_copy_change_id(cwd);
                let planning_event = TaskEvent::Created {
                    task_id: planning_id.clone(),
                    name: "Review all subtasks and start first batch".to_string(),
                    priority: TaskPriority::default(),
                    assignee: None,
                    sources: Vec::new(),
                    template: None,
                    working_copy: working_copy.clone(),
                    instructions: None,
                    data: std::collections::HashMap::new(),
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
                    sources: Vec::new(),
                    template: None,
                    working_copy,
                    instructions: None,
                    data: std::collections::HashMap::new(),
                    created_at: timestamp,
                    started_at: None,
                    claimed_by_session: None,
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
    // Session detection: find session by PID matching or agent-type fallback
    let session_match = find_active_session(cwd);
    let agent_type_str = session_match
        .as_ref()
        .map(|m| m.agent_type.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = session_match.as_ref().map(|m| m.session_id.clone());

    let timestamp = chrono::Utc::now();
    let start_event = TaskEvent::Started {
        task_ids: actual_ids_to_start.clone(),
        agent_type: agent_type_str,
        session_id: session_id.clone(),
        timestamp,
        stopped: current_in_progress_ids.clone(),
    };
    write_event(cwd, &start_event)?;

    // Update task statuses
    for task in &mut stopped_tasks {
        task.status = TaskStatus::Stopped;
        task.claimed_by_session = None;
    }
    for task in &mut started_tasks {
        task.status = TaskStatus::InProgress;
        task.stopped_reason = None;
        task.claimed_by_session = session_id.clone();
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

    // Show created task if quick-start was used
    if let Some(ref new_task) = created_new_task {
        content.push_str(&format_added(&[new_task]));
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
    let working_copy = get_working_copy_change_id(cwd);
    for blocked_reason in &blocked {
        let blocker_id = generate_task_id(blocked_reason);
        let blocker_event = TaskEvent::Created {
            task_id: blocker_id.clone(),
            name: blocked_reason.clone(),
            priority: TaskPriority::P0, // Blockers are high priority
            assignee: Some("human".to_string()),
            sources: Vec::new(),
            template: None,
            working_copy: working_copy.clone(),
            instructions: None,
            data: std::collections::HashMap::new(),
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
                sources: Vec::new(),
                template: None,
                working_copy: working_copy.clone(),
                instructions: None,
                data: std::collections::HashMap::new(),
                created_at: timestamp,
                started_at: None,
                claimed_by_session: None,
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
    let scope_set = ScopeSet {
        include_root,
        scopes,
    };

    // Get scoped ready queue
    let mut ready: Vec<Task> = get_ready_queue_for_scope_set(&tasks, &scope_set)
        .into_iter()
        .map(|t| (*t).clone())
        .collect();

    // Add stopped task if it's in scope
    let stopped_in_scope = match (
        crate::tasks::id::get_parent_id(&stopped_task.id),
        &scope_set,
    ) {
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
    use crate::tasks::manager::{all_subtasks_closed, get_all_unclosed_descendants};
    use std::io::Read;

    let events = read_events(cwd)?;
    let mut tasks = materialize_tasks(&events);

    // Get in-progress task IDs first (to avoid borrow issues)
    let in_progress_ids: Vec<String> = get_in_progress(&tasks)
        .iter()
        .map(|t| t.id.clone())
        .collect();

    // Determine which task(s) to close
    let mut ids_to_close = if ids.is_empty() {
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

    // Keep track of explicitly requested tasks vs cascade-closed descendants
    let explicit_ids = ids_to_close.clone();

    // Cascade close: collect all unclosed descendants for any parent tasks being closed
    // This allows closing a parent to automatically close all its subtasks
    let mut descendants_to_close: Vec<String> = Vec::new();
    for id in &ids_to_close {
        if has_subtasks(&tasks, id) {
            let unclosed = get_all_unclosed_descendants(&tasks, id);
            for task in unclosed {
                if !ids_to_close.contains(&task.id) && !descendants_to_close.contains(&task.id) {
                    descendants_to_close.push(task.id.clone());
                }
            }
        }
    }
    // Prepend descendants (they're in depth-first order, deepest first)
    // so they get closed before their parents
    descendants_to_close.append(&mut ids_to_close);
    ids_to_close = descendants_to_close;

    // Handle stdin for --comment -
    let comment_text = if comment.as_deref() == Some("-") {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Some(buffer.trim().to_string())
    } else {
        comment
    };

    // Always require a comment when closing tasks - ensures work is documented
    if comment_text.is_none() {
        return Err(AikiError::TaskCommentRequired(
            "Closing tasks requires a comment. Please summarize your work with --comment.".to_string()
        ));
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

    // Add comments before close (1ms before close for chronological order)
    let close_timestamp = chrono::Utc::now();
    let comment_timestamp = close_timestamp - chrono::Duration::milliseconds(1);

    // Descendants that were cascade-closed get "Closed with parent" comment
    let cascade_ids: Vec<String> = ids_to_close
        .iter()
        .filter(|id| !explicit_ids.contains(id))
        .cloned()
        .collect();
    if !cascade_ids.is_empty() {
        let cascade_comment = TaskEvent::CommentAdded {
            task_ids: cascade_ids,
            text: "Closed with parent".to_string(),
            timestamp: comment_timestamp,
        };
        write_event(cwd, &cascade_comment)?;
    }

    // User's comment goes only to explicitly requested tasks
    if let Some(ref comment) = comment_text {
        let explicit_comment = TaskEvent::CommentAdded {
            task_ids: explicit_ids,
            text: comment.clone(),
            timestamp: comment_timestamp,
        };
        write_event(cwd, &explicit_comment)?;
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
        // Check if all subtasks are now closed
        if all_subtasks_closed(&tasks, parent_id) {
            if let Some(parent) = tasks.get_mut(parent_id) {
                // Guard: skip if already closed or in-progress
                if parent.status == TaskStatus::Closed {
                    continue;
                }
                if parent.status == TaskStatus::InProgress {
                    continue;
                }

                // Auto-start the parent for review/finalization
                // Note: session_id is None since close doesn't have session context
                let start_event = TaskEvent::Started {
                    task_ids: vec![parent_id.clone()],
                    agent_type: "claude-code".to_string(),
                    session_id: None,
                    timestamp: chrono::Utc::now(),
                    stopped: Vec::new(),
                };
                write_event(cwd, &start_event)?;

                parent.status = TaskStatus::InProgress;
                parent.claimed_by_session = None;
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

/// Query changes that have a task ID in their provenance
fn query_changes_for_task(cwd: &Path, task_id: &str) -> Result<Vec<ChangeInfo>> {
    use crate::jj::jj_cmd;

    // Query JJ for changes with this task ID in their description
    // Format: change_id timestamp (first line only)
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!("description(substring:\"task={}\")", task_id),
            "--no-graph",
            "-T",
            r#"change_id ++ " " ++ author.timestamp().format("%Y-%m-%dT%H:%M:%S") ++ "\n""#,
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to query changes: {}", e)))?;

    if !output.status.success() {
        // Empty result is not an error
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut changes = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() >= 1 {
            let change_id = parts[0].to_string();
            let timestamp = parts.get(1).map(|s| s.to_string());
            changes.push(ChangeInfo {
                change_id,
                timestamp,
            });
        }
    }

    Ok(changes)
}

/// Get the diff for a specific change
fn get_change_diff(cwd: &Path, change_id: &str) -> Result<String> {
    use crate::jj::jj_cmd;

    let output = jj_cmd()
        .current_dir(cwd)
        .args(["diff", "-r", change_id, "--color=never", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to get diff: {}", e)))?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Information about a change linked to a task
struct ChangeInfo {
    change_id: String,
    timestamp: Option<String>,
}

/// Show task details (including subtasks for parent tasks)
fn run_show(cwd: &Path, id: Option<String>, show_diff: bool) -> Result<()> {
    use crate::tasks::manager::get_subtasks;
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

    // Get subtasks if this is a parent task
    let subtasks = get_subtasks(&tasks, &task_id);
    let has_subtasks = !subtasks.is_empty();

    // Calculate progress if has subtasks
    let (completed, total) = if has_subtasks {
        let total = subtasks.len();
        let completed = subtasks
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

    // Add sources if any
    if !task.sources.is_empty() {
        content.push_str("\n    <sources>");
        for source in &task.sources {
            content.push_str(&format!("\n      <source>{}</source>", escape_xml(source)));
        }
        content.push_str("\n    </sources>");
    }

    // Add subtasks section if this is a parent
    if has_subtasks {
        content.push_str("\n    <subtasks>");
        for subtask in &subtasks {
            content.push_str(&format!(
                "\n      <task id=\"{}\" status=\"{}\" name=\"{}\"/>",
                escape_xml(&subtask.id),
                subtask.status,
                escape_xml(&subtask.name)
            ));
        }
        content.push_str("\n    </subtasks>");

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

    // Query changes for this task
    let changes = query_changes_for_task(cwd, &task_id)?;
    if !changes.is_empty() {
        content.push_str(&format!("\n    <changes count=\"{}\">", changes.len()));
        for change in &changes {
            if show_diff {
                // Include full diff
                let diff = get_change_diff(cwd, &change.change_id)?;
                content.push_str(&format!(
                    "\n      <change id=\"{}\"{}>\n<![CDATA[{}]]>\n      </change>",
                    escape_xml(&change.change_id),
                    change
                        .timestamp
                        .as_ref()
                        .map_or(String::new(), |ts| format!(" timestamp=\"{}\"", ts)),
                    diff
                ));
            } else {
                // Just list change IDs
                content.push_str(&format!(
                    "\n      <change id=\"{}\"{} />",
                    escape_xml(&change.change_id),
                    change
                        .timestamp
                        .as_ref()
                        .map_or(String::new(), |ts| format!(" timestamp=\"{}\"", ts))
                ));
            }
        }
        content.push_str("\n    </changes>");
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
    assignee_arg: Option<String>,
    unassign: bool,
) -> Result<()> {
    use crate::agents::Assignee;
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

    // Determine new assignee: Some(Some(a)) = assign, Some(None) = unassign, None = no change
    let new_assignee: Option<Option<String>> = if unassign {
        Some(None) // Unassign
    } else if let Some(ref a) = assignee_arg {
        // Validate and normalize the assignee
        match Assignee::from_str(a) {
            Some(parsed) => Some(parsed.as_str().map(|s| s.to_string())),
            None => return Err(AikiError::UnknownAssignee(a.clone())),
        }
    } else {
        None // No change
    };

    // Check if there's anything to update
    if new_priority.is_none() && name.is_none() && new_assignee.is_none() {
        let xml = XmlBuilder::new("update").error().build_error(
            "No updates specified. Use --name, --for, --unassign, or --p0/--p1/--p2/--p3",
        );
        println!("{}", xml);
        return Ok(());
    }

    // Write the update event
    let event = TaskEvent::Updated {
        task_id: task_id.clone(),
        name: name.clone(),
        priority: new_priority,
        assignee: new_assignee.clone(),
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
        if let Some(ref new_a) = new_assignee {
            task.assignee = new_a.clone();
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

    // Write the comment event (batch operation with single task)
    let event = TaskEvent::CommentAdded {
        task_ids: vec![task_id.clone()],
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

/// Run a task by spawning an agent session
fn run_run(cwd: &Path, id: String, agent: Option<String>) -> Result<()> {
    // Parse and validate agent override if provided
    let agent_override = if let Some(ref agent_str) = agent {
        match AgentType::from_str(agent_str) {
            Some(agent_type) => Some(agent_type),
            None => return Err(AikiError::UnknownAgentType(agent_str.clone())),
        }
    } else {
        None
    };

    // Build options
    let mut options = TaskRunOptions::new();
    if let Some(agent_type) = agent_override {
        options = options.with_agent(agent_type);
    }

    // Run the task with XML output
    run_task_with_xml(cwd, &id, options)
}

/// Handle template subcommands (list, show)
fn run_template(cwd: &Path, command: TemplateCommands) -> Result<()> {
    use crate::tasks::templates::{find_templates_dir, list_templates, load_template};
    use crate::tasks::xml::escape_xml;

    // Find templates directory
    let templates_dir = match find_templates_dir(cwd) {
        Ok(dir) => dir,
        Err(_) => {
            // No templates directory found - show helpful message
            let xml = XmlBuilder::new("template")
                .build_error("No templates directory found. Create .aiki/templates/ to add templates.");
            println!("{}", xml);
            return Ok(());
        }
    };

    match command {
        TemplateCommands::List => {
            let templates = list_templates(&templates_dir)?;

            if templates.is_empty() {
                let xml = XmlBuilder::new("template")
                    .build_error("No templates found. Create template files in .aiki/templates/");
                println!("{}", xml);
                return Ok(());
            }

            // Build XML output
            let mut content = String::new();
            content.push_str("  <templates>\n");
            for template in &templates {
                let desc = template.description.as_deref().unwrap_or("");
                content.push_str(&format!(
                    "    <template name=\"{}\" description=\"{}\" />\n",
                    escape_xml(&template.name),
                    escape_xml(desc)
                ));
            }
            content.push_str("  </templates>");

            let empty: Vec<&Task> = vec![];
            let xml = XmlBuilder::new("template").build(&content, &empty, &empty);
            println!("{}", xml);
        }
        TemplateCommands::Show { name } => {
            let template = load_template(&name, &templates_dir)?;

            // Build XML output showing template details
            let mut content = String::new();
            content.push_str("  <template>\n");
            content.push_str(&format!("    <name>{}</name>\n", escape_xml(&template.name)));

            // Show source location
            if let Some(ref path) = template.source_path {
                content.push_str(&format!("    <source>{}</source>\n", escape_xml(path)));
            }

            if let Some(ref v) = template.version {
                content.push_str(&format!("    <version>{}</version>\n", escape_xml(v)));
            }
            if let Some(ref desc) = template.description {
                content.push_str(&format!("    <description>{}</description>\n", escape_xml(desc)));
            }
            if let Some(ref t) = template.defaults.task_type {
                content.push_str(&format!("    <type>{}</type>\n", escape_xml(t)));
            }
            if let Some(ref a) = template.defaults.assignee {
                content.push_str(&format!("    <assignee>{}</assignee>\n", escape_xml(a)));
            }
            if let Some(ref p) = template.defaults.priority {
                content.push_str(&format!("    <priority>{}</priority>\n", escape_xml(p)));
            }

            // Show parent task name
            content.push_str(&format!("    <parent_name>{}</parent_name>\n", escape_xml(&template.parent.name)));

            // Show subtasks
            if !template.subtasks.is_empty() {
                content.push_str("    <subtasks>\n");
                for subtask in &template.subtasks {
                    content.push_str(&format!("      <subtask name=\"{}\" />\n", escape_xml(&subtask.name)));
                }
                content.push_str("    </subtasks>\n");
            }

            // Show full template content
            if let Some(ref raw) = template.raw_content {
                content.push_str("    <content><![CDATA[\n");
                content.push_str(raw);
                content.push_str("\n]]></content>\n");
            }

            content.push_str("  </template>");

            let empty: Vec<&Task> = vec![];
            let xml = XmlBuilder::new("template").build(&content, &empty, &empty);
            println!("{}", xml);
        }
    }

    Ok(())
}

/// Create task(s) from a template (shared logic for create and start --template)
fn create_from_template(
    cwd: &Path,
    template_name: &str,
    data_args: &[String],
    sources: &[String],
    assignee_override: Option<&str>,
    p0: bool,
    p1: bool,
    _p2: bool,
    p3: bool,
) -> Result<String> {
    use crate::agents::Assignee;
    use crate::tasks::templates::{coerce_to_string, find_templates_dir, load_template, substitute_with_template_name, VariableContext};

    // Validate source prefixes
    validate_sources(sources)?;

    // Resolve "prompt" source to actual prompt change_id
    let sources = resolve_prompt_sources(cwd, sources.to_vec())?;

    // Parse data arguments into HashMap with type coercion
    // "true"/"false" → boolean string, numeric strings normalized
    let mut data: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for arg in data_args {
        let (key, value) = arg.split_once('=').ok_or_else(|| {
            AikiError::InvalidTaskSource(format!("Invalid --data format: '{}'. Use: --data key=value", arg))
        })?;
        // Apply type coercion: normalizes booleans and numbers
        data.insert(key.to_string(), coerce_to_string(value));
    }

    // Find and load template
    let templates_dir = find_templates_dir(cwd)?;
    let template = load_template(template_name, &templates_dir)?;

    // Determine priority
    let priority = if p0 {
        TaskPriority::P0
    } else if p1 {
        TaskPriority::P1
    } else if p3 {
        TaskPriority::P3
    } else if let Some(ref p) = template.defaults.priority {
        TaskPriority::from_str(p).unwrap_or_default()
    } else {
        TaskPriority::default()
    };

    // Determine assignee
    let assignee = if let Some(a) = assignee_override {
        match Assignee::from_str(a) {
            Some(parsed) => parsed.as_str().map(|s| s.to_string()),
            None => return Err(AikiError::UnknownAssignee(a.to_string())),
        }
    } else if let Some(ref a) = template.defaults.assignee {
        Some(a.clone())
    } else {
        None
    };

    // Merge data: template defaults + CLI overrides (CLI wins)
    for (key, value) in &template.defaults.data {
        if !data.contains_key(key) {
            // Convert serde_json::Value to string
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            data.insert(key.clone(), value_str);
        }
    }

    // Build variable context
    let mut ctx = VariableContext::new();
    for (key, value) in &data {
        ctx.set_data(key, value);
    }
    if let Some(ref a) = assignee {
        ctx.set_builtin("assignee", a);
    }
    ctx.set_builtin("priority", priority.to_string());
    if let Some(ref t) = template.defaults.task_type {
        ctx.set_builtin("type", t);
    }
    if let Some(source) = sources.first() {
        ctx.set_source(source);
    }

    // Substitute variables in parent task name
    let parent_name = substitute_with_template_name(&template.parent.name, &ctx, Some(template_name))?;

    // Generate task ID
    let task_id = generate_task_id(&parent_name);

    // Set id in context for substitution
    ctx.set_builtin("id", &task_id);

    let timestamp = chrono::Utc::now();
    ctx.set_builtin("created", timestamp.to_rfc3339());

    // Substitute variables in parent instructions
    let parent_instructions = if !template.parent.instructions.is_empty() {
        Some(substitute_with_template_name(&template.parent.instructions, &ctx, Some(template_name))?)
    } else {
        None
    };

    // Create parent task event
    let create_event = TaskEvent::Created {
        task_id: task_id.clone(),
        name: parent_name.clone(),
        priority,
        assignee: assignee.clone(),
        sources: sources.clone(),
        template: Some(template.template_id()),
        working_copy: get_working_copy_change_id(cwd),
        instructions: parent_instructions,
        data: data.clone(),
        timestamp,
    };
    write_event(cwd, &create_event)?;

    // Create subtasks
    for (i, subtask_def) in template.subtasks.iter().enumerate() {
        // Generate subtask ID first (only depends on parent ID and index)
        let subtask_id = generate_child_id(&task_id, i + 1);

        // Determine subtask priority (override or inherit)
        let subtask_priority = if let Some(ref p) = subtask_def.priority {
            TaskPriority::from_str(p).unwrap_or(priority)
        } else {
            priority
        };

        // Determine subtask assignee (override or inherit)
        let subtask_assignee = if let Some(ref a) = subtask_def.assignee {
            Some(a.clone())
        } else {
            assignee.clone()
        };

        // Merge data: parent data + subtask frontmatter data (subtask wins on conflict)
        let mut subtask_data = data.clone();
        for (key, value) in &subtask_def.data {
            // Convert serde_json::Value to string
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            subtask_data.insert(key.clone(), value_str);
        }

        // Build subtask-specific context for variable substitution
        // Subtask context uses subtask's data/assignee/priority, with parent.* prefix for parent values
        let mut subtask_ctx = VariableContext::new();
        for (key, value) in &subtask_data {
            subtask_ctx.set_data(key, value);
        }
        subtask_ctx.set_builtin("id", &subtask_id);
        if let Some(ref a) = subtask_assignee {
            subtask_ctx.set_builtin("assignee", a);
        }
        subtask_ctx.set_builtin("priority", subtask_priority.to_string());
        subtask_ctx.set_builtin("created", timestamp.to_rfc3339());
        if let Some(ref t) = template.defaults.task_type {
            subtask_ctx.set_builtin("type", t);
        }
        // Parent context accessible via parent.* prefix
        subtask_ctx.set_builtin("parent.id", &task_id);
        if let Some(ref a) = assignee {
            subtask_ctx.set_builtin("parent.assignee", a);
        }
        subtask_ctx.set_builtin("parent.priority", priority.to_string());
        for (key, value) in &data {
            subtask_ctx.set_builtin(&format!("parent.data.{}", key), value);
        }
        if let Some(source) = sources.first() {
            subtask_ctx.set_source(source);
            subtask_ctx.set_builtin("parent.source", source);
        }

        // Substitute variables in subtask name and instructions using subtask context
        let subtask_name = substitute_with_template_name(&subtask_def.name, &subtask_ctx, Some(template_name))?;
        let subtask_instructions = if !subtask_def.instructions.is_empty() {
            Some(substitute_with_template_name(&subtask_def.instructions, &subtask_ctx, Some(template_name))?)
        } else {
            None
        };

        let subtask_event = TaskEvent::Created {
            task_id: subtask_id,
            name: subtask_name,
            priority: subtask_priority,
            assignee: subtask_assignee,
            sources: vec![format!("task:{}", task_id)],
            template: Some(template.template_id()),
            working_copy: None, // Inherit from parent (captured once)
            instructions: subtask_instructions,
            data: subtask_data,
            timestamp,
        };
        write_event(cwd, &subtask_event)?;
    }

    Ok(task_id)
}

/// Get the current working copy change_id from JJ
///
/// Returns the change_id of the current working copy (`@` in jj terms).
/// This is captured when creating tasks from templates for historical template lookup.
fn get_working_copy_change_id(cwd: &Path) -> Option<String> {
    use crate::jj::jj_cmd;

    let output = jj_cmd()
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if change_id.is_empty() {
        None
    } else {
        Some(change_id)
    }
}


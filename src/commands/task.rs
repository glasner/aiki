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
use crate::events::{AikiEvent, AikiTaskClosedPayload, AikiTaskStartedPayload, TaskEventPayload};
use std::collections::HashSet;

use crate::tasks::{
    generate_child_id, generate_task_id, get_next_subtask_number, is_task_id,
    manager::{
        find_task, get_current_scope_set, get_in_progress, get_ready_queue_for_agent_scoped,
        get_ready_queue_for_scope_set, has_subtasks, materialize_tasks, materialize_tasks_with_ids,
        ScopeSet,
    },
    runner::{run_task_with_xml, TaskRunOptions},
    storage::{read_events, read_events_with_ids, write_event},
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
    let session = find_active_session(cwd).ok_or(AikiError::NoActiveSessionForPromptSource)?;

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

/// Parse --data key=value arguments into a HashMap
///
/// If `coerce` is true, values are type-coerced (booleans/numbers normalized).
/// If `coerce` is false, values are stored verbatim.
fn parse_data_flags(
    args: &[String],
    coerce: bool,
) -> Result<std::collections::HashMap<String, String>> {
    use crate::tasks::templates::coerce_to_string;

    let mut data = std::collections::HashMap::new();
    for arg in args {
        let (key, value) = arg
            .split_once('=')
            .ok_or_else(|| AikiError::InvalidDataFormat(arg.clone()))?;
        let value = if coerce {
            coerce_to_string(value)
        } else {
            value.to_string()
        };
        data.insert(key.to_string(), value);
    }
    Ok(data)
}

/// Infer task type from task properties
///
/// Looks at task name and sources to determine type:
/// - "review" if task name contains "review" or has task: source (follow-up)
/// - "bug" if task name contains "fix" or "bug"
/// - "feature" otherwise (default)
fn infer_task_type(task: &Task) -> String {
    let name_lower = task.name.to_lowercase();

    // Check name patterns
    if name_lower.contains("review") {
        return "review".to_string();
    }
    if name_lower.contains("fix") || name_lower.contains("bug") {
        return "bug".to_string();
    }

    // Check sources for task: prefix (indicates follow-up/review)
    if task.sources.iter().any(|s| s.starts_with("task:")) {
        return "review".to_string();
    }

    // Default to feature
    "feature".to_string()
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

        /// Force stop even if task is claimed by another session (for orchestrator cleanup)
        #[arg(long)]
        force: bool,
    },

    /// Close task(s) as done
    Close {
        /// Task ID(s) to close (defaults to current in-progress task)
        #[arg(value_name = "ID")]
        ids: Vec<String>,

        /// Closure outcome: done (default), wont_do
        #[arg(long, default_value = "done")]
        outcome: String,

        /// Shortcut for --outcome wont_do
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

        /// Expand source references (task: name+instructions, file: content, prompt: text, comment: text+data)
        #[arg(long)]
        with_source: bool,
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
        /// Task ID to comment on (defaults to current in-progress task)
        id: Option<String>,

        /// Comment text (required)
        text: String,

        /// Add structured data to the comment. Can be specified multiple times.
        #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        data: Vec<String>,
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

        /// Run asynchronously (spawn agent and return immediately)
        #[arg(long = "async", short = 'a')]
        run_async: bool,
    },

    /// Show diff of changes made while working on a task
    ///
    /// Shows the net result of all changes made during a task (baseline → final).
    /// Uses jj revsets to derive the baseline from provenance metadata.
    ///
    /// Examples:
    ///   aiki task diff abc123...     # Full diff for task
    ///   aiki task diff abc123 -s     # Summary (file paths with +/- counts)
    ///   aiki task diff abc123 --stat # Histogram of changes
    Diff {
        /// Task ID to show diff for (required)
        id: String,

        /// Show summary (file paths with +/- counts)
        #[arg(short = 's', long)]
        summary: bool,

        /// Show histogram of changes
        #[arg(long)]
        stat: bool,

        /// Show only changed file names
        #[arg(long)]
        name_only: bool,
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
        } => run_add(
            &cwd, name, template, data, parent, assignee, source, p0, p1, p2, p3,
        ),
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
        } => run_start(
            &cwd, ids, template, data, reopen, reason, p0, p1, p2, p3, source, assignee,
        ),
        TaskCommands::Stop {
            id,
            reason,
            blocked,
            force,
        } => run_stop(&cwd, id, reason, blocked, force),
        TaskCommands::Close {
            ids,
            outcome,
            wont_do,
            comment,
        } => run_close(&cwd, ids, &outcome, wont_do, comment),
        TaskCommands::Show {
            id,
            diff,
            with_source,
        } => run_show(&cwd, id, diff, with_source),
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
        TaskCommands::Comment { id, text, data } => run_comment(&cwd, id, text, data),
        TaskCommands::Run {
            id,
            agent,
            run_async,
        } => run_run(&cwd, id, agent, run_async),
        TaskCommands::Diff {
            id,
            summary,
            stat,
            name_only,
        } => run_diff(&cwd, id, summary, stat, name_only),
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
            (None, _) => true,        // No filter applied
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
    let list_tasks: Vec<&Task> = if all
        || has_status_filters
        || has_explicit_assignee_filters
        || filter_source.is_some()
        || filter_template.is_some()
    {
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

        let task_id = create_from_template(
            cwd,
            template,
            &data_args,
            &sources,
            assignee_arg.as_deref(),
            p0,
            p1,
            false,
            p3,
        )?;

        // Read events to get the task we just created
        let events = read_events(cwd)?;
        let tasks = materialize_tasks(&events);
        let in_progress = get_in_progress(&tasks);

        let task = tasks
            .get(&task_id)
            .ok_or_else(|| AikiError::TaskNotFound(task_id.clone()))?;

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
    let name = name.ok_or_else(|| {
        AikiError::InvalidArgument(
            "Task name required. Either provide a name or use --template".to_string(),
        )
    })?;

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
        task_type: None,
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
        task_type: None,
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
        last_session_id: None,
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
    use crate::agents::Assignee;
    use crate::session::find_active_session;

    // If --template is provided, create from template and start
    if let Some(ref template) = template_name {
        // Create task from template first
        let task_id = create_from_template(
            cwd,
            template,
            &data_args,
            &sources,
            assignee_arg.as_deref(),
            p0,
            p1,
            false,
            p3,
        )?;
        // Now start that task - recursive call with just the task ID
        return run_start(
            cwd,
            vec![task_id],
            None,
            Vec::new(),
            false,
            None,
            false,
            false,
            false,
            false,
            Vec::new(),
            None,
        );
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
    // Note: We collect all IDs here; parent-preservation filtering happens later
    // once we know which actual IDs we're starting (after quick-start/template handling)
    let all_in_progress_ids: Vec<String> = get_in_progress(&tasks)
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
            task_type: None,
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
            task_type: None,
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
            last_session_id: None,
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
                    task_type: None,
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
                    task_type: None,
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
                    last_session_id: None,
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

    // Now that we know which IDs we're actually starting, filter out parent tasks
    // When starting a subtask (id contains '.'), preserve its parent from being auto-stopped
    let parent_ids_to_preserve: std::collections::HashSet<String> = actual_ids_to_start
        .iter()
        .filter_map(|id| {
            // If this is a subtask (contains '.'), preserve its parent
            id.rsplit_once('.').map(|(parent, _)| parent.to_string())
        })
        .collect();

    let current_in_progress_ids: Vec<String> = all_in_progress_ids
        .iter()
        .filter(|id| !parent_ids_to_preserve.contains(*id))
        .cloned()
        .collect();

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

    // Emit task.started flow events for each started task
    for task_id in &actual_ids_to_start {
        if let Some(task) = find_task(&tasks, task_id) {
            let task_event = AikiEvent::TaskStarted(AikiTaskStartedPayload {
                task: TaskEventPayload {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    task_type: infer_task_type(&task),
                    status: "in_progress".to_string(),
                    assignee: task.assignee.clone(),
                    outcome: None,
                    source: task.sources.first().cloned(),
                    files: None,
                    changes: None,
                },
                cwd: cwd.to_path_buf(),
                timestamp,
            });

            // Dispatch event (fire-and-forget, don't block on failure)
            let _ = crate::event_bus::dispatch(task_event);
        }
    }

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
    force: bool,
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

    // Session ownership guard: only owning session can stop (unless --force)
    if let Some(ref claimed_session) = stopped_task.claimed_by_session {
        if !force {
            use crate::session::find_active_session;
            let is_owner = find_active_session(cwd)
                .map(|m| &m.session_id == claimed_session)
                .unwrap_or(false);

            if !is_owner {
                let xml = XmlBuilder::new("stop").error().build_error(&format!(
                    "Task '{}' is claimed by another session. Use --force to override.",
                    task_id
                ));
                println!("{}", xml);
                return Ok(());
            }
        }
    }

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
            task_type: None,
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
                task_type: None,
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
                last_session_id: None,
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
fn run_close(
    cwd: &Path,
    ids: Vec<String>,
    outcome_str: &str,
    wont_do: bool,
    comment: Option<String>,
) -> Result<()> {
    use crate::tasks::manager::{all_subtasks_closed, get_all_unclosed_descendants};
    use std::io::Read;

    // Validate outcome (unless --wont_do is used, which overrides)
    if !wont_do {
        match outcome_str {
            "done" | "wont_do" => {}
            _ => {
                return Err(AikiError::InvalidOutcome(
                    outcome_str.to_string(),
                    vec!["done".to_string(), "wont_do".to_string()],
                ));
            }
        }
    }

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
            "Closing tasks requires a comment. Please summarize your work with --comment."
                .to_string(),
        ));
    }

    // --wont_do flag overrides --outcome for backwards compatibility
    let outcome = if wont_do || outcome_str == "wont_do" {
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
            data: std::collections::HashMap::new(),
            timestamp: comment_timestamp,
        };
        write_event(cwd, &cascade_comment)?;
    }

    // User's comment goes only to explicitly requested tasks
    if let Some(ref comment) = comment_text {
        let explicit_comment = TaskEvent::CommentAdded {
            task_ids: explicit_ids,
            text: comment.clone(),
            data: std::collections::HashMap::new(),
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

    // Note: We intentionally do NOT terminate background processes on close.
    // Close is called by the agent when it finishes work gracefully.
    // Use `aiki task stop` to forcibly terminate a running agent.

    // Emit task.closed flow events for each closed task
    for task_id in &ids_to_close {
        if let Some(task) = tasks.get(task_id) {
            let task_event = AikiEvent::TaskClosed(AikiTaskClosedPayload {
                task: TaskEventPayload {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    task_type: infer_task_type(task),
                    status: "closed".to_string(),
                    assignee: task.assignee.clone(),
                    outcome: Some(outcome.to_string()),
                    source: task.sources.first().cloned(),
                    // TODO: Implement lazy loading for files/changes (see ops/now/lazy-load-payloads.md)
                    files: None,
                    changes: None,
                },
                cwd: cwd.to_path_buf(),
                timestamp: close_timestamp,
            });

            // Dispatch event (fire-and-forget, don't block on failure)
            let _ = crate::event_bus::dispatch(task_event);
        }
    }

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
                let auto_start_timestamp = chrono::Utc::now();
                let start_event = TaskEvent::Started {
                    task_ids: vec![parent_id.clone()],
                    agent_type: "claude-code".to_string(),
                    session_id: None,
                    timestamp: auto_start_timestamp,
                    stopped: Vec::new(),
                };
                write_event(cwd, &start_event)?;

                // Emit task.started flow event for auto-started parent
                let task_event = AikiEvent::TaskStarted(AikiTaskStartedPayload {
                    task: TaskEventPayload {
                        id: parent.id.clone(),
                        name: parent.name.clone(),
                        task_type: infer_task_type(parent),
                        status: "in_progress".to_string(),
                        assignee: parent.assignee.clone(),
                        outcome: None,
                        source: parent.sources.first().cloned(),
                        files: None,
                        changes: None,
                    },
                    cwd: cwd.to_path_buf(),
                    timestamp: auto_start_timestamp,
                });
                let _ = crate::event_bus::dispatch(task_event);

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

    // Use the same revset pattern as task diff to ensure consistency
    // This filters out task lifecycle events on aiki/tasks branch
    let pattern = build_task_revset_pattern(task_id);

    // Query JJ for changes with this task ID in their description
    // Format: change_id timestamp (first line only)
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &pattern,
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
///
/// Uses git format with 5 lines of context for better agent comprehension.
fn get_change_diff(cwd: &Path, change_id: &str) -> Result<String> {
    use crate::jj::jj_cmd;

    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "diff",
            "-r",
            change_id,
            "--color=never",
            "--ignore-working-copy",
            "--git",
            "--context",
            "5",
        ])
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

/// Parsed source reference types
enum SourceRef {
    Task { id: String },
    Prompt { id: String },
    File { path: String },
    Comment { id: String },
    Unknown { raw: String },
}

/// Parse a source string into a typed reference
fn parse_source(source: &str) -> SourceRef {
    if let Some((source_type, id)) = source.split_once(':') {
        match source_type {
            "task" => SourceRef::Task { id: id.to_string() },
            "prompt" => SourceRef::Prompt { id: id.to_string() },
            "file" => SourceRef::File {
                path: id.to_string(),
            },
            "comment" => SourceRef::Comment { id: id.to_string() },
            _ => SourceRef::Unknown {
                raw: source.to_string(),
            },
        }
    } else {
        SourceRef::Unknown {
            raw: source.to_string(),
        }
    }
}

/// Format a source reference as XML
///
/// When `expand` is false, returns minimal XML: `<source type="task" id="..."/>`
/// When `expand` is true, includes full content from the source.
fn format_source(
    cwd: &Path,
    source: &str,
    tasks: &std::collections::HashMap<String, Task>,
    expand: bool,
) -> String {
    use crate::tasks::xml::escape_xml;

    let parsed = parse_source(source);

    match parsed {
        SourceRef::Task { id } => {
            if expand {
                // Look up the task and include its name + instructions
                if let Some(task) = tasks.get(&id) {
                    let mut xml =
                        format!("\n    <source type=\"task\" id=\"{}\">", escape_xml(&id));
                    xml.push_str(&format!("\n      <name>{}</name>", escape_xml(&task.name)));
                    if let Some(ref instructions) = task.instructions {
                        xml.push_str(&format!(
                            "\n      <instructions>{}</instructions>",
                            escape_xml(instructions)
                        ));
                    }
                    // Show nested sources as minimal refs (not recursively expanded)
                    for nested_source in &task.sources {
                        let nested_parsed = parse_source(nested_source);
                        xml.push_str(&format_source_minimal(&nested_parsed));
                    }
                    xml.push_str("\n    </source>");
                    xml
                } else {
                    format!(
                        "\n    <source type=\"task\" id=\"{}\" error=\"not_found\"/>",
                        escape_xml(&id)
                    )
                }
            } else {
                format!("\n    <source type=\"task\" id=\"{}\"/>", escape_xml(&id))
            }
        }
        SourceRef::Prompt { id } => {
            if expand {
                // Load prompt from global aiki history repo
                use crate::global::global_aiki_dir;
                use crate::history::get_prompt_by_change_id;

                let global_repo = global_aiki_dir();
                match get_prompt_by_change_id(&global_repo, &id) {
                    Ok(Some(content)) => {
                        format!(
                            "\n    <source type=\"prompt\" id=\"{}\">\n      <text><![CDATA[{}]]></text>\n    </source>",
                            escape_xml(&id),
                            content
                        )
                    }
                    _ => {
                        format!(
                            "\n    <source type=\"prompt\" id=\"{}\" error=\"not_found\"/>",
                            escape_xml(&id)
                        )
                    }
                }
            } else {
                format!("\n    <source type=\"prompt\" id=\"{}\"/>", escape_xml(&id))
            }
        }
        SourceRef::File { path } => {
            if expand {
                // Try to read the file content
                let full_path = cwd.join(&path);
                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        format!(
                            "\n    <source type=\"file\" path=\"{}\">\n      <content><![CDATA[{}]]></content>\n    </source>",
                            escape_xml(&path),
                            content
                        )
                    }
                    Err(_) => {
                        format!(
                            "\n    <source type=\"file\" path=\"{}\" error=\"not_found\"/>",
                            escape_xml(&path)
                        )
                    }
                }
            } else {
                format!(
                    "\n    <source type=\"file\" path=\"{}\"/>",
                    escape_xml(&path)
                )
            }
        }
        SourceRef::Comment { id } => {
            if expand {
                // Comment IDs are in format "task_id:comment_index"
                // Try to parse and look up the comment
                if let Some((task_id, index_str)) = id.split_once(':') {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if let Some(task) = tasks.get(task_id) {
                            if let Some(comment) = task.comments.get(index) {
                                let mut xml = format!(
                                    "\n    <source type=\"comment\" id=\"{}\" task_id=\"{}\">",
                                    escape_xml(&id),
                                    escape_xml(task_id)
                                );
                                xml.push_str(&format!(
                                    "\n      <text>{}</text>",
                                    escape_xml(&comment.text)
                                ));
                                if !comment.data.is_empty() {
                                    xml.push_str("\n      <data>");
                                    for (key, value) in &comment.data {
                                        xml.push_str(&format!(
                                            "\n        <field key=\"{}\">{}</field>",
                                            escape_xml(key),
                                            escape_xml(value)
                                        ));
                                    }
                                    xml.push_str("\n      </data>");
                                }
                                xml.push_str("\n    </source>");
                                return xml;
                            }
                        }
                    }
                }
                // Could not find or parse comment
                format!(
                    "\n    <source type=\"comment\" id=\"{}\" error=\"not_found\"/>",
                    escape_xml(&id)
                )
            } else {
                format!(
                    "\n    <source type=\"comment\" id=\"{}\"/>",
                    escape_xml(&id)
                )
            }
        }
        SourceRef::Unknown { raw } => {
            format!(
                "\n    <source type=\"unknown\" raw=\"{}\"/>",
                escape_xml(&raw)
            )
        }
    }
}

/// Format a source reference as minimal XML (for nested sources)
fn format_source_minimal(source: &SourceRef) -> String {
    use crate::tasks::xml::escape_xml;

    match source {
        SourceRef::Task { id } => {
            format!("\n      <source type=\"task\" id=\"{}\"/>", escape_xml(id))
        }
        SourceRef::Prompt { id } => {
            format!(
                "\n      <source type=\"prompt\" id=\"{}\"/>",
                escape_xml(id)
            )
        }
        SourceRef::File { path } => {
            format!(
                "\n      <source type=\"file\" path=\"{}\"/>",
                escape_xml(path)
            )
        }
        SourceRef::Comment { id } => {
            format!(
                "\n      <source type=\"comment\" id=\"{}\"/>",
                escape_xml(id)
            )
        }
        SourceRef::Unknown { raw } => {
            format!(
                "\n      <source type=\"unknown\" raw=\"{}\"/>",
                escape_xml(raw)
            )
        }
    }
}

/// Show task details (including subtasks for parent tasks)
fn run_show(cwd: &Path, id: Option<String>, show_diff: bool, with_source: bool) -> Result<()> {
    use crate::tasks::manager::get_subtasks;
    use crate::tasks::xml::escape_xml;

    let events = read_events_with_ids(cwd)?;
    let tasks = materialize_tasks_with_ids(&events);
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
    let type_attr = task
        .task_type
        .as_ref()
        .map(|t| format!(" type=\"{}\"", escape_xml(t)))
        .unwrap_or_default();
    let mut content = format!(
        "  <task id=\"{}\" name=\"{}\" status=\"{}\" priority=\"{}\"{}>",
        escape_xml(&task.id),
        escape_xml(&task.name),
        task.status,
        task.priority,
        type_attr
    );

    // Add sources if any
    if !task.sources.is_empty() {
        for source in &task.sources {
            content.push_str(&format_source(cwd, source, &tasks, with_source));
        }
    }

    // Add instructions if present
    if let Some(ref instructions) = task.instructions {
        content.push_str(&format!(
            "\n    <instructions><![CDATA[{}]]></instructions>",
            instructions
        ));
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
            // Build id attribute if present
            let id_attr = comment
                .id
                .as_ref()
                .map(|id| format!(" id=\"{}\"", escape_xml(id)))
                .unwrap_or_default();
            // Build data.* attributes from comment.data
            let data_attrs: String = comment
                .data
                .iter()
                .map(|(k, v)| format!(" data.{}=\"{}\"", escape_xml(k), escape_xml(v)))
                .collect();
            content.push_str(&format!(
                "\n      <comment timestamp=\"{}\"{}{}>{}",
                comment.timestamp.to_rfc3339(),
                id_attr,
                data_attrs,
                escape_xml(&comment.text)
            ));
            content.push_str("</comment>");
        }
        content.push_str("\n    </comments>");
    }

    // Add files_changed summary for closed tasks
    if task.status == TaskStatus::Closed {
        if let Some(files) = get_task_changed_files(cwd, &task_id)? {
            let total_files = files.len();
            content.push_str(&format!("\n    <files_changed total=\"{}\">", total_files));
            for path in &files {
                content.push_str(&format!("\n      <file path=\"{}\" />", escape_xml(path)));
            }
            content.push_str("\n    </files_changed>");
        }
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

/// Show diff of changes made while working on a task
///
/// Shows the net result (baseline → final) of all task work.
/// Uses jj revsets to derive baseline from provenance metadata.
fn run_diff(cwd: &Path, id: String, summary: bool, stat: bool, name_only: bool) -> Result<()> {
    use crate::jj::jj_cmd;

    // Verify task exists
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    if find_task(&tasks, &id).is_none() {
        return Err(AikiError::TaskNotFound(id));
    }

    // Build revset pattern for task
    // For parent tasks with subtasks, match task=<id> AND task=<id>.* (subtasks)
    let pattern = build_task_revset_pattern(&id);

    // Check if any changes exist for this task
    let check_output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &pattern,
            "--no-graph",
            "-T",
            "change_id",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to query changes: {}", e)))?;

    // Distinguish between jj failure and empty results
    if !check_output.status.success() {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        // "Revset resolved to no revisions" is not an error - just means no matches
        if !stderr.contains("no revisions") {
            return Err(AikiError::JjCommandFailed(format!(
                "jj log failed: {}",
                stderr.trim()
            )));
        }
    }

    if String::from_utf8_lossy(&check_output.stdout)
        .trim()
        .is_empty()
    {
        println!(
            "No changes found for task {}.\n\n\
             The task exists but has no associated code changes in jj history.\n\
             This may happen if:\n\
             - Task has no code changes yet\n\
             - Changes were made without aiki provenance tracking",
            id
        );
        return Ok(());
    }

    // Build revset expressions for baseline and final
    // - roots(pattern) = earliest changes for task
    // - parents(roots(...)) = state before task started (baseline)
    // - heads(pattern) = latest changes for task (final)
    let from_revset = format!("parents(roots({}))", pattern);
    let to_revset = format!("heads({})", pattern);

    // Build jj diff command
    let mut cmd = jj_cmd();
    cmd.current_dir(cwd)
        .arg("diff")
        .arg("--from")
        .arg(&from_revset)
        .arg("--to")
        .arg(&to_revset)
        .arg("--ignore-working-copy");

    // Add format options
    if summary {
        cmd.arg("--summary");
    } else if stat {
        cmd.arg("--stat");
    } else if name_only {
        // jj doesn't have --name-only but we can use -T to just print names
        // Actually, use --summary and filter to just paths
        // For now, use --summary which gives similar output
        cmd.arg("--summary");
    } else {
        // Default: Use git format with 5 lines of context for better agent comprehension.
        // Git diff format is more recognizable to AI agents trained on GitHub/GitLab diffs,
        // and 5 lines of context helps understand surrounding code structure.
        cmd.arg("--git").arg("--context").arg("5");
    }

    let output = cmd
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to get diff: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Handle "no common ancestor" case
        if stderr.contains("no common ancestor") || stderr.contains("empty") {
            println!("Warning: Task changes have no common baseline - showing full content");
        } else {
            return Err(AikiError::JjCommandFailed(format!(
                "jj diff failed: {}",
                stderr.trim()
            )));
        }
    }

    // Output jj's native format directly (no XML wrapper)
    let stdout = String::from_utf8_lossy(&output.stdout);

    if name_only && !summary && !stat {
        // Parse --summary output to extract just file names
        for line in stdout.lines() {
            // Summary format: "M path/to/file" or "A path/to/file"
            let line = line.trim();
            if line.len() > 2 {
                // Skip status char and space
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    println!("{}", parts[1]);
                }
            }
        }
    } else {
        print!("{}", stdout);
    }

    Ok(())
}

/// Build revset pattern for a task, including subtasks
///
/// For task ID "abc123", this matches:
/// - description(substring:"task=abc123") - the task itself (provenance metadata)
/// - description(substring:"task=abc123.") - any subtasks (abc123.1, abc123.2, etc.)
///
/// Excludes `::aiki/tasks` to filter out task lifecycle events (which contain
/// `stopped_task=<id>`, `task_id=<id>`, etc.) that live on a separate branch.
fn build_task_revset_pattern(task_id: &str) -> String {
    format!(
        "(description(substring:\"task={}\") | description(substring:\"task={}.\")) ~ ::aiki/tasks",
        task_id, task_id
    )
}

/// Get list of files changed during a task
///
/// Uses jj diff --summary with revset-based baseline/final approach.
/// Returns None if no changes found, otherwise returns list of file paths.
fn get_task_changed_files(cwd: &Path, task_id: &str) -> Result<Option<Vec<String>>> {
    use crate::jj::jj_cmd;

    let pattern = build_task_revset_pattern(task_id);

    // Check if any changes exist for this task
    let check_output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &pattern,
            "--no-graph",
            "-T",
            "change_id",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to query changes: {}", e)))?;

    // Distinguish between jj failure and empty results
    if !check_output.status.success() {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        if !stderr.contains("no revisions") {
            return Err(AikiError::JjCommandFailed(format!(
                "jj log failed: {}",
                stderr.trim()
            )));
        }
        return Ok(None);
    }

    if String::from_utf8_lossy(&check_output.stdout)
        .trim()
        .is_empty()
    {
        return Ok(None);
    }

    // Build revset expressions for baseline and final
    let from_revset = format!("parents(roots({}))", pattern);
    let to_revset = format!("heads({})", pattern);

    // Run jj diff --summary to get file paths
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "diff",
            "--from",
            &from_revset,
            "--to",
            &to_revset,
            "--summary",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to get diff: {}", e)))?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = parse_diff_summary_files(&stdout);

    if files.is_empty() {
        Ok(None)
    } else {
        Ok(Some(files))
    }
}

/// Parse jj diff --summary output to extract file paths
///
/// Example output:
/// ```
/// M src/auth.ts
/// A src/new_file.ts
/// D src/old_file.ts
/// ```
fn parse_diff_summary_files(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // Format: "M path/to/file" - status char, space, path
            if line.len() > 2 && line.chars().nth(1) == Some(' ') {
                Some(line[2..].to_string())
            } else {
                None
            }
        })
        .collect()
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
fn run_comment(cwd: &Path, id: Option<String>, text: String, data_args: Vec<String>) -> Result<()> {
    use crate::tasks::xml::escape_xml;

    // Parse data arguments (verbatim, no coercion for comment metadata)
    let data = parse_data_flags(&data_args, false)?;

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
        data,
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
fn run_run(cwd: &Path, id: String, agent: Option<String>, run_async: bool) -> Result<()> {
    use crate::tasks::runner::run_task_async_with_xml;

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

    // Run the task with XML output - async or blocking
    if run_async {
        run_task_async_with_xml(cwd, &id, options)
    } else {
        run_task_with_xml(cwd, &id, options)
    }
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
            let xml = XmlBuilder::new("template").build_error(
                "No templates directory found. Create .aiki/templates/ to add templates.",
            );
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
            content.push_str(&format!(
                "    <name>{}</name>\n",
                escape_xml(&template.name)
            ));

            // Show source location
            if let Some(ref path) = template.source_path {
                content.push_str(&format!("    <source>{}</source>\n", escape_xml(path)));
            }

            if let Some(ref v) = template.version {
                content.push_str(&format!("    <version>{}</version>\n", escape_xml(v)));
            }
            if let Some(ref desc) = template.description {
                content.push_str(&format!(
                    "    <description>{}</description>\n",
                    escape_xml(desc)
                ));
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
            content.push_str(&format!(
                "    <parent_name>{}</parent_name>\n",
                escape_xml(&template.parent.name)
            ));

            // Show subtasks
            if !template.subtasks.is_empty() {
                content.push_str("    <subtasks>\n");
                for subtask in &template.subtasks {
                    content.push_str(&format!(
                        "      <subtask name=\"{}\" />\n",
                        escape_xml(&subtask.name)
                    ));
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
    use crate::tasks::templates::{
        create_tasks_from_template, find_templates_dir, load_template, parse_data_source,
        resolve_data_source, substitute_with_template_name, VariableContext,
    };

    // Validate source prefixes
    validate_sources(sources)?;

    // Resolve "prompt" source to actual prompt change_id
    let sources = resolve_prompt_sources(cwd, sources.to_vec())?;

    // Parse data arguments (with type coercion for template variable substitution)
    let mut data = parse_data_flags(data_args, true)?;

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
    let parent_name =
        substitute_with_template_name(&template.parent.name, &ctx, Some(template_name))?;

    // Generate task ID
    let task_id = generate_task_id(&parent_name);

    // Set id in context for substitution
    ctx.set_builtin("id", &task_id);

    let timestamp = chrono::Utc::now();
    ctx.set_builtin("created", timestamp.to_rfc3339());

    // Substitute variables in parent instructions
    let parent_instructions = if !template.parent.instructions.is_empty() {
        Some(substitute_with_template_name(
            &template.parent.instructions,
            &ctx,
            Some(template_name),
        )?)
    } else {
        None
    };

    // Create parent task event
    let create_event = TaskEvent::Created {
        task_id: task_id.clone(),
        name: parent_name.clone(),
        task_type: template.parent.task_type.clone(),
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

    // Create subtasks - either dynamic (from data source) or static (from template)
    if let Some(ref subtasks_source_str) = template.subtasks_source {
        // Dynamic subtasks: iterate over a data source (e.g., comments from a task)
        create_dynamic_subtasks(
            cwd,
            &template,
            template_name,
            subtasks_source_str,
            &sources,
            &task_id,
            &ctx,
            priority,
            &assignee,
            &data,
            timestamp,
        )?;
    } else {
        // Static subtasks: use predefined subtasks from template
        create_static_subtasks(
            cwd,
            &template,
            template_name,
            &task_id,
            &sources,
            priority,
            &assignee,
            &data,
            timestamp,
        )?;
    }

    Ok(task_id)
}

/// Create dynamic subtasks by iterating over a data source
fn create_dynamic_subtasks(
    cwd: &Path,
    template: &crate::tasks::templates::TaskTemplate,
    template_name: &str,
    subtasks_source_str: &str,
    sources: &[String],
    parent_id: &str,
    parent_ctx: &crate::tasks::templates::VariableContext,
    parent_priority: TaskPriority,
    parent_assignee: &Option<String>,
    parent_data: &std::collections::HashMap<String, String>,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    use crate::tasks::templates::{
        create_tasks_from_template, parse_data_source, resolve_data_source,
    };

    // Parse the data source specification (e.g., "source.comments")
    let data_source = parse_data_source(subtasks_source_str)?;

    // Find the task ID from sources (look for task: prefix)
    let source_task_id = sources
        .iter()
        .find_map(|s| s.strip_prefix("task:"))
        .ok_or_else(|| {
            AikiError::MissingSourceTask(
                "Dynamic subtasks require --source task:<id> to specify data source".to_string(),
            )
        })?;

    // Materialize tasks to get the source task's comments
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Resolve the data source (e.g., fetch comments from the source task)
    let data_items = resolve_data_source(&data_source, source_task_id, &tasks)?;

    // Build context with parent.* builtins for subtask variable substitution
    // This allows templates to reference parent values via {parent.id}, {parent.data.key}, etc.
    let mut ctx_with_parent = parent_ctx.clone();
    ctx_with_parent.set_builtin("parent.id", parent_id);
    if let Some(ref a) = parent_assignee {
        ctx_with_parent.set_builtin("parent.assignee", a);
    }
    ctx_with_parent.set_builtin("parent.priority", parent_priority.to_string());
    for (key, value) in parent_data {
        ctx_with_parent.set_builtin(&format!("parent.data.{}", key), value);
    }
    if let Some(source) = sources.first() {
        ctx_with_parent.set_builtin("parent.source", source);
    }

    // Use the template resolver to create subtask definitions
    let (_, subtask_defs) =
        create_tasks_from_template(template, &ctx_with_parent, Some(data_items))?;

    // Create events for each subtask
    for (i, subtask_def) in subtask_defs.iter().enumerate() {
        let subtask_id = generate_child_id(parent_id, i + 1);

        // Determine subtask priority (override or inherit)
        let subtask_priority = if let Some(ref p) = subtask_def.priority {
            TaskPriority::from_str(p).unwrap_or(parent_priority)
        } else {
            parent_priority
        };

        // Determine subtask assignee (override or inherit)
        let subtask_assignee = if let Some(ref a) = subtask_def.assignee {
            Some(a.clone())
        } else {
            parent_assignee.clone()
        };

        // Build subtask data: start with parent data, then merge subtask-specific data
        let mut subtask_data = parent_data.clone();
        for (key, value) in &subtask_def.data {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            subtask_data.insert(key.clone(), value_str);
        }

        // Build subtask sources: subtask frontmatter sources + parent task reference
        let mut subtask_sources = subtask_def.sources.clone();
        subtask_sources.push(format!("task:{}", parent_id));

        let subtask_event = TaskEvent::Created {
            task_id: subtask_id,
            name: subtask_def.name.clone(),
            task_type: None, // Subtasks inherit type from parent context
            priority: subtask_priority,
            assignee: subtask_assignee,
            sources: subtask_sources,
            template: Some(template.template_id()),
            working_copy: None,
            instructions: if subtask_def.instructions.is_empty() {
                None
            } else {
                Some(subtask_def.instructions.clone())
            },
            data: subtask_data,
            timestamp,
        };
        write_event(cwd, &subtask_event)?;
    }

    Ok(())
}

/// Create static subtasks from template definitions
fn create_static_subtasks(
    cwd: &Path,
    template: &crate::tasks::templates::TaskTemplate,
    template_name: &str,
    parent_id: &str,
    sources: &[String],
    parent_priority: TaskPriority,
    parent_assignee: &Option<String>,
    parent_data: &std::collections::HashMap<String, String>,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    use crate::tasks::templates::substitute_with_template_name;

    for (i, subtask_def) in template.subtasks.iter().enumerate() {
        // Generate subtask ID first (only depends on parent ID and index)
        let subtask_id = generate_child_id(parent_id, i + 1);

        // Determine subtask priority (override or inherit)
        let subtask_priority = if let Some(ref p) = subtask_def.priority {
            TaskPriority::from_str(p).unwrap_or(parent_priority)
        } else {
            parent_priority
        };

        // Determine subtask assignee (override or inherit)
        let subtask_assignee = if let Some(ref a) = subtask_def.assignee {
            Some(a.clone())
        } else {
            parent_assignee.clone()
        };

        // Merge data: parent data + subtask frontmatter data (subtask wins on conflict)
        let mut subtask_data = parent_data.clone();
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
        let mut subtask_ctx = crate::tasks::templates::VariableContext::new();
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
        subtask_ctx.set_parent("id", parent_id);
        if let Some(ref a) = parent_assignee {
            subtask_ctx.set_parent("assignee", a);
        }
        subtask_ctx.set_parent("priority", parent_priority.to_string());
        for (key, value) in parent_data {
            subtask_ctx.set_parent(&format!("data.{}", key), value);
        }
        if let Some(source) = sources.first() {
            subtask_ctx.set_source(source);
            subtask_ctx.set_parent("source", source);
        }

        // Substitute variables in subtask name and instructions using subtask context
        let subtask_name =
            substitute_with_template_name(&subtask_def.name, &subtask_ctx, Some(template_name))?;
        let subtask_instructions = if !subtask_def.instructions.is_empty() {
            Some(substitute_with_template_name(
                &subtask_def.instructions,
                &subtask_ctx,
                Some(template_name),
            )?)
        } else {
            None
        };

        // Build subtask sources: subtask frontmatter sources (with variable substitution) + parent task reference
        let mut subtask_sources: Vec<String> = subtask_def
            .sources
            .iter()
            .map(|s| substitute_with_template_name(s, &subtask_ctx, Some(template_name)))
            .collect::<Result<Vec<_>>>()?;
        subtask_sources.push(format!("task:{}", parent_id));

        let subtask_event = TaskEvent::Created {
            task_id: subtask_id,
            name: subtask_name,
            task_type: None, // Subtasks inherit type from parent context
            priority: subtask_priority,
            assignee: subtask_assignee,
            sources: subtask_sources,
            template: Some(template.template_id()),
            working_copy: None, // Inherit from parent (captured once)
            instructions: subtask_instructions,
            data: subtask_data,
            timestamp,
        };
        write_event(cwd, &subtask_event)?;
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_revset_pattern() {
        let pattern = build_task_revset_pattern("abc123");
        assert!(pattern.contains("task=abc123"));
        assert!(pattern.contains("task=abc123."));
    }

    #[test]
    fn test_parse_diff_summary_files_basic() {
        let output = r#"M src/auth.ts
A src/new_file.ts
D src/old_file.ts
"#;
        let files = parse_diff_summary_files(output);

        assert_eq!(files.len(), 3);
        assert_eq!(files[0], "src/auth.ts");
        assert_eq!(files[1], "src/new_file.ts");
        assert_eq!(files[2], "src/old_file.ts");
    }

    #[test]
    fn test_parse_diff_summary_files_single() {
        let output = "M path/to/file.rs\n";
        let files = parse_diff_summary_files(output);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "path/to/file.rs");
    }

    #[test]
    fn test_parse_diff_summary_files_empty() {
        let output = "";
        let files = parse_diff_summary_files(output);
        assert!(files.is_empty());
    }
}

//! Review command for creating and running code review tasks
//!
//! This module provides the `aiki review` command which:
//! - Creates a review task with subtasks (digest, review)
//! - Runs the review task (default: blocking, --async: async, --start: hand off)
//! - Supports different review scopes (task ID or session)
//! - Lists review tasks (list subcommand)
//! - Shows review task details (show subcommand)

use clap::Subcommand;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use crate::output_utils;

use super::async_spawn;
use crate::agents::{determine_reviewer, AgentType};
use crate::commands::OutputFormat;
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::templates::create_review_task_from_template;
use crate::tasks::md::MdBuilder;
use crate::tui;
use crate::tui::buffer_ansi::buffer_to_ansi;
use crate::tui::theme::{detect_mode, Theme};
use crate::tui::views::issue_list::{render_issue_list, IssueListItem};
use crate::tasks::{
    find_task, materialize_graph, read_events, reassign_task, start_task_core,
    write_link_event_with_autorun, Task, TaskComment, TaskStatus,
};

/// What kind of review scope this is
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewScopeKind {
    Task,
    Plan,
    Code,
    Session,
}

impl ReviewScopeKind {
    /// Convert to string representation for serialization
    pub fn as_str(&self) -> &str {
        match self {
            ReviewScopeKind::Task => "task",
            ReviewScopeKind::Plan => "plan",
            ReviewScopeKind::Code => "code",
            ReviewScopeKind::Session => "session",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "task" => Ok(ReviewScopeKind::Task),
            "plan" => Ok(ReviewScopeKind::Plan),
            "code" => Ok(ReviewScopeKind::Code),
            "session" => Ok(ReviewScopeKind::Session),
            _ => Err(AikiError::UnknownReviewScope(s.to_string())),
        }
    }
}

/// What is being reviewed and how
#[derive(Debug, Clone)]
pub struct ReviewScope {
    pub kind: ReviewScopeKind,
    /// Task ID or file path depending on kind
    pub id: String,
    /// Task IDs for session reviews (empty otherwise)
    pub task_ids: Vec<String>,
}

impl ReviewScope {
    /// Get display name (computed from kind and id)
    pub fn name(&self) -> String {
        match self.kind {
            ReviewScopeKind::Task => format!("Task ({})", &self.id),
            ReviewScopeKind::Plan => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Plan ({})", filename)
            }
            ReviewScopeKind::Code => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Code ({})", filename)
            }
            ReviewScopeKind::Session => "Session".to_string(),
        }
    }

    /// Serialize to task data HashMap for persistence
    pub fn to_data(&self) -> HashMap<String, String> {
        let mut data = HashMap::new();
        data.insert("scope.kind".into(), self.kind.as_str().into());
        data.insert("scope.id".into(), self.id.clone());
        data.insert("scope.name".into(), self.name());
        if !self.task_ids.is_empty() {
            data.insert("scope.task_ids".into(), self.task_ids.join(","));
        }
        data
    }

    /// Deserialize from task data HashMap
    pub fn from_data(data: &HashMap<String, String>) -> Result<Self> {
        let kind_str = data.get("scope.kind").ok_or_else(|| {
            AikiError::InvalidArgument("Missing scope.kind in review task data".into())
        })?;
        let kind = ReviewScopeKind::from_str(kind_str)?;

        // scope.id is required for non-Session scopes (Task, Plan, Code)
        let id = match kind {
            ReviewScopeKind::Session => {
                data.get("scope.id").cloned().unwrap_or_default()
            }
            _ => {
                data.get("scope.id")
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        AikiError::InvalidArgument(format!(
                            "Missing scope.id in review task data (required for {:?} scope kind)",
                            kind_str
                        ))
                    })?
            }
        };

        Ok(Self {
            kind,
            id,
            task_ids: data
                .get("scope.task_ids")
                .map(|s| s.split(',').map(String::from).collect())
                .unwrap_or_default(),
        })
    }
}

/// A file location referenced by a review issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

impl Location {
    /// Parse a location string in the format `path`, `path:line`, or `path:line-end_line`.
    pub fn parse(s: &str) -> Result<Location> {
        let s = s.trim();
        if s.is_empty() {
            return Err(AikiError::InvalidArgument("Location path must not be empty".into()));
        }

        if let Some(colon_pos) = s.rfind(':') {
            let path = &s[..colon_pos];
            let line_spec = &s[colon_pos + 1..];

            if !line_spec.is_empty() && line_spec.chars().all(|c| c.is_ascii_digit() || c == '-') {
                if path.is_empty() {
                    return Err(AikiError::InvalidArgument("Location path must not be empty".into()));
                }
                if let Some(dash_pos) = line_spec.find('-') {
                    let start_str = &line_spec[..dash_pos];
                    let end_str = &line_spec[dash_pos + 1..];
                    let start: u32 = start_str.parse().map_err(|_| {
                        AikiError::InvalidArgument(format!("Invalid start line: {}", start_str))
                    })?;
                    let end: u32 = end_str.parse().map_err(|_| {
                        AikiError::InvalidArgument(format!("Invalid end line: {}", end_str))
                    })?;
                    if start == 0 || end == 0 {
                        return Err(AikiError::InvalidArgument("Line numbers must be positive".into()));
                    }
                    if end < start {
                        return Err(AikiError::InvalidArgument(format!(
                            "End line ({}) must be >= start line ({})", end, start
                        )));
                    }
                    return Ok(Location {
                        path: path.to_string(),
                        start_line: Some(start),
                        end_line: Some(end),
                    });
                } else {
                    let line: u32 = line_spec.parse().map_err(|_| {
                        AikiError::InvalidArgument(format!("Invalid line number: {}", line_spec))
                    })?;
                    if line == 0 {
                        return Err(AikiError::InvalidArgument("Line numbers must be positive".into()));
                    }
                    return Ok(Location {
                        path: path.to_string(),
                        start_line: Some(line),
                        end_line: None,
                    });
                }
            }
        }

        Ok(Location {
            path: s.to_string(),
            start_line: None,
            end_line: None,
        })
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)?;
        if let Some(start) = self.start_line {
            write!(f, ":{}", start)?;
            if let Some(end) = self.end_line {
                if end != start {
                    write!(f, "-{}", end)?;
                }
            }
        }
        Ok(())
    }
}

/// Parse locations from a task comment's data fields.
///
/// Handles both single-file format (`path`/`start_line`/`end_line` keys) and
/// multi-file format (comma-separated `locations` key).
pub fn parse_locations(data: &HashMap<String, String>) -> Vec<Location> {
    if let Some(locations_str) = data.get("locations") {
        return locations_str
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| Location::parse(s.trim()).ok())
            .collect();
    }

    if let Some(path) = data.get("path") {
        if path.is_empty() {
            return vec![];
        }
        let start_line = data.get("start_line").and_then(|s| s.parse::<u32>().ok());
        let end_line = data.get("end_line").and_then(|s| s.parse::<u32>().ok());
        return vec![Location {
            path: path.clone(),
            start_line,
            end_line,
        }];
    }

    vec![]
}

/// Format a `Vec<Location>` for display as a parenthesized suffix.
pub fn format_locations(locations: &[Location]) -> String {
    if locations.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = locations.iter().map(|l| l.to_string()).collect();
    format!("({})", parts.join(", "))
}

/// Sort order for severity values (lower = higher priority).
fn severity_order(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 1,
    }
}

/// Get severity from a comment's data, defaulting to "medium".
fn comment_severity(comment: &TaskComment) -> &str {
    comment.data.get("severity").map(|s| s.as_str()).unwrap_or("medium")
}

/// Parse and validate a severity value for clap's value_parser.
fn parse_severity(s: &str) -> std::result::Result<String, String> {
    match s {
        "high" | "medium" | "low" => Ok(s.to_string()),
        _ => Err(format!("invalid severity '{}': must be high, medium, or low", s)),
    }
}

/// Review subcommands (for list, show, and issue management)
#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum ReviewSubcommands {
    /// List review tasks
    List {
        /// Show all reviews (not just open)
        #[arg(long)]
        all: bool,
    },

    /// Show review task details
    Show {
        /// Review task ID
        task_id: String,
    },

    /// Manage review issues
    Issue {
        #[command(subcommand)]
        command: ReviewIssueSubcommands,
    },
}

/// Subcommands for managing review issues
#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum ReviewIssueSubcommands {
    /// Add an issue to a review
    Add {
        /// The review task ID
        review_id: String,
        /// Description of the issue
        text: String,
        /// Issue severity: high, medium, or low (default: medium)
        #[arg(long, value_parser = parse_severity)]
        severity: Option<String>,
        /// File location (path, path:line, or path:line-end). Repeatable.
        #[arg(long = "file")]
        files: Vec<String>,
        /// Shorthand for --severity high
        #[arg(long, conflicts_with = "severity")]
        high: bool,
        /// Shorthand for --severity low
        #[arg(long, conflicts_with = "severity")]
        low: bool,
    },
    /// List issues on a review
    List {
        /// The review task ID
        review_id: String,
    },
}

/// Arguments for the review command (top-level create args)
#[derive(clap::Args)]
pub struct ReviewArgs {
    /// Target to review: task ID, file path (.md), or nothing for session review
    pub target: Option<String>,

    /// Review the codebase implementation described in a plan (only with file targets)
    #[arg(long)]
    pub code: bool,

    /// Auto-fix issues after review
    #[arg(long, short = 'f')]
    pub fix: bool,

    /// Auto-fix with custom template (implies --fix)
    #[arg(long = "fix-template")]
    pub fix_template: Option<String>,

    /// Run review asynchronously (return immediately)
    #[arg(long = "async")]
    pub run_async: bool,

    /// Start review and return control to calling agent
    #[arg(long)]
    pub start: bool,

    /// Task template to use (default: scope-specific, e.g. review/task)
    #[arg(long)]
    pub template: Option<String>,

    /// Agent for review assignment (default: opposite of task worker)
    #[arg(long)]
    pub agent: Option<String>,

    /// Enable autorun (auto-start this review when its target closes)
    #[arg(long)]
    pub autorun: bool,

    /// Output format (e.g., `id` for bare task ID on stdout)
    #[arg(long, short = 'o', value_name = "FORMAT")]
    pub output: Option<OutputFormat>,

    /// Internal: continue an async review+fix from a previously created review task
    #[arg(long = "_continue-async", hide = true)]
    pub continue_async: Option<String>,

    /// Subcommand (list or show)
    #[command(subcommand)]
    pub subcommand: Option<ReviewSubcommands>,
}

/// Run the review command
pub fn run(args: ReviewArgs) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    // Resolve --fix / --fix-template into a single Option<String>
    let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });

    // Internal: continue an async review+fix from a previously created review task
    if let Some(ref review_id) = args.continue_async {
        return run_continue_async(&cwd, review_id, fix_template, args.agent, args.autorun);
    }

    // If a subcommand is provided, dispatch to it
    if let Some(subcommand) = args.subcommand {
        return match subcommand {
            ReviewSubcommands::List { all } => list_reviews(&cwd, all),
            ReviewSubcommands::Show { task_id } => show_review(&cwd, &task_id),
            ReviewSubcommands::Issue { command } => match command {
                ReviewIssueSubcommands::Add { review_id, text, severity, files, high, low } => {
                    run_issue_add(&cwd, &review_id, &text, severity, &files, high, low)
                }
                ReviewIssueSubcommands::List { review_id } => run_issue_list(&cwd, &review_id),
            },
        };
    }

    // Otherwise, run the create/review flow with top-level args
    run_review(
        &cwd,
        args.target,
        args.code,
        fix_template,
        args.run_async,
        args.start,
        args.template,
        args.agent,
        args.autorun,
        args.output,
    )
}

/// Parameters for creating a review task
#[derive(Debug, Clone)]
pub struct CreateReviewParams {
    /// Pre-resolved review scope (caller detects target type)
    pub scope: ReviewScope,
    /// Override the reviewer agent
    pub agent_override: Option<String>,
    /// Template to use (default: scope-specific, e.g. review/task)
    pub template: Option<String>,
    /// Fix plan template (e.g., "fix"); Some means fix is enabled
    pub fix_template: Option<String>,
    /// Enable autorun on the validates link (default: false, opt-in only)
    pub autorun: bool,
}

/// Result of creating a review task
#[derive(Debug, Clone)]
pub struct CreateReviewResult {
    /// The created review task ID
    pub review_task_id: String,
    /// The review scope (typed, replaces loose scope_name/scope_id)
    #[allow(dead_code)]
    pub scope: ReviewScope,
}

/// Check if a string looks like it could be a task ID, prefix, or subtask ID.
///
/// Task IDs are 32 lowercase letters (a-z only). Prefixes are shorter
/// but follow the same pattern. Subtask IDs append `.N` suffixes
/// (e.g., `abcdef.1`, `abcdef.1.2`). This is a heuristic used by
/// detect_target to distinguish task IDs from file paths.
fn looks_like_task_id(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Split off optional subtask suffix (e.g., "abc.1.2" → root "abc")
    let root = s.split('.').next().unwrap_or(s);
    // Root must be non-empty lowercase letters
    if root.is_empty() || !root.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    // Every part after the root must be a non-empty digit sequence
    let mut parts = s.split('.');
    parts.next(); // skip root
    parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Detect the review target from the CLI argument and flags.
///
/// Returns a `ReviewScope` and optionally a worker agent string (for task targets).
/// The `cwd` is needed to resolve file paths and load tasks.
pub fn detect_target(
    cwd: &Path,
    arg: Option<&str>,
    code: bool,
) -> Result<(ReviewScope, Option<String>)> {
    match arg {
        None => {
            if code {
                return Err(AikiError::InvalidArgument(
                    "--code flag only applies to file targets".to_string(),
                ));
            }

            // Session scope — collect closed tasks from current session
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let session = find_active_session(cwd);

            let (session_id, session_agent) = match &session {
                Some(s) => (
                    Some(s.session_id.clone()),
                    Some(s.agent_type.as_str().to_string()),
                ),
                None => (None, None),
            };

            let closed_tasks: Vec<Task> = tasks
                .values()
                .filter(|t| {
                    t.status == TaskStatus::Closed
                        && match (&t.last_session_id, &session_id) {
                            (Some(task_session), Some(current_session)) => {
                                task_session == current_session
                            }
                            (_, None) => true,
                            (None, Some(_)) => false,
                        }
                })
                .cloned()
                .collect();

            if closed_tasks.is_empty() {
                output_nothing_to_review()?;
                return Err(AikiError::NothingToReview);
            }

            let task_ids: Vec<String> = closed_tasks.iter().map(|t| t.id.clone()).collect();
            let fallback_id = {
                let mut ids = task_ids.clone();
                ids.sort();
                let hash_input = ids.join(",");
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, hash_input.as_bytes()).to_string()
            };
            let scope = ReviewScope {
                kind: ReviewScopeKind::Session,
                id: session_id.unwrap_or(fallback_id),
                task_ids,
            };
            Ok((scope, session_agent))
        }

        Some(s) if s.ends_with(".md") && PathBuf::from(s).exists() => {
            let kind = if code {
                ReviewScopeKind::Code
            } else {
                ReviewScopeKind::Plan
            };
            Ok((
                ReviewScope {
                    kind,
                    id: s.to_string(),
                    task_ids: vec![],
                },
                None,
            ))
        }

        Some(s) if s.ends_with(".md") => {
            Err(AikiError::InvalidArgument(format!("File not found: {}", s)))
        }

        Some(s) if looks_like_task_id(s) => {
            if code {
                return Err(AikiError::InvalidArgument(
                    "--code flag only applies to file targets".to_string(),
                ));
            }

            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let task = find_task(&tasks, s)?;
            let worker = task.assignee.as_deref().map(|s| s.to_string());
            let scope = ReviewScope {
                kind: ReviewScopeKind::Task,
                id: task.id.clone(),
                task_ids: vec![],
            };
            Ok((scope, worker))
        }

        Some(s) if Path::new(s).exists() => {
            Err(AikiError::InvalidArgument(
                "File review only supports .md files currently".to_string(),
            ))
        }

        Some(s) => {
            Err(AikiError::InvalidArgument(format!("Target not found: {}", s)))
        }
    }
}

/// Core review creation logic. Used by both CLI and flow action.
///
/// This function creates the review task with subtasks but does NOT
/// start or run the task. The caller is responsible for the execution mode.
/// The scope must be pre-resolved by the caller (via `detect_target()` for CLI,
/// or directly constructed for flow actions).
pub fn create_review(cwd: &Path, params: CreateReviewParams) -> Result<CreateReviewResult> {
    let scope = params.scope;

    // Determine worker for reviewer assignment (for task scope)
    let worker = match scope.kind {
        ReviewScopeKind::Task => {
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let task = find_task(&tasks, &scope.id)?;
            task.assignee.as_deref().map(|s| s.to_string())
        }
        ReviewScopeKind::Session => {
            find_active_session(cwd)
                .map(|s| s.agent_type.as_str().to_string())
        }
        _ => None,
    };

    // Determine assignee for review task
    let assignee = params
        .agent_override
        .or_else(|| Some(determine_reviewer(worker.as_deref())));

    // Create review task with subtasks from template
    let default_template = match scope.kind {
        ReviewScopeKind::Session => "review/task".to_string(),
        _ => format!("review/{}", scope.kind.as_str()),
    };
    let template = params.template.as_deref().unwrap_or(&default_template);
    let mut scope_data = scope.to_data();

    // Add options data
    if let Some(ref tmpl) = params.fix_template {
        scope_data.insert("options.fix".to_string(), "true".to_string());

        scope_data.insert("options.fix_template".to_string(), tmpl.clone());
    }

    // Build sources for lineage (not routing)
    let sources = match scope.kind {
        ReviewScopeKind::Task => vec![format!("task:{}", scope.id)],
        ReviewScopeKind::Plan | ReviewScopeKind::Code => {
            vec![format!("file:{}", scope.id)]
        }
        _ => vec![],
    };

    let review_id = create_review_task_from_template(
        cwd,
        &scope_data,
        &sources,
        &assignee,
        template,
    )?;

    // Emit validates link for task-scoped reviews: review validates the original task
    // Autorun is opt-in only (--autorun flag); default is no autorun
    if scope.kind == ReviewScopeKind::Task {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);
        let autorun = if params.autorun { Some(true) } else { None };
        write_link_event_with_autorun(cwd, &graph, "validates", &review_id, &scope.id, autorun)?;
    }

    Ok(CreateReviewResult {
        review_task_id: review_id,
        scope,
    })
}

/// Build the args for spawning an async review background process.
pub(crate) fn build_async_review_args(
    review_id: &str,
    fix_template: Option<&str>,
    agent: Option<&str>,
    autorun: bool,
) -> Vec<String> {
    let mut args = vec![
        "review".to_string(),
        "--_continue-async".to_string(),
        review_id.to_string(),
    ];
    if let Some(tmpl) = fix_template {
        args.push("--fix-template".to_string());
        args.push(tmpl.to_string());
    }
    if let Some(a) = agent {
        args.push("--agent".to_string());
        args.push(a.to_string());
    }
    if autorun {
        args.push("--autorun".to_string());
    }
    args
}

/// Core review implementation
fn run_review(
    cwd: &Path,
    target: Option<String>,
    code: bool,
    fix_template: Option<String>,
    run_async: bool,
    start: bool,
    template_name: Option<String>,
    agent: Option<String>,
    autorun: bool,
    output_format: Option<OutputFormat>,
) -> Result<()> {
    // --fix/--fix-template and --start cannot be used together
    if fix_template.is_some() && start {
        return Err(AikiError::InvalidArgument(
            "--fix and --start cannot be used together. Use --fix with blocking or --async mode.".to_string(),
        ));
    }

    // Parse agent if provided
    let agent_override = if let Some(ref agent_str) = agent {
        let agent_type = AgentType::from_str(agent_str)
            .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?;
        Some(agent_type.as_str().to_string())
    } else {
        None
    };

    // Detect target and resolve scope at CLI layer
    let (scope, _worker) = match detect_target(cwd, target.as_deref(), code) {
        Ok(r) => r,
        Err(AikiError::NothingToReview) => {
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    // --fix is not supported for session reviews
    if fix_template.is_some() && scope.kind == ReviewScopeKind::Session {
        return Err(AikiError::InvalidArgument(
            "--fix is not supported for session reviews".to_string(),
        ));
    }

    // Create review task using shared logic
    let fix_template_for_async = fix_template.clone();
    let result = match create_review(
        cwd,
        CreateReviewParams {
            scope,
            agent_override,
            template: template_name,
            fix_template,
            autorun,
        },
    ) {
        Ok(r) => r,
        Err(AikiError::NothingToReview) => {
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    let review_id = result.review_task_id;

    let output_id = matches!(output_format, Some(OutputFormat::Id));

    // Handle execution mode
    if start {
        // Reassign task to current agent (caller takes over)
        if let Some(session) = find_active_session(cwd) {
            reassign_task(cwd, &review_id, session.agent_type.as_str())?;
        }
        // Start task using core logic (validates, auto-stops, emits events)
        start_task_core(cwd, &[review_id.clone()])?;
        if !output_id {
            output_review_started(cwd, &review_id)?;
        }
    } else if run_async {
        // Spawn background process: aiki review --_continue-async <review-id> [--fix-template T] [--agent ...]
        let spawn_args = build_async_review_args(
            &review_id,
            fix_template_for_async.as_deref(),
            agent.as_deref(),
            autorun,
        );
        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if !output_id {
            output_review_async(cwd, &review_id)?;
        }
    } else {
        // Run to completion (default)
        let options = TaskRunOptions::new();
        task_run(cwd, &review_id, options)?;

        // Check data.issue_count to determine if issues were found
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);
        let has_issues = find_task(&graph.tasks, &review_id)
            .map(|t| t.data.get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0) > 0)
            .unwrap_or(false);

        if fix_template_for_async.is_some() && has_issues {
            super::fix::run_fix(
                cwd,
                &review_id,
                false,         // not async
                None,          // no continue-async
                fix_template_for_async.clone(),  // forward caller's fix template
                None,          // default decompose template
                None,          // default loop template
                None,          // default review template
                agent.clone(), // pass through agent override
                autorun,
                false,         // not --once
                output_format.clone(), // pass through caller's output format
            )?;
        }
        if output_id {
            println!("{}", review_id);
        } else if !(fix_template_for_async.is_some() && has_issues) {
            output_review_completed(cwd, &review_id)?;
        }
    }

    // For start-only and async branches, print the bare review ID when
    // output=id is requested. The blocking branch (else) handles this inline
    // to avoid run_fix() output contaminating stdout.
    if output_id && (start || run_async) {
        println!("{}", review_id);
    }

    Ok(())
}

/// Background process entry point for async review+fix.
///
/// This is called when `--_continue-async` is provided. The parent process has
/// already created the review task and returned its ID to the caller. This function
/// picks up from there: runs the review to completion, checks for issues, and if
/// `fix` is true and issues exist, runs the fix pipeline.
fn run_continue_async(
    cwd: &Path,
    review_id: &str,
    fix_template: Option<String>,
    agent: Option<String>,
    autorun: bool,
) -> Result<()> {
    // Run the review
    let options = TaskRunOptions::new();
    task_run(cwd, review_id, options)?;

    if fix_template.is_none() {
        return Ok(());
    }

    // Check for issues
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let has_issues = find_task(&graph.tasks, review_id)
        .map(|t| {
            t.data
                .get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0)
                > 0
        })
        .unwrap_or(false);

    if has_issues {
        super::fix::run_fix(
            cwd, review_id, false, None, fix_template, None, None, None, agent, autorun, false, None,
        )?;
    }

    Ok(())
}

/// Output message when there's nothing to review
fn output_nothing_to_review() -> Result<()> {
    output_utils::emit(|| {
        MdBuilder::new("review")
            .build("Nothing to review — no closed tasks in session.\n", &[], &[])
    });
    Ok(())
}

/// Render the workflow view for a review task by finding its epic via the `validates` edge.
fn render_review_workflow(cwd: &Path, review_id: &str) -> Result<String> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    // Find the review task
    let _review_task = find_task(&graph.tasks, review_id)?;

    // Follow validates edge to find the epic
    let epic_ids = graph.edges.targets(review_id, "validates");
    let epic = match epic_ids.first().and_then(|id| graph.tasks.get(id.as_str())) {
        Some(epic) => epic,
        None => return Ok(String::new()),
    };

    let plan_path = epic.data.get("plan").map(|s| s.as_str()).unwrap_or("unknown");
    let repo_name = crate::repos::repo_folder_name(cwd);
    let subtasks: Vec<&Task> = graph.children_of(&epic.id);

    let theme = Theme::from_mode(detect_mode());
    let view = tui::builder::build_workflow_view_focused(epic, &subtasks, plan_path, &repo_name, &graph, Some(review_id));
    let buf = tui::views::workflow::render_workflow(&view, &theme);
    Ok(buffer_to_ansi(&buf))
}

/// Output review started message (for --start mode)
fn output_review_started(cwd: &Path, review_id: &str) -> Result<()> {
    let rendered = render_review_workflow(cwd, review_id)?;
    output_utils::emit(|| {
        let status = format!("Started: {review_id}\n");
        if rendered.trim().is_empty() {
            format!("{status}Review started — run `aiki task show {review_id}` for details.\n")
        } else {
            format!("{status}{rendered}")
        }
    });
    Ok(())
}

/// Output review async message (for --async mode)
fn output_review_async(cwd: &Path, review_id: &str) -> Result<()> {
    let rendered = render_review_workflow(cwd, review_id)?;
    output_utils::emit(|| {
        let status = format!("Dispatched: {review_id}\n");
        if rendered.trim().is_empty() {
            format!("{status}Review dispatched — run `aiki task show {review_id}` for details.\n")
        } else {
            format!("{status}{rendered}")
        }
    });
    Ok(())
}

/// Output review completed message (for blocking mode)
fn output_review_completed(cwd: &Path, review_id: &str) -> Result<()> {
    let rendered = render_review_workflow(cwd, review_id)?;
    let hint = format!("\n---\nRun `aiki fix {}` to remediate.\n", review_id);
    output_utils::emit(|| {
        let status = format!("Completed: {review_id}\n");
        if rendered.trim().is_empty() {
            format!("{status}Review completed — run `aiki task show {review_id}` for details.{hint}")
        } else {
            format!("{status}{rendered}{hint}")
        }
    });
    Ok(())
}

/// Get all issue comments from a task (comments where data.issue == "true").
///
/// This is the canonical function for filtering issue comments — used by both
/// `aiki review issue list` and `aiki fix`.
pub fn get_issue_comments(task: &Task) -> Vec<&TaskComment> {
    task.comments
        .iter()
        .filter(|c| c.data.get("issue").map(|v| v == "true").unwrap_or(false))
        .collect()
}

/// Add an issue to a review task
fn run_issue_add(
    cwd: &Path,
    review_id: &str,
    text: &str,
    severity: Option<String>,
    files: &[String],
    high: bool,
    low: bool,
) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;
    let task = find_task(&tasks, review_id)?;

    // Validate it's a review task
    if !super::fix::is_review_task(task) {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task.",
            review_id
        )));
    }

    // Validate it's not closed
    if task.status == TaskStatus::Closed {
        return Err(AikiError::InvalidArgument(format!(
            "Review task {} is already closed.",
            review_id
        )));
    }

    // Use shared comment codepath with issue data
    let mut data = HashMap::new();
    data.insert("issue".to_string(), "true".to_string());

    // Resolve severity: --high/--low shorthands, explicit --severity, or default
    let resolved_severity = if high {
        "high"
    } else if low {
        "low"
    } else {
        severity.as_deref().unwrap_or("medium")
    };
    data.insert("severity".to_string(), resolved_severity.to_string());

    // Parse and store file locations
    if !files.is_empty() {
        let locations: Vec<Location> = files
            .iter()
            .map(|f| Location::parse(f))
            .collect::<Result<Vec<_>>>()?;

        if locations.len() == 1 {
            let loc = &locations[0];
            data.insert("path".to_string(), loc.path.clone());
            if let Some(start) = loc.start_line {
                data.insert("start_line".to_string(), start.to_string());
            }
            if let Some(end) = loc.end_line {
                data.insert("end_line".to_string(), end.to_string());
            }
        } else {
            let parts: Vec<String> = locations.iter().map(|l| l.to_string()).collect();
            data.insert("locations".to_string(), parts.join(","));
        }
    }

    super::task::comment_on_task(cwd, &task.id, text, data)?;

    output_utils::emit(|| format!("Added issue to review {}", review_id));
    Ok(())
}

/// List issues on a review task
fn run_issue_list(cwd: &Path, review_id: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;
    let task = find_task(&tasks, review_id)?;

    // Validate it's a review task
    if !super::fix::is_review_task(task) {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task.",
            review_id
        )));
    }

    let mut issues = get_issue_comments(task);

    // Sort by severity: high → medium → low
    issues.sort_by_key(|c| severity_order(comment_severity(c)));

    let items: Vec<IssueListItem> = issues
        .iter()
        .map(|comment| {
            let severity = comment_severity(comment).to_string();
            let locations = parse_locations(&comment.data);
            IssueListItem {
                severity,
                text: comment.text.clone(),
                location: format_locations(&locations),
            }
        })
        .collect();

    output_utils::emit(|| {
        let theme = Theme::from_mode(detect_mode());
        let buffer = render_issue_list(&task.id, &task.name, &items, &theme);
        buffer_to_ansi(&buffer)
    });

    Ok(())
}

/// List review tasks
fn list_reviews(cwd: &Path, all: bool) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    // Filter to tasks with task_type == "review"
    let mut reviews: Vec<&Task> = tasks
        .values()
        .filter(|t| t.task_type.as_deref() == Some("review"))
        .filter(|t| {
            // If not --all, only show open reviews (not closed)
            all || t.status != TaskStatus::Closed
        })
        .collect();

    // Sort by created_at (most recent first)
    reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if reviews.is_empty() {
        output_utils::emit(|| {
            let content = if all {
                "No review tasks found.\n"
            } else {
                "No open review tasks. Use --all to see closed reviews.\n"
            };
            MdBuilder::new("review-list").build(content, &[], &[])
        });
        return Ok(());
    }

    output_utils::emit(|| {
        let mut content = String::from("## Reviews\n| ID | Status | Outcome | Issues | Name |\n|----|--------|---------|--------|------|\n");
        for review in &reviews {
            let status_str = match review.status {
                TaskStatus::Open => "open",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Stopped => "stopped",
                TaskStatus::Closed => "closed",
            };

            let outcome_str = review
                .closed_outcome
                .as_ref()
                .map(|o| format!("{:?}", o).to_lowercase())
                .unwrap_or_default();

            let issue_count = if let Some(count) = review.data.get("issue_count") {
                count.parse::<usize>().unwrap_or(review.comments.len())
            } else {
                review.comments.len()
            };

            content.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                &review.id,
                status_str,
                outcome_str,
                issue_count,
                &review.name
            ));
        }
        MdBuilder::new("review-list").build(&content, &[], &[])
    });

    Ok(())
}

/// Show review task details
fn show_review(cwd: &Path, task_id: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    let task = find_task(&tasks, task_id)?;

    // Verify it's a review task
    if task.task_type.as_deref() != Some("review") {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task (type: {:?})",
            task_id, task.task_type
        )));
    }

    let mut issues = get_issue_comments(task);
    issues.sort_by_key(|c| severity_order(comment_severity(c)));

    let items: Vec<IssueListItem> = issues
        .iter()
        .map(|comment| {
            let severity = comment_severity(comment).to_string();
            let locations = parse_locations(&comment.data);
            IssueListItem {
                severity,
                text: comment.text.clone(),
                location: format_locations(&locations),
            }
        })
        .collect();

    output_utils::emit(|| {
        let theme = Theme::from_mode(detect_mode());
        let buffer = render_issue_list(&task.id, &task.name, &items, &theme);
        buffer_to_ansi(&buffer)
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_reviewer_opposite_claude() {
        // Worker is claude-code, reviewer should be codex
        let result = determine_reviewer(Some("claude-code"));
        assert_eq!(result, "codex".to_string());
    }

    #[test]
    fn test_determine_reviewer_opposite_codex() {
        // Worker is codex, reviewer should be claude-code
        let result = determine_reviewer(Some("codex"));
        assert_eq!(result, "claude-code".to_string());
    }

    #[test]
    fn test_determine_reviewer_default() {
        // No worker, default to codex
        let result = determine_reviewer(None);
        assert_eq!(result, "codex".to_string());
    }

    #[test]
    fn test_determine_reviewer_unknown_agent() {
        // Unknown worker, default to codex
        let result = determine_reviewer(Some("unknown-agent"));
        assert_eq!(result, "codex".to_string());
    }

    // ReviewScopeKind tests

    #[test]
    fn test_scope_kind_as_str() {
        assert_eq!(ReviewScopeKind::Task.as_str(), "task");
        assert_eq!(ReviewScopeKind::Plan.as_str(), "plan");
        assert_eq!(ReviewScopeKind::Code.as_str(), "code");
        assert_eq!(ReviewScopeKind::Session.as_str(), "session");
    }

    #[test]
    fn test_scope_kind_from_str() {
        assert_eq!(ReviewScopeKind::from_str("task").unwrap(), ReviewScopeKind::Task);
        assert_eq!(ReviewScopeKind::from_str("plan").unwrap(), ReviewScopeKind::Plan);
        assert_eq!(ReviewScopeKind::from_str("code").unwrap(), ReviewScopeKind::Code);
        assert_eq!(ReviewScopeKind::from_str("session").unwrap(), ReviewScopeKind::Session);
    }

    #[test]
    fn test_scope_kind_from_str_unknown() {
        let result = ReviewScopeKind::from_str("unknown");
        assert!(result.is_err());
    }

    // ReviewScope tests

    #[test]
    fn test_scope_name_task() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "abc123".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Task (abc123)");
    }

    #[test]
    fn test_scope_name_spec() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Plan,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Plan (feature.md)");
    }

    #[test]
    fn test_scope_name_code() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Code,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Code (feature.md)");
    }

    #[test]
    fn test_scope_name_session() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Session");
    }

    #[test]
    fn test_scope_to_data_task() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "abc123".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        assert_eq!(data.get("scope.kind").unwrap(), "task");
        assert_eq!(data.get("scope.id").unwrap(), "abc123");
        assert_eq!(data.get("scope.name").unwrap(), "Task (abc123)");
        assert!(data.get("scope.task_ids").is_none());
    }

    #[test]
    fn test_scope_to_data_session_with_task_ids() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            task_ids: vec!["t1".to_string(), "t2".to_string()],
        };
        let data = scope.to_data();
        assert_eq!(data.get("scope.kind").unwrap(), "session");
        assert_eq!(data.get("scope.task_ids").unwrap(), "t1,t2");
    }

    #[test]
    fn test_scope_roundtrip_task() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "abc123".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Task);
        assert_eq!(restored.id, "abc123");
        assert!(restored.task_ids.is_empty());
    }

    #[test]
    fn test_scope_roundtrip_session() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            task_ids: vec!["t1".to_string(), "t2".to_string()],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Session);
        assert_eq!(restored.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(restored.task_ids, vec!["t1", "t2"]);
    }

    #[test]
    fn test_scope_roundtrip_spec() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Plan,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Plan);
        assert_eq!(restored.id, "ops/now/feature.md");
    }

    #[test]
    fn test_scope_roundtrip_code() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Code,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Code);
        assert_eq!(restored.id, "ops/now/feature.md");
    }

    #[test]
    fn test_scope_from_data_missing_type() {
        let data = HashMap::new();
        let result = ReviewScope::from_data(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_from_data_unknown_type() {
        let mut data = HashMap::new();
        data.insert("scope.kind".to_string(), "bogus".to_string());
        let result = ReviewScope::from_data(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_from_data_missing_id_for_task_scope() {
        let mut data = HashMap::new();
        data.insert("scope.kind".to_string(), "task".to_string());
        // No scope.id — should fail for Task scope
        let result = ReviewScope::from_data(&data);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Missing scope.id"),
            "Error should mention missing scope.id"
        );
    }

    #[test]
    fn test_scope_from_data_empty_id_for_task_scope() {
        let mut data = HashMap::new();
        data.insert("scope.kind".to_string(), "task".to_string());
        data.insert("scope.id".to_string(), "".to_string());
        // Empty scope.id — should also fail for Task scope
        let result = ReviewScope::from_data(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_from_data_missing_id_ok_for_session() {
        let mut data = HashMap::new();
        data.insert("scope.kind".to_string(), "session".to_string());
        // No scope.id — should be fine for Session scope
        let result = ReviewScope::from_data(&data);
        assert!(result.is_ok());
    }

    // detect_target tests

    #[test]
    fn test_detect_target_md_file_spec() {
        let dir = tempfile::tempdir().unwrap();
        let md_path = dir.path().join("feature.md");
        std::fs::write(&md_path, "# Feature\n").unwrap();
        let path_str = md_path.to_str().unwrap();

        let (scope, worker) = detect_target(dir.path(), Some(path_str), false).unwrap();
        assert_eq!(scope.kind, ReviewScopeKind::Plan);
        assert_eq!(scope.id, path_str);
        assert!(scope.task_ids.is_empty());
        assert!(worker.is_none());
    }

    #[test]
    fn test_detect_target_md_file_code() {
        let dir = tempfile::tempdir().unwrap();
        let md_path = dir.path().join("feature.md");
        std::fs::write(&md_path, "# Feature\n").unwrap();
        let path_str = md_path.to_str().unwrap();

        let (scope, worker) = detect_target(dir.path(), Some(path_str), true).unwrap();
        assert_eq!(scope.kind, ReviewScopeKind::Code);
        assert_eq!(scope.id, path_str);
        assert!(worker.is_none());
    }

    #[test]
    fn test_detect_target_md_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), Some("nonexistent.md"), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[test]
    fn test_detect_target_code_flag_no_target() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), None, true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--code flag only applies to file targets"));
    }

    #[test]
    fn test_detect_target_code_flag_task_id() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), Some("abcdefgh"), true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--code flag only applies to file targets"));
    }

    #[test]
    fn test_detect_target_non_md_file() {
        let dir = tempfile::tempdir().unwrap();
        let txt_path = dir.path().join("file.txt");
        std::fs::write(&txt_path, "content").unwrap();
        let path_str = txt_path.to_str().unwrap();

        let result = detect_target(dir.path(), Some(path_str), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("File review only supports .md files"));
    }

    #[test]
    fn test_detect_target_unknown_target() {
        let dir = tempfile::tempdir().unwrap();
        // Not a file, not a task ID (has digits and hyphen)
        let result = detect_target(dir.path(), Some("not-a-target-123"), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Target not found"));
    }

    // looks_like_task_id tests

    #[test]
    fn test_looks_like_task_id_valid() {
        assert!(looks_like_task_id("abcdefghijklmnopqrstuvwxyzabcdef"));
        assert!(looks_like_task_id("abc")); // prefix
        assert!(looks_like_task_id("x"));
        assert!(looks_like_task_id("abcdefghijklmnopqrstuvwxyzabcdef.1")); // subtask
        assert!(looks_like_task_id("abcdefghijklmnopqrstuvwxyzabcdef.1.2")); // nested subtask
        assert!(looks_like_task_id("abc.3")); // prefix with subtask
    }

    #[test]
    fn test_looks_like_task_id_invalid() {
        assert!(!looks_like_task_id("")); // empty
        assert!(!looks_like_task_id("ABC")); // uppercase
        assert!(!looks_like_task_id("abc123")); // digits in root
        assert!(!looks_like_task_id("ops/now/feature.md")); // path
        assert!(!looks_like_task_id("hello-world")); // hyphen
        assert!(!looks_like_task_id("has spaces")); // spaces
        assert!(!looks_like_task_id(".1")); // no root
        assert!(!looks_like_task_id("abc.")); // trailing dot, empty suffix
    }

    // Location tests

    #[test]
    fn test_location_parse_path_only() {
        let loc = Location::parse("src/auth.rs").unwrap();
        assert_eq!(loc.path, "src/auth.rs");
        assert_eq!(loc.start_line, None);
        assert_eq!(loc.end_line, None);
    }

    #[test]
    fn test_location_parse_path_and_line() {
        let loc = Location::parse("src/auth.rs:42").unwrap();
        assert_eq!(loc.path, "src/auth.rs");
        assert_eq!(loc.start_line, Some(42));
        assert_eq!(loc.end_line, None);
    }

    #[test]
    fn test_location_parse_path_and_range() {
        let loc = Location::parse("src/auth.rs:42-50").unwrap();
        assert_eq!(loc.path, "src/auth.rs");
        assert_eq!(loc.start_line, Some(42));
        assert_eq!(loc.end_line, Some(50));
    }

    #[test]
    fn test_location_parse_empty() {
        assert!(Location::parse("").is_err());
        assert!(Location::parse("  ").is_err());
    }

    #[test]
    fn test_location_parse_zero_line() {
        assert!(Location::parse("src/auth.rs:0").is_err());
    }

    #[test]
    fn test_location_parse_end_before_start() {
        assert!(Location::parse("src/auth.rs:50-42").is_err());
    }

    #[test]
    fn test_location_display_path_only() {
        let loc = Location { path: "src/auth.rs".into(), start_line: None, end_line: None };
        assert_eq!(loc.to_string(), "src/auth.rs");
    }

    #[test]
    fn test_location_display_with_line() {
        let loc = Location { path: "src/auth.rs".into(), start_line: Some(42), end_line: None };
        assert_eq!(loc.to_string(), "src/auth.rs:42");
    }

    #[test]
    fn test_location_display_with_range() {
        let loc = Location { path: "src/auth.rs".into(), start_line: Some(42), end_line: Some(50) };
        assert_eq!(loc.to_string(), "src/auth.rs:42-50");
    }

    #[test]
    fn test_location_display_same_start_end() {
        let loc = Location { path: "src/auth.rs".into(), start_line: Some(42), end_line: Some(42) };
        assert_eq!(loc.to_string(), "src/auth.rs:42");
    }

    #[test]
    fn test_parse_locations_empty() {
        let data = HashMap::new();
        assert!(parse_locations(&data).is_empty());
    }

    #[test]
    fn test_parse_locations_single_path_only() {
        let mut data = HashMap::new();
        data.insert("path".into(), "src/auth.rs".into());
        let locs = parse_locations(&data);
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].path, "src/auth.rs");
        assert_eq!(locs[0].start_line, None);
    }

    #[test]
    fn test_parse_locations_single_with_lines() {
        let mut data = HashMap::new();
        data.insert("path".into(), "src/auth.rs".into());
        data.insert("start_line".into(), "42".into());
        data.insert("end_line".into(), "50".into());
        let locs = parse_locations(&data);
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].start_line, Some(42));
        assert_eq!(locs[0].end_line, Some(50));
    }

    #[test]
    fn test_parse_locations_multi() {
        let mut data = HashMap::new();
        data.insert("locations".into(), "src/auth.rs:42-50,src/main.rs:108".into());
        let locs = parse_locations(&data);
        assert_eq!(locs.len(), 2);
        assert_eq!(locs[0].path, "src/auth.rs");
        assert_eq!(locs[1].path, "src/main.rs");
    }

    #[test]
    fn test_format_locations_empty() {
        assert_eq!(format_locations(&[]), "");
    }

    #[test]
    fn test_format_locations_single() {
        let locs = vec![Location { path: "src/auth.rs".into(), start_line: Some(42), end_line: Some(50) }];
        assert_eq!(format_locations(&locs), "(src/auth.rs:42-50)");
    }

    #[test]
    fn test_format_locations_multiple() {
        let locs = vec![
            Location { path: "src/auth.rs".into(), start_line: Some(42), end_line: Some(50) },
            Location { path: "src/main.rs".into(), start_line: Some(108), end_line: None },
        ];
        assert_eq!(format_locations(&locs), "(src/auth.rs:42-50, src/main.rs:108)");
    }

    // build_async_review_args tests

    #[test]
    fn build_async_review_args_minimal() {
        let args = build_async_review_args("rev123", None, None, false);
        assert_eq!(args, vec!["review", "--_continue-async", "rev123"]);
    }

    #[test]
    fn build_async_review_args_includes_autorun_when_set() {
        let args = build_async_review_args("rev123", None, None, true);
        assert!(args.contains(&"--autorun".to_string()));
    }

    #[test]
    fn build_async_review_args_excludes_autorun_when_unset() {
        let args = build_async_review_args("rev123", None, None, false);
        assert!(!args.contains(&"--autorun".to_string()));
    }

    #[test]
    fn build_async_review_args_includes_fix_template() {
        let args = build_async_review_args("rev123", Some("fix"), None, false);
        assert!(args.contains(&"--fix-template".to_string()));
        assert!(args.contains(&"fix".to_string()));
    }

    #[test]
    fn build_async_review_args_includes_agent() {
        let args = build_async_review_args("rev123", None, Some("claude-code"), false);
        assert!(args.contains(&"--agent".to_string()));
        assert!(args.contains(&"claude-code".to_string()));
    }

    #[test]
    fn build_async_review_args_all_flags() {
        let args = build_async_review_args("rev123", Some("fix"), Some("claude-code"), true);
        assert_eq!(args, vec![
            "review",
            "--_continue-async",
            "rev123",
            "--fix-template",
            "fix",
            "--agent",
            "claude-code",
            "--autorun",
        ]);
    }

    // ── Regression tests for review-fix execution paths ──────────────

    fn make_test_task(id: &str) -> Task {
        use crate::tasks::{TaskPriority, TaskStatus};
        Task {
            id: id.to_string(),
            name: format!("Task {}", id),
            slug: None,
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            summary: None,
            turn_started: None,
                closed_at: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    #[test]
    fn test_get_issue_comments_empty_task() {
        let task = make_test_task("review-empty");
        assert!(get_issue_comments(&task).is_empty());
    }

    #[test]
    fn test_get_issue_comments_filters_non_issue_comments() {
        let mut task = make_test_task("review-mixed");
        // Regular comment (not an issue)
        task.comments.push(TaskComment {
            id: None,
            text: "Looks good overall".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        // Progress comment
        let mut progress_data = HashMap::new();
        progress_data.insert("type".to_string(), "progress".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Still reviewing".to_string(),
            timestamp: chrono::Utc::now(),
            data: progress_data,
        });
        assert!(get_issue_comments(&task).is_empty());
    }

    #[test]
    fn test_get_issue_comments_finds_issue_comments() {
        let mut task = make_test_task("review-issues");
        // Non-issue comment
        task.comments.push(TaskComment {
            id: None,
            text: "Nice refactor".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        // Issue comment
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        issue_data.insert("severity".to_string(), "high".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Missing null check in auth handler".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });
        // Another issue comment
        let mut issue_data2 = HashMap::new();
        issue_data2.insert("issue".to_string(), "true".to_string());
        issue_data2.insert("severity".to_string(), "low".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Consider adding docstring".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data2,
        });

        let issues = get_issue_comments(&task);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Missing null check in auth handler");
        assert_eq!(issues[1].text, "Consider adding docstring");
    }

    #[test]
    fn test_get_issue_comments_ignores_false_issue_flag() {
        let mut task = make_test_task("review-false-issue");
        let mut data = HashMap::new();
        data.insert("issue".to_string(), "false".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Not actually an issue".to_string(),
            timestamp: chrono::Utc::now(),
            data,
        });
        assert!(get_issue_comments(&task).is_empty());
    }

    #[test]
    fn build_async_review_args_fix_template_only() {
        // Verify --fix-template flag is correctly placed in args for async path
        let args = build_async_review_args("rev456", Some("fix"), None, false);
        assert_eq!(args, vec![
            "review",
            "--_continue-async",
            "rev456",
            "--fix-template",
            "fix",
        ]);
    }

    #[test]
    fn build_async_review_args_fix_template_with_autorun() {
        // --fix-template + --autorun (no agent) for async path
        let args = build_async_review_args("rev789", Some("fix"), None, true);
        assert_eq!(args, vec![
            "review",
            "--_continue-async",
            "rev789",
            "--fix-template",
            "fix",
            "--autorun",
        ]);
    }

    #[test]
    fn build_async_review_args_preserves_review_id() {
        // Ensure the review ID is passed as the third element
        let args = build_async_review_args("abcdefghijklmnopqrstuvwxyzabcdef", None, None, false);
        assert_eq!(args[2], "abcdefghijklmnopqrstuvwxyzabcdef");
    }
}

//! Review domain types and helpers.
//!
//! These types (ReviewScope, Location, etc.) and functions (create_review,
//! detect_target, etc.) are the core review domain logic, extracted from
//! `commands/review.rs` so they can be shared across commands and workflows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agents::{determine_reviewer, is_agent_available, AgentType};
use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::session::find_active_session;
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::templates::create_review_task_from_template;
use crate::tasks::{
    find_task, materialize_graph, read_events, write_link_event_with_autorun, Task, TaskComment,
    TaskStatus,
};
use crate::workflow::{StepResult, WorkflowContext};

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
            ReviewScopeKind::Session => data.get("scope.id").cloned().unwrap_or_default(),
            _ => data
                .get("scope.id")
                .filter(|s| !s.is_empty())
                .cloned()
                .ok_or_else(|| {
                    AikiError::InvalidArgument(format!(
                        "Missing scope.id in review task data (required for {:?} scope kind)",
                        kind_str
                    ))
                })?,
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
            return Err(AikiError::InvalidArgument(
                "Location path must not be empty".into(),
            ));
        }

        if let Some(colon_pos) = s.rfind(':') {
            let path = &s[..colon_pos];
            let line_spec = &s[colon_pos + 1..];

            if !line_spec.is_empty() && line_spec.chars().all(|c| c.is_ascii_digit() || c == '-') {
                if path.is_empty() {
                    return Err(AikiError::InvalidArgument(
                        "Location path must not be empty".into(),
                    ));
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
                        return Err(AikiError::InvalidArgument(
                            "Line numbers must be positive".into(),
                        ));
                    }
                    if end < start {
                        return Err(AikiError::InvalidArgument(format!(
                            "End line ({}) must be >= start line ({})",
                            end, start
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
                        return Err(AikiError::InvalidArgument(
                            "Line numbers must be positive".into(),
                        ));
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
pub(crate) fn parse_locations(data: &HashMap<String, String>) -> Vec<Location> {
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
pub(crate) fn format_locations(locations: &[Location]) -> String {
    if locations.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = locations.iter().map(|l| l.to_string()).collect();
    format!("({})", parts.join(", "))
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

/// Check if a string looks like it could be a task ID or task ID prefix.
///
/// This is a heuristic used by `detect_target` to distinguish task IDs
/// from file paths.
pub(crate) fn looks_like_task_id(s: &str) -> bool {
    crate::tasks::is_task_id(s) || crate::tasks::is_task_id_prefix(s)
}

fn output_nothing_to_review() -> Result<()> {
    output_utils::emit(|| {
        MdBuilder::new().build("Nothing to review — no closed tasks in session.\n")
    });
    Ok(())
}

/// Detect the review target from the CLI argument and flags.
///
/// Returns a `ReviewScope` and optionally a worker agent string (for task targets).
/// The `cwd` is needed to resolve file paths and load tasks.
pub(crate) fn detect_target(
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

        Some(s) if Path::new(s).exists() => Err(AikiError::InvalidArgument(
            "File review only supports .md files currently".to_string(),
        )),

        Some(s) => Err(AikiError::InvalidArgument(format!(
            "Target not found: {}",
            s
        ))),
    }
}

/// Core review creation logic. Used by both CLI and flow action.
///
/// This function creates the review task with subtasks but does NOT
/// start or run the task. The caller is responsible for the execution mode.
/// The scope must be pre-resolved by the caller (via `detect_target()` for CLI,
/// or directly constructed for flow actions).
pub(crate) fn create_review(cwd: &Path, params: CreateReviewParams) -> Result<CreateReviewResult> {
    let scope = params.scope;

    // Determine worker for reviewer assignment.
    // For task scope, use the task's assignee. For all other scopes (code, plan,
    // session), detect the current agent from the active session so that
    // determine_reviewer() can pick a cross-reviewer.
    let worker = match scope.kind {
        ReviewScopeKind::Task => {
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let task = find_task(&tasks, &scope.id)?;
            task.assignee.as_deref().map(|s| s.to_string())
        }
        _ => find_active_session(cwd).map(|s| s.agent_type.as_str().to_string()),
    };

    // Determine assignee for review task
    let assignee = match params.agent_override {
        Some(a) => a,
        None => determine_reviewer(worker.as_deref())?,
    };

    // Validate the reviewer agent is actually installed
    if !is_agent_available(&assignee) {
        let agent_type = AgentType::from_str(&assignee);
        let hint = agent_type
            .map(|a| a.install_hint().to_string())
            .unwrap_or_else(|| format!("Unknown agent: {}", assignee));
        return Err(AikiError::AgentNotInstalled {
            agent: assignee,
            hint,
        });
    }

    let assignee = Some(assignee);

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

    let review_id =
        create_review_task_from_template(cwd, &scope_data, &sources, &assignee, template)?;

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

/// Build the review scope for a build workflow review step.
///
/// Uses `Task` scope so that downstream fix tasks become subtasks of the epic,
/// which triggers `reopen_if_closed` and keeps the epic in-progress during the
/// review/fix cycle.
pub(crate) fn build_review_scope(epic_id: &str) -> ReviewScope {
    ReviewScope {
        kind: ReviewScopeKind::Task,
        id: epic_id.to_string(),
        task_ids: vec![],
    }
}

/// Review step: create a task-scoped review for the epic and run it.
pub(crate) fn run_review_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let epic_id = ctx
        .task_id
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No epic ID in workflow context".to_string()))?
        .clone();
    let scope = build_review_scope(&epic_id);

    let result = create_review(
        &ctx.cwd,
        CreateReviewParams {
            scope,
            agent_override: agent.clone(),
            template,
            fix_template: None,
            autorun: false,
        },
    )?;

    // Link review to epic
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    crate::tasks::write_link_event(
        &ctx.cwd,
        &graph,
        "validates",
        &result.review_task_id,
        &epic_id,
    )?;

    // Run the review to completion
    let options = TaskRunOptions::new().quiet();
    task_run(&ctx.cwd, &result.review_task_id, options)?;

    Ok(StepResult {
        message: "Review complete".to_string(),
        task_id: Some(result.review_task_id),
    })
}

/// Standalone review step: create review, run it, count issues.
pub(crate) fn run_standalone_review_step(
    ctx: &mut WorkflowContext,
    scope: ReviewScope,
    template: Option<String>,
    agent: Option<String>,
    fix_template: Option<String>,
    autorun: bool,
) -> anyhow::Result<StepResult> {
    let result = create_review(
        &ctx.cwd,
        CreateReviewParams {
            scope,
            agent_override: agent,
            template,
            fix_template,
            autorun,
        },
    )?;
    let review_id = result.review_task_id;
    ctx.task_id = Some(review_id.clone());

    let options = TaskRunOptions::new().quiet();
    task_run(&ctx.cwd, &review_id, options)?;

    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    let issue_count = find_task(&graph.tasks, &review_id)
        .map(|t| {
            t.data
                .get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0)
        })
        .unwrap_or(0);

    let message = if issue_count > 0 {
        format!("Found {} issues", issue_count)
    } else {
        "approved".to_string()
    };

    Ok(StepResult {
        message,
        task_id: Some(review_id),
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

/// Get all issue comments from a task (comments where data.issue == "true").
///
/// This is the canonical function for filtering issue comments — used by both
/// `aiki review issue list` and `aiki fix`.
pub(crate) fn get_issue_comments(task: &Task) -> Vec<&TaskComment> {
    task.comments
        .iter()
        .filter(|c| c.data.get("issue").map(|v| v == "true").unwrap_or(false))
        .collect()
}

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
use crate::agents::AgentType;
use crate::commands::OutputFormat;
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::{
    find_task, materialize_graph, read_events, reassign_task, start_task_core, Task, TaskComment,
    TaskStatus,
};
use crate::workflow::builders::review_workflow;
use crate::workflow::orchestrate::{is_review_task, run_fix};
#[cfg(test)]
use crate::workflow::steps::review::looks_like_task_id;
use crate::workflow::steps::review::{
    build_async_review_args, create_review, detect_target, format_locations, get_issue_comments,
    parse_locations, CreateReviewParams, CreateReviewResult, Location, ReviewScope,
    ReviewScopeKind,
};
use crate::workflow::RunMode;

/// Parse and validate a severity value for clap's value_parser.
fn parse_severity(s: &str) -> std::result::Result<String, String> {
    match s {
        "high" | "medium" | "low" => Ok(s.to_string()),
        _ => Err(format!(
            "invalid severity '{}': must be high, medium, or low",
            s
        )),
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
    let fix_template = args.fix_template.or(if args.fix {
        Some("fix".to_string())
    } else {
        None
    });

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
                ReviewIssueSubcommands::Add {
                    review_id,
                    text,
                    severity,
                    files,
                    high,
                    low,
                } => run_issue_add(&cwd, &review_id, &text, severity, &files, high, low),
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
            "--fix and --start cannot be used together. Use --fix with blocking or --async mode."
                .to_string(),
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

    // Detect target and resolve scope at CLI layer (BEFORE workflow assembly)
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

    let output_id = matches!(output_format, Some(OutputFormat::Id));

    // --start path: create review task but don't run it (NOT a workflow)
    if start {
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
            Err(AikiError::NothingToReview) => return Ok(()),
            Err(e) => return Err(e),
        };
        let review_id = result.review_task_id;

        // Reassign task to current agent (caller takes over)
        if let Some(session) = find_active_session(cwd) {
            reassign_task(cwd, &review_id, session.agent_type.as_str())?;
        }
        // Start task using core logic (validates, auto-stops, emits events)
        start_task_core(cwd, &[review_id.clone()])?;
        if !output_id {
            output_review_started(cwd, &review_id)?;
        }
        if output_id {
            println!("{}", review_id);
        }
        return Ok(());
    }

    // --async path: create review task and spawn background process
    if run_async {
        let fix_template_for_spawn = fix_template.clone();
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
            Err(AikiError::NothingToReview) => return Ok(()),
            Err(e) => return Err(e),
        };
        let review_id = result.review_task_id;

        let spawn_args = build_async_review_args(
            &review_id,
            fix_template_for_spawn.as_deref(),
            agent.as_deref(),
            autorun,
        );
        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if !output_id {
            output_review_async(cwd, &review_id)?;
        }
        if output_id {
            println!("{}", review_id);
        }
        return Ok(());
    }

    // Sync path: blocking review (+ optional fix)
    {
        let has_fix = fix_template.is_some();
        let wf = review_workflow(
            cwd.to_path_buf(),
            scope,
            template_name,
            agent_override,
            fix_template.clone(),
            autorun,
        );
        let ctx = wf.run(RunMode::Text)?;
        let review_id = ctx
            .task_id
            .expect("Review step should set task_id in context");

        // Post-workflow: check for issues, maybe run fix
        let has_issues = {
            let events = read_events(cwd)?;
            let graph = materialize_graph(&events);
            find_task(&graph.tasks, &review_id)
                .map(|t| {
                    t.data
                        .get("issue_count")
                        .and_then(|c| c.parse::<usize>().ok())
                        .unwrap_or(0)
                        > 0
                })
                .unwrap_or(false)
        };

        if has_fix && has_issues {
            run_fix(
                cwd,
                &review_id,
                false,
                None,
                fix_template,
                None,
                None,
                None,
                agent,
                autorun,
                false,
                output_format,
            )?;
        } else if output_id {
            println!("{}", review_id);
        } else {
            output_review_completed(cwd, &review_id)?;
        }
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
    // Run the review (quiet — no TUI, background/workflow handles output)
    let mut options = TaskRunOptions::new().quiet();
    if let Some(ref agent_str) = agent {
        if let Some(agent_type) = AgentType::from_str(agent_str) {
            options = options.with_agent(agent_type);
        }
    }
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
        run_fix(
            cwd,
            review_id,
            false,
            None,
            fix_template,
            None,
            None,
            None,
            agent,
            autorun,
            false,
            None,
        )?;
    }

    Ok(())
}

/// Summarize a review task as a short text line (issue count or approved).
fn review_summary(cwd: &Path, review_id: &str) -> Result<String> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let task = find_task(&graph.tasks, review_id)?;
    let issue_count = task
        .data
        .get("issue_count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0);
    if issue_count > 0 {
        Ok(format!("Found {} issues", issue_count))
    } else {
        Ok("approved".to_string())
    }
}

/// Output review started message (for --start mode)
fn output_review_started(_cwd: &Path, review_id: &str) -> Result<()> {
    output_utils::emit(|| format!("Started: {review_id}\n"));
    Ok(())
}

/// Output review async message (for --async mode)
fn output_review_async(_cwd: &Path, review_id: &str) -> Result<()> {
    output_utils::emit(|| format!("Dispatched: {review_id}\n"));
    Ok(())
}

/// Output review completed message (for blocking mode)
fn output_review_completed(cwd: &Path, review_id: &str) -> Result<()> {
    let summary = review_summary(cwd, review_id)?;
    let hint = format!("\n---\nRun `aiki fix {}` to remediate.\n", review_id);
    output_utils::emit(|| {
        let status = format!("Completed: {review_id} — {summary}\n");
        format!("{status}{hint}")
    });
    Ok(())
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
    if !is_review_task(task) {
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
    let graph = materialize_graph(&events);
    let task = find_task(&graph.tasks, review_id)?;

    // Validate it's a review task
    if !is_review_task(task) {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task.",
            review_id
        )));
    }

    let issues = get_issue_comments(task);
    if issues.is_empty() {
        output_utils::emit(|| "No issues found.\n".to_string());
    } else {
        output_utils::emit(|| {
            let mut out = format!("{} issues:\n", issues.len());
            for (i, issue) in issues.iter().enumerate() {
                let severity = issue
                    .data
                    .get("severity")
                    .map(|s| s.as_str())
                    .unwrap_or("medium");
                out.push_str(&format!("  {}. [{}] {}\n", i + 1, severity, issue.text));
                // Show file locations if present
                if let Some(files) = issue.data.get("files") {
                    for file in files.split(',') {
                        let file = file.trim();
                        if !file.is_empty() {
                            out.push_str(&format!("     {}\n", file));
                        }
                    }
                }
            }
            out
        });
    }

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
            MdBuilder::new().build(content)
        });
        return Ok(());
    }

    output_utils::emit(|| {
        let mut content = String::from("## Reviews\n| ID | Status | Outcome | Issues | Name |\n|----|--------|---------|--------|------|\n");
        for review in &reviews {
            let status_str = match review.status {
                TaskStatus::Open => "open",
                TaskStatus::Reserved => "reserved",
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
                &review.id, status_str, outcome_str, issue_count, &review.name
            ));
        }
        MdBuilder::new().build(&content)
    });

    Ok(())
}

/// Show review task details
fn show_review(cwd: &Path, task_id: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    let task = find_task(&graph.tasks, task_id)?;

    // Verify it's a review task
    if task.task_type.as_deref() != Some("review") {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task (type: {:?})",
            task_id, task.task_type
        )));
    }

    let issues = get_issue_comments(task);
    let status_str = match task.status {
        TaskStatus::Open => "open",
        TaskStatus::Reserved => "reserved",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Closed => "closed",
    };
    let scope_kind = task
        .data
        .get("scope.kind")
        .map(|s| s.as_str())
        .unwrap_or("unknown");
    let scope_id = task.data.get("scope.id").map(|s| s.as_str()).unwrap_or("");

    output_utils::emit(|| {
        let mut out = format!("Review: {}\n", task_id);
        out.push_str(&format!("Status: {}\n", status_str));
        out.push_str(&format!("Scope: {} {}\n", scope_kind, scope_id));
        if let Some(agent) = task.agent_label() {
            out.push_str(&format!("Agent: {}\n", agent));
        }
        if issues.is_empty() {
            out.push_str("Result: approved\n");
        } else {
            out.push_str(&format!("Issues: {}\n", issues.len()));
            for (i, issue) in issues.iter().enumerate() {
                let severity = issue
                    .data
                    .get("severity")
                    .map(|s| s.as_str())
                    .unwrap_or("medium");
                out.push_str(&format!("  {}. [{}] {}\n", i + 1, severity, issue.text));
            }
        }
        out
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::determine_reviewer_with;

    #[test]
    fn test_determine_reviewer_empty_list_errors() {
        let result = determine_reviewer_with(None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_determine_reviewer_single_agent_no_worker() {
        let agents = [AgentType::ClaudeCode];
        let result = determine_reviewer_with(None, &agents).unwrap();
        assert_eq!(result, "claude-code");
    }

    #[test]
    fn test_determine_reviewer_single_agent_matching_worker() {
        // Self-review when only the worker agent is available
        let agents = [AgentType::ClaudeCode];
        let result = determine_reviewer_with(Some("claude-code"), &agents).unwrap();
        assert_eq!(result, "claude-code");
    }

    #[test]
    fn test_determine_reviewer_two_agents_cross_review() {
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        // Worker is claude-code → reviewer should be codex
        let result = determine_reviewer_with(Some("claude-code"), &agents).unwrap();
        assert_eq!(result, "codex");
    }

    #[test]
    fn test_determine_reviewer_unknown_worker() {
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        // Unknown worker → returns first available
        let result = determine_reviewer_with(Some("unknown-agent"), &agents).unwrap();
        assert_eq!(result, "claude-code");
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
        assert_eq!(
            ReviewScopeKind::from_str("task").unwrap(),
            ReviewScopeKind::Task
        );
        assert_eq!(
            ReviewScopeKind::from_str("plan").unwrap(),
            ReviewScopeKind::Plan
        );
        assert_eq!(
            ReviewScopeKind::from_str("code").unwrap(),
            ReviewScopeKind::Code
        );
        assert_eq!(
            ReviewScopeKind::from_str("session").unwrap(),
            ReviewScopeKind::Session
        );
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
    fn test_review_workflow_is_single_step() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "abc123".to_string(),
            task_ids: vec![],
        };

        let wf = review_workflow(
            PathBuf::from("."),
            scope,
            Some("review/code".to_string()),
            Some("codex".to_string()),
            Some("fix".to_string()),
            true,
        );

        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].name(), "review");
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
        assert!(result.unwrap_err().to_string().contains("Target not found"));
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
        let loc = Location {
            path: "src/auth.rs".into(),
            start_line: None,
            end_line: None,
        };
        assert_eq!(loc.to_string(), "src/auth.rs");
    }

    #[test]
    fn test_location_display_with_line() {
        let loc = Location {
            path: "src/auth.rs".into(),
            start_line: Some(42),
            end_line: None,
        };
        assert_eq!(loc.to_string(), "src/auth.rs:42");
    }

    #[test]
    fn test_location_display_with_range() {
        let loc = Location {
            path: "src/auth.rs".into(),
            start_line: Some(42),
            end_line: Some(50),
        };
        assert_eq!(loc.to_string(), "src/auth.rs:42-50");
    }

    #[test]
    fn test_location_display_same_start_end() {
        let loc = Location {
            path: "src/auth.rs".into(),
            start_line: Some(42),
            end_line: Some(42),
        };
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
        data.insert(
            "locations".into(),
            "src/auth.rs:42-50,src/main.rs:108".into(),
        );
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
        let locs = vec![Location {
            path: "src/auth.rs".into(),
            start_line: Some(42),
            end_line: Some(50),
        }];
        assert_eq!(format_locations(&locs), "(src/auth.rs:42-50)");
    }

    #[test]
    fn test_format_locations_multiple() {
        let locs = vec![
            Location {
                path: "src/auth.rs".into(),
                start_line: Some(42),
                end_line: Some(50),
            },
            Location {
                path: "src/main.rs".into(),
                start_line: Some(108),
                end_line: None,
            },
        ];
        assert_eq!(
            format_locations(&locs),
            "(src/auth.rs:42-50, src/main.rs:108)"
        );
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
        assert_eq!(
            args,
            vec![
                "review",
                "--_continue-async",
                "rev123",
                "--fix-template",
                "fix",
                "--agent",
                "claude-code",
                "--autorun",
            ]
        );
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
        assert_eq!(
            args,
            vec![
                "review",
                "--_continue-async",
                "rev456",
                "--fix-template",
                "fix",
            ]
        );
    }

    #[test]
    fn build_async_review_args_fix_template_with_autorun() {
        // --fix-template + --autorun (no agent) for async path
        let args = build_async_review_args("rev789", Some("fix"), None, true);
        assert_eq!(
            args,
            vec![
                "review",
                "--_continue-async",
                "rev789",
                "--fix-template",
                "fix",
                "--autorun",
            ]
        );
    }

    #[test]
    fn build_async_review_args_preserves_review_id() {
        // Ensure the review ID is passed as the third element
        let args = build_async_review_args("abcdefghijklmnopqrstuvwxyzabcdef", None, None, false);
        assert_eq!(args[2], "abcdefghijklmnopqrstuvwxyzabcdef");
    }

    // ═══════════════════════════════════════════════════════════════════
    // Pre-refactor behavioral contract tests for review orchestration
    // ═══════════════════════════════════════════════════════════════════

    // --- detect_target contract ---

    #[test]
    fn test_detect_target_md_file_defaults_to_plan_scope() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("plan.md");
        std::fs::write(&md, "# Plan").unwrap();
        let (scope, _) = detect_target(dir.path(), Some(md.to_str().unwrap()), false).unwrap();
        assert_eq!(scope.kind, ReviewScopeKind::Plan);
    }

    #[test]
    fn test_detect_target_md_file_with_code_flag_is_code_scope() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("code.md");
        std::fs::write(&md, "# Code").unwrap();
        let (scope, _) = detect_target(dir.path(), Some(md.to_str().unwrap()), true).unwrap();
        assert_eq!(scope.kind, ReviewScopeKind::Code);
    }

    #[test]
    fn test_detect_target_missing_md_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), Some("missing.md"), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_target_code_flag_without_target_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), None, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--code"));
    }

    #[test]
    fn test_detect_target_code_flag_with_task_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_target(dir.path(), Some("abcdefgh"), true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--code"));
    }

    #[test]
    fn test_detect_target_non_md_file_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("file.txt");
        std::fs::write(&txt, "content").unwrap();
        let result = detect_target(dir.path(), Some(txt.to_str().unwrap()), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".md"));
    }

    // --- looks_like_task_id contract ---

    #[test]
    fn test_looks_like_task_id_full_id() {
        assert!(looks_like_task_id("abcdefghijklmnopqrstuvwxyzabcdef"));
    }

    #[test]
    fn test_looks_like_task_id_prefix() {
        assert!(looks_like_task_id("abc"));
        assert!(looks_like_task_id("x"));
    }

    #[test]
    fn test_looks_like_task_id_subtask() {
        assert!(looks_like_task_id("abc.1"));
        assert!(looks_like_task_id("abc.1.2"));
    }

    #[test]
    fn test_looks_like_task_id_rejects_paths() {
        assert!(!looks_like_task_id("ops/now/feature.md"));
        assert!(!looks_like_task_id("./feature.md"));
        assert!(!looks_like_task_id("/abs/path"));
    }

    #[test]
    fn test_looks_like_task_id_rejects_mixed_chars() {
        assert!(!looks_like_task_id("abc123")); // digits in root
        assert!(!looks_like_task_id("ABC")); // uppercase
        assert!(!looks_like_task_id("hello-world")); // hyphen
        assert!(!looks_like_task_id("")); // empty
        assert!(!looks_like_task_id(".1")); // no root
        assert!(!looks_like_task_id("abc.")); // trailing dot
    }

    // --- get_issue_comments contract ---

    #[test]
    fn test_get_issue_comments_only_returns_true_issues() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-filter");

        // Non-issue comment
        task.comments.push(TaskComment {
            id: None,
            text: "Looks good".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });

        // Issue with data.issue="true"
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Bug here".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });

        // Comment with data.issue="false" — NOT an issue
        let mut false_data = HashMap::new();
        false_data.insert("issue".to_string(), "false".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Resolved".to_string(),
            timestamp: chrono::Utc::now(),
            data: false_data,
        });

        let issues = get_issue_comments(&task);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Bug here");
    }

    // --- ReviewScope.name() contract ---

    #[test]
    fn test_review_scope_name_includes_filename_for_plan() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Plan,
            id: "ops/now/very/deep/plan.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Plan (plan.md)");
    }

    #[test]
    fn test_review_scope_name_session_is_plain() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "anything".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Session");
    }

    // --- ReviewScopeKind roundtrip contract ---

    #[test]
    fn test_scope_kind_roundtrip_all_variants() {
        for kind in [
            ReviewScopeKind::Task,
            ReviewScopeKind::Plan,
            ReviewScopeKind::Code,
            ReviewScopeKind::Session,
        ] {
            let s = kind.as_str();
            let restored = ReviewScopeKind::from_str(s).unwrap();
            assert_eq!(restored, kind);
        }
    }

    // --- create_review params contract: scope-specific default templates ---

    #[test]
    fn test_default_template_for_session_scope() {
        let kind = ReviewScopeKind::Session;
        let default = match kind {
            ReviewScopeKind::Session => "review/task".to_string(),
            _ => format!("review/{}", kind.as_str()),
        };
        assert_eq!(default, "review/task");
    }

    #[test]
    fn test_default_template_for_task_scope() {
        let kind = ReviewScopeKind::Task;
        let default = match kind {
            ReviewScopeKind::Session => "review/task".to_string(),
            _ => format!("review/{}", kind.as_str()),
        };
        assert_eq!(default, "review/task");
    }

    #[test]
    fn test_default_template_for_plan_scope() {
        let kind = ReviewScopeKind::Plan;
        let default = match kind {
            ReviewScopeKind::Session => "review/task".to_string(),
            _ => format!("review/{}", kind.as_str()),
        };
        assert_eq!(default, "review/plan");
    }

    #[test]
    fn test_default_template_for_code_scope() {
        let kind = ReviewScopeKind::Code;
        let default = match kind {
            ReviewScopeKind::Session => "review/task".to_string(),
            _ => format!("review/{}", kind.as_str()),
        };
        assert_eq!(default, "review/code");
    }

    // --- Scope data includes fix options when provided ---

    #[test]
    fn test_scope_data_stores_fix_options() {
        let mut scope_data = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "task123".to_string(),
            task_ids: vec![],
        }
        .to_data();

        // Simulate what create_review does when fix_template is provided
        let fix_template = Some("custom/fix".to_string());
        if let Some(ref tmpl) = fix_template {
            scope_data.insert("options.fix".to_string(), "true".to_string());
            scope_data.insert("options.fix_template".to_string(), tmpl.clone());
        }

        assert_eq!(scope_data.get("options.fix").unwrap(), "true");
        assert_eq!(
            scope_data.get("options.fix_template").unwrap(),
            "custom/fix"
        );
    }

    #[test]
    fn test_scope_data_no_fix_options_when_none() {
        let scope_data = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "task123".to_string(),
            task_ids: vec![],
        }
        .to_data();

        assert!(scope_data.get("options.fix").is_none());
        assert!(scope_data.get("options.fix_template").is_none());
    }

    // --- build_async_review_args contract ---

    #[test]
    fn test_async_review_args_structure() {
        // The args must always start with ["review", "--_continue-async", <review_id>]
        let args = build_async_review_args("rev1", None, None, false);
        assert_eq!(args[0], "review");
        assert_eq!(args[1], "--_continue-async");
        assert_eq!(args[2], "rev1");
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_async_review_args_with_all_options() {
        let args = build_async_review_args("rev1", Some("fix"), Some("claude-code"), true);
        assert!(args.contains(&"--fix-template".to_string()));
        assert!(args.contains(&"fix".to_string()));
        assert!(args.contains(&"--agent".to_string()));
        assert!(args.contains(&"claude-code".to_string()));
        assert!(args.contains(&"--autorun".to_string()));
    }
}

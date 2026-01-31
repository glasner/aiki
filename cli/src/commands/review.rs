//! Review command for creating and running code review tasks
//!
//! This module provides the `aiki review` command which:
//! - Creates a review task with subtasks (digest, review)
//! - Runs the review task (default: completion, --async: async, --start: hand off)
//! - Supports different review scopes (task ID or session)
//! - Lists review tasks (list subcommand)
//! - Shows review task details (show subcommand)

use clap::Subcommand;
use std::env;
use std::io::IsTerminal;
use std::path::Path;

use crate::agents::{determine_reviewer, AgentType};
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::runner::{task_run, task_run_async, TaskRunOptions};
use crate::tasks::templates::create_review_task_from_template;
use crate::tasks::xml::{escape_xml, XmlBuilder};
use crate::tasks::{
    find_task, get_current_scope_set, get_in_progress,
    get_ready_queue_for_scope_set, materialize_tasks, read_events,
    reassign_task, start_task_core, Task, TaskStatus,
};

/// Review subcommands
#[derive(Subcommand)]
pub enum ReviewCommands {
    /// Create and run a review (default when no subcommand)
    Create {
        /// Task ID to review (reviews all closed tasks in session if not specified)
        task_id: Option<String>,

        /// Run review asynchronously (return immediately)
        #[arg(long = "async")]
        run_async: bool,

        /// Start review and return control to calling agent
        #[arg(long)]
        start: bool,

        /// Task template to use (default: aiki/review)
        #[arg(long)]
        template: Option<String>,

        /// Agent for review assignment (default: opposite of task worker)
        #[arg(long)]
        agent: Option<String>,
    },

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
}

/// Run the review command
pub fn run(command: Option<ReviewCommands>) -> Result<()> {
    let cwd = env::current_dir().map_err(|_| {
        AikiError::InvalidArgument("Failed to get current directory".to_string())
    })?;

    match command {
        None => {
            // Default: create review for session
            run_review(&cwd, None, false, false, None, None)
        }
        Some(ReviewCommands::Create {
            task_id,
            run_async,
            start,
            template,
            agent,
        }) => run_review(&cwd, task_id, run_async, start, template, agent),
        Some(ReviewCommands::List { all }) => list_reviews(&cwd, all),
        Some(ReviewCommands::Show { task_id }) => show_review(&cwd, &task_id),
    }
}

/// Parameters for creating a review task
#[derive(Debug, Clone)]
pub struct CreateReviewParams {
    /// Task ID to review (None for session scope)
    pub task_id: Option<String>,
    /// Override the reviewer agent
    pub agent_override: Option<String>,
    /// Template to use (default: aiki/review)
    pub template: Option<String>,
}

/// Result of creating a review task
#[derive(Debug, Clone)]
pub struct CreateReviewResult {
    /// The created review task ID
    pub review_task_id: String,
    /// The scope name (task name or session name)
    pub scope_name: String,
    /// The scope ID (task ID or "session")
    pub scope_id: String,
    /// The assigned reviewer
    pub assignee: Option<String>,
}

/// Core review creation logic. Used by both CLI and flow action.
///
/// This function creates the review task with subtasks but does NOT
/// start or run the task. The caller is responsible for the execution mode.
pub fn create_review(cwd: &Path, params: CreateReviewParams) -> Result<CreateReviewResult> {
    // Load tasks
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Get session info (needed for session scope)
    let session = find_active_session(cwd);

    // Determine review scope and worker (for reviewer assignment)
    let (scope_name, scope_id, worker) = match params.task_id {
        Some(ref id) => {
            // Review specific task - worker is task's assignee
            let task = find_task(&tasks, id)
                .ok_or_else(|| AikiError::TaskNotFound(id.clone()))?;
            let worker = task.assignee.as_deref();
            (task.name.clone(), id.clone(), worker.map(|s| s.to_string()))
        }
        None => {
            // Review all closed tasks in current session - worker is session's agent
            let (session_id, session_name, session_agent) = match &session {
                Some(s) => (
                    Some(s.session_id.clone()),
                    format!("session {}", &s.external_session_id[..8.min(s.external_session_id.len())]),
                    Some(s.agent_type.as_str().to_string()),
                ),
                None => (None, "current session".to_string(), None),
            };

            // Filter to closed tasks that were worked on in the current session
            let closed_tasks: Vec<Task> = tasks
                .values()
                .filter(|t| {
                    t.status == TaskStatus::Closed
                        && match (&t.last_session_id, &session_id) {
                            (Some(task_session), Some(current_session)) => {
                                task_session == current_session
                            }
                            // If no session, fall back to all closed tasks
                            (_, None) => true,
                            // If task has no session but we have one, skip it
                            (None, Some(_)) => false,
                        }
                })
                .cloned()
                .collect();

            if closed_tasks.is_empty() {
                // No closed tasks to review - succeed with message
                output_nothing_to_review()?;
                return Err(AikiError::NothingToReview);
            }

            (session_name, "session".to_string(), session_agent)
        }
    };

    // Determine assignee for review task
    let assignee = params.agent_override
        .or_else(|| Some(determine_reviewer(worker.as_deref())));

    // Create review task with subtasks from template
    let template = params.template.as_deref().unwrap_or("aiki/review");
    let review_id = create_review_task_from_template(cwd, &scope_name, &scope_id, &assignee, template)?;

    Ok(CreateReviewResult {
        review_task_id: review_id,
        scope_name,
        scope_id,
        assignee,
    })
}

/// Core review implementation
fn run_review(
    cwd: &Path,
    task_id: Option<String>,
    run_async: bool,
    start: bool,
    template_name: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    // Parse agent if provided
    let agent_override = if let Some(ref agent_str) = agent {
        let agent_type = AgentType::from_str(agent_str)
            .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?;
        Some(agent_type.as_str().to_string())
    } else {
        None
    };

    // Create review task using shared logic
    let result = match create_review(cwd, CreateReviewParams {
        task_id,
        agent_override,
        template: template_name,
    }) {
        Ok(r) => r,
        Err(AikiError::NothingToReview) => {
            // Already output message in create_review
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    let review_id = result.review_task_id;

    // Re-read tasks to include newly created review task
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let scope_set = get_current_scope_set(&tasks);
    let in_progress: Vec<&Task> = get_in_progress(&tasks).into_iter().collect();
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);

    // Handle execution mode
    if start {
        // Reassign task to current agent (caller takes over)
        if let Some(session) = find_active_session(cwd) {
            reassign_task(cwd, &review_id, session.agent_type.as_str())?;
        }
        // Start task using core logic (validates, auto-stops, emits events)
        start_task_core(cwd, &[review_id.clone()])?;
        output_review_started(&review_id, &in_progress, &ready)?;
    } else if run_async {
        // Run async and return immediately
        let options = TaskRunOptions::new();
        task_run_async(cwd, &review_id, options)?;
        output_review_async(&review_id)?;
        // Output task ID to stdout if piped
        if !std::io::stdout().is_terminal() {
            println!("{}", review_id);
        }
    } else {
        // Run to completion (default)
        let options = TaskRunOptions::new();
        task_run(cwd, &review_id, options)?;
        output_review_completed(&review_id)?;
        // Output task ID to stdout if piped
        if !std::io::stdout().is_terminal() {
            println!("{}", review_id);
        }
    }

    Ok(())
}

/// Output message when there's nothing to review
fn output_nothing_to_review() -> Result<()> {
    let content =
        "  <approved>\n    Nothing to review - no closed tasks in session.\n  </approved>";
    let xml = XmlBuilder::new("review").build(content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Output review started message (for --start mode)
fn output_review_started(review_id: &str, in_progress: &[&Task], ready: &[&Task]) -> Result<()> {
    let content = format!(
        "  <started task_id=\"{}\">\n    Review task started. You are now reviewing.\n  </started>",
        escape_xml(review_id)
    );
    let xml = XmlBuilder::new("review").build(&content, in_progress, ready);
    eprintln!("{}", xml);

    // Output task ID to stdout if piped
    if !std::io::stdout().is_terminal() {
        println!("{}", review_id);
    }

    Ok(())
}

/// Output review async message (for --async mode)
fn output_review_async(review_id: &str) -> Result<()> {
    let content = format!(
        "  <started task_id=\"{}\">\n    Review started in background.\n  </started>",
        escape_xml(review_id)
    );
    let xml = XmlBuilder::new("review").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Output review completed message (for blocking mode)
fn output_review_completed(review_id: &str) -> Result<()> {
    // TODO: Get actual comment count from task
    let content = format!(
        "  <completed task_id=\"{}\">\n    Review completed.\n  </completed>",
        escape_xml(review_id)
    );
    let xml = XmlBuilder::new("review").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// List review tasks
fn list_reviews(cwd: &Path, all: bool) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

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
        let content = if all {
            "  <empty>No review tasks found.</empty>"
        } else {
            "  <empty>No open review tasks. Use --all to see closed reviews.</empty>"
        };
        let xml = XmlBuilder::new("review-list").build(content, &[], &[]);
        eprintln!("{}", xml);
        return Ok(());
    }

    // Format review list
    let mut lines = Vec::new();
    for review in &reviews {
        let status_str = match review.status {
            TaskStatus::Open => "open",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Stopped => "stopped",
            TaskStatus::Closed => "closed",
        };

        // Get outcome if closed
        let outcome_str = review
            .closed_outcome
            .as_ref()
            .map(|o| format!(" outcome=\"{}\"", escape_xml(&format!("{:?}", o).to_lowercase())))
            .unwrap_or_default();

        // Count comments with issues
        let issue_count = review.comments.len();
        let issues_str = if issue_count > 0 {
            format!(" issues=\"{}\"", issue_count)
        } else {
            String::new()
        };

        lines.push(format!(
            "  <review id=\"{}\" status=\"{}\"{}{}>\n    {}\n  </review>",
            escape_xml(&review.id),
            status_str,
            outcome_str,
            issues_str,
            escape_xml(&review.name)
        ));
    }

    let content = lines.join("\n");
    let xml = XmlBuilder::new("review-list").build(&content, &[], &[]);
    eprintln!("{}", xml);

    Ok(())
}

/// Show review task details
fn show_review(cwd: &Path, task_id: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    let task = find_task(&tasks, task_id)
        .ok_or_else(|| AikiError::TaskNotFound(task_id.to_string()))?;

    // Verify it's a review task
    if task.task_type.as_deref() != Some("review") {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task (type: {:?})",
            task_id, task.task_type
        )));
    }

    let status_str = match task.status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Closed => "closed",
    };

    // Get outcome if closed
    let outcome_str = task
        .closed_outcome
        .as_ref()
        .map(|o| format!(" outcome=\"{}\"", escape_xml(&format!("{:?}", o).to_lowercase())))
        .unwrap_or_default();

    // Get assignee
    let assignee_str = task
        .assignee
        .as_ref()
        .map(|a| format!(" assignee=\"{}\"", escape_xml(a)))
        .unwrap_or_default();

    // Build content
    let mut content_lines = vec![
        format!(
            "  <review id=\"{}\" status=\"{}\"{}{}>\n    <name>{}</name>",
            escape_xml(&task.id),
            status_str,
            outcome_str,
            assignee_str,
            escape_xml(&task.name)
        ),
    ];

    // Add sources if any
    if !task.sources.is_empty() {
        content_lines.push("    <sources>".to_string());
        for source in &task.sources {
            content_lines.push(format!("      <source>{}</source>", escape_xml(source)));
        }
        content_lines.push("    </sources>".to_string());
    }

    // Add comments/issues
    if !task.comments.is_empty() {
        content_lines.push("    <issues>".to_string());
        for (idx, comment) in task.comments.iter().enumerate() {
            let mut attrs = Vec::new();

            // Extract structured data
            if let Some(file) = comment.data.get("file") {
                attrs.push(format!("file=\"{}\"", escape_xml(file)));
            }
            if let Some(line) = comment.data.get("line") {
                attrs.push(format!("line=\"{}\"", escape_xml(line)));
            }
            if let Some(severity) = comment.data.get("severity") {
                attrs.push(format!("severity=\"{}\"", escape_xml(severity)));
            }
            if let Some(category) = comment.data.get("category") {
                attrs.push(format!("category=\"{}\"", escape_xml(category)));
            }

            let attrs_str = if attrs.is_empty() {
                String::new()
            } else {
                format!(" {}", attrs.join(" "))
            };

            // Use index-based ID for display since TaskComment doesn't have an id field
            content_lines.push(format!(
                "      <issue n=\"{}\"{}>\n        {}\n      </issue>",
                idx + 1,
                attrs_str,
                escape_xml(&comment.text)
            ));
        }
        content_lines.push("    </issues>".to_string());
    }

    // Find followup tasks (tasks sourced from this review's comments)
    let followups: Vec<&Task> = tasks
        .values()
        .filter(|t| {
            t.sources.iter().any(|s| {
                s.starts_with(&format!("comment:{}", task.id))
                    || s.starts_with(&format!("task:{}", task.id))
            })
        })
        .collect();

    if !followups.is_empty() {
        content_lines.push("    <followups>".to_string());
        for followup in &followups {
            let fu_status = match followup.status {
                TaskStatus::Open => "open",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Stopped => "stopped",
                TaskStatus::Closed => "closed",
            };
            content_lines.push(format!(
                "      <followup id=\"{}\" status=\"{}\">{}</followup>",
                escape_xml(&followup.id),
                fu_status,
                escape_xml(&followup.name)
            ));
        }
        content_lines.push("    </followups>".to_string());
    }

    content_lines.push("  </review>".to_string());

    let content = content_lines.join("\n");
    let xml = XmlBuilder::new("review-show").build(&content, &[], &[]);
    eprintln!("{}", xml);

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
}

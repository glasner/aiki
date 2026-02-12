//! Review command for creating and running code review tasks
//!
//! This module provides the `aiki review` command which:
//! - Creates a review task with subtasks (digest, review)
//! - Runs the review task (default: completion, --async: async, --start: hand off)
//! - Supports different review scopes (task ID or session)
//! - Lists review tasks (list subcommand)
//! - Shows review task details (show subcommand)

use clap::Subcommand;
use std::collections::HashMap;
use std::env;
use std::io::IsTerminal;
use std::path::Path;

use crate::agents::{determine_reviewer, AgentType};
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::runner::{task_run, task_run_async, TaskRunOptions};
use crate::tasks::templates::create_review_task_from_template;
use crate::tasks::md::MdBuilder;
use crate::tasks::{
    find_task, get_current_scope_set, get_in_progress, get_ready_queue_for_scope_set,
    materialize_graph, read_events, reassign_task, start_task_core, Task, TaskStatus,
};

/// What kind of review scope this is
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewScopeKind {
    Task,
    Spec,
    Implementation,
    Session,
}

impl ReviewScopeKind {
    /// Convert to string representation for serialization
    pub fn as_str(&self) -> &str {
        match self {
            ReviewScopeKind::Task => "task",
            ReviewScopeKind::Spec => "spec",
            ReviewScopeKind::Implementation => "implementation",
            ReviewScopeKind::Session => "session",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "task" => Ok(ReviewScopeKind::Task),
            "spec" => Ok(ReviewScopeKind::Spec),
            "implementation" => Ok(ReviewScopeKind::Implementation),
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
            ReviewScopeKind::Spec => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Spec ({})", filename)
            }
            ReviewScopeKind::Implementation => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Implementation ({})", filename)
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

        // scope.id is required for non-Session scopes (Task, Spec, Implementation)
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

/// Review subcommands (for list and show only)
#[derive(Subcommand)]
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
}

/// Arguments for the review command (top-level create args)
#[derive(clap::Args)]
pub struct ReviewArgs {
    /// Task ID to review (reviews all closed tasks in session if not specified)
    pub task_id: Option<String>,

    /// Run review asynchronously (return immediately)
    #[arg(long = "async")]
    pub run_async: bool,

    /// Start review and return control to calling agent
    #[arg(long)]
    pub start: bool,

    /// Task template to use (default: aiki/review)
    #[arg(long)]
    pub template: Option<String>,

    /// Agent for review assignment (default: opposite of task worker)
    #[arg(long)]
    pub agent: Option<String>,

    /// Subcommand (list or show)
    #[command(subcommand)]
    pub subcommand: Option<ReviewSubcommands>,
}

/// Run the review command
pub fn run(args: ReviewArgs) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    // If a subcommand is provided, dispatch to it
    if let Some(subcommand) = args.subcommand {
        return match subcommand {
            ReviewSubcommands::List { all } => list_reviews(&cwd, all),
            ReviewSubcommands::Show { task_id } => show_review(&cwd, &task_id),
        };
    }

    // Otherwise, run the create/review flow with top-level args
    run_review(
        &cwd,
        args.task_id,
        args.run_async,
        args.start,
        args.template,
        args.agent,
    )
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
    /// The review scope (typed, replaces loose scope_name/scope_id)
    pub scope: ReviewScope,
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
    let tasks = materialize_graph(&events).tasks;

    // Get session info (needed for session scope)
    let session = find_active_session(cwd);

    // Determine review scope and worker (for reviewer assignment)
    let (scope, worker) = match params.task_id {
        Some(ref id) => {
            // Review specific task - worker is task's assignee
            let task = find_task(&tasks, id)?;
            let worker = task.assignee.as_deref().map(|s| s.to_string());
            let scope = ReviewScope {
                kind: ReviewScopeKind::Task,
                id: task.id.clone(),
                task_ids: vec![],
            };
            (scope, worker)
        }
        None => {
            // Review all closed tasks in current session - worker is session's agent
            let (session_id, session_agent) = match &session {
                Some(s) => (
                    Some(s.session_id.clone()),
                    Some(s.agent_type.as_str().to_string()),
                ),
                None => (None, None),
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

            let task_ids: Vec<String> = closed_tasks.iter().map(|t| t.id.clone()).collect();
            let scope = ReviewScope {
                kind: ReviewScopeKind::Session,
                id: "session".to_string(),
                task_ids,
            };
            (scope, session_agent)
        }
    };

    // Determine assignee for review task
    let assignee = params
        .agent_override
        .or_else(|| Some(determine_reviewer(worker.as_deref())));

    // Create review task with subtasks from template
    let template = params.template.as_deref().unwrap_or("aiki/review");
    let scope_data = scope.to_data();

    // Build sources for lineage (not routing)
    let sources = match scope.kind {
        ReviewScopeKind::Task => vec![format!("task:{}", scope.id)],
        _ => vec![],
    };

    let review_id = create_review_task_from_template(
        cwd,
        &scope_data,
        &sources,
        &assignee,
        template,
    )?;

    Ok(CreateReviewResult {
        review_task_id: review_id,
        scope,
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
    let result = match create_review(
        cwd,
        CreateReviewParams {
            task_id,
            agent_override,
            template: template_name,
        },
    ) {
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
    let graph = materialize_graph(&events);
    let tasks = &graph.tasks;
    let scope_set = get_current_scope_set(&graph);
    let in_progress: Vec<&Task> = get_in_progress(tasks).into_iter().collect();
    let ready = get_ready_queue_for_scope_set(&graph, &scope_set);

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
    let content = "## Approved\nNothing to review - no closed tasks in session.\n";
    let md = MdBuilder::new("review").build(content, &[], &[]);
    eprintln!("{}", md);
    Ok(())
}

/// Output review started message (for --start mode)
fn output_review_started(review_id: &str, in_progress: &[&Task], ready: &[&Task]) -> Result<()> {
    let content = format!(
        "## Review Started\n- **Task:** {}\n- Review task started. You are now reviewing.\n",
        review_id
    );
    let md = MdBuilder::new("review").build(&content, in_progress, ready);
    eprintln!("{}", md);

    // Output task ID to stdout if piped
    if !std::io::stdout().is_terminal() {
        println!("{}", review_id);
    }

    Ok(())
}

/// Output review async message (for --async mode)
fn output_review_async(review_id: &str) -> Result<()> {
    let content = format!(
        "## Review Started\n- **Task:** {}\n- Review started in background.\n",
        review_id
    );
    let md = MdBuilder::new("review").build(&content, &[], &[]);
    eprintln!("{}", md);
    Ok(())
}

/// Output review completed message (for blocking mode)
fn output_review_completed(review_id: &str) -> Result<()> {
    let content = format!(
        "## Review Completed\n- **Task:** {}\n- Review completed.\n",
        review_id
    );
    let md = MdBuilder::new("review").build(&content, &[], &[]);
    eprintln!("{}", md);
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
        let content = if all {
            "No review tasks found.\n"
        } else {
            "No open review tasks. Use --all to see closed reviews.\n"
        };
        let md = MdBuilder::new("review-list").build(content, &[], &[]);
        eprintln!("{}", md);
        return Ok(());
    }

    // Format review list
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

        let issue_count = review.comments.len();

        content.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            &review.id,
            status_str,
            outcome_str,
            issue_count,
            &review.name
        ));
    }

    let md = MdBuilder::new("review-list").build(&content, &[], &[]);
    eprintln!("{}", md);

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

    let status_str = match task.status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Closed => "closed",
    };

    let outcome_str = task
        .closed_outcome
        .as_ref()
        .map(|o| format!("{:?}", o).to_lowercase())
        .unwrap_or_default();

    let assignee_str = task
        .assignee
        .as_ref()
        .map(|a| format!("- **Assignee:** {}\n", a))
        .unwrap_or_default();

    // Build content
    let mut content = format!(
        "## Review: {}\n- **ID:** {}\n- **Status:** {}\n",
        &task.name, &task.id, status_str
    );
    if !outcome_str.is_empty() {
        content.push_str(&format!("- **Outcome:** {}\n", outcome_str));
    }
    content.push_str(&assignee_str);

    // Add sources if any
    if !task.sources.is_empty() {
        content.push_str("\n### Sources\n");
        for source in &task.sources {
            content.push_str(&format!("- {}\n", source));
        }
    }

    // Add comments/issues
    if !task.comments.is_empty() {
        content.push_str("\n### Issues\n");
        for (idx, comment) in task.comments.iter().enumerate() {
            content.push_str(&format!("{}. {}\n", idx + 1, &comment.text));
        }
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
        content.push_str("\n### Followups\n");
        for followup in &followups {
            let fu_status = match followup.status {
                TaskStatus::Open => "open",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Stopped => "stopped",
                TaskStatus::Closed => "closed",
            };
            content.push_str(&format!(
                "- **{}** [{}] {}\n",
                &followup.id, fu_status, &followup.name
            ));
        }
    }

    let md = MdBuilder::new("review-show").build(&content, &[], &[]);
    eprintln!("{}", md);

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
        assert_eq!(ReviewScopeKind::Spec.as_str(), "spec");
        assert_eq!(ReviewScopeKind::Implementation.as_str(), "implementation");
        assert_eq!(ReviewScopeKind::Session.as_str(), "session");
    }

    #[test]
    fn test_scope_kind_from_str() {
        assert_eq!(ReviewScopeKind::from_str("task").unwrap(), ReviewScopeKind::Task);
        assert_eq!(ReviewScopeKind::from_str("spec").unwrap(), ReviewScopeKind::Spec);
        assert_eq!(ReviewScopeKind::from_str("implementation").unwrap(), ReviewScopeKind::Implementation);
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
            kind: ReviewScopeKind::Spec,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Spec (feature.md)");
    }

    #[test]
    fn test_scope_name_implementation() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Implementation,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(scope.name(), "Implementation (feature.md)");
    }

    #[test]
    fn test_scope_name_session() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "session".to_string(),
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
            id: "session".to_string(),
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
            id: "session".to_string(),
            task_ids: vec!["t1".to_string(), "t2".to_string()],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Session);
        assert_eq!(restored.id, "session");
        assert_eq!(restored.task_ids, vec!["t1", "t2"]);
    }

    #[test]
    fn test_scope_roundtrip_spec() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Spec,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Spec);
        assert_eq!(restored.id, "ops/now/feature.md");
    }

    #[test]
    fn test_scope_roundtrip_implementation() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Implementation,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Implementation);
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
}

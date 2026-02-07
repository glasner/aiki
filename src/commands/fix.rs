//! Fix command for creating followup tasks from review comments
//!
//! This module provides the `aiki fix` command which:
//! - Reads a task ID (from argument or stdin for piping)
//! - Checks the task for comments
//! - If no comments: succeeds with "approved" message (review passed)
//! - If comments found: creates followup task (agent creates subtasks from comments)
//! - Runs the followup task (default: completion, --async: async, --start: hand off)

use std::env;
use std::io::{self, BufRead, IsTerminal};
use std::path::Path;

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::runner::{task_run, task_run_async, TaskRunOptions};
use crate::tasks::xml::{escape_xml, XmlBuilder};
use crate::tasks::{
    find_task, get_current_scope_set, get_in_progress,
    get_ready_queue_for_scope_set, materialize_tasks,
    materialize_tasks_with_ids, read_events, read_events_with_ids, reassign_task,
    reopen_if_closed, start_task_core, Task, TaskComment,
};

/// Run the fix command
///
/// Creates followup tasks from review comments and runs them.
pub fn run(
    task_id: Option<String>,
    run_async: bool,
    start: bool,
    template_name: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    let cwd = env::current_dir().map_err(|_| {
        AikiError::InvalidArgument("Failed to get current directory".to_string())
    })?;

    // Get task ID from argument or stdin
    let task_id = match task_id {
        Some(id) => extract_task_id(&id),
        None => read_task_id_from_stdin()?,
    };

    run_fix(&cwd, &task_id, run_async, start, template_name, agent)
}

/// Extract task ID from input, handling XML output format
fn extract_task_id(input: &str) -> String {
    let trimmed = input.trim();

    // Try to extract from XML task_id attribute
    if let Some(start) = trimmed.find("task_id=\"") {
        let after_quote = &trimmed[start + 9..];
        if let Some(end) = after_quote.find('"') {
            return after_quote[..end].to_string();
        }
    }

    trimmed.to_string()
}

/// Read task ID from stdin
fn read_task_id_from_stdin() -> Result<String> {
    let stdin = io::stdin();
    let mut input = String::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| {
            AikiError::InvalidArgument(format!("Failed to read from stdin: {}", e))
        })?;
        input.push_str(&line);
        input.push('\n');
    }

    if input.trim().is_empty() {
        return Err(AikiError::InvalidArgument(
            "No task ID provided. Pass as argument or pipe from another command.".to_string(),
        ));
    }

    Ok(extract_task_id(&input))
}

/// What was reviewed — determines fix behavior
enum ReviewTarget {
    /// Review targeted a task (source: task:<id>)
    Task(String),
    /// Review targeted a file (future: ops/next/review-and-fix-files.md)
    #[allow(dead_code)]
    File(String),
    /// Could not determine review target
    Unknown,
}

/// Determine what was reviewed by inspecting the review task's sources.
///
/// Priority: task > file (only task is supported currently)
fn get_review_target(review_task: &Task) -> ReviewTarget {
    // Priority: task > file
    for source in &review_task.sources {
        if let Some(task_id) = source.strip_prefix("task:") {
            return ReviewTarget::Task(task_id.to_string());
        }
    }
    for source in &review_task.sources {
        if let Some(path) = source.strip_prefix("file:") {
            return ReviewTarget::File(path.to_string());
        }
    }
    ReviewTarget::Unknown
}

/// Core fix implementation
fn run_fix(
    cwd: &Path,
    task_id: &str,
    run_async: bool,
    start: bool,
    template_name: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    // Parse agent if provided
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Load tasks with change IDs (needed for comment IDs)
    let events_with_ids = read_events_with_ids(cwd)?;
    let tasks = materialize_tasks_with_ids(&events_with_ids);

    // Find the review task (the task we're creating followups for)
    let review_task = find_task(&tasks, task_id)
        .ok_or_else(|| AikiError::TaskNotFound(task_id.to_string()))?;

    // Validate that the input task is actually a review task
    // Check task_type first, then fall back to template name for backward compatibility
    // (older review tasks may not have task_type set)
    if !is_review_task(review_task) {
        return Err(AikiError::InvalidArgument(format!(
            "Task {} is not a review task.",
            task_id
        )));
    }

    // Get all comments from the review task
    // Note: closing a review requires a comment, so 1 comment = just the closing comment (no issues)
    // More than 1 comment means there are issues to fix
    let comments: Vec<TaskComment> = review_task.comments.clone();

    // If only the closing comment (or no comments), output "approved" message and succeed
    if comments.len() <= 1 {
        output_approved(task_id)?;
        return Ok(());
    }

    // Determine what was reviewed and branch on target type
    let review_target = get_review_target(review_task);

    let followup_id = match review_target {
        ReviewTarget::Task(original_task_id) => {
            // Fix targets a task — add fix subtask to the original task
            let original_task = find_task(&tasks, &original_task_id)
                .ok_or_else(|| AikiError::TaskNotFound(original_task_id.clone()))?;

            // Determine assignee for followup task
            let assignee = determine_followup_assignee(agent_type, Some(original_task));

            // Create fix subtask on the original task
            let template = template_name.as_deref().unwrap_or("aiki/fix");
            create_fix_subtask_on_original(
                cwd,
                review_task,
                original_task,
                &assignee,
                template,
            )?
        }
        ReviewTarget::File(_) => {
            return Err(AikiError::InvalidArgument(
                "Fixing file-targeted reviews is not yet supported. Only task-targeted reviews can be fixed.".to_string(),
            ));
        }
        ReviewTarget::Unknown => {
            return Err(AikiError::InvalidArgument(
                "Could not determine what was reviewed. The review task has no task: or file: source.".to_string(),
            ));
        }
    };

    // Re-read tasks to include newly created followup task
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);
    let scope_set = get_current_scope_set(&tasks);
    let in_progress: Vec<&Task> = get_in_progress(&tasks).into_iter().collect();
    let ready = get_ready_queue_for_scope_set(&tasks, &scope_set);

    // Handle execution mode
    if start {
        // Reassign task to current agent (caller takes over)
        if let Some(session) = find_active_session(cwd) {
            reassign_task(cwd, &followup_id, session.agent_type.as_str())?;
        }
        // Start task using core logic (validates, auto-stops, emits events)
        start_task_core(cwd, &[followup_id.clone()])?;
        output_followup_started(&followup_id, review_task, &comments, &in_progress, &ready)?;
    } else if run_async {
        // Run async and return immediately
        let options = TaskRunOptions::new();
        task_run_async(cwd, &followup_id, options)?;
        output_followup_async(&followup_id, &comments)?;
        // Output task ID to stdout if piped
        if !std::io::stdout().is_terminal() {
            println!("{}", followup_id);
        }
    } else {
        // Run to completion (default)
        let options = TaskRunOptions::new();
        task_run(cwd, &followup_id, options)?;
        output_followup_completed(&followup_id, &comments)?;
        // Output task ID to stdout if piped
        if !std::io::stdout().is_terminal() {
            println!("{}", followup_id);
        }
    }

    Ok(())
}

/// Output approved message when no issues found
fn output_approved(task_id: &str) -> Result<()> {
    let content = format!(
        "  <approved task_id=\"{}\">\n    Review approved - no issues found.\n  </approved>",
        escape_xml(task_id)
    );
    let xml = XmlBuilder::new("fix").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Output followup started message
fn output_followup_started(
    followup_id: &str,
    _source_task: &Task,
    comments: &[TaskComment],
    in_progress: &[&Task],
    ready: &[&Task],
) -> Result<()> {
    let issue_count = comments.len().saturating_sub(1);
    let mut content = String::new();
    content.push_str(&format!(
        "  <followup task_id=\"{}\" issues_found=\"{}\" status=\"started\">\n",
        escape_xml(followup_id),
        issue_count
    ));
    content.push_str(&format!(
        "    Created fix followup subtask under original task ({} issue(s)).\n\n",
        issue_count
    ));

    for (i, comment) in comments.iter().enumerate() {
        // Truncate long comments for display
        let display_text = if comment.text.len() > 60 {
            format!("{}...", &comment.text[..57])
        } else {
            comment.text.clone()
        };
        content.push_str(&format!(
            "    {}. {}\n",
            i + 1,
            escape_xml(&display_text)
        ));
    }

    content.push_str("  </followup>");

    let xml = XmlBuilder::new("fix").build(&content, in_progress, ready);
    eprintln!("{}", xml);

    // Output task ID to stdout if piped
    if !std::io::stdout().is_terminal() {
        println!("{}", followup_id);
    }

    Ok(())
}

/// Output followup async message (for --async mode)
fn output_followup_async(followup_id: &str, comments: &[TaskComment]) -> Result<()> {
    let issue_count = comments.len().saturating_sub(1);
    let content = format!(
        "  <started task_id=\"{}\" issues_found=\"{}\">\n    Fix followup subtask started in background.\n  </started>",
        escape_xml(followup_id),
        issue_count
    );
    let xml = XmlBuilder::new("fix").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Output followup completed message (for blocking mode)
fn output_followup_completed(followup_id: &str, comments: &[TaskComment]) -> Result<()> {
    let issue_count = comments.len().saturating_sub(1);
    let content = format!(
        "  <completed task_id=\"{}\" issues_found=\"{}\">\n    Fix followup subtask completed.\n  </completed>",
        escape_xml(followup_id),
        issue_count
    );
    let xml = XmlBuilder::new("fix").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Check if a task is a review task.
///
/// A task is considered a review task if:
/// 1. Its task_type is explicitly "review", OR
/// 2. It was created from a review template (template starts with "aiki/review")
///
/// The fallback to template name provides backward compatibility with review tasks
/// created before the template set the `type` field in frontmatter.
fn is_review_task(task: &Task) -> bool {
    if task.task_type.as_deref() == Some("review") {
        return true;
    }
    if let Some(ref template) = task.template {
        if template.starts_with("aiki/review") {
            return true;
        }
    }
    false
}

/// Determine assignee for followup task.
///
/// The followup should be assigned to whoever did the original work (the reviewed task's assignee),
/// not the opposite of the reviewer. The person who wrote the code should fix issues in their code.
fn determine_followup_assignee(agent_override: Option<AgentType>, reviewed_task: Option<&Task>) -> Option<String> {
    if let Some(agent) = agent_override {
        return Some(agent.as_str().to_string());
    }

    // Assign to whoever did the original work
    if let Some(task) = reviewed_task {
        if let Some(ref assignee) = task.assignee {
            return Some(assignee.clone());
        }
    }

    // Fallback to claude-code if we can't determine the original worker
    Some("claude-code".to_string())
}

/// Create a fix subtask on the original task (not a standalone task).
///
/// The fix subtask is added as a child of the original task (e.g., X.1),
/// with the review task as its source. The agent will create nested
/// subtasks (X.1.1, X.1.2, etc.) for each issue found in the review.
///
/// If the original task is closed, this function reopens it before creating the subtask.
fn create_fix_subtask_on_original(
    cwd: &Path,
    review_task: &Task,
    original_task: &Task,
    assignee: &Option<String>,
    template_name: &str,
) -> Result<String> {
    use super::task::{create_from_template, TemplateTaskParams};

    // Reopen the original task if closed before adding subtask
    let events = read_events(cwd)?;
    let current_tasks = materialize_tasks(&events);
    reopen_if_closed(cwd, &original_task.id, &current_tasks, "Subtasks added")?;

    let mut source_data = std::collections::HashMap::new();
    source_data.insert("name".to_string(), review_task.name.clone());
    source_data.insert("id".to_string(), review_task.id.clone());

    let params = TemplateTaskParams {
        template_name: template_name.to_string(),
        sources: vec![format!("task:{}", review_task.id)],
        assignee: assignee.clone(),
        priority: Some(original_task.priority),
        parent_id: Some(original_task.id.clone()),
        parent_name: Some(original_task.name.clone()),
        source_data,
        ..Default::default()
    };

    create_from_template(cwd, params)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{TaskPriority, TaskStatus};
    use std::collections::HashMap;

    #[test]
    fn test_extract_task_id_plain() {
        assert_eq!(extract_task_id("xqrmnpst"), "xqrmnpst");
        assert_eq!(extract_task_id("  xqrmnpst  "), "xqrmnpst");
    }

    #[test]
    fn test_extract_task_id_xml() {
        let xml = r#"<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" comments="2"/>
</aiki_review>"#;
        assert_eq!(extract_task_id(xml), "xqrmnpst");
    }

    #[test]
    fn test_determine_followup_assignee_override() {
        let task = Task {
            id: "test".to_string(),
            name: "Test".to_string(),
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: Some("codex".to_string()),
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            comments: Vec::new(),
        };

        // Override should take precedence
        let result = determine_followup_assignee(Some(AgentType::Codex), Some(&task));
        assert_eq!(result, Some("codex".to_string()));
    }

    #[test]
    fn test_determine_followup_assignee_from_reviewed_task() {
        // The reviewed task's assignee is who should fix the issues
        let reviewed_task = Task {
            id: "reviewed".to_string(),
            name: "Original Work".to_string(),
            task_type: None,
            status: TaskStatus::Closed,
            priority: TaskPriority::P2,
            assignee: Some("claude-code".to_string()), // claude-code did the work
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            comments: Vec::new(),
        };

        // Should return the reviewed task's assignee (claude-code fixes their own work)
        let result = determine_followup_assignee(None, Some(&reviewed_task));
        assert_eq!(result, Some("claude-code".to_string()));
    }

    #[test]
    fn test_determine_followup_assignee_no_reviewed_task() {
        // If we can't find the reviewed task, fall back to claude-code
        let result = determine_followup_assignee(None, None);
        assert_eq!(result, Some("claude-code".to_string()));
    }

    fn make_task_with_sources(id: &str, sources: Vec<&str>) -> Task {
        Task {
            id: id.to_string(),
            name: format!("Task {}", id),
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: None,
            sources: sources.into_iter().map(|s| s.to_string()).collect(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            comments: Vec::new(),
        }
    }

    #[test]
    fn test_get_review_target_task_source() {
        let review = make_task_with_sources("review1", vec!["task:original123"]);
        match get_review_target(&review) {
            ReviewTarget::Task(id) => assert_eq!(id, "original123"),
            _ => panic!("Expected ReviewTarget::Task"),
        }
    }

    #[test]
    fn test_get_review_target_task_priority_over_file() {
        // task: should take priority over file:
        let review = make_task_with_sources("review2", vec!["file:src/main.rs", "task:abc"]);
        match get_review_target(&review) {
            ReviewTarget::Task(id) => assert_eq!(id, "abc"),
            _ => panic!("Expected ReviewTarget::Task"),
        }
    }

    #[test]
    fn test_get_review_target_file_source() {
        let review = make_task_with_sources("review3", vec!["file:src/lib.rs"]);
        match get_review_target(&review) {
            ReviewTarget::File(path) => assert_eq!(path, "src/lib.rs"),
            _ => panic!("Expected ReviewTarget::File"),
        }
    }

    #[test]
    fn test_get_review_target_unknown() {
        let review = make_task_with_sources("review4", vec!["prompt:xyz"]);
        assert!(matches!(get_review_target(&review), ReviewTarget::Unknown));
    }

    #[test]
    fn test_get_review_target_no_sources() {
        let review = make_task_with_sources("review5", vec![]);
        assert!(matches!(get_review_target(&review), ReviewTarget::Unknown));
    }

    #[test]
    fn test_is_review_task_by_type() {
        let mut task = make_task_with_sources("t1", vec![]);
        task.task_type = Some("review".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_by_template() {
        let mut task = make_task_with_sources("t2", vec![]);
        task.template = Some("aiki/review@1.0.0".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_neither() {
        let task = make_task_with_sources("t3", vec![]);
        assert!(!is_review_task(&task));
    }
}

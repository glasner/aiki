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
use crate::tasks::templates::{
    create_tasks_from_template, find_templates_dir, load_template, VariableContext,
};
use crate::tasks::xml::{escape_xml, XmlBuilder};
use crate::tasks::{
    find_task, generate_task_id, get_current_scope_set, get_in_progress,
    get_ready_queue_for_scope_set, materialize_tasks, materialize_tasks_with_ids,
    read_events, read_events_with_ids, reassign_task, start_task_core,
    write_event, Task, TaskComment, TaskEvent, TaskPriority,
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

    // Get all comments from the review task
    // Note: closing a review requires a comment, so 1 comment = just the closing comment (no issues)
    // More than 1 comment means there are issues to fix
    let comments: Vec<TaskComment> = review_task.comments.clone();

    // If only the closing comment (or no comments), output "approved" message and succeed
    if comments.len() <= 1 {
        output_approved(task_id)?;
        return Ok(());
    }

    // Find the originally reviewed task to determine followup assignee
    // The review task's source field contains "task:<id>" of the task that was reviewed
    let reviewed_task = find_reviewed_task(&tasks, review_task);

    // Determine assignee for followup task
    let assignee = determine_followup_assignee(agent_type, reviewed_task);

    // Create followup task from template (agent will create subtasks from comments)
    let template = template_name.as_deref().unwrap_or("aiki/fix");
    let followup_id = create_followup_task_from_template(cwd, review_task, &assignee, template)?;

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
    let mut content = String::new();
    content.push_str(&format!(
        "  <followup task_id=\"{}\" issues_found=\"{}\" status=\"started\">\n",
        escape_xml(followup_id),
        comments.len()
    ));
    content.push_str(&format!(
        "    Created and started followup task with {} subtask(s).\n\n",
        comments.len()
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
    let content = format!(
        "  <started task_id=\"{}\" issues_found=\"{}\">\n    Followup task started in background.\n  </started>",
        escape_xml(followup_id),
        comments.len()
    );
    let xml = XmlBuilder::new("fix").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Output followup completed message (for blocking mode)
fn output_followup_completed(followup_id: &str, comments: &[TaskComment]) -> Result<()> {
    let content = format!(
        "  <completed task_id=\"{}\" issues_found=\"{}\">\n    Followup task completed.\n  </completed>",
        escape_xml(followup_id),
        comments.len()
    );
    let xml = XmlBuilder::new("fix").build(&content, &[], &[]);
    eprintln!("{}", xml);
    Ok(())
}

/// Find the task that was originally reviewed by looking at the review task's source field.
///
/// The review task has `source: task:<id>` pointing to the task that was reviewed.
/// We need to find that task to determine who should fix the issues (the original worker).
fn find_reviewed_task<'a>(
    tasks: &'a std::collections::HashMap<String, Task>,
    review_task: &Task,
) -> Option<&'a Task> {
    // Look for "task:<id>" in review task's sources
    for source in &review_task.sources {
        if let Some(task_id) = source.strip_prefix("task:") {
            if let Some(task) = find_task(tasks, task_id) {
                return Some(task);
            }
        }
    }
    None
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

/// Create followup task from template (agent creates subtasks)
fn create_followup_task_from_template(
    cwd: &Path,
    source_task: &Task,
    assignee: &Option<String>,
    template_name: &str,
) -> Result<String> {
    let timestamp = chrono::Utc::now();
    let working_copy = get_working_copy_change_id(cwd);

    // Load the template
    let templates_dir = find_templates_dir(cwd)?;
    let template = load_template(template_name, &templates_dir)?;

    // Set up variable context for template substitution
    let mut variables = VariableContext::new();

    // Set source variables (template uses {source.name}, {source.id})
    variables.set_source(&format!("task:{}", source_task.id));
    variables.set_source_data("name", &source_task.name);
    variables.set_source_data("id", &source_task.id);

    // Create task from template (no subtasks - agent will create them)
    let (parent_def, _subtask_defs) =
        create_tasks_from_template(&template, &variables, None)?;

    // Generate parent task ID from the resolved name
    let parent_id = generate_task_id(&parent_def.name);

    // Build sources list
    let mut sources = parent_def.sources.clone();
    if !sources.iter().any(|s| s.starts_with("task:")) {
        sources.push(format!("task:{}", source_task.id));
    }

    // Create parent task event
    let parent_event = TaskEvent::Created {
        task_id: parent_id.clone(),
        name: parent_def.name.clone(),
        task_type: parent_def.task_type.or(template.defaults.task_type.clone()),
        priority: source_task.priority, // Inherit from source task
        assignee: assignee.clone(),
        sources,
        template: Some(template.template_id()),
        working_copy: working_copy.clone(),
        instructions: Some(parent_def.instructions.clone()),
        data: convert_data(&parent_def.data),
        timestamp,
    };
    write_event(cwd, &parent_event)?;

    Ok(parent_id)
}

/// Convert serde_json::Value HashMap to String HashMap for TaskEvent
fn convert_data(
    data: &std::collections::HashMap<String, serde_json::Value>,
) -> std::collections::HashMap<String, String> {
    data.iter()
        .map(|(k, v)| {
            let value_str = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), value_str)
        })
        .collect()
}

/// Returns the change_id of the current working copy (`@` in jj terms).
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
    use crate::tasks::TaskStatus;
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
}

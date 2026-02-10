//! Plan command for creating implementation plans from spec files
//!
//! This module provides the `aiki plan` command which:
//! - Creates a planning task from a spec file
//! - The planning agent reads the spec and creates a plan task with subtasks
//! - Supports resuming existing plans or starting fresh
//! - Shows plan status via the `show` subcommand

use std::env;
use std::io::IsTerminal;
use std::path::Path;

use clap::Subcommand;

use crate::agents::AgentType;
use crate::config::get_aiki_binary_path;
use crate::error::{AikiError, Result};
use crate::tasks::id::is_task_id;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::templates::{
    convert_data, create_tasks_from_template, find_templates_dir, get_working_copy_change_id,
    load_template, parse_priority, VariableContext,
};
use crate::tasks::md::MdBuilder;
use crate::tasks::{
    find_task, generate_task_id, get_subtasks, is_task_id_prefix, materialize_tasks, read_events,
    write_event, Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus,
};

/// Plan subcommands
#[derive(Subcommand)]
pub enum PlanSubcommands {
    /// Show plan status and subtasks
    Show {
        /// Spec path or plan task ID (32 lowercase letters)
        arg: String,
    },
}

/// Arguments for the plan command
#[derive(clap::Args)]
pub struct PlanArgs {
    /// Path to spec file (e.g., ops/now/my-feature.md)
    pub spec_path: Option<String>,

    /// Ignore existing plan and create a new one from scratch
    #[arg(long)]
    pub restart: bool,

    /// Planning template to use (default: aiki/plan)
    #[arg(long)]
    pub template: Option<String>,

    /// Agent for planning (default: claude-code)
    #[arg(long)]
    pub agent: Option<String>,

    /// Subcommand (show)
    #[command(subcommand)]
    pub subcommand: Option<PlanSubcommands>,
}

/// Run the plan command
pub fn run(args: PlanArgs) -> Result<()> {
    let cwd = env::current_dir().map_err(|_| {
        AikiError::InvalidArgument("Failed to get current directory".to_string())
    })?;

    // If a subcommand is provided, dispatch to it
    if let Some(subcommand) = args.subcommand {
        return match subcommand {
            PlanSubcommands::Show { arg } => run_show(&cwd, &arg),
        };
    }

    // Otherwise, run the plan creation flow
    let spec_path = args.spec_path.ok_or_else(|| {
        AikiError::InvalidArgument(
            "No spec path provided. Usage: aiki plan <spec-path>".to_string(),
        )
    })?;

    run_plan(&cwd, &spec_path, args.restart, args.template, args.agent)
}

/// User choice when an incomplete plan exists
enum PlanChoice {
    Resume,
    StartFresh,
}

/// Core plan creation implementation
fn run_plan(
    cwd: &Path,
    spec_path: &str,
    restart: bool,
    template_name: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    // Validate spec file exists and is .md
    validate_spec_path(cwd, spec_path)?;

    // Parse agent if provided
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Load current tasks to check for existing plans
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Check for existing plan with data.spec matching spec_path
    let existing_plan = find_plan_for_spec(&tasks, spec_path);

    match existing_plan {
        Some(plan) if plan.status != TaskStatus::Closed => {
            // Incomplete plan exists
            if restart {
                // Undo completed subtask changes, then close existing plan
                undo_completed_subtasks(cwd, &plan.id)?;
                close_plan(cwd, &plan.id)?;
            } else {
                // Show interactive prompt or error
                let subtasks = get_subtasks(&tasks, &plan.id);
                let choice = prompt_existing_plan(plan, &subtasks)?;
                match choice {
                    PlanChoice::Resume => {
                        // Return existing plan ID
                        output_plan_resumed(&plan.id, &subtasks)?;
                        // Output to stdout if piped
                        if !std::io::stdout().is_terminal() {
                            println!("<aiki_plan plan_id=\"{}\"/>", plan.id);
                        }
                        return Ok(());
                    }
                    PlanChoice::StartFresh => {
                        // Undo completed subtask changes, then close existing
                        undo_completed_subtasks(cwd, &plan.id)?;
                        close_plan(cwd, &plan.id)?;
                    }
                }
            }
        }
        Some(_plan) => {
            // Plan exists but is closed (completed) - create a new one for a new implementation cycle
        }
        None => {
            // No existing plan - create a new one
        }
    }

    // Create the planning task from template
    let template = template_name.as_deref().unwrap_or("aiki/plan");
    let assignee = agent_type
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let planning_task_id = create_planning_task(cwd, spec_path, template, assignee)?;

    // Run the planning task to completion
    // The planning agent will read the spec and create the plan task with subtasks
    let options = if let Some(agent) = agent_type {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };
    task_run(cwd, &planning_task_id, options)?;

    // After the planning agent finishes, find the plan task it created
    // Re-read tasks since the planning agent created new ones
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Find the plan task created by the planning agent
    // It should have data.spec=<spec-path> and source=task:<planning_task_id>
    let plan_task = find_created_plan(&tasks, spec_path, &planning_task_id);

    match plan_task {
        Some(plan) => {
            let subtasks = get_subtasks(&tasks, &plan.id);
            output_plan_created(&plan.id, &subtasks)?;

            // Output machine-readable XML to stdout if piped
            if !std::io::stdout().is_terminal() {
                println!("<aiki_plan plan_id=\"{}\"/>", plan.id);
            }
        }
        None => {
            // Planning agent didn't create a plan task - report error
            eprintln!(
                "Warning: Planning task completed but no plan task found with data.spec={}",
                spec_path
            );
            // Still output the planning task ID as fallback
            if !std::io::stdout().is_terminal() {
                println!("<aiki_plan plan_id=\"{}\"/>", planning_task_id);
            }
        }
    }

    Ok(())
}

/// Show plan status and subtasks
fn run_show(cwd: &Path, arg: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_tasks(&events);

    // Determine if arg is a task ID or spec path
    let plan = if is_task_id(arg) || is_task_id_prefix(arg) {
        // Task ID or prefix lookup
        find_task(&tasks, arg)?
    } else {
        // Spec path lookup - find most recent plan with data.spec=<path>
        find_plan_for_spec(&tasks, arg).ok_or_else(|| {
            AikiError::InvalidArgument(format!("No plan found for spec: {}", arg))
        })?
    };

    let subtasks = get_subtasks(&tasks, &plan.id);
    output_plan_show(plan, &subtasks)?;

    Ok(())
}

/// Validate that the spec path is a .md file and exists
fn validate_spec_path(cwd: &Path, spec_path: &str) -> Result<()> {
    if !spec_path.ends_with(".md") {
        return Err(AikiError::InvalidArgument(
            "Spec file must be markdown (.md)".to_string(),
        ));
    }

    let full_path = if spec_path.starts_with('/') {
        std::path::PathBuf::from(spec_path)
    } else {
        cwd.join(spec_path)
    };

    if !full_path.exists() {
        return Err(AikiError::InvalidArgument(format!(
            "Spec file not found: {}",
            spec_path
        )));
    }

    if !full_path.is_file() {
        return Err(AikiError::InvalidArgument(format!(
            "Not a file: {}",
            spec_path
        )));
    }

    Ok(())
}

/// Find the most recent plan task for a given spec path.
///
/// A plan task is identified by:
/// - Having `data.spec` matching the spec path
/// - Having a `source` containing `task:` (created by a planning task)
/// - NOT being a planning task itself (type != "plan" which is the planning task type)
///
/// If no plan created by a planning task is found, falls back to any task with
/// `data.spec` matching the spec path that has subtasks (a plan without a planning task source).
fn find_plan_for_spec<'a>(
    tasks: &'a std::collections::HashMap<String, Task>,
    spec_path: &str,
) -> Option<&'a Task> {
    // First, look for plan tasks created by a planning task (have source: task:...)
    let plan_from_planning = tasks
        .values()
        .filter(|t| {
            t.data.get("spec").map(|s| s.as_str()) == Some(spec_path)
                && t.task_type.as_deref() != Some("plan") // Exclude the planning task itself
                && t.sources.iter().any(|s| s.starts_with("task:"))
        })
        .max_by_key(|t| t.created_at);

    if plan_from_planning.is_some() {
        return plan_from_planning;
    }

    // Fallback: any task with data.spec matching, excluding the planning task type
    tasks
        .values()
        .filter(|t| {
            t.data.get("spec").map(|s| s.as_str()) == Some(spec_path)
                && t.task_type.as_deref() != Some("plan")
        })
        .max_by_key(|t| t.created_at)
}

/// Find the plan task created by a specific planning task.
///
/// After the planning agent runs, it creates a plan task with:
/// - `data.spec=<spec-path>`
/// - `source: task:<planning_task_id>`
fn find_created_plan<'a>(
    tasks: &'a std::collections::HashMap<String, Task>,
    spec_path: &str,
    planning_task_id: &str,
) -> Option<&'a Task> {
    let source_prefix = format!("task:{}", planning_task_id);
    tasks
        .values()
        .filter(|t| {
            t.data.get("spec").map(|s| s.as_str()) == Some(spec_path)
                && t.sources.iter().any(|s| s == &source_prefix)
        })
        .max_by_key(|t| t.created_at)
}

/// Undo file changes made by completed subtasks of a plan.
///
/// Invokes `aiki task undo <plan-id> --completed` to revert changes before
/// closing the plan. If no completed subtasks exist, this is a no-op.
fn undo_completed_subtasks(cwd: &Path, plan_id: &str) -> Result<()> {
    let output = std::process::Command::new(get_aiki_binary_path())
        .current_dir(cwd)
        .args(["task", "undo", plan_id, "--completed"])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to run task undo: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If there are no completed subtasks, that's fine - nothing to undo
        if stderr.contains("no completed subtasks") || stderr.contains("NoCompletedSubtasks") {
            return Ok(());
        }
        return Err(AikiError::JjCommandFailed(format!(
            "task undo failed: {}",
            stderr.trim()
        )));
    }

    // Forward undo output to stderr so user sees what was reverted
    let stderr_output = String::from_utf8_lossy(&output.stderr);
    if !stderr_output.is_empty() {
        eprint!("{}", stderr_output);
    }

    Ok(())
}

/// Close an existing plan as wont_do
fn close_plan(cwd: &Path, plan_id: &str) -> Result<()> {
    let timestamp = chrono::Utc::now();

    // Add comment before closing
    let comment_event = TaskEvent::CommentAdded {
        task_ids: vec![plan_id.to_string()],
        text: "Closed by --restart".to_string(),
        data: std::collections::HashMap::new(),
        timestamp: timestamp - chrono::Duration::milliseconds(1),
    };
    write_event(cwd, &comment_event)?;

    let close_event = TaskEvent::Closed {
        task_ids: vec![plan_id.to_string()],
        outcome: TaskOutcome::WontDo,
        summary: Some("Closed by --restart".to_string()),
        timestamp,
    };
    write_event(cwd, &close_event)?;
    Ok(())
}

/// Create the planning task from template.
///
/// The planning task is an ephemeral task that runs the planning agent.
/// The agent reads the spec and creates the actual plan task with subtasks.
fn create_planning_task(
    cwd: &Path,
    spec_path: &str,
    template_name: &str,
    assignee: Option<String>,
) -> Result<String> {
    let timestamp = chrono::Utc::now();
    let working_copy = get_working_copy_change_id(cwd);

    let templates_dir = find_templates_dir(cwd)?;
    let template = load_template(template_name, &templates_dir)?;

    let mut variables = VariableContext::new();
    variables.set_data("spec", spec_path);

    let (parent_def, _subtask_defs) = create_tasks_from_template(&template, &variables, None)?;

    let task_id = generate_task_id(&parent_def.name);

    let task_type = parent_def
        .task_type
        .or_else(|| template.defaults.task_type.clone());

    let priority = parent_def
        .priority
        .as_ref()
        .and_then(|p| parse_priority(p))
        .or_else(|| {
            template
                .defaults
                .priority
                .as_ref()
                .and_then(|p| parse_priority(p))
        })
        .unwrap_or(TaskPriority::P2);

    let mut sources = parent_def.sources.clone();
    sources.push(format!("file:{}", spec_path));

    // Merge spec into the task data so planning tasks are queryable by spec
    let mut data = convert_data(&parent_def.data);
    data.insert("spec".to_string(), spec_path.to_string());

    let event = TaskEvent::Created {
        task_id: task_id.clone(),
        name: parent_def.name.clone(),
        task_type,
        priority,
        assignee: assignee
            .or_else(|| template.defaults.assignee.clone())
            .or_else(|| Some("claude-code".to_string())),
        sources,
        template: Some(template.template_id()),
        working_copy,
        instructions: Some(parent_def.instructions.clone()),
        data,
        timestamp,
    };
    write_event(cwd, &event)?;

    Ok(task_id)
}

/// Prompt user to choose between resuming or starting fresh when an incomplete plan exists.
///
/// If stdin is not a TTY (piped input), returns an error with helpful suggestions.
fn prompt_existing_plan(plan: &Task, subtasks: &[&Task]) -> Result<PlanChoice> {
    use std::io::{self, Write};

    let stdin = io::stdin();
    if !stdin.is_terminal() {
        return Err(AikiError::InvalidArgument(format!(
            "Incomplete plan exists ({}). Use --restart to start fresh, or run: aiki plan show {}",
            &plan.id[..8.min(plan.id.len())],
            plan.id
        )));
    }

    let completed = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed)
        .count();
    let total = subtasks.len();

    eprintln!("Incomplete plan exists for this spec.\n");
    eprintln!(
        "Plan: {} ({}/{} subtasks done)",
        &plan.id[..20.min(plan.id.len())],
        completed,
        total
    );
    for subtask in subtasks {
        let check = if subtask.status == TaskStatus::Closed {
            "x"
        } else {
            " "
        };
        eprintln!("  [{}] {}", check, subtask.name);
    }
    eprintln!();
    eprintln!("  1. Resume this plan");
    eprintln!("  2. Start fresh (closes existing plan)");
    eprintln!();
    eprint!("Choice [1-2]: ");
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    match input.trim() {
        "2" => Ok(PlanChoice::StartFresh),
        _ => Ok(PlanChoice::Resume),
    }
}

/// Output plan created message to stderr
fn output_plan_created(plan_id: &str, subtasks: &[&Task]) -> Result<()> {
    let mut content = format!("## Plan Created\n- **ID:** {}\n\n", plan_id);
    for (i, subtask) in subtasks.iter().enumerate() {
        content.push_str(&format!("{}. {}\n", i + 1, &subtask.name));
    }
    content.push_str(&format!(
        "\n- Review:  `aiki plan show {}`\n- Execute: `aiki build {}`\n",
        plan_id, plan_id
    ));

    let md = MdBuilder::new("plan").build(&content, &[], &[]);
    eprintln!("{}", md);
    Ok(())
}

/// Output plan resumed message to stderr
fn output_plan_resumed(plan_id: &str, subtasks: &[&Task]) -> Result<()> {
    let completed = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed)
        .count();
    let total = subtasks.len();

    let mut content = format!(
        "## Plan Resumed\n- **ID:** {}\n- Resuming existing plan ({}/{} subtasks done).\n\n",
        plan_id, completed, total
    );
    for (i, subtask) in subtasks.iter().enumerate() {
        let status_mark = if subtask.status == TaskStatus::Closed {
            "done"
        } else {
            "pending"
        };
        content.push_str(&format!("{}. [{}] {}\n", i + 1, status_mark, &subtask.name));
    }
    content.push_str(&format!(
        "\n- Review:  `aiki plan show {}`\n- Execute: `aiki build {}`\n",
        plan_id, plan_id
    ));

    let md = MdBuilder::new("plan").build(&content, &[], &[]);
    eprintln!("{}", md);
    Ok(())
}

/// Output plan show (detailed status display)
fn output_plan_show(plan: &Task, subtasks: &[&Task]) -> Result<()> {
    let completed = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed)
        .count();
    let total = subtasks.len();

    let status_str = match plan.status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Closed => "closed",
    };

    let outcome_str = plan
        .closed_outcome
        .as_ref()
        .map(|o| format!("- **Outcome:** {}\n", o))
        .unwrap_or_default();

    let spec_str = plan
        .data
        .get("spec")
        .map(|s| format!("- **Spec:** {}\n", s))
        .unwrap_or_default();

    let mut content = format!(
        "## Plan: {}\n- **ID:** {}\n- **Status:** {}\n{}{}",
        &plan.name, &plan.id, status_str, outcome_str, spec_str
    );

    // Add progress summary
    content.push_str(&format!("- **Progress:** {}/{}\n", completed, total));

    // Add subtask list
    if !subtasks.is_empty() {
        content.push_str("\n### Subtasks\n| # | ID | Status | Outcome | Name |\n|---|-----|--------|---------|------|\n");
        for (i, subtask) in subtasks.iter().enumerate() {
            let sub_status = match subtask.status {
                TaskStatus::Open => "open",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Stopped => "stopped",
                TaskStatus::Closed => "closed",
            };

            let sub_outcome = subtask
                .closed_outcome
                .as_ref()
                .map(|o| o.to_string())
                .unwrap_or_default();

            content.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                i + 1, &subtask.id, sub_status, sub_outcome, &subtask.name
            ));
        }
    }

    // Add sources
    if !plan.sources.is_empty() {
        content.push_str("\n### Sources\n");
        for source in &plan.sources {
            content.push_str(&format!("- {}\n", source));
        }
    }

    let md = MdBuilder::new("plan-show").build(&content, &[], &[]);
    eprintln!("{}", md);

    Ok(())
}

/// Detect whether a string looks like a spec path (ends in .md) vs other input
#[cfg(test)]
fn is_spec_path(input: &str) -> bool {
    input.ends_with(".md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_task(id: &str, name: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            task_type: None,
            status,
            priority: TaskPriority::P2,
            assignee: None,
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
            summary: None,
            comments: Vec::new(),
        }
    }

    fn make_task_with_data(
        id: &str,
        name: &str,
        status: TaskStatus,
        data: HashMap<String, String>,
    ) -> Task {
        let mut task = make_task(id, name, status);
        task.data = data;
        task
    }

    fn make_task_with_data_and_sources(
        id: &str,
        name: &str,
        status: TaskStatus,
        data: HashMap<String, String>,
        sources: Vec<String>,
    ) -> Task {
        let mut task = make_task_with_data(id, name, status, data);
        task.sources = sources;
        task
    }

    #[test]
    fn test_is_spec_path() {
        assert!(is_spec_path("ops/now/feature.md"));
        assert!(is_spec_path("simple.md"));
        assert!(is_spec_path("/absolute/path/to/spec.md"));
        assert!(!is_spec_path("mvslrspmoynoxyyywqyutmovxpvztkls"));
        assert!(!is_spec_path("not-a-spec"));
        assert!(!is_spec_path(""));
    }

    #[test]
    fn test_find_plan_for_spec_none() {
        let tasks = HashMap::new();
        assert!(find_plan_for_spec(&tasks, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_plan_for_spec_found() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        let task = make_task_with_data_and_sources(
            "plan1",
            "Plan: Feature",
            TaskStatus::Open,
            data,
            vec!["task:planning1".to_string()],
        );
        tasks.insert("plan1".to_string(), task);

        let result = find_plan_for_spec(&tasks, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_for_spec_excludes_planning_task() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        // This is a planning task (type: "plan") - should be excluded
        let mut planning_task = make_task_with_data(
            "planning1",
            "Plan: ops/now/feature.md",
            TaskStatus::Closed,
            data.clone(),
        );
        planning_task.task_type = Some("plan".to_string());
        tasks.insert("planning1".to_string(), planning_task);

        // This is the actual plan task created by the planning agent
        let plan_task = make_task_with_data_and_sources(
            "plan1",
            "Plan: My Feature",
            TaskStatus::Open,
            data,
            vec!["task:planning1".to_string()],
        );
        tasks.insert("plan1".to_string(), plan_task);

        let result = find_plan_for_spec(&tasks, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_for_spec_wrong_spec() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/other.md".to_string());

        let task = make_task_with_data_and_sources(
            "plan1",
            "Plan: Other",
            TaskStatus::Open,
            data,
            vec!["task:planning1".to_string()],
        );
        tasks.insert("plan1".to_string(), task);

        assert!(find_plan_for_spec(&tasks, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_plan_for_spec_most_recent() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        let mut task1 = make_task_with_data_and_sources(
            "plan_old",
            "Plan: Old",
            TaskStatus::Closed,
            data.clone(),
            vec!["task:planning1".to_string()],
        );
        task1.created_at = chrono::Utc::now() - chrono::Duration::hours(1);
        tasks.insert("plan_old".to_string(), task1);

        let task2 = make_task_with_data_and_sources(
            "plan_new",
            "Plan: New",
            TaskStatus::Open,
            data,
            vec!["task:planning2".to_string()],
        );
        tasks.insert("plan_new".to_string(), task2);

        let result = find_plan_for_spec(&tasks, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan_new");
    }

    #[test]
    fn test_find_created_plan() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        let task = make_task_with_data_and_sources(
            "plan1",
            "Plan: Feature",
            TaskStatus::Open,
            data,
            vec!["task:planning123".to_string()],
        );
        tasks.insert("plan1".to_string(), task);

        let result = find_created_plan(&tasks, "ops/now/feature.md", "planning123");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_created_plan_wrong_planning_id() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        let task = make_task_with_data_and_sources(
            "plan1",
            "Plan: Feature",
            TaskStatus::Open,
            data,
            vec!["task:other_planning".to_string()],
        );
        tasks.insert("plan1".to_string(), task);

        // Looking for a different planning task ID
        let result = find_created_plan(&tasks, "ops/now/feature.md", "planning123");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_plan_for_spec_fallback_no_task_source() {
        let mut tasks = HashMap::new();
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        // Task without task: source (fallback path)
        let task = make_task_with_data(
            "plan_direct",
            "Plan: Feature",
            TaskStatus::Open,
            data,
        );
        tasks.insert("plan_direct".to_string(), task);

        let result = find_plan_for_spec(&tasks, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan_direct");
    }

    #[test]
    fn test_validate_spec_path_not_md() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let result = validate_spec_path(temp_dir.path(), "not-markdown.txt");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be markdown"));
    }

    #[test]
    fn test_validate_spec_path_not_found() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let result = validate_spec_path(temp_dir.path(), "nonexistent.md");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Spec file not found"));
    }

    #[test]
    fn test_validate_spec_path_exists() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let spec_file = temp_dir.path().join("my-spec.md");
        std::fs::write(&spec_file, "# My Spec").unwrap();
        let result = validate_spec_path(temp_dir.path(), "my-spec.md");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_spec_path_absolute() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let spec_file = temp_dir.path().join("absolute-spec.md");
        std::fs::write(&spec_file, "# Spec").unwrap();
        let result = validate_spec_path(temp_dir.path(), &spec_file.to_string_lossy());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_spec_path_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir.md");
        std::fs::create_dir_all(&dir_path).unwrap();
        let result = validate_spec_path(temp_dir.path(), "subdir.md");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not a file"));
    }
}

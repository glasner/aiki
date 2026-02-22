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

use super::OutputFormat;
use crate::agents::AgentType;
use crate::config::get_aiki_binary_path;
use crate::error::{AikiError, Result};
use crate::tasks::id::is_task_id;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::templates::get_working_copy_change_id;
use crate::specs::{parse_spec_metadata, SpecGraph};
use crate::tasks::md::MdBuilder;
use crate::tasks::{
    find_task, generate_task_id, get_subtasks, is_task_id_prefix, materialize_graph, read_events,
    write_event, write_link_event, Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus,
};

/// Plan subcommands
#[derive(Subcommand)]
pub enum PlanSubcommands {
    /// Show plan status and subtasks
    Show {
        /// Spec path or plan task ID (32 lowercase letters)
        arg: String,

        /// Output format (e.g., `id` for bare task ID)
        #[arg(long, short = 'o', value_name = "FORMAT")]
        output: Option<OutputFormat>,
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
            PlanSubcommands::Show { arg, output } => run_show(&cwd, &arg, output),
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

/// Core plan creation implementation — deterministic find-or-create.
///
/// Behavior (no interactive prompts):
/// - `--restart` → always close existing plan and create new
/// - No plan exists → create new plan
/// - Valid incomplete plan exists (has subtasks) → return it
/// - Invalid plan exists (no subtasks, still open) → close as wont_do, create new
/// - Closed plan exists → create new plan
fn run_plan(
    cwd: &Path,
    spec_path: &str,
    restart: bool,
    template_name: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    // Validate spec file exists and is .md
    validate_spec_path(cwd, spec_path)?;

    // Check if spec is a draft
    let full_path = if spec_path.starts_with('/') {
        std::path::PathBuf::from(spec_path)
    } else {
        cwd.join(spec_path)
    };
    let metadata = parse_spec_metadata(&full_path);
    if metadata.draft {
        return Err(AikiError::InvalidArgument(
            "Cannot create plan for draft spec. Remove `draft: true` from frontmatter first."
                .to_string(),
        ));
    }

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
    let graph = materialize_graph(&events);
    let spec_graph = SpecGraph::build(&graph);

    // --restart always creates a new plan
    if restart {
        if let Some(plan) = spec_graph.find_plan_for_spec(spec_path, &graph) {
            if plan.status != TaskStatus::Closed {
                undo_completed_subtasks(cwd, &plan.id)?;
                close_plan(cwd, &plan.id)?;
            }
        }
        let plan_id = create_plan(cwd, spec_path, template_name.as_deref(), agent_type)?;
        return output_plan_result(cwd, &plan_id, true);
    }

    // Find-or-create: check for existing plan
    let existing_plan = spec_graph.find_plan_for_spec(spec_path, &graph);

    match existing_plan {
        Some(plan) if plan.status != TaskStatus::Closed => {
            // Plan is open — validate it has subtasks
            let subtasks = get_subtasks(&graph, &plan.id);
            if subtasks.is_empty() {
                // Invalid plan (no subtasks) — planning agent failed
                close_plan_as_invalid(cwd, &plan.id)?;
                let plan_id = create_plan(cwd, spec_path, template_name.as_deref(), agent_type)?;
                return output_plan_result(cwd, &plan_id, true);
            }

            // Valid incomplete plan — return it (deterministic, no prompt)
            output_plan_resumed(&plan.id, &subtasks)?;
            if !std::io::stdout().is_terminal() {
                println!("<aiki_plan plan_id=\"{}\"/>", plan.id);
            }
            Ok(())
        }
        _ => {
            // No plan, or plan is closed — create new
            let plan_id = create_plan(cwd, spec_path, template_name.as_deref(), agent_type)?;
            output_plan_result(cwd, &plan_id, true)
        }
    }
}

/// Create a new plan by running the planning agent.
///
/// 1. Creates the plan task (container for subtasks)
/// 2. Creates and runs the planning agent task with `data.plan` pointing to it
/// 3. Returns the plan task ID
fn create_plan(
    cwd: &Path,
    spec_path: &str,
    template_name: Option<&str>,
    agent_type: Option<AgentType>,
) -> Result<String> {
    // Create the plan task first so the planning agent can add subtasks to it
    let plan_id = create_plan_task(cwd, spec_path)?;

    let template = template_name.unwrap_or("aiki/plan");
    let assignee = agent_type
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let planning_task_id = create_planning_task(cwd, spec_path, &plan_id, template, assignee)?;

    // Plan task depends on planning task — blocked until planning finishes
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(cwd, &graph, "depends-on", &plan_id, &planning_task_id)?;

    // Run the planning task to completion
    let options = if let Some(agent) = agent_type {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };
    task_run(cwd, &planning_task_id, options)?;

    Ok(plan_id)
}

/// Create the plan task — the container that holds subtasks.
///
/// Extracts the spec title from the H1 heading (or filename as fallback).
/// Sets `data.spec`, `implements` link, and source.
fn create_plan_task(cwd: &Path, spec_path: &str) -> Result<String> {
    let full_path = if spec_path.starts_with('/') {
        std::path::PathBuf::from(spec_path)
    } else {
        cwd.join(spec_path)
    };
    let metadata = parse_spec_metadata(&full_path);

    // Use H1 title from spec, or fall back to filename without extension
    let spec_title = metadata.title.unwrap_or_else(|| {
        Path::new(spec_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let plan_name = format!("Plan: {}", spec_title);
    let plan_id = generate_task_id(&plan_name);
    let timestamp = chrono::Utc::now();
    let working_copy = get_working_copy_change_id(cwd);

    let mut data = std::collections::HashMap::new();
    data.insert("spec".to_string(), spec_path.to_string());

    let event = TaskEvent::Created {
        task_id: plan_id.clone(),
        name: plan_name,
        slug: None,
        task_type: None,
        priority: TaskPriority::P2,
        assignee: None,
        sources: vec![format!("file:{}", spec_path)],
        template: None,
        working_copy,
        instructions: None,
        data,
        timestamp,
    };
    write_event(cwd, &event)?;

    // Emit implements link
    let spec_target = if spec_path.starts_with("file:") {
        spec_path.to_string()
    } else {
        format!("file:{}", spec_path)
    };
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(cwd, &graph, "implements", &plan_id, &spec_target)?;

    Ok(plan_id)
}

/// Find an existing plan or create a new one for the given spec.
///
/// Deterministic behavior (no interactive prompts):
/// - Valid incomplete plan exists (has subtasks) → return its ID
/// - Invalid plan exists (no subtasks, still open) → close as wont_do, create new
/// - No plan or closed plan → create new via planning agent
///
/// Returns the plan task ID.
pub fn find_or_create_plan(cwd: &Path, spec_path: &str) -> Result<String> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let spec_graph = SpecGraph::build(&graph);

    let existing_plan = spec_graph.find_plan_for_spec(spec_path, &graph);

    match existing_plan {
        Some(plan) if plan.status != TaskStatus::Closed => {
            let subtasks = get_subtasks(&graph, &plan.id);
            if subtasks.is_empty() {
                close_plan_as_invalid(cwd, &plan.id)?;
                create_plan(cwd, spec_path, None, None)
            } else {
                Ok(plan.id.clone())
            }
        }
        _ => create_plan(cwd, spec_path, None, None),
    }
}

/// Close a plan as invalid (no subtasks — planning agent failed).
fn close_plan_as_invalid(cwd: &Path, plan_id: &str) -> Result<()> {
    let timestamp = chrono::Utc::now();
    let close_event = TaskEvent::Closed {
        task_ids: vec![plan_id.to_string()],
        outcome: TaskOutcome::WontDo,
        summary: Some("No subtasks created — plan invalid".to_string()),
        turn_id: None,
        timestamp,
    };
    write_event(cwd, &close_event)?;
    Ok(())
}

/// Show plan status and subtasks
fn run_show(cwd: &Path, arg: &str, output_format: Option<OutputFormat>) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    // Determine if arg is a task ID or spec path
    let plan = if is_task_id(arg) || is_task_id_prefix(arg) {
        // Task ID or prefix lookup
        find_task(&graph.tasks, arg)?
    } else {
        // Spec path lookup via SpecGraph
        let spec_graph = SpecGraph::build(&graph);
        spec_graph.find_plan_for_spec(arg, &graph).ok_or_else(|| {
            AikiError::InvalidArgument(format!("No plan found for spec: {}", arg))
        })?
    };

    match output_format {
        Some(OutputFormat::Id) => {
            println!("{}", plan.id);
        }
        None => {
            let subtasks = get_subtasks(&graph, &plan.id);
            output_plan_show(plan, &subtasks)?;
        }
    }

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
        turn_id: None,
        timestamp,
    };
    write_event(cwd, &close_event)?;
    Ok(())
}

/// Create the planning task from template.
///
/// The planning task is an ephemeral task that runs the planning agent.
/// The agent adds subtasks to the pre-created plan task at `plan_id`.
fn create_planning_task(
    cwd: &Path,
    spec_path: &str,
    plan_id: &str,
    template_name: &str,
    assignee: Option<String>,
) -> Result<String> {
    use super::task::{create_from_template, TemplateTaskParams};

    let mut data = std::collections::HashMap::new();
    data.insert("spec".to_string(), spec_path.to_string());
    data.insert("plan".to_string(), plan_id.to_string());

    let params = TemplateTaskParams {
        template_name: template_name.to_string(),
        data,
        sources: vec![format!("file:{}", spec_path)],
        assignee: assignee.or_else(|| Some("claude-code".to_string())),
        ..Default::default()
    };

    let task_id = create_from_template(cwd, params)?;

    // Emit scoped-to link for the spec (dual-write with data.spec attribute)
    let spec_target = if spec_path.starts_with("file:") {
        spec_path.to_string()
    } else {
        format!("file:{}", spec_path)
    };
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(cwd, &graph, "scoped-to", &task_id, &spec_target)?;

    Ok(task_id)
}

/// Output plan result (created or found) to stderr and stdout.
fn output_plan_result(cwd: &Path, plan_id: &str, created: bool) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let subtasks = get_subtasks(&graph, plan_id);

    if created {
        output_plan_created(plan_id, &subtasks)?;
    } else {
        output_plan_resumed(plan_id, &subtasks)?;
    }

    if !std::io::stdout().is_terminal() {
        println!("<aiki_plan plan_id=\"{}\"/>", plan_id);
    }

    Ok(())
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
    use crate::tasks::graph::{EdgeStore, TaskGraph};
    use crate::tasks::types::FastHashMap;
    use std::collections::HashMap;

    fn make_task(id: &str, name: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            slug: None,
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
            turn_started: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn make_graph(tasks: FastHashMap<String, Task>, edges: EdgeStore) -> TaskGraph {
        TaskGraph {
            tasks,
            edges,
            slug_index: FastHashMap::default(),
        }
    }

    /// Helper: find plan for spec via SpecGraph (replaces removed find_plan_for_spec)
    fn find_plan_for_spec_via_graph<'a>(
        graph: &'a TaskGraph,
        spec_path: &str,
    ) -> Option<&'a Task> {
        let sg = SpecGraph::build(graph);
        sg.find_plan_for_spec(spec_path, graph)
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
        let graph = make_graph(FastHashMap::default(), EdgeStore::new());
        assert!(find_plan_for_spec_via_graph(&graph, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_plan_for_spec_via_implements_link() {
        let mut tasks = FastHashMap::default();
        let task = make_task("plan1", "Plan: Feature", TaskStatus::Open);
        tasks.insert("plan1".to_string(), task);

        let mut edges = EdgeStore::new();
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let graph = make_graph(tasks, edges);
        let result = find_plan_for_spec_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_for_spec_ignores_build_subtask_with_inherited_data() {
        let mut tasks = FastHashMap::default();

        let mut orchestrator = make_task("orch1", "Build: feature", TaskStatus::InProgress);
        orchestrator.task_type = Some("orchestrator".to_string());
        tasks.insert("orch1".to_string(), orchestrator);

        let plan = make_task("plan1", "Plan: Feature", TaskStatus::Open);
        tasks.insert("plan1".to_string(), plan);

        let mut edges = EdgeStore::new();
        edges.add("orch1", "file:ops/now/feature.md", "implements");
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let graph = make_graph(tasks, edges);
        let result = find_plan_for_spec_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_for_spec_excludes_planning_task() {
        let mut tasks = FastHashMap::default();

        let mut planning_task =
            make_task("planning1", "Plan: ops/now/feature.md", TaskStatus::Closed);
        planning_task.task_type = Some("plan".to_string());
        tasks.insert("planning1".to_string(), planning_task);

        let plan_task = make_task("plan1", "Plan: My Feature", TaskStatus::Open);
        tasks.insert("plan1".to_string(), plan_task);

        let mut edges = EdgeStore::new();
        edges.add("planning1", "file:ops/now/feature.md", "implements");
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let graph = make_graph(tasks, edges);
        let result = find_plan_for_spec_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_for_spec_wrong_spec() {
        let mut tasks = FastHashMap::default();
        let task = make_task("plan1", "Plan: Other", TaskStatus::Open);
        tasks.insert("plan1".to_string(), task);

        let mut edges = EdgeStore::new();
        edges.add("plan1", "file:ops/now/other.md", "implements");

        let graph = make_graph(tasks, edges);
        assert!(find_plan_for_spec_via_graph(&graph, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_plan_for_spec_most_recent() {
        let mut tasks = FastHashMap::default();

        let mut task1 = make_task("plan_old", "Plan: Old", TaskStatus::Closed);
        task1.created_at = chrono::Utc::now() - chrono::Duration::hours(1);
        tasks.insert("plan_old".to_string(), task1);

        let task2 = make_task("plan_new", "Plan: New", TaskStatus::Open);
        tasks.insert("plan_new".to_string(), task2);

        let mut edges = EdgeStore::new();
        edges.add("plan_old", "file:ops/now/feature.md", "implements");
        edges.add("plan_new", "file:ops/now/feature.md", "implements");

        let graph = make_graph(tasks, edges);
        let result = find_plan_for_spec_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan_new");
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

    #[test]
    fn test_output_format_id_variant() {
        // Verify OutputFormat::Id can be constructed and matched
        let fmt = OutputFormat::Id;
        assert!(matches!(fmt, OutputFormat::Id));
    }

    #[test]
    fn test_output_format_clap_parse() {
        // Verify clap can parse "id" into OutputFormat::Id
        use clap::ValueEnum;
        let parsed = OutputFormat::from_str("id", false);
        assert!(parsed.is_ok());
        assert!(matches!(parsed.unwrap(), OutputFormat::Id));
    }

    #[test]
    fn test_output_format_clap_rejects_unknown() {
        use clap::ValueEnum;
        let parsed = OutputFormat::from_str("unknown_format", false);
        assert!(parsed.is_err());
    }
}

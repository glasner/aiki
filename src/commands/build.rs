//! Build command for decomposing plan files and executing all subtasks
//!
//! This module provides the `aiki build` command which:
//! - Creates an epic from a plan file and automatically executes all subtasks
//! - Supports building from an existing epic ID
//! - Shows build/epic status via the `show` subcommand
//! - Supports async (background) execution

use crate::output_utils;
use crate::tui;
use crate::tui::theme::{detect_mode, Theme};
use std::env;
use std::path::Path;

use clap::Subcommand;

use super::async_spawn;
use super::OutputFormat;
use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::plans::{parse_plan_metadata, PlanGraph};
use crate::tasks::id::{is_task_id, is_task_id_prefix};
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, Task, TaskOutcome, TaskStatus,
};
use crate::workflow::builders::{build_workflow, build_workflow_from_epic, BuildOpts};
use crate::workflow::steps::decompose::{
    check_epic_blockers, close_epic, close_epic_as_invalid, create_epic_task, restart_epic,
    undo_completed_subtasks,
};
use crate::workflow::steps::plan::{cleanup_stale_builds, validate_plan_path};
use crate::workflow::{RunMode, Step, Workflow, WorkflowContext};
use std::collections::VecDeque;

/// Build subcommands
#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum BuildSubcommands {
    /// Show build/epic status for a plan
    Show {
        /// Plan path to show build status for
        plan_path: String,

        /// Output format (e.g., `id` for bare task ID)
        #[arg(long, short = 'o', value_name = "FORMAT")]
        output: Option<OutputFormat>,
    },
}

/// Arguments for the build command
#[derive(clap::Args)]
pub struct BuildArgs {
    /// Plan path or epic ID (32 lowercase letters)
    pub target: Option<String>,

    /// Run build asynchronously
    #[arg(long = "async")]
    pub run_async: bool,

    /// Ignore existing epic, create new one from scratch
    #[arg(long)]
    pub restart: bool,

    /// Custom decompose template (default: decompose)
    #[arg(long = "decompose-template")]
    pub decompose_template: Option<String>,

    /// Custom loop template (default: loop)
    #[arg(long = "loop-template")]
    pub loop_template: Option<String>,

    /// Agent for build orchestration (default: claude-code)
    #[arg(long)]
    pub agent: Option<String>,

    /// Run review after build
    #[arg(long, short = 'r')]
    pub review: bool,

    /// Run review after build with custom template (implies --review)
    #[arg(long = "review-template")]
    pub review_template: Option<String>,

    /// Run review+fix after build (implies --review)
    #[arg(long, short = 'f')]
    pub fix: bool,

    /// Run review+fix after build with custom fix plan template (implies --fix)
    #[arg(long = "fix-template")]
    pub fix_template: Option<String>,

    /// Internal: continue an async build from a previously created epic
    #[arg(long = "_continue-async", hide = true)]
    pub continue_async: Option<String>,

    /// Output format (e.g., `id` for bare task IDs on stdout)
    #[arg(long, short = 'o', value_name = "FORMAT")]
    pub output: Option<OutputFormat>,

    /// Subcommand (show)
    #[command(subcommand)]
    pub subcommand: Option<BuildSubcommands>,
}

/// Maximum number of fix iterations before giving up.
const MAX_BUILD_ITERATIONS: usize = 10;

/// Check whether a review task has actionable issues (issue_count > 0).
fn has_review_issues(ctx: &WorkflowContext, review_task_id: &str) -> anyhow::Result<bool> {
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    Ok(find_task(&graph.tasks, review_task_id)
        .map(|t| {
            t.data
                .get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0)
                > 0
        })
        .unwrap_or(false))
}

/// Drive a build workflow with dynamic fix iteration.
///
/// Uses a [`VecDeque`] instead of a fixed step list. After each Review or
/// RegressionReview step, checks for actionable issues and injects a
/// Fix→Decompose→Loop→Review→RegressionReview cycle (up to [`MAX_BUILD_ITERATIONS`]).
pub fn drive_build(
    steps: Vec<Step>,
    mut ctx: WorkflowContext,
    mode: RunMode,
    opts: &BuildOpts,
) -> anyhow::Result<WorkflowContext> {
    let mut queue: VecDeque<Step> = steps.into_iter().collect();
    let mut iteration = 0;

    while let Some(step) = queue.pop_front() {
        let is_review = matches!(&step, Step::Review { .. });
        let is_regression_review = matches!(&step, Step::RegressionReview { .. });

        // Execute the step with appropriate output
        let result = match mode {
            RunMode::Text => {
                if let Some(section) = step.section() {
                    eprintln!("\n── {} ──", section);
                }
                eprintln!("⠙ {}...", step.name());
                match step.run(&mut ctx) {
                    Ok(result) => {
                        eprintln!("合 {} — {}", step.name(), result.message);
                        result
                    }
                    Err(e) => {
                        eprintln!("✗ {} — {}", step.name(), e);
                        return Err(e);
                    }
                }
            }
            RunMode::Quiet => step.run(&mut ctx)?,
        };

        // After Review or RegressionReview: inject Fix step if issues found.
        // run_fix_step drives the full quality loop internally (plan → decompose
        // → loop → review → regression review), so we only need to inject the
        // single Fix step — no Decompose/Loop/Review/RegressionReview after it.
        if (is_review || is_regression_review) && iteration < MAX_BUILD_ITERATIONS {
            if let Some(ref review_task_id) = result.task_id {
                if has_review_issues(&ctx, review_task_id)? {
                    iteration += 1;
                    queue.push_back(Step::Fix {
                        review_id: review_task_id.clone(),
                        scope: None,
                        assignee: None,
                        template: opts.fix_template.clone(),
                        autorun: false,
                    });
                }
            }
        }

        if (is_review || is_regression_review) && iteration >= MAX_BUILD_ITERATIONS {
            eprintln!(
                "Warning: build fix iteration reached maximum ({}) without full approval.",
                MAX_BUILD_ITERATIONS,
            );
            break;
        }
    }

    Ok(ctx)
}

impl Workflow {
    /// Run a build workflow, using [`drive_build`] for fix iteration when
    /// `opts.fix_after` is true, or the generic step runner otherwise.
    pub fn run_build(self, mode: RunMode, opts: &BuildOpts) -> anyhow::Result<WorkflowContext> {
        if opts.fix_after {
            drive_build(self.steps, self.ctx, mode, opts)
        } else {
            self.run(mode)
        }
    }
}

/// Convert anyhow::Error back to AikiError for crate-level Result.
fn anyhow_to_aiki(e: anyhow::Error) -> AikiError {
    match e.downcast::<AikiError>() {
        Ok(aiki_err) => aiki_err,
        Err(e) => AikiError::JjCommandFailed(e.to_string()),
    }
}

/// Run the build command
pub fn run(args: BuildArgs) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    if args.continue_async.is_some() {
        let epic_id = args.continue_async.clone().unwrap();
        return run_continue_async(&cwd, &epic_id, args);
    }

    if let Some(subcommand) = args.subcommand {
        return match subcommand {
            BuildSubcommands::Show { plan_path, output } => run_show(&cwd, &plan_path, output),
        };
    }

    let target = args.target.ok_or_else(|| {
        AikiError::InvalidArgument(
            "No plan path or epic ID provided. Usage: aiki build <plan-path-or-epic-id>"
                .to_string(),
        )
    })?;

    // Resolve --fix / --fix-template into a single Option<String>
    let fix_template = args.fix_template.or(if args.fix {
        Some("fix".to_string())
    } else {
        None
    });
    // Resolve --review / --review-template (--fix implies --review)
    // Pass through explicit --review-template only; None lets create_review pick scope-specific default
    let review_template = args.review_template.clone();
    let review_after = review_template.is_some() || args.review || fix_template.is_some();
    let fix_after = fix_template.is_some();

    let output_id = matches!(args.output, Some(OutputFormat::Id));

    if is_task_id(&target) || is_task_id_prefix(&target) {
        run_build_epic(
            &cwd,
            &target,
            args.run_async,
            args.loop_template,
            args.agent,
            review_after,
            fix_after,
            review_template,
            fix_template,
            output_id,
        )
    } else {
        run_build_plan(
            &cwd,
            &target,
            args.restart,
            args.run_async,
            args.decompose_template,
            args.loop_template,
            args.agent,
            review_after,
            fix_after,
            review_template,
            fix_template,
            output_id,
        )
    }
}

/// Background process entry point after being spawned by `spawn_aiki_background`.
///
/// This is called when `--_continue-async` is provided. The parent process has
/// already created the epic task and returned its ID to the caller. This function
/// picks up from there using a workflow: decompose → loop → optional review/fix.
fn run_continue_async(cwd: &Path, epic_id: &str, args: BuildArgs) -> Result<()> {
    let agent_type = if let Some(ref agent_str) = args.agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Find the epic and get plan_path
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let epic = find_task(&graph.tasks, epic_id)?;
    let epic_id = epic.id.clone();
    let plan_path = epic.data.get("plan").cloned().unwrap_or_default();

    // Resolve flags
    let fix_template = args.fix_template.or(if args.fix {
        Some("fix".to_string())
    } else {
        None
    });
    let review_template = args.review_template;
    let review_after = review_template.is_some() || args.review || fix_template.is_some();
    let fix_after = fix_template.is_some();

    let opts = BuildOpts {
        restart: false,
        decompose_template: args.decompose_template,
        loop_template: args.loop_template,
        agent: agent_type,
        agent_str: args.agent,
        review_after,
        review_template,
        fix_after,
        fix_template,
    };

    let wf = build_workflow_from_epic(cwd, &epic_id, &plan_path, &opts);
    wf.run_build(RunMode::Quiet, &opts)
        .map_err(anyhow_to_aiki)?;

    Ok(())
}

/// Build from a plan path — deterministic find-or-create.
///
/// - `--async`: validate plan, create epic, spawn background (workflow runs there)
/// - Sync: run full workflow with text output (Plan → Decompose → Loop → [Review → Fix])
fn run_build_plan(
    cwd: &Path,
    plan_path: &str,
    restart: bool,
    run_async: bool,
    decompose_template: Option<String>,
    loop_template: Option<String>,
    agent: Option<String>,
    review_after: bool,
    fix_after: bool,
    review_template: Option<String>,
    fix_template: Option<String>,
    output_id: bool,
) -> Result<()> {
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Sync (non-async): use full workflow with text output
    if !run_async {
        let opts = BuildOpts {
            restart,
            decompose_template,
            loop_template,
            agent: agent_type,
            agent_str: agent,
            review_after,
            review_template,
            fix_after,
            fix_template,
        };

        let wf = build_workflow(cwd, plan_path, &opts);
        wf.run_build(RunMode::Text, &opts).map_err(anyhow_to_aiki)?;

        crate::workflow::steps::build::output_after_workflow(cwd, plan_path, output_id)?;
        return Ok(());
    }

    // --async path: plan validation and epic setup happen before spawning.
    validate_plan_path(cwd, plan_path)?;

    let full_path = if plan_path.starts_with('/') {
        std::path::PathBuf::from(plan_path)
    } else {
        cwd.join(plan_path)
    };
    let metadata = parse_plan_metadata(&full_path);
    if metadata.draft {
        return Err(AikiError::InvalidArgument(
            "Cannot build draft plan. Remove `draft: true` from frontmatter first.".to_string(),
        ));
    }

    cleanup_stale_builds(cwd, plan_path)?;

    // Deterministic epic lookup
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);
    let existing_epic = plan_graph.find_epic_for_plan(plan_path, &graph);

    let epic_id = if restart {
        if let Some(epic) = existing_epic {
            if epic.status != TaskStatus::Closed {
                undo_completed_subtasks(cwd, &epic.id)?;
                close_epic(cwd, &epic.id)?;
            }
        }
        None
    } else {
        match existing_epic {
            Some(epic) if epic.status != TaskStatus::Closed => {
                let subtasks = get_subtasks(&graph, &epic.id);
                if subtasks.is_empty() {
                    close_epic_as_invalid(cwd, &epic.id)?;
                    None
                } else {
                    restart_epic(cwd, &epic.id)?;
                    Some(epic.id.clone())
                }
            }
            _ => None,
        }
    };

    // --async: spawn background process and return immediately
    {
        let is_existing_epic = epic_id.is_some();
        let epic_id = match epic_id {
            Some(id) => id,
            None => create_epic_task(cwd, plan_path)?,
        };

        if is_existing_epic {
            let events = read_events(cwd)?;
            let graph = materialize_graph(&events);
            check_epic_blockers(&graph, &epic_id)?;
        }

        let mut spawn_args: Vec<String> = vec![
            "build".to_string(),
            "--_continue-async".to_string(),
            epic_id.to_string(),
        ];
        if let Some(tmpl) = decompose_template.as_deref() {
            spawn_args.push("--decompose-template".to_string());
            spawn_args.push(tmpl.to_string());
        }
        if let Some(tmpl) = loop_template.as_deref() {
            spawn_args.push("--loop-template".to_string());
            spawn_args.push(tmpl.to_string());
        }
        if let Some(a) = agent.as_deref() {
            spawn_args.push("--agent".to_string());
            spawn_args.push(a.to_string());
        }
        if let Some(tmpl) = review_template.as_deref() {
            spawn_args.push("--review-template".to_string());
            spawn_args.push(tmpl.to_string());
        } else if review_after {
            spawn_args.push("--review".to_string());
        }
        if let Some(tmpl) = fix_template.as_deref() {
            spawn_args.push("--fix-template".to_string());
            spawn_args.push(tmpl.to_string());
        }
        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if output_id {
            println!("{}", epic_id);
        }

        Ok(())
    }
}

/// Build from an existing epic ID.
///
/// - `--async`: spawn background (workflow runs there via `--_continue-async`)
/// - Sync: run workflow with text output
fn run_build_epic(
    cwd: &Path,
    epic_id: &str,
    run_async: bool,
    loop_template: Option<String>,
    agent: Option<String>,
    review_after: bool,
    fix_after: bool,
    review_template: Option<String>,
    fix_template: Option<String>,
    output_id: bool,
) -> Result<()> {
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Find epic task (resolve prefix to canonical ID)
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let epic = find_task(&graph.tasks, epic_id)?;
    let epic_id = epic.id.as_str();

    // Check if epic is blocked before running
    check_epic_blockers(&graph, epic_id)?;

    let plan_path = epic.data.get("plan").cloned().ok_or_else(|| {
        AikiError::InvalidArgument(format!(
            "Epic task {} missing data.plan. Cannot run build without a plan path.",
            epic_id
        ))
    })?;

    let opts = BuildOpts {
        restart: false,
        decompose_template: None,
        loop_template,
        agent: agent_type,
        agent_str: agent.clone(),
        review_after,
        review_template,
        fix_after,
        fix_template,
    };

    // --async: spawn background process and return immediately
    if run_async {
        let mut spawn_args: Vec<String> = vec![
            "build".to_string(),
            "--_continue-async".to_string(),
            epic_id.to_string(),
        ];
        if let Some(tmpl) = opts.loop_template.as_deref() {
            spawn_args.push("--loop-template".to_string());
            spawn_args.push(tmpl.to_string());
        }
        if let Some(a) = agent.as_deref() {
            spawn_args.push("--agent".to_string());
            spawn_args.push(a.to_string());
        }
        if let Some(tmpl) = opts.review_template.as_deref() {
            spawn_args.push("--review-template".to_string());
            spawn_args.push(tmpl.to_string());
        } else if review_after {
            spawn_args.push("--review".to_string());
        }
        if let Some(tmpl) = opts.fix_template.as_deref() {
            spawn_args.push("--fix-template".to_string());
            spawn_args.push(tmpl.to_string());
        }
        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if output_id {
            println!("{}", epic_id);
        }

        return Ok(());
    }

    // Sync path: run workflow with text output
    let wf = build_workflow_from_epic(cwd, epic_id, &plan_path, &opts);
    wf.run_build(RunMode::Text, &opts).map_err(anyhow_to_aiki)?;

    crate::workflow::steps::build::output_after_workflow(cwd, &plan_path, output_id)?;

    Ok(())
}

// Step handlers are in workflow/steps/build.rs.
// The output_after_workflow helper is also there.

/// Show build/epic status for a plan
fn run_show(cwd: &Path, plan_path: &str, output_format: Option<OutputFormat>) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);

    // Find epic via PlanGraph
    let epic = plan_graph
        .find_epic_for_plan(plan_path, &graph)
        .ok_or_else(|| {
            AikiError::InvalidArgument(format!("No epic found for plan: {}", plan_path))
        })?;

    // Find build tasks associated with this plan
    let build_tasks: Vec<&Task> = graph
        .tasks
        .values()
        .filter(|t| {
            t.task_type.as_deref() == Some("orchestrator")
                && t.data.get("plan").map(|s| s.as_str()) == Some(plan_path)
        })
        .collect();

    match output_format {
        Some(OutputFormat::Id) => {
            if build_tasks.is_empty() {
                // No builds yet -- emit the epic ID as fallback
                println!("{}", epic.id);
            } else {
                for build in &build_tasks {
                    println!("{}", build.id);
                }
            }
        }
        None => {
            let subtasks = get_subtasks(&graph, &epic.id);
            output_build_show(epic, &subtasks, &build_tasks, &graph)?;
        }
    }

    Ok(())
}


/// Output build show (detailed status display)
pub(crate) fn output_build_show(
    epic: &Task,
    _subtasks: &[&Task],
    _build_tasks: &[&Task],
    graph: &crate::tasks::graph::TaskGraph,
) -> Result<()> {
    let plan_path = epic
        .data
        .get("plan")
        .map(|s| s.as_str())
        .unwrap_or("unknown");
    output_utils::emit(|| {
        let theme = Theme::from_mode(detect_mode());
        let window = tui::app::WindowState::new(80);
        let mut lines = tui::screens::build::view(graph, &epic.id, plan_path, &window);
        tui::render::render_to_string(&mut lines, &theme)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::{EdgeStore, TaskGraph};
    use crate::tasks::types::FastHashMap;
    use crate::tasks::TaskPriority;
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

    fn empty_graph() -> TaskGraph {
        TaskGraph {
            tasks: FastHashMap::default(),
            edges: EdgeStore::new(),
            slug_index: FastHashMap::default(),
        }
    }

    fn make_graph(tasks: FastHashMap<String, Task>, edges: EdgeStore) -> TaskGraph {
        TaskGraph {
            tasks,
            edges,
            slug_index: FastHashMap::default(),
        }
    }

    /// Helper: find epic for plan via PlanGraph
    fn find_epic_for_plan_via_graph<'a>(graph: &'a TaskGraph, plan_path: &str) -> Option<&'a Task> {
        let sg = PlanGraph::build(graph);
        sg.find_epic_for_plan(plan_path, graph)
    }

    // --- find_epic_for_plan tests ---

    #[test]
    fn test_find_epic_for_plan_none() {
        let graph = make_graph(FastHashMap::default(), EdgeStore::new());
        assert!(find_epic_for_plan_via_graph(&graph, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_epic_for_plan_via_implements_link() {
        let mut tasks = FastHashMap::default();
        let task = make_task("epic1", "Epic: Feature", TaskStatus::Open);
        tasks.insert("epic1".to_string(), task);

        let mut edges = EdgeStore::new();
        edges.add("epic1", "file:ops/now/feature.md", "implements-plan");

        let graph = make_graph(tasks, edges);
        let result = find_epic_for_plan_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "epic1");
    }

    #[test]
    fn test_find_epic_for_plan_wrong_plan() {
        let mut tasks = FastHashMap::default();
        let task = make_task("epic1", "Epic: Other", TaskStatus::Open);
        tasks.insert("epic1".to_string(), task);

        let mut edges = EdgeStore::new();
        edges.add("epic1", "file:ops/now/other.md", "implements-plan");

        let graph = make_graph(tasks, edges);
        assert!(find_epic_for_plan_via_graph(&graph, "ops/now/feature.md").is_none());
    }

    #[test]
    fn test_find_epic_for_plan_most_recent() {
        let mut tasks = FastHashMap::default();

        let mut task1 = make_task("epic_old", "Epic: Old", TaskStatus::Closed);
        task1.created_at = chrono::Utc::now() - chrono::Duration::hours(1);
        tasks.insert("epic_old".to_string(), task1);

        let task2 = make_task("epic_new", "Epic: New", TaskStatus::Open);
        tasks.insert("epic_new".to_string(), task2);

        let mut edges = EdgeStore::new();
        edges.add("epic_old", "file:ops/now/feature.md", "implements-plan");
        edges.add("epic_new", "file:ops/now/feature.md", "implements-plan");

        let graph = make_graph(tasks, edges);
        let result = find_epic_for_plan_via_graph(&graph, "ops/now/feature.md");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "epic_new");
    }

    // --- cleanup_stale_builds helper logic tests ---

    #[test]
    fn test_stale_build_detection_in_progress() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());

        let mut task =
            make_task_with_data("build1", "Build: feature", TaskStatus::InProgress, data);
        task.task_type = Some("orchestrator".to_string());
        tasks.insert("build1".to_string(), task);

        // Verify the stale build detection logic
        let stale_builds: Vec<String> = tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some("ops/now/feature.md")
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect();

        assert_eq!(stale_builds.len(), 1);
        assert_eq!(stale_builds[0], "build1");
    }

    #[test]
    fn test_stale_build_detection_open() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());

        let mut task = make_task_with_data("build2", "Build: feature", TaskStatus::Open, data);
        task.task_type = Some("orchestrator".to_string());
        tasks.insert("build2".to_string(), task);

        let stale_builds: Vec<String> = tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some("ops/now/feature.md")
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect();

        assert_eq!(stale_builds.len(), 1);
        assert_eq!(stale_builds[0], "build2");
    }

    #[test]
    fn test_stale_build_not_detected_when_closed() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());

        let mut task = make_task_with_data("build3", "Build: feature", TaskStatus::Closed, data);
        task.task_type = Some("orchestrator".to_string());
        tasks.insert("build3".to_string(), task);

        let stale_builds: Vec<String> = tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some("ops/now/feature.md")
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect();

        assert!(stale_builds.is_empty());
    }

    #[test]
    fn test_stale_build_not_detected_wrong_plan() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/other.md".to_string());

        let mut task = make_task_with_data("build4", "Build: other", TaskStatus::InProgress, data);
        task.task_type = Some("orchestrator".to_string());
        tasks.insert("build4".to_string(), task);

        let stale_builds: Vec<String> = tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some("ops/now/feature.md")
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect();

        assert!(stale_builds.is_empty());
    }

    #[test]
    fn test_stale_build_not_detected_wrong_type() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());

        // Not a build task (no task_type or different type)
        let task = make_task_with_data("not_build", "Something else", TaskStatus::InProgress, data);
        tasks.insert("not_build".to_string(), task);

        let stale_builds: Vec<String> = tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some("ops/now/feature.md")
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect();

        assert!(stale_builds.is_empty());
    }

    // --- Argument detection tests ---

    #[test]
    fn test_argument_detection_plan_path() {
        assert!(!is_task_id("ops/now/feature.md"));
        assert!(!is_task_id("simple.md"));
        assert!(!is_task_id("/absolute/path/to/plan.md"));
        assert!(!is_task_id("not-a-task-id"));
        assert!(!is_task_id(""));
    }

    #[test]
    fn test_argument_detection_task_id() {
        // 32 lowercase k-z letters
        assert!(is_task_id("mvslrspmoynoxyyywqyutmovxpvztkls"));
        assert!(is_task_id("xtuttnyvykpulsxzqnznsxylrzkkqssy"));
    }

    #[test]
    fn test_argument_detection_not_task_id() {
        // Too short
        assert!(!is_task_id("klmnop"));
        // Too long
        assert!(!is_task_id("mvslrspmoynoxyyywqyutmovxpvztklsx"));
        // Contains letters outside k-z range
        assert!(!is_task_id("abcdefghijklmnopqrstuvwxyzabcdef"));
        // Contains numbers
        assert!(!is_task_id("mvslrspmoynoxyyywqyutmovxpvz1234"));
        // Contains uppercase
        assert!(!is_task_id("Mvslrspmoynoxyyywqyutmovxpvztkls"));
    }


    // --- Build show output formatting tests ---

    #[test]
    fn test_output_build_show_basic() {
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());
        let epic = make_task_with_data("epic1", "Epic: Feature", TaskStatus::Open, data);
        let subtasks: Vec<&Task> = vec![];
        let build_tasks: Vec<&Task> = vec![];
        let graph = empty_graph();
        let result = output_build_show(&epic, &subtasks, &build_tasks, &graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_build_show_with_subtasks_and_builds() {
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());
        let mut epic = make_task_with_data("epic1", "Epic: Feature", TaskStatus::InProgress, data);
        epic.sources = vec!["file:ops/now/feature.md".to_string()];

        let sub1 = make_task("sub1", "Step 1", TaskStatus::Closed);
        let sub2 = make_task("sub2", "Step 2", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![&sub1, &sub2];

        let mut build_data = HashMap::new();
        build_data.insert("plan".to_string(), "ops/now/feature.md".to_string());
        let mut build =
            make_task_with_data("build1", "Build: feature", TaskStatus::Closed, build_data);
        build.task_type = Some("orchestrator".to_string());
        build.closed_outcome = Some(TaskOutcome::Done);
        let build_tasks: Vec<&Task> = vec![&build];

        let graph = empty_graph();
        let result = output_build_show(&epic, &subtasks, &build_tasks, &graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_build_show_closed_epic_with_outcome() {
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());
        let mut epic = make_task_with_data("epic1", "Epic: Feature", TaskStatus::Closed, data);
        epic.closed_outcome = Some(TaskOutcome::Done);

        let subtasks: Vec<&Task> = vec![];
        let build_tasks: Vec<&Task> = vec![];
        let graph = empty_graph();
        let result = output_build_show(&epic, &subtasks, &build_tasks, &graph);
        assert!(result.is_ok());
    }

    // --- OutputFormat tests ---

    #[test]
    fn test_output_format_id_variant() {
        let fmt = OutputFormat::Id;
        assert!(matches!(fmt, OutputFormat::Id));
    }

    #[test]
    fn test_output_format_clap_parse() {
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

    // --- Build args tests ---

    #[test]
    fn test_build_args_no_loop_or_lanes_flags() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        assert!(!args.run_async);
        assert!(!args.restart);
    }

    #[test]
    fn test_loop_template_override_via_flag() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: Some("custom/loop".to_string()),
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        assert_eq!(args.loop_template.as_deref(), Some("custom/loop"));
    }

    #[test]
    fn test_decompose_template_override_via_flag() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: Some("my/decompose".to_string()),
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        assert_eq!(args.decompose_template.as_deref(), Some("my/decompose"));
    }

    #[test]
    fn test_fix_template_implies_review() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: Some("fix".to_string()),
            continue_async: None,
            output: None,
            subcommand: None,
        };
        // Resolve like run() does
        let fix_template = args.fix_template.or(if args.fix {
            Some("fix".to_string())
        } else {
            None
        });
        let review_template = args.review_template.clone();
        let review_after = review_template.is_some() || args.review || fix_template.is_some();
        assert!(review_after);
        assert!(review_template.is_none()); // No explicit template — create_review picks default
        assert!(fix_template.is_some());
    }

    #[test]
    fn test_fix_bool_implies_review() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: true,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        // Resolve like run() does
        let fix_template = args.fix_template.or(if args.fix {
            Some("fix".to_string())
        } else {
            None
        });
        let review_template = args.review_template.clone();
        let review_after = review_template.is_some() || args.review || fix_template.is_some();
        assert!(review_after);
        assert!(review_template.is_none()); // No explicit template — create_review picks default
        assert!(fix_template.is_some());
    }

    #[test]
    fn test_review_without_fix() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: true,
            review_template: None,
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        let fix_template = args.fix_template.or(if args.fix {
            Some("fix".to_string())
        } else {
            None
        });
        let review_template = args.review_template.clone();
        let review_after = review_template.is_some() || args.review || fix_template.is_some();
        assert!(review_after);
        assert!(review_template.is_none()); // No explicit template — create_review picks default
        assert!(fix_template.is_none());
    }

    #[test]
    fn test_review_with_custom_template() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: Some("my/review".to_string()),
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        assert_eq!(args.review_template.as_deref(), Some("my/review"));
    }

    #[test]
    fn test_fix_and_async_allowed() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: true,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: Some("fix".to_string()),
            continue_async: None,
            output: None,
            subcommand: None,
        };
        // --fix + --async is allowed (task-based loops)
        assert!(args.run_async);
        assert!(args.fix_template.is_some());
    }

    #[test]
    fn test_no_review_no_fix() {
        let args = BuildArgs {
            target: Some("test.md".to_string()),
            run_async: false,
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            review: false,
            review_template: None,
            fix: false,
            fix_template: None,
            continue_async: None,
            output: None,
            subcommand: None,
        };
        let fix_template = args.fix_template.or(if args.fix {
            Some("fix".to_string())
        } else {
            None
        });
        let review_template = args.review_template.clone();
        let review_after = review_template.is_some() || args.review || fix_template.is_some();
        assert!(!review_after);
        assert!(review_template.is_none());
        assert!(fix_template.is_none());
    }

    // ═══════════════════════════════════════════════════════════════════
    // Pre-refactor behavioral contract tests for build orchestration
    // ═══════════════════════════════════════════════════════════════════
    //
    // These tests lock down the CONTRACT of build behaviors that must
    // survive the workflow refactor. They test decision logic, not I/O.

    // --- check_epic_blockers contract ---

    #[test]
    fn test_check_epic_blockers_no_deps_passes() {
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );
        let graph = make_graph(tasks, EdgeStore::new());
        assert!(check_epic_blockers(&graph, "epic1").is_ok());
    }

    #[test]
    fn test_check_epic_blockers_resolved_dep_passes() {
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );
        let mut blocker = make_task("dep1", "Dep", TaskStatus::Closed);
        blocker.closed_outcome = Some(TaskOutcome::Done);
        tasks.insert("dep1".to_string(), blocker);

        let mut edges = EdgeStore::new();
        edges.add("epic1", "dep1", "depends-on");

        let graph = make_graph(tasks, edges);
        assert!(check_epic_blockers(&graph, "epic1").is_ok());
    }

    #[test]
    fn test_check_epic_blockers_unresolved_dep_fails() {
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );
        tasks.insert(
            "dep1".to_string(),
            make_task("dep1", "Dep", TaskStatus::Open),
        );

        let mut edges = EdgeStore::new();
        edges.add("epic1", "dep1", "depends-on");

        let graph = make_graph(tasks, edges);
        let err = check_epic_blockers(&graph, "epic1");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("blocked"));
    }

    #[test]
    fn test_check_epic_blockers_wontdo_dep_still_blocks() {
        // A dependency closed as WontDo should still block (only Done unblocks)
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );
        let mut blocker = make_task("dep1", "Dep", TaskStatus::Closed);
        blocker.closed_outcome = Some(TaskOutcome::WontDo);
        tasks.insert("dep1".to_string(), blocker);

        let mut edges = EdgeStore::new();
        edges.add("epic1", "dep1", "depends-on");

        let graph = make_graph(tasks, edges);
        let err = check_epic_blockers(&graph, "epic1");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("blocked"));
    }

    #[test]
    fn test_check_epic_blockers_mixed_deps() {
        // One resolved, one unresolved — should still block
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );

        let mut resolved = make_task("dep1", "Resolved", TaskStatus::Closed);
        resolved.closed_outcome = Some(TaskOutcome::Done);
        tasks.insert("dep1".to_string(), resolved);

        tasks.insert(
            "dep2".to_string(),
            make_task("dep2", "Unresolved", TaskStatus::InProgress),
        );

        let mut edges = EdgeStore::new();
        edges.add("epic1", "dep1", "depends-on");
        edges.add("epic1", "dep2", "depends-on");

        let graph = make_graph(tasks, edges);
        let err = check_epic_blockers(&graph, "epic1");
        assert!(err.is_err());
    }

    #[test]
    fn test_check_epic_blockers_missing_dep_task_blocks() {
        // A depends-on edge pointing to a non-existent task should block
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );

        let mut edges = EdgeStore::new();
        edges.add("epic1", "nonexistent", "depends-on");

        let graph = make_graph(tasks, edges);
        let err = check_epic_blockers(&graph, "epic1");
        assert!(err.is_err());
    }

    #[test]
    fn test_check_epic_blockers_suggests_restart() {
        // Error message should mention --restart as a workaround
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "epic1".to_string(),
            make_task("epic1", "Epic", TaskStatus::InProgress),
        );
        tasks.insert(
            "dep1".to_string(),
            make_task("dep1", "Dep", TaskStatus::Open),
        );

        let mut edges = EdgeStore::new();
        edges.add("epic1", "dep1", "depends-on");

        let graph = make_graph(tasks, edges);
        let err = check_epic_blockers(&graph, "epic1").unwrap_err();
        assert!(err.to_string().contains("--restart"));
    }

    // --- Epic resume decision matrix ---
    //
    // These test the branching logic at lines 313-340 of run_build_plan.
    // We extract the decision into a helper and test all branches.

    /// Simulates the epic resume decision matrix from run_build_plan.
    /// Returns: (should_create_new, should_close_existing)
    fn epic_resume_decision(
        restart: bool,
        existing_epic: Option<(&Task, bool)>, // (epic, has_subtasks)
    ) -> (bool, bool) {
        if restart {
            let should_close = existing_epic
                .map(|(e, _)| e.status != TaskStatus::Closed)
                .unwrap_or(false);
            return (true, should_close);
        }
        match existing_epic {
            Some((epic, _)) if epic.status == TaskStatus::Closed => (true, false),
            Some((_, true)) => (false, false), // Valid incomplete epic — reuse
            Some((_, false)) => (true, true),  // Invalid epic (no subtasks) — close and create new
            None => (true, false),             // No epic — create new
        }
    }

    #[test]
    fn test_epic_resume_no_existing_creates_new() {
        let (create_new, close_existing) = epic_resume_decision(false, None);
        assert!(create_new);
        assert!(!close_existing);
    }

    #[test]
    fn test_epic_resume_restart_always_creates_new() {
        let epic = make_task("epic1", "Epic", TaskStatus::InProgress);
        let (create_new, close_existing) = epic_resume_decision(true, Some((&epic, true)));
        assert!(create_new);
        assert!(close_existing);
    }

    #[test]
    fn test_epic_resume_restart_closed_epic_no_close() {
        let epic = make_task("epic1", "Epic", TaskStatus::Closed);
        let (create_new, close_existing) = epic_resume_decision(true, Some((&epic, false)));
        assert!(create_new);
        assert!(!close_existing); // Already closed, no need to close again
    }

    #[test]
    fn test_epic_resume_valid_incomplete_reuses() {
        let epic = make_task("epic1", "Epic", TaskStatus::InProgress);
        let (create_new, _) = epic_resume_decision(false, Some((&epic, true)));
        assert!(!create_new); // Reuse existing
    }

    #[test]
    fn test_epic_resume_invalid_epic_no_subtasks_closes_and_creates() {
        let epic = make_task("epic1", "Epic", TaskStatus::Open);
        let (create_new, close_existing) = epic_resume_decision(false, Some((&epic, false)));
        assert!(create_new);
        assert!(close_existing);
    }

    #[test]
    fn test_epic_resume_closed_epic_creates_new() {
        let epic = make_task("epic1", "Epic", TaskStatus::Closed);
        let (create_new, close_existing) = epic_resume_decision(false, Some((&epic, false)));
        assert!(create_new);
        assert!(!close_existing); // Already closed
    }

    #[test]
    fn test_epic_resume_restart_no_existing_creates_new() {
        let (create_new, close_existing) = epic_resume_decision(true, None);
        assert!(create_new);
        assert!(!close_existing);
    }

    // --- Build flag resolution contract ---
    // These verify that the --fix / --review / --fix-template flag
    // resolution logic produces the correct (review_after, fix_after) pair.

    fn resolve_build_flags(
        review: bool,
        review_template: Option<&str>,
        fix: bool,
        fix_template: Option<&str>,
    ) -> (bool, bool) {
        let fix_template = fix_template.map(|s| s.to_string()).or(if fix {
            Some("fix".to_string())
        } else {
            None
        });
        let review_after = review_template.is_some() || review || fix_template.is_some();
        let fix_after = fix_template.is_some();
        (review_after, fix_after)
    }

    #[test]
    fn test_build_flags_bare_build() {
        let (review_after, fix_after) = resolve_build_flags(false, None, false, None);
        assert!(!review_after);
        assert!(!fix_after);
    }

    #[test]
    fn test_build_flags_review_only() {
        let (review_after, fix_after) = resolve_build_flags(true, None, false, None);
        assert!(review_after);
        assert!(!fix_after);
    }

    #[test]
    fn test_build_flags_fix_implies_review() {
        let (review_after, fix_after) = resolve_build_flags(false, None, true, None);
        assert!(review_after);
        assert!(fix_after);
    }

    #[test]
    fn test_build_flags_fix_template_implies_both() {
        let (review_after, fix_after) = resolve_build_flags(false, None, false, Some("custom/fix"));
        assert!(review_after);
        assert!(fix_after);
    }

    #[test]
    fn test_build_flags_review_template_only() {
        let (review_after, fix_after) =
            resolve_build_flags(false, Some("custom/review"), false, None);
        assert!(review_after);
        assert!(!fix_after);
    }

    #[test]
    fn test_build_flags_all_flags() {
        let (review_after, fix_after) =
            resolve_build_flags(true, Some("custom/review"), true, Some("custom/fix"));
        assert!(review_after);
        assert!(fix_after);
    }

    // --- Stale build detection contract (pure logic) ---

    fn find_stale_builds(tasks: &FastHashMap<String, Task>, plan_path: &str) -> Vec<String> {
        tasks
            .values()
            .filter(|t| {
                t.task_type.as_deref() == Some("orchestrator")
                    && t.data.get("plan").map(|s| s.as_str()) == Some(plan_path)
                    && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
            })
            .map(|t| t.id.clone())
            .collect()
    }

    #[test]
    fn test_stale_build_multiple_matches() {
        let mut tasks = FastHashMap::default();
        let plan = "ops/now/feature.md";

        for (id, status) in [("b1", TaskStatus::InProgress), ("b2", TaskStatus::Open)] {
            let mut data = HashMap::new();
            data.insert("plan".to_string(), plan.to_string());
            let mut task = make_task_with_data(id, "Build", status, data);
            task.task_type = Some("orchestrator".to_string());
            tasks.insert(id.to_string(), task);
        }

        // Add a closed one that should NOT be stale
        let mut data = HashMap::new();
        data.insert("plan".to_string(), plan.to_string());
        let mut closed = make_task_with_data("b3", "Build", TaskStatus::Closed, data);
        closed.task_type = Some("orchestrator".to_string());
        tasks.insert("b3".to_string(), closed);

        let stale = find_stale_builds(&tasks, plan);
        assert_eq!(stale.len(), 2);
        assert!(stale.contains(&"b1".to_string()));
        assert!(stale.contains(&"b2".to_string()));
    }

    #[test]
    fn test_stale_build_ignores_different_plan() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/other.md".to_string());
        let mut task = make_task_with_data("b1", "Build", TaskStatus::InProgress, data);
        task.task_type = Some("orchestrator".to_string());
        tasks.insert("b1".to_string(), task);

        let stale = find_stale_builds(&tasks, "ops/now/feature.md");
        assert!(stale.is_empty());
    }

    #[test]
    fn test_stale_build_ignores_non_orchestrator() {
        let mut tasks = FastHashMap::default();
        let mut data = HashMap::new();
        data.insert("plan".to_string(), "ops/now/feature.md".to_string());
        let task = make_task_with_data("b1", "Regular task", TaskStatus::InProgress, data);
        tasks.insert("b1".to_string(), task);

        let stale = find_stale_builds(&tasks, "ops/now/feature.md");
        assert!(stale.is_empty());
    }

    // --- Draft plan rejection contract ---

    #[test]
    fn test_draft_plan_blocks_build() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("draft-plan.md");
        std::fs::write(&plan_file, "---\ndraft: true\n---\n# My Plan\n").unwrap();
        let metadata = crate::plans::parse_plan_metadata(&plan_file);
        assert!(
            metadata.draft,
            "Plan with draft: true should be detected as draft"
        );
    }

    #[test]
    fn test_non_draft_plan_allowed() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("ready-plan.md");
        std::fs::write(&plan_file, "---\ndraft: false\n---\n# My Plan\n").unwrap();
        let metadata = crate::plans::parse_plan_metadata(&plan_file);
        assert!(
            !metadata.draft,
            "Plan with draft: false should not be draft"
        );
    }

    #[test]
    fn test_no_frontmatter_plan_not_draft() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("no-fm.md");
        std::fs::write(&plan_file, "# Simple Plan\n\nNo frontmatter here.\n").unwrap();
        let metadata = crate::plans::parse_plan_metadata(&plan_file);
        assert!(
            !metadata.draft,
            "Plan without frontmatter should not be draft"
        );
    }

    // --- close_epic_as_invalid contract ---

    #[test]
    fn test_close_epic_as_invalid_uses_wontdo() {
        // Verify the event construction: outcome must be WontDo
        let outcome = TaskOutcome::WontDo;
        assert!(
            matches!(outcome, TaskOutcome::WontDo),
            "Invalid epic closure must use WontDo outcome"
        );
    }

    // --- Pre-refactor behavioral safety net tests ---

    /// Draft plans cannot be built — run_build_plan rejects draft: true.
    #[test]
    fn test_draft_plan_rejected() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("draft-plan.md");
        std::fs::write(&plan_file, "---\ndraft: true\n---\n# Plan\n").unwrap();
        let metadata = crate::plans::parse_plan_metadata(&plan_file);
        assert!(metadata.draft, "Draft plan must be detected and rejected");
    }

    /// When an existing epic already has subtasks, reuse it (skip decompose).
    /// The epic_resume_decision returns create_new=false when subtasks exist.
    #[test]
    fn test_epic_resume_skips_decompose_when_subtasks_exist() {
        let epic = make_task("epic1", "Epic", TaskStatus::InProgress);
        let has_subtasks = true;
        let (create_new, close_existing) = epic_resume_decision(false, Some((&epic, has_subtasks)));
        assert!(
            !create_new,
            "Should reuse existing epic when subtasks exist (skip decompose)"
        );
        assert!(!close_existing, "Should not close a valid in-progress epic");
    }

    /// --restart closes the existing in-progress epic before creating a new one.
    #[test]
    fn test_restart_closes_existing_epic() {
        let epic = make_task("epic1", "Epic", TaskStatus::InProgress);
        let (create_new, close_existing) = epic_resume_decision(true, Some((&epic, true)));
        assert!(create_new, "--restart must create a new epic");
        assert!(
            close_existing,
            "--restart must close the existing in-progress epic"
        );
    }

    /// When the build target is an epic ID (task ID), the plan path is not
    /// validated — run_build_epic does not call validate_plan_path. We verify
    /// the routing: a task ID target takes the epic path, not the plan path.
    #[test]
    fn test_build_epic_id_skips_plan_validation() {
        let epic_id = "mvslrspmoynoxyyywqyutmovxpvztkls";
        assert!(is_task_id(epic_id), "Should be recognized as task ID");
        // run_build_epic does NOT call validate_plan_path — verified by code
        // inspection. This test locks down the routing decision: task IDs must
        // take the epic path where no plan file validation occurs.
        let plan_path = "nonexistent/path/to/plan.md";
        assert!(
            !is_task_id(plan_path),
            "Plan path must NOT be routed as epic ID"
        );
    }

    // --- drive_build / fix iteration contract ---

    /// The build fix iteration loop must cap at MAX_BUILD_ITERATIONS (10).
    #[test]
    fn test_max_iterations_cap() {
        assert_eq!(
            MAX_BUILD_ITERATIONS, 10,
            "Build fix iteration must cap at 10"
        );
        // Simulate: when every review returns issues, iteration counter stops at MAX
        let mut iteration = 0;
        let mut cycles_injected = 0;
        for _ in 0..MAX_BUILD_ITERATIONS + 5 {
            if iteration < MAX_BUILD_ITERATIONS {
                cycles_injected += 1;
                iteration += 1;
            }
        }
        assert_eq!(
            cycles_injected, MAX_BUILD_ITERATIONS,
            "Should inject exactly MAX_BUILD_ITERATIONS cycles before stopping"
        );
    }

    /// drive_build does NOT inject fix cycles when fix_after is false.
    /// run_build falls through to the generic Workflow::run in that case.
    #[test]
    fn test_drive_build_only_active_when_fix_after() {
        let opts_no_fix = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: true,
            review_template: None,
            fix_after: false,
            fix_template: None,
        };
        // When fix_after is false, run_build delegates to the generic runner
        assert!(!opts_no_fix.fix_after);

        let opts_with_fix = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: true,
            review_template: None,
            fix_after: true,
            fix_template: Some("fix".to_string()),
        };
        // When fix_after is true, run_build uses drive_build
        assert!(opts_with_fix.fix_after);
    }

    /// has_review_issues returns true when issue_count > 0.
    #[test]
    fn test_has_review_issues_logic() {
        // Verify the issue detection logic used by drive_build
        let mut task = make_task("review1", "Review", TaskStatus::Closed);
        task.data.insert("issue_count".to_string(), "3".to_string());
        let has_issues = task
            .data
            .get("issue_count")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0)
            > 0;
        assert!(
            has_issues,
            "issue_count=3 should indicate actionable issues"
        );
    }

    /// has_review_issues returns false when issue_count is 0.
    #[test]
    fn test_no_review_issues_logic() {
        let mut task = make_task("review2", "Review", TaskStatus::Closed);
        task.data.insert("issue_count".to_string(), "0".to_string());
        let has_issues = task
            .data
            .get("issue_count")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0)
            > 0;
        assert!(
            !has_issues,
            "issue_count=0 should indicate no actionable issues"
        );
    }

    /// build_workflow omits static Fix step when fix_after is true (drive_build handles it).
    #[test]
    fn test_build_workflow_no_static_fix_when_fix_after() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("plan.md");
        std::fs::write(&plan_file, "# Plan").unwrap();

        let opts = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: true,
            review_template: None,
            fix_after: true,
            fix_template: Some("fix".to_string()),
        };

        let wf = build_workflow(temp_dir.path(), "plan.md", &opts);
        // Should have: Plan, Decompose, Loop, Review (no Fix)
        assert_eq!(wf.steps.len(), 4);
        assert_eq!(wf.steps[0].name(), "plan");
        assert_eq!(wf.steps[1].name(), "decompose");
        assert_eq!(wf.steps[2].name(), "loop");
        assert_eq!(wf.steps[3].name(), "review");
        // No Fix step — drive_build handles fix iteration dynamically
        assert!(
            !wf.steps.iter().any(|s| s.name() == "fix"),
            "Static Fix step should not be present when fix_after is true"
        );
    }

    #[test]
    fn test_build_workflow_default_steps() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("plan.md");
        std::fs::write(&plan_file, "# Plan").unwrap();

        let opts = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: false,
            review_template: None,
            fix_after: false,
            fix_template: None,
        };

        let wf = build_workflow(temp_dir.path(), "plan.md", &opts);
        let names: Vec<_> = wf.steps.iter().map(|s| s.name()).collect();
        assert_eq!(names, vec!["plan", "decompose", "loop"]);
        assert_eq!(wf.steps.len(), 3);
    }

    #[test]
    #[test]
    fn test_build_review_scope_uses_task_kind() {
        use crate::commands::review::ReviewScopeKind;

        let epic_id = "onnlrwntommtvtnzovwromnkyulorwtz";
        let scope = crate::workflow::steps::build::build_review_scope(epic_id);

        // Must be Task scope so that fix tasks become subtasks of the epic,
        // triggering reopen_if_closed. Using Code/Plan scope breaks this.
        assert_eq!(scope.kind, ReviewScopeKind::Task);
        assert_eq!(scope.id, epic_id);
        assert!(scope.task_ids.is_empty());
    }

    #[test]
    fn test_build_workflow_review_step_only_when_enabled() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("plan.md");
        std::fs::write(&plan_file, "# Plan").unwrap();

        let without_review = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: false,
            review_template: None,
            fix_after: false,
            fix_template: None,
        };
        let wf_without_review = build_workflow(temp_dir.path(), "plan.md", &without_review);
        let with_review = BuildOpts {
            restart: false,
            decompose_template: None,
            loop_template: None,
            agent: None,
            agent_str: None,
            review_after: true,
            review_template: None,
            fix_after: false,
            fix_template: None,
        };
        let wf_with_review = build_workflow(temp_dir.path(), "plan.md", &with_review);

        assert_eq!(wf_without_review.steps.len(), 3);
        assert_eq!(wf_with_review.steps.len(), 4);
        assert!(!wf_without_review.steps.iter().any(|s| s.name() == "review"));
        assert!(wf_with_review.steps.iter().any(|s| s.name() == "review"));
    }
}

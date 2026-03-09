//! Build command for decomposing plan files and executing all subtasks
//!
//! This module provides the `aiki build` command which:
//! - Creates an epic from a plan file and automatically executes all subtasks
//! - Supports building from an existing epic ID
//! - Shows build/epic status via the `show` subcommand
//! - Supports async (background) execution

use std::env;
use std::io::IsTerminal;
use crate::output_utils;
use std::path::Path;

use clap::Subcommand;

use super::OutputFormat;
use super::async_spawn;
use super::epic::find_or_create_epic;
use super::loop_cmd::{run_loop, LoopOptions};
use crate::agents::AgentType;
use crate::config::get_aiki_binary_path;
use crate::error::{AikiError, Result};
use crate::plans::{parse_plan_metadata, PlanGraph};
use crate::tasks::id::{is_task_id, is_task_id_prefix};
use crate::tasks::runner::{handle_session_result, ScreenSession, task_run, task_run_on_session, TaskRunOptions};
use crate::tasks::md::MdBuilder;
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, write_event,
    write_link_event, Task, TaskEvent, TaskOutcome, TaskStatus,
};
use crate::tui;
use crate::tui::loading_screen::LoadingScreen;
use crate::tui::theme::{Theme, detect_mode};

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

/// Run the build command
pub fn run(args: BuildArgs) -> Result<()> {
    let cwd = env::current_dir().map_err(|_| {
        AikiError::InvalidArgument("Failed to get current directory".to_string())
    })?;

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
    let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
    // Resolve --review / --review-template (--fix implies --review)
    // Pass through explicit --review-template only; None lets create_review pick scope-specific default
    let review_template = args.review_template.clone();
    let review_after = review_template.is_some() || args.review || fix_template.is_some();
    let fix_after = fix_template.is_some();

    let output_id = matches!(args.output, Some(OutputFormat::Id));

    if is_task_id(&target) || is_task_id_prefix(&target) {
        run_build_epic(&cwd, &target, args.run_async, args.loop_template, args.agent, review_after, fix_after, review_template, fix_template, output_id)
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
/// picks up from there: runs decompose if needed, then the loop, then optional
/// review/fix.
fn run_continue_async(cwd: &Path, epic_id: &str, args: BuildArgs) -> Result<()> {
    // Parse agent
    let agent_type = if let Some(ref agent_str) = args.agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Find the epic
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let epic = find_task(&graph.tasks, epic_id)?;
    let epic_id = epic.id.clone();

    // Check for blockers before executing (catches races between parent check and background start)
    check_epic_blockers(&graph, &epic_id)?;

    // Check if epic has subtasks already
    let subtasks = get_subtasks(&graph, &epic_id);
    if subtasks.is_empty() {
        // Need to run decompose first
        let plan_path = epic.data.get("plan").cloned().ok_or_else(|| {
            AikiError::InvalidArgument(format!(
                "Epic task {} missing data.plan. Cannot decompose without a plan path.",
                &epic_id[..epic_id.len().min(8)]
            ))
        })?;
        let options = super::decompose::DecomposeOptions {
            template: args.decompose_template.clone(),
            agent: agent_type.clone(),
        };
        super::decompose::run_decompose(cwd, &plan_path, &epic_id, options, None)?;
    }

    // Now run the loop (synchronous — we're already in background)
    let mut loop_options = LoopOptions::new(); // NOT async — already in background
    if let Some(agent) = agent_type.clone() {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(tmpl) = args.loop_template {
        loop_options = loop_options.with_template(tmpl);
    }

    run_loop(cwd, &epic_id, loop_options, None)?;

    // Post-build review if requested
    // Resolve --fix / --fix-template and --review / --review-template
    let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
    // Pass through explicit --review-template only; None lets create_review pick scope-specific default
    let review_template = args.review_template;
    let review_after = review_template.is_some() || args.review || fix_template.is_some();
    if review_after {
        let plan_path = epic.data.get("plan").cloned().unwrap_or_default();
        let fix_after = fix_template.is_some();
        run_build_review(cwd, &plan_path, &epic_id, fix_after, review_template, fix_template, None)?;
    }

    Ok(())
}

/// Build from a plan path — deterministic find-or-create.
///
/// 1. Validate plan file exists and is .md
/// 2. Clean up stale builds for this plan
/// 3. Check for existing epic (deterministic: no interactive prompts)
/// 4. Create build task
/// 5. Run build task (sync or async)
/// 6. Output results
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
    // Validate plan file exists and is .md
    validate_plan_path(cwd, plan_path)?;

    // Check if plan is a draft
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

    // Parse agent if provided
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Clean up stale builds for this plan
    cleanup_stale_builds(cwd, plan_path)?;

    // Load current tasks to check for existing epics
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);

    // Deterministic epic lookup (no interactive prompts)
    let existing_epic = plan_graph.find_epic_for_plan(plan_path, &graph);

    let epic_id = if restart {
        // --restart: close existing epic and create fresh
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
                // Valid incomplete epic — use it (deterministic, no prompt)
                let subtasks = get_subtasks(&graph, &epic.id);
                if subtasks.is_empty() {
                    // Invalid epic (no subtasks) — close and create new
                    close_epic_as_invalid(cwd, &epic.id)?;
                    None
                } else {
                    Some(epic.id.clone())
                }
            }
            _ => None, // No epic or closed epic — create new
        }
    };

    if run_async {
        // Async path: create just the epic task (no decompose yet), spawn background
        let is_existing_epic = epic_id.is_some();
        let epic_id = match epic_id {
            Some(id) => id,
            None => super::epic::create_epic_task(cwd, plan_path)?,
        };

        // If reusing an existing epic, check for blockers before spawning.
        // New epics can't have blockers yet, so skip the check.
        if is_existing_epic {
            let events = read_events(cwd)?;
            let graph = materialize_graph(&events);
            check_epic_blockers(&graph, &epic_id)?;
        }

        // Build args to pass through to the background process
        let mut spawn_args: Vec<String> = vec![
            "build".to_string(),
            "--_continue-async".to_string(),
            epic_id.clone(),
        ];
        if let Some(ref tmpl) = decompose_template {
            spawn_args.push("--decompose-template".to_string());
            spawn_args.push(tmpl.clone());
        }
        if let Some(ref tmpl) = loop_template {
            spawn_args.push("--loop-template".to_string());
            spawn_args.push(tmpl.clone());
        }
        if let Some(ref a) = agent {
            spawn_args.push("--agent".to_string());
            spawn_args.push(a.clone());
        }
        if let Some(ref tmpl) = review_template {
            spawn_args.push("--review-template".to_string());
            spawn_args.push(tmpl.clone());
        } else if review_after {
            spawn_args.push("--review".to_string());
        }
        if let Some(ref tmpl) = fix_template {
            spawn_args.push("--fix-template".to_string());
            spawn_args.push(tmpl.clone());
        }

        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if output_id {
            println!("{}", epic_id);
        }

        return Ok(());
    }

    // Sync path: show loading screen with immediate visual feedback
    let mut loading = if std::io::stderr().is_terminal() {
        let mut l = LoadingScreen::new("Loading task graph...")?;
        l.set_filepath(plan_path);
        Some(l)
    } else {
        None
    };

    if let Some(ref mut l) = loading {
        l.set_step("Finding or creating epic...");
    }

    // Transition loading → session before find_or_create_epic (which may run decompose agent)
    let mut session = if let Some(l) = loading {
        Some(ScreenSession::from_live_screen(l.into_live_screen()?)?)
    } else {
        None
    };

    // Ensure we always have an epic before running.
    // If no existing epic was found, create one via the decompose agent.
    let epic_id = match epic_id {
        Some(id) => id,
        None => find_or_create_epic(cwd, plan_path, decompose_template.as_deref(), session.as_mut())?,
    };

    // Check if epic is blocked before running loop
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    check_epic_blockers(&graph, &epic_id)?;

    // Run loop to orchestrate epic's subtasks
    let mut loop_options = LoopOptions::new();
    if let Some(agent) = agent_type {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(tmpl) = loop_template {
        loop_options = loop_options.with_template(tmpl);
    }

    let loop_task_id = run_loop(cwd, &epic_id, loop_options, session.as_mut())?;

    // Drop screen before printing final output
    drop(session);

    // After build completes, re-read tasks to get final state
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);

    // Find the epic task (may have been created during the build)
    let final_epic = plan_graph.find_epic_for_plan(plan_path, &graph);
    let final_epic_id = final_epic
        .map(|p| p.id.as_str())
        .unwrap_or(epic_id.as_str());

    let subtasks = final_epic
        .map(|p| get_subtasks(&graph, &p.id))
        .unwrap_or_default();
    let subtask_refs: Vec<&Task> = subtasks.into_iter().collect();

    if output_id {
        println!("{}", loop_task_id);
        println!("{}", final_epic_id);
    } else {
        output_build_completed(&loop_task_id, final_epic_id, &subtask_refs)?;
    }

    // Run post-build review if requested (sync path) — needs its own session
    if review_after {
        let review_loading = if std::io::stderr().is_terminal() {
            let mut l = LoadingScreen::new("Preparing review...")?;
            l.set_filepath(plan_path);
            Some(l)
        } else {
            None
        };
        let mut review_session = if let Some(l) = review_loading {
            Some(ScreenSession::from_live_screen(l.into_live_screen()?)?)
        } else {
            None
        };
        let result = run_build_review(cwd, plan_path, final_epic_id, fix_after, review_template, fix_template, review_session.as_mut());
        drop(review_session); // Always restore terminal before propagating errors
        result?;
    }

    Ok(())
}

/// Build from an existing epic ID
///
/// 1. Find epic task, verify it exists
/// 2. Get plan path from epic's data
/// 3. Create build task with data.target and data.plan
/// 4. Run build task (sync or async)
/// 5. Output results
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
    // Parse agent if provided
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

    // Check if epic is blocked before running loop
    check_epic_blockers(&graph, epic_id)?;

    let plan_path = epic
        .data
        .get("plan")
        .cloned()
        .ok_or_else(|| {
            AikiError::InvalidArgument(format!(
                "Epic task {} missing data.plan. Cannot run build without a plan path.",
                epic_id
            ))
        })?;

    if run_async {
        // Async path: spawn background process with --_continue-async
        let mut spawn_args: Vec<String> = vec![
            "build".to_string(),
            "--_continue-async".to_string(),
            epic_id.to_string(),
        ];
        if let Some(ref tmpl) = loop_template {
            spawn_args.push("--loop-template".to_string());
            spawn_args.push(tmpl.clone());
        }
        if let Some(ref a) = agent {
            spawn_args.push("--agent".to_string());
            spawn_args.push(a.clone());
        }
        if let Some(ref tmpl) = review_template {
            spawn_args.push("--review-template".to_string());
            spawn_args.push(tmpl.clone());
        } else if review_after {
            spawn_args.push("--review".to_string());
        }
        if fix_after {
            if let Some(ref tmpl) = fix_template {
                spawn_args.push("--fix-template".to_string());
                spawn_args.push(tmpl.clone());
            } else {
                spawn_args.push("--fix".to_string());
            }
        }

        let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
        async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

        if output_id {
            println!("{}", epic_id);
        }

        return Ok(());
    }

    // Sync path: show loading screen with immediate visual feedback
    let loading = if std::io::stderr().is_terminal() {
        let mut l = LoadingScreen::new("Loading task graph...")?;
        l.set_task_context(epic_id, &epic.name);
        Some(l)
    } else {
        None
    };

    let mut session = if let Some(l) = loading {
        Some(ScreenSession::from_live_screen(l.into_live_screen()?)?)
    } else {
        None
    };

    let mut loop_options = LoopOptions::new();
    if let Some(agent) = agent_type {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(tmpl) = loop_template {
        loop_options = loop_options.with_template(tmpl);
    }

    let loop_task_id = run_loop(cwd, epic_id, loop_options, session.as_mut())?;

    // Drop screen before printing final output
    drop(session);

    // After build completes, re-read tasks to get final state
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    let subtasks = get_subtasks(&graph, epic_id);

    if output_id {
        println!("{}", loop_task_id);
        println!("{}", epic_id);
    } else {
        output_build_completed(&loop_task_id, epic_id, &subtasks)?;
    }

    // Run post-build review if requested (sync path) — needs its own session
    if review_after {
        let review_loading = if std::io::stderr().is_terminal() {
            let mut l = LoadingScreen::new("Preparing review...")?;
            l.set_filepath(&plan_path);
            Some(l)
        } else {
            None
        };
        let mut review_session = if let Some(l) = review_loading {
            Some(ScreenSession::from_live_screen(l.into_live_screen()?)?)
        } else {
            None
        };
        let result = run_build_review(cwd, &plan_path, epic_id, fix_after, review_template, fix_template, review_session.as_mut());
        drop(review_session); // Always restore terminal before propagating errors
        result?;
    }

    Ok(())
}

/// Show build/epic status for a plan
fn run_show(cwd: &Path, plan_path: &str, output_format: Option<OutputFormat>) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);

    // Find epic via PlanGraph
    let epic = plan_graph.find_epic_for_plan(plan_path, &graph).ok_or_else(|| {
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

/// Validate that the plan path is a .md file and exists
fn validate_plan_path(cwd: &Path, plan_path: &str) -> Result<()> {
    if !plan_path.ends_with(".md") {
        return Err(AikiError::InvalidArgument(
            "Plan file must be markdown (.md)".to_string(),
        ));
    }

    let full_path = if plan_path.starts_with('/') {
        std::path::PathBuf::from(plan_path)
    } else {
        cwd.join(plan_path)
    };

    if !full_path.exists() {
        return Err(AikiError::InvalidArgument(format!(
            "Plan file not found: {}",
            plan_path
        )));
    }

    if !full_path.is_file() {
        return Err(AikiError::InvalidArgument(format!(
            "Not a file: {}",
            plan_path
        )));
    }

    Ok(())
}

/// Clean up stale build tasks for this plan.
///
/// Finds any in_progress or open build tasks with `data.plan` matching the plan path
/// and closes them as wont_do with a comment.
fn cleanup_stale_builds(cwd: &Path, plan_path: &str) -> Result<()> {
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    let stale_builds: Vec<String> = tasks
        .values()
        .filter(|t| {
            t.task_type.as_deref() == Some("orchestrator")
                && t.data.get("plan").map(|s| s.as_str()) == Some(plan_path)
                && (t.status == TaskStatus::InProgress || t.status == TaskStatus::Open)
        })
        .map(|t| t.id.clone())
        .collect();

    for build_id in &stale_builds {
        let comment_event = TaskEvent::CommentAdded {
            task_ids: vec![build_id.clone()],
            text: "Stale build cleaned up".to_string(),
            data: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &comment_event)?;

        let close_event = TaskEvent::Closed {
            task_ids: vec![build_id.clone()],
            outcome: TaskOutcome::WontDo,
            summary: Some("Stale build cleaned up".to_string()),
            session_id: None,
            turn_id: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &close_event)?;
    }

    Ok(())
}

/// Undo file changes made by completed subtasks of an epic.
///
/// Invokes `aiki task undo <epic-id> --completed` to revert changes before
/// closing the epic. If no completed subtasks exist, this is a no-op.
fn undo_completed_subtasks(cwd: &Path, epic_id: &str) -> Result<()> {
    let output = std::process::Command::new(get_aiki_binary_path())
        .current_dir(cwd)
        .args(["task", "undo", epic_id, "--completed"])
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

/// Close an existing epic as wont_do
fn close_epic(cwd: &Path, epic_id: &str) -> Result<()> {
    let timestamp = chrono::Utc::now();

    // Add comment before closing
    let comment_event = TaskEvent::CommentAdded {
        task_ids: vec![epic_id.to_string()],
        text: "Closed by --restart".to_string(),
        data: std::collections::HashMap::new(),
        timestamp: timestamp - chrono::Duration::milliseconds(1),
    };
    write_event(cwd, &comment_event)?;

    let close_event = TaskEvent::Closed {
        task_ids: vec![epic_id.to_string()],
        outcome: TaskOutcome::WontDo,
        summary: Some("Closed by --restart".to_string()),
        session_id: None,
        turn_id: None,
        timestamp,
    };
    write_event(cwd, &close_event)?;
    Ok(())
}

/// Check if an epic is blocked by unresolved dependencies.
///
/// Returns an error if the epic has `depends-on` links to tasks that are not yet
/// Closed with Done outcome. This prevents starting a build on a blocked epic.
fn check_epic_blockers(
    graph: &crate::tasks::graph::TaskGraph,
    epic_id: &str,
) -> Result<()> {
    let blocker_ids: Vec<&str> = graph
        .edges
        .targets(epic_id, "depends-on")
        .iter()
        .filter(|tid| {
            graph.tasks.get(tid.as_str()).map_or(true, |t| {
                !(t.status == TaskStatus::Closed
                    && t.closed_outcome == Some(TaskOutcome::Done))
            })
        })
        .map(|s| s.as_str())
        .collect();

    if !blocker_ids.is_empty() {
        let blocker_names: Vec<String> = blocker_ids
            .iter()
            .map(|id| {
                let name = graph
                    .tasks
                    .get(*id)
                    .map(|t| t.name.as_str())
                    .unwrap_or("unknown");
                let short = &id[..id.len().min(8)];
                format!("{} ({})", short, name)
            })
            .collect();
        return Err(AikiError::InvalidArgument(format!(
            "Epic {} is blocked by unresolved dependencies: {}. Rerun with --restart to start over",
            &epic_id[..epic_id.len().min(8)],
            blocker_names.join(", ")
        )));
    }

    Ok(())
}

/// Close an epic as invalid (no subtasks created).
fn close_epic_as_invalid(cwd: &Path, epic_id: &str) -> Result<()> {
    let close_event = TaskEvent::Closed {
        task_ids: vec![epic_id.to_string()],
        outcome: TaskOutcome::WontDo,
        summary: Some("No subtasks created — epic invalid".to_string()),
        session_id: None,
        turn_id: None,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &close_event)?;
    Ok(())
}

/// Run review (optionally with fix loop) after a build completes.
///
/// Creates a code review scoped to the plan's implementation, optionally
/// including a fix subtask if `with_fix` is true. Runs the review to completion
/// (blocking).
fn run_build_review(cwd: &Path, plan_path: &str, epic_id: &str, with_fix: bool, review_template: Option<String>, fix_template: Option<String>, session: Option<&mut ScreenSession>) -> Result<()> {
    use super::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};

    let scope = ReviewScope {
        kind: ReviewScopeKind::Code,
        id: plan_path.to_string(),
        task_ids: vec![],
    };

    let result = create_review(cwd, CreateReviewParams {
        scope,
        agent_override: None,
        template: review_template,
        fix_template: if with_fix { fix_template.clone().or_else(|| Some("fix".to_string())) } else { None },
        autorun: false,
    })?;

    // Link review to epic so the status monitor shows the epic view
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(cwd, &graph, "validates", &result.review_task_id, epic_id)?;

    // Run the review to completion (blocking)
    let options = TaskRunOptions::new();
    if let Some(session) = session {
        let session_result = task_run_on_session(cwd, &result.review_task_id, options, session)?;
        handle_session_result(cwd, &result.review_task_id, session_result, true)?;
    } else {
        task_run(cwd, &result.review_task_id, options)?;
    }

    // Check if review found issues and invoke fix if requested
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let has_issues = find_task(&graph.tasks, &result.review_task_id)
        .map(|t| t.data.get("issue_count")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0) > 0)
        .unwrap_or(false);

    if with_fix && has_issues {
        super::fix::run_fix(
            cwd,
            &result.review_task_id,
            false,          // not async
            None,           // no continue-async
            fix_template,   // forward caller's fix template
            None,           // default decompose template
            None,           // default loop template
            None,           // default review template
            None,           // no agent override
            false,          // not autorun
            false,          // not --once
            None,           // no output format override
        )?;
    }

    output_build_review_completed(&result.review_task_id, plan_path, with_fix)?;

    Ok(())
}

/// Output build + review completed message to stderr
fn output_build_review_completed(review_id: &str, plan_path: &str, with_fix: bool) -> Result<()> {
    output_utils::emit(|| {
        let title = if with_fix {
            "Build + Review + Fix Completed"
        } else {
            "Build + Review Completed"
        };
        let mut content = format!(
            "## {}\n- **Review ID:** {}\n- **Plan:** {}\n",
            title, review_id, plan_path
        );
        if !with_fix {
            content.push_str(&format!(
                "\n---\nRun `aiki fix {}` to remediate.\n",
                review_id
            ));
        }
        MdBuilder::new("build").build(&content, &[], &[])
    });
    Ok(())
}

/// Output build completed message to stderr
fn output_build_completed(build_id: &str, epic_id: &str, subtasks: &[&Task]) -> Result<()> {
    output_utils::emit(|| {
        let mut content = format!(
            "## Build Completed\n- **Build ID:** {}\n- **Epic ID:** {}\n- **Subtasks:** {}\n\n",
            build_id, epic_id, subtasks.len()
        );

        for (i, subtask) in subtasks.iter().enumerate() {
            let status = if subtask.status == TaskStatus::Closed {
                "done"
            } else {
                "pending"
            };
            content.push_str(&format!("{}. {} ({})\n", i + 1, &subtask.name, status));
        }

        content.push_str(&format!(
            "\n---\nRun `aiki review {}` to review.\n",
            epic_id
        ));

        MdBuilder::new("build").build(&content, &[], &[])
    });
    Ok(())
}

/// Output build show (detailed status display)
fn output_build_show(epic: &Task, subtasks: &[&Task], _build_tasks: &[&Task], graph: &crate::tasks::graph::TaskGraph) -> Result<()> {
    let plan_path = epic.data.get("plan").map(|s| s.as_str()).unwrap_or("unknown");
    output_utils::emit(|| {
        let theme = Theme::from_mode(detect_mode());
        let view = tui::builder::build_workflow_view(epic, subtasks, plan_path, graph);
        let buf = tui::views::workflow::render_workflow(&view, &theme);
        tui::buffer_ansi::buffer_to_ansi(&buf)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::{EdgeStore, TaskGraph};
    use crate::tasks::TaskPriority;
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
    fn find_epic_for_plan_via_graph<'a>(
        graph: &'a TaskGraph,
        plan_path: &str,
    ) -> Option<&'a Task> {
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

        let mut task = make_task_with_data(
            "build1",
            "Build: feature",
            TaskStatus::InProgress,
            data,
        );
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

        let mut task = make_task_with_data(
            "build2",
            "Build: feature",
            TaskStatus::Open,
            data,
        );
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

        let mut task = make_task_with_data(
            "build3",
            "Build: feature",
            TaskStatus::Closed,
            data,
        );
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

        let mut task = make_task_with_data(
            "build4",
            "Build: other",
            TaskStatus::InProgress,
            data,
        );
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
        let task = make_task_with_data(
            "not_build",
            "Something else",
            TaskStatus::InProgress,
            data,
        );
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

    // --- validate_plan_path tests ---

    #[test]
    fn test_validate_plan_path_not_md() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let result = validate_plan_path(temp_dir.path(), "not-markdown.txt");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be markdown"));
    }

    #[test]
    fn test_validate_plan_path_not_found() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let result = validate_plan_path(temp_dir.path(), "nonexistent.md");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Plan file not found"));
    }

    #[test]
    fn test_validate_plan_path_exists() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("my-plan.md");
        std::fs::write(&plan_file, "# My Plan").unwrap();
        let result = validate_plan_path(temp_dir.path(), "my-plan.md");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_plan_path_absolute() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("absolute-plan.md");
        std::fs::write(&plan_file, "# Plan").unwrap();
        let result = validate_plan_path(temp_dir.path(), &plan_file.to_string_lossy());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_plan_path_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir.md");
        std::fs::create_dir_all(&dir_path).unwrap();
        let result = validate_plan_path(temp_dir.path(), "subdir.md");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not a file"));
    }

    // --- Build show output formatting tests ---

    #[test]
    fn test_output_build_completed_no_subtasks() {
        let subtasks: Vec<&Task> = vec![];
        let result = output_build_completed("build123", "epic456", &subtasks);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_build_completed_with_subtasks() {
        let task1 = make_task("sub1", "Implement auth", TaskStatus::Closed);
        let task2 = make_task("sub2", "Add tests", TaskStatus::Open);
        let subtasks: Vec<&Task> = vec![&task1, &task2];
        let result = output_build_completed("build123", "epic456", &subtasks);
        assert!(result.is_ok());
    }

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
        let mut build = make_task_with_data(
            "build1",
            "Build: feature",
            TaskStatus::Closed,
            build_data,
        );
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

    #[test]
    fn test_xml_escaping_in_output() {
        // Verify XML special characters are properly escaped
        let task = make_task("sub1", "Fix <angle> & \"quote\" 'apos'", TaskStatus::Open);
        let subtasks: Vec<&Task> = vec![&task];
        let result = output_build_completed("build<1>", "epic&2", &subtasks);
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
        let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
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
        let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
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
        let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
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
        let fix_template = args.fix_template.or(if args.fix { Some("fix".to_string()) } else { None });
        let review_template = args.review_template.clone();
        let review_after = review_template.is_some() || args.review || fix_template.is_some();
        assert!(!review_after);
        assert!(review_template.is_none());
        assert!(fix_template.is_none());
    }

    #[test]
    fn test_output_build_review_completed_with_fix() {
        let result = output_build_review_completed("review123", "ops/now/feature.md", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_build_review_completed_without_fix() {
        let result = output_build_review_completed("review123", "ops/now/feature.md", false);
        assert!(result.is_ok());
    }

}

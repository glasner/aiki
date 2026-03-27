//! Build workflow step handlers.
//!
//! These functions implement the individual steps of a build workflow:
//! plan validation, epic decomposition, loop execution, review, and fix.
//! Extracted from `commands/build.rs` to match the pattern of other
//! step modules under `workflow/steps/`.

use std::path::Path;

use crate::agents::AgentType;
use crate::commands::loop_cmd::{run_loop, LoopOptions};
use crate::commands::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};
use crate::commands::OutputFormat;
use crate::error::AikiError;
use crate::plans::{parse_plan_metadata, PlanGraph};
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, write_link_event, Task,
};
use crate::workflow::{StepResult, WorkflowContext};

use super::decompose::{
    check_epic_blockers, close_epic, close_epic_as_invalid, create_epic_task, restart_epic,
    undo_completed_subtasks,
};
use super::plan::{cleanup_stale_builds, validate_plan_path};

/// Plan step: validate plan path, check draft status, clean up stale builds.
pub(crate) fn run_plan_step(ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
    let plan_path = ctx
        .plan_path
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No plan path in workflow context".to_string()))?
        .clone();

    validate_plan_path(&ctx.cwd, &plan_path)?;

    let full_path = if plan_path.starts_with('/') {
        std::path::PathBuf::from(&plan_path)
    } else {
        ctx.cwd.join(&plan_path)
    };
    let metadata = parse_plan_metadata(&full_path);
    if metadata.draft {
        return Err(AikiError::InvalidArgument(
            "Cannot build draft plan. Remove `draft: true` from frontmatter first.".to_string(),
        )
        .into());
    }

    cleanup_stale_builds(&ctx.cwd, &plan_path)?;

    Ok(StepResult {
        message: "Plan validated".to_string(),
        task_id: None,
    })
}

/// Decompose step: find/create epic, check blockers, run decompose if needed.
pub(crate) fn run_decompose_step(
    ctx: &mut WorkflowContext,
    restart: bool,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let plan_path = ctx
        .plan_path
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No plan path in workflow context".to_string()))?
        .clone();

    // If no epic in context, find or create one
    if ctx.task_id.is_none() {
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let plan_graph = PlanGraph::build(&graph);
        let existing_epic = plan_graph.find_epic_for_plan(&plan_path, &graph);

        let epic_id = if restart {
            if let Some(epic) = existing_epic {
                if epic.status != crate::tasks::TaskStatus::Closed {
                    undo_completed_subtasks(&ctx.cwd, &epic.id)?;
                    close_epic(&ctx.cwd, &epic.id)?;
                }
            }
            None
        } else {
            match existing_epic {
                Some(epic) if epic.status != crate::tasks::TaskStatus::Closed => {
                    let subtasks = get_subtasks(&graph, &epic.id);
                    if subtasks.is_empty() {
                        close_epic_as_invalid(&ctx.cwd, &epic.id)?;
                        None
                    } else {
                        restart_epic(&ctx.cwd, &epic.id)?;
                        Some(epic.id.clone())
                    }
                }
                _ => None,
            }
        };

        let epic_id = match epic_id {
            Some(id) => id,
            None => create_epic_task(&ctx.cwd, &plan_path)?,
        };

        ctx.task_id = Some(epic_id);
    }

    let epic_id = ctx.task_id.as_ref().unwrap().clone();

    // Check blockers
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    check_epic_blockers(&graph, &epic_id)?;

    // Run decompose if no subtasks exist
    let subtasks = get_subtasks(&graph, &epic_id);
    if subtasks.is_empty() {
        let options = crate::commands::decompose::DecomposeOptions { template, agent };
        let decompose_task_id =
            crate::commands::decompose::run_decompose(&ctx.cwd, &plan_path, &epic_id, options, false)?;

        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let count = get_subtasks(&graph, &epic_id).len();

        Ok(StepResult {
            message: format!("{} subtasks created", count),
            task_id: Some(decompose_task_id),
        })
    } else {
        Ok(StepResult {
            message: "Epic resumed (subtasks already exist)".to_string(),
            task_id: Some(epic_id),
        })
    }
}

/// Loop step: run the orchestration loop over epic subtasks.
pub(crate) fn run_loop_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let epic_id = ctx
        .task_id
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No epic ID in workflow context".to_string()))?
        .clone();

    let mut loop_options = LoopOptions::new();
    if let Some(agent) = agent {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(tmpl) = template {
        loop_options = loop_options.with_template(tmpl);
    }

    let loop_task_id = run_loop(&ctx.cwd, &epic_id, loop_options, false)?;

    Ok(StepResult {
        message: "All lanes complete".to_string(),
        task_id: Some(loop_task_id),
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
    write_link_event(
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

/// Fix step: check if review found issues and run fix if so.
pub(crate) fn run_fix_step(
    ctx: &mut WorkflowContext,
    review_id: &str,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let epic_id = ctx
        .task_id
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No epic ID in workflow context".to_string()))?
        .clone();

    // Resolve review_id: use provided or find from graph
    let review_id = if review_id.is_empty() {
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let reviews = graph.edges.referrers(&epic_id, "validates");
        reviews
            .last()
            .ok_or_else(|| AikiError::InvalidArgument("No review found to fix".to_string()))?
            .clone()
    } else {
        review_id.to_string()
    };

    // Check if review found issues
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    let has_issues = find_task(&graph.tasks, &review_id)
        .map(|t| {
            t.data
                .get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0)
                > 0
        })
        .unwrap_or(false);

    if !has_issues {
        return Ok(StepResult {
            message: "No issues to fix".to_string(),
            task_id: None,
        });
    }

    crate::commands::fix::run_fix(
        &ctx.cwd,
        &review_id,
        false,                         // not async
        None,                          // no continue-async
        template,                      // forward caller's fix template
        None,                          // default decompose template
        None,                          // default loop template
        None,                          // default review template
        agent,                         // forward agent override
        false,                         // not autorun
        false,                         // not --once
        Some(OutputFormat::Id),        // prevent nested TUI in worker thread
    )?;

    Ok(StepResult {
        message: "Fix complete".to_string(),
        task_id: Some(review_id),
    })
}

/// Post-workflow output: re-read graph and display build status.
pub(crate) fn output_after_workflow(
    cwd: &Path,
    plan_path: &str,
    output_id: bool,
) -> crate::error::Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let plan_graph = PlanGraph::build(&graph);

    let epic = plan_graph.find_epic_for_plan(plan_path, &graph);
    if let Some(epic) = epic {
        if output_id {
            println!("{}", epic.id);
        } else {
            let subtasks = get_subtasks(&graph, &epic.id);
            let build_tasks: Vec<&Task> = graph
                .tasks
                .values()
                .filter(|t| {
                    t.task_type.as_deref() == Some("orchestrator")
                        && t.data.get("plan").map(|s| s.as_str()) == Some(plan_path)
                })
                .collect();
            crate::commands::build::output_build_show(epic, &subtasks, &build_tasks, &graph)?;
        }
    }

    Ok(())
}

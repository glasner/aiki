use std::collections::HashMap;
use std::path::Path;

use crate::agents::AgentType;
use crate::commands::OutputFormat;
use crate::error::Result;
use crate::jj::get_working_copy_change_id;
use crate::tasks::runner::{handle_session_result, task_run, task_run_on_session, TaskRunOptions};
use crate::tasks::{find_task, materialize_graph_with_ids, read_events_with_ids};
use crate::tasks::{
    generate_task_id, materialize_graph, read_events, write_event, write_link_event,
    write_link_event_with_autorun, TaskEvent, TaskPriority,
};

use crate::workflow::orchestrate::has_actionable_issues;
use crate::workflow::steps::review::{ReviewScope, ReviewScopeKind};
use crate::workflow::steps::{decompose, r#loop, review};
use crate::workflow::{StepResult, WorkflowContext};
// TODO: tech debt — create_from_template/TemplateTaskParams live in commands::task but
// should move to tasks::templates to eliminate workflow→commands coupling. The function
// is ~300 lines with private helpers (create_subtasks_from_entries, etc.) and 8+ callers
// across commands/, making the move non-trivial.
use crate::commands::task::{create_from_template, TemplateTaskParams};

/// Run a task with optional TUI display.
pub(crate) fn run_task_with_show_tui(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
    show_tui: bool,
) -> Result<()> {
    if show_tui {
        let result = task_run_on_session(cwd, task_id, options, true)?;
        handle_session_result(cwd, task_id, result, true)?;
    } else {
        task_run(cwd, task_id, options.quiet())?;
    }
    Ok(())
}

/// Create the fix-parent task (container for fix subtasks, like an epic).
///
/// Emits `remediates` link to the review task and `fixes` links to the
/// reviewed targets.
pub(crate) fn create_fix_parent(
    cwd: &Path,
    review_id: &str,
    scope: &ReviewScope,
    assignee: &Option<String>,
    autorun: bool,
) -> Result<String> {
    let fix_parent_id = generate_task_id("fix-parent");
    let working_copy = get_working_copy_change_id(cwd);

    let name = format!("Fix: {}", scope.name());
    let mut data = HashMap::new();
    data.insert("review".to_string(), review_id.to_string());

    // Add scope data
    for (k, v) in scope.to_data() {
        data.insert(k, v);
    }

    let event = TaskEvent::Created {
        task_id: fix_parent_id.clone(),
        name,
        slug: None,
        task_type: None,
        priority: TaskPriority::P2,
        assignee: assignee.clone(),
        sources: vec![format!("task:{}", review_id)],
        template: None,
        working_copy,
        instructions: None,
        data,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &event)?;

    // Emit remediates link: fix-parent remediates the review task
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let autorun_opt = if autorun { Some(true) } else { None };
    write_link_event_with_autorun(
        cwd,
        &graph,
        "remediates",
        &fix_parent_id,
        review_id,
        autorun_opt,
    )?;

    // Emit fixes link to the target(s) that were reviewed
    let reviewed_targets = graph.edges.targets(review_id, "validates");
    for target in reviewed_targets {
        write_link_event(cwd, &graph, "fixes", &fix_parent_id, target)?;
    }

    // Add fix-parent as subtask of the original task (epic) so that
    // `task diff <epic>` includes fix changes in the 2-stage review.
    if scope.kind == ReviewScopeKind::Task {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);
        write_link_event(cwd, &graph, "subtask-of", &fix_parent_id, &scope.id)?;
    }

    Ok(fix_parent_id)
}

/// Create a plan-fix task from the `fix` template.
pub(crate) fn create_plan_fix_task(
    cwd: &Path,
    review_id: &str,
    fix_parent_id: &str,
    assignee: &Option<String>,
    template_override: Option<&str>,
) -> Result<String> {
    let mut data = HashMap::new();
    data.insert("review".to_string(), review_id.to_string());
    data.insert("target".to_string(), fix_parent_id.to_string());

    let params = TemplateTaskParams {
        template_name: template_override.unwrap_or("fix").to_string(),
        data,
        sources: vec![format!("task:{}", review_id)],
        assignee: assignee.clone(),
        ..Default::default()
    };

    create_from_template(cwd, params)
}

/// Fix plan step: check actionable issues, create fix parent and plan-fix task.
pub(crate) fn run_fix_plan_step(
    ctx: &mut WorkflowContext,
    review_id: &str,
    scope: &ReviewScope,
    assignee: &Option<String>,
    template: Option<&str>,
    autorun: bool,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = if let Some(ref id) = ctx.task_id {
        id.clone()
    } else {
        let events_with_ids = read_events_with_ids(&cwd)?;
        let tasks = materialize_graph_with_ids(&events_with_ids).tasks;
        let review_task = find_task(&tasks, review_id)?;

        if !has_actionable_issues(review_task) {
            return Ok(StepResult {
                message: "approved — no actionable issues".to_string(),
                task_id: None,
            });
        }

        create_fix_parent(&cwd, review_id, scope, assignee, autorun)?
    };

    let template_name = template.unwrap_or("fix");
    let plan_fix_id = create_plan_fix_task(
        &cwd,
        review_id,
        &fix_parent_id,
        assignee,
        Some(template_name),
    )?;
    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &plan_fix_id, run_options, false)?;

    ctx.task_id = Some(fix_parent_id.clone());
    ctx.plan_path = Some(format!("/tmp/aiki/plans/{}.md", plan_fix_id));

    Ok(StepResult {
        message: "fix plan created".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix decompose step: decompose fix plan into subtasks, then delete plan file.
pub(crate) fn run_fix_decompose_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };
    let plan_path = match ctx.plan_path {
        Some(ref path) => path.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped (no plan)".to_string(),
                task_id: None,
            })
        }
    };

    let decompose_options = decompose::DecomposeOptions { template, agent };
    decompose::run_decompose(&cwd, &plan_path, &fix_parent_id, decompose_options, false)?;

    let _ = std::fs::remove_file(&plan_path);
    ctx.plan_path = None;

    Ok(StepResult {
        message: "plan decomposed into subtasks".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix loop step: run subtasks via the loop orchestrator.
pub(crate) fn run_fix_loop_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    let mut loop_options = r#loop::LoopOptions::new();
    if let Some(ref tmpl) = template {
        loop_options = loop_options.with_template(tmpl.clone());
    }
    r#loop::run_loop(&cwd, &fix_parent_id, loop_options, false)?;

    Ok(StepResult {
        message: "subtasks executed".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix review step: create and run a review of the fix-parent's changes.
pub(crate) fn run_fix_review_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    let review_scope = ReviewScope {
        kind: ReviewScopeKind::Task,
        id: fix_parent_id,
        task_ids: vec![],
    };

    let review_result = review::create_review(
        &cwd,
        review::CreateReviewParams {
            scope: review_scope,
            agent_override: agent,
            template,
            fix_template: None,
            autorun: false,
        },
    )?;

    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &review_result.review_task_id, run_options, false)?;

    Ok(StepResult {
        message: "review complete".to_string(),
        task_id: Some(review_result.review_task_id),
    })
}

/// Regression review step: re-review the original scope to catch regressions.
pub(crate) fn run_regression_review_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    let events_with_ids = read_events_with_ids(&cwd)?;
    let tasks = materialize_graph_with_ids(&events_with_ids).tasks;
    let fix_parent = find_task(&tasks, &fix_parent_id)?;
    let scope = ReviewScope::from_data(&fix_parent.data)?;

    let review_result = review::create_review(
        &cwd,
        review::CreateReviewParams {
            scope,
            agent_override: agent,
            template,
            fix_template: None,
            autorun: false,
        },
    )?;

    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &review_result.review_task_id, run_options, false)?;

    Ok(StepResult {
        message: "regression review complete".to_string(),
        task_id: Some(review_result.review_task_id),
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
        .ok_or_else(|| {
            crate::error::AikiError::InvalidArgument("No epic ID in workflow context".to_string())
        })?
        .clone();

    let review_id = if review_id.is_empty() {
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let reviews = graph.edges.referrers(&epic_id, "validates");
        reviews
            .last()
            .ok_or_else(|| {
                crate::error::AikiError::InvalidArgument("No review found to fix".to_string())
            })?
            .clone()
    } else {
        review_id.to_string()
    };

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

    crate::workflow::orchestrate::run_fix(
        &ctx.cwd,
        &review_id,
        false,
        None,
        template,
        None,
        None,
        None,
        agent,
        false,
        false,
        Some(OutputFormat::Id),
    )?;

    Ok(StepResult {
        message: "Fix complete".to_string(),
        task_id: Some(review_id),
    })
}

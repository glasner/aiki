//! Decompose workflow step — runs epic decomposition into subtasks.
//!
//! Epic lifecycle functions (create, close, restart, etc.) live in `crate::epic`.
//! This module handles the decompose step within the build workflow.

use std::path::Path;

use super::downstream_review_steps;
use super::fix_skip_to_regression_review;
use super::Step;
use super::StepResult;
use super::WorkflowChange;
use super::WorkflowContext;
use crate::agents::AgentType;
use crate::commands::task::{create_from_template, TemplateTaskParams};
use crate::epic::{close_epic, close_epic_as_invalid, create_epic_task, restart_epic, undo_completed_subtasks};
use crate::error::{AikiError, Result};
use crate::tasks::runner::TaskRunOptions;
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, write_link_event,
};

/// Options for `run_decompose` that callers can customize.
pub struct DecomposeOptions {
    pub template: Option<String>,
    pub agent: Option<AgentType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmptyDecomposePolicy {
    Build,
    Fix,
}

/// Decompose a plan into subtasks under `target_id`.
///
/// Steps:
/// 1. Write `implements-plan` link: target → `file:<plan_path>`
/// 2. Create decompose task from template with `data.target` and `data.plan`
/// 3. Write `decomposes-plan` link: decompose task → `file:<plan_path>`
/// 4. Write `populated-by` link: target → decompose task
/// 5. `task_run(decompose_task)` with agent options
/// 6. Return decompose task ID
pub fn run_decompose(
    cwd: &Path,
    plan_path: &str,
    target_id: &str,
    options: DecomposeOptions,
    show_tui: bool,
) -> Result<String> {
    let spec_target = make_spec_target(plan_path);

    // 0. Validate target exists before emitting any links/events
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    find_task(&graph.tasks, target_id)?;

    // 1. Write implements-plan link: target → file:<plan_path>
    write_link_event(cwd, &graph, "implements-plan", target_id, &spec_target)?;

    // 2. Create decompose task from template with data.target and data.plan
    let params = build_decompose_params(plan_path, target_id, &spec_target, &options);

    let decompose_task_id = create_from_template(cwd, params)?;

    // 3. Write decomposes-plan link: decompose task → file:<plan_path>
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(
        cwd,
        &graph,
        "decomposes-plan",
        &decompose_task_id,
        &spec_target,
    )?;

    // 4. Write populated-by link: target → decompose task
    write_link_event(cwd, &graph, "populated-by", target_id, &decompose_task_id)?;

    // 5. task_run(decompose_task) with agent options
    let run_options = if let Some(agent) = options.agent {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };
    super::run_task_with_show_tui(cwd, &decompose_task_id, run_options, show_tui)?;

    // 6. Return decompose task ID (subtask validation is done by the step handler)
    Ok(decompose_task_id)
}

/// Decompose step: find/create epic, check blockers, run decompose if needed.
pub(crate) fn run(ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
    let restart = ctx.opts.restart;
    let template = ctx.opts.decompose_template.clone();
    let agent = ctx.opts.agent;

    let plan_path = ctx
        .plan_path
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No plan path in workflow context".to_string()))?
        .clone();

    // If no epic in context, find or create one
    if ctx.task_id.is_none() {
        ctx.status("resolving epic");
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let plan_graph = crate::plans::PlanGraph::build(&graph);
        let existing_epic = plan_graph.resolve_epic_for_plan(&plan_path, &graph)?;

        let epic_id = if restart {
            if let Some(epic) = existing_epic {
                if epic.status != crate::tasks::TaskStatus::Closed {
                    ctx.status("restarting epic");
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
                        ctx.status("resuming existing epic");
                        restart_epic(&ctx.cwd, &epic.id)?;
                        Some(epic.id.clone())
                    }
                }
                _ => None,
            }
        };

        let epic_id = match epic_id {
            Some(id) => id,
            None => {
                ctx.status("creating epic");
                create_epic_task(&ctx.cwd, &plan_path)?
            }
        };

        ctx.task_id = Some(epic_id);
    }

    let epic_id = ctx.task_id.as_ref().unwrap().clone();

    // Check blockers
    ctx.status("checking blockers");
    let events = read_events(&ctx.cwd)?;
    let graph = materialize_graph(&events);
    if graph.is_blocked(&epic_id) {
        return Err(anyhow::anyhow!(
            "Epic {} is blocked by unresolved dependencies. Rerun with --restart to start over",
            &epic_id[..epic_id.len().min(8)]
        ));
    }

    // Run decompose if no subtasks exist
    let subtasks = get_subtasks(&graph, &epic_id);
    if subtasks.is_empty() {
        ctx.status("decomposing plan into subtasks");
        let options = DecomposeOptions { template, agent };
        let decompose_task_id = run_decompose(&ctx.cwd, &plan_path, &epic_id, options, false)?;

        ctx.status("validating subtasks");
        let events = read_events(&ctx.cwd)?;
        let graph = materialize_graph(&events);
        let count = get_subtasks(&graph, &epic_id).len();

        if count == 0 {
            return handle_empty_decompose_result(ctx, &epic_id, decompose_task_id);
        }

        Ok(StepResult {
            change: WorkflowChange::None,
            message: format!("{} subtasks created", count),
            task_id: Some(decompose_task_id),
        })
    } else {
        Ok(StepResult {
            change: WorkflowChange::None,
            message: "Epic resumed (subtasks already exist)".to_string(),
            task_id: Some(epic_id),
        })
    }
}

fn handle_empty_decompose_result(
    ctx: &WorkflowContext,
    epic_id: &str,
    decompose_task_id: String,
) -> anyhow::Result<StepResult> {
    match empty_decompose_policy(ctx) {
        EmptyDecomposePolicy::Build => {
            handle_empty_build_decompose(epic_id, decompose_task_id, ctx)
        }
        EmptyDecomposePolicy::Fix => handle_empty_fix_decompose(decompose_task_id),
    }
}

fn empty_decompose_policy(ctx: &WorkflowContext) -> EmptyDecomposePolicy {
    if ctx.review_id.is_some() {
        EmptyDecomposePolicy::Fix
    } else {
        EmptyDecomposePolicy::Build
    }
}

fn handle_empty_build_decompose(
    epic_id: &str,
    decompose_task_id: String,
    ctx: &WorkflowContext,
) -> anyhow::Result<StepResult> {
    close_epic_as_invalid(&ctx.cwd, epic_id)?;
    let mut skip_steps = vec![Step::Loop];
    skip_steps.extend(downstream_review_steps());
    Ok(StepResult {
        change: WorkflowChange::SkipSteps(skip_steps),
        message: "no subtasks created — skipping loop and downstream review".to_string(),
        task_id: Some(decompose_task_id),
    })
}

fn handle_empty_fix_decompose(decompose_task_id: String) -> anyhow::Result<StepResult> {
    Ok(StepResult {
        change: WorkflowChange::SkipSteps(fix_skip_to_regression_review()),
        message: "no subtasks created during fix decomposition".to_string(),
        task_id: Some(decompose_task_id),
    })
}

/// Normalize a plan path into a `file:` spec target for link events.
fn make_spec_target(plan_path: &str) -> String {
    if plan_path.starts_with("file:") {
        plan_path.to_string()
    } else {
        format!("file:{}", plan_path)
    }
}

/// Build the `TemplateTaskParams` for the decompose task.
fn build_decompose_params(
    plan_path: &str,
    target_id: &str,
    spec_target: &str,
    options: &DecomposeOptions,
) -> TemplateTaskParams {
    let template = options.template.as_deref().unwrap_or("decompose");

    let assignee = options
        .agent
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let mut data = std::collections::HashMap::new();
    data.insert("plan".to_string(), plan_path.to_string());
    data.insert("target".to_string(), target_id.to_string());

    TemplateTaskParams {
        template_name: template.to_string(),
        data,
        sources: vec![spec_target.to_string()],
        assignee,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_spec_target_adds_file_prefix() {
        assert_eq!(
            make_spec_target("ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_make_spec_target_preserves_existing_prefix() {
        assert_eq!(
            make_spec_target("file:ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn invalid_decompose_skips_loop_and_full_review_tail() {
        let mut skip_steps = vec![Step::Loop];
        skip_steps.extend(downstream_review_steps());

        assert_eq!(skip_steps.len(), 4);
        assert!(skip_steps.contains(&Step::Loop));
        assert!(skip_steps.contains(&Step::SetupReview));
        assert!(skip_steps.contains(&Step::Review));
        assert!(skip_steps.contains(&Step::RegressionReview));
    }

    #[test]
    fn empty_fix_decompose_uses_unified_skip_to_regression_review() {
        let skip_steps = fix_skip_to_regression_review();

        assert_eq!(skip_steps.len(), 4);
        assert!(skip_steps.contains(&Step::Decompose));
        assert!(skip_steps.contains(&Step::Loop));
        assert!(skip_steps.contains(&Step::SetupReview));
        assert!(skip_steps.contains(&Step::Review));
        assert!(!skip_steps.contains(&Step::RegressionReview));
    }

    #[test]
    fn zero_subtask_policy_uses_build_when_no_review_context_exists() {
        let ctx = WorkflowContext {
            task_id: None,
            plan_path: Some("ops/now/feature.md".to_string()),
            cwd: std::env::temp_dir(),
            output: crate::workflow::WorkflowOutput::new(crate::workflow::OutputKind::Quiet),
            opts: crate::workflow::WorkflowOpts::default(),
            review_id: None,
            scope: None,
            assignee: None,
            iteration: 0,
        };

        assert_eq!(empty_decompose_policy(&ctx), EmptyDecomposePolicy::Build);
    }

    #[test]
    fn zero_subtask_policy_uses_fix_when_review_context_exists() {
        let ctx = WorkflowContext {
            task_id: None,
            plan_path: None,
            cwd: std::env::temp_dir(),
            output: crate::workflow::WorkflowOutput::new(crate::workflow::OutputKind::Quiet),
            opts: crate::workflow::WorkflowOpts::default(),
            review_id: Some("review123".to_string()),
            scope: None,
            assignee: None,
            iteration: 0,
        };

        assert_eq!(empty_decompose_policy(&ctx), EmptyDecomposePolicy::Fix);
    }

    #[test]
    fn test_build_decompose_params_defaults() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params(
            "ops/now/feat.md",
            "target123",
            "file:ops/now/feat.md",
            &options,
        );

        assert_eq!(params.template_name, "decompose");
        assert_eq!(params.assignee, Some("claude-code".to_string()));
        assert_eq!(params.data.get("plan").unwrap(), "ops/now/feat.md");
        assert_eq!(params.data.get("target").unwrap(), "target123");
        assert_eq!(params.sources, vec!["file:ops/now/feat.md"]);
    }

    #[test]
    fn test_build_decompose_params_custom_template() {
        let options = DecomposeOptions {
            template: Some("my/custom-decompose".to_string()),
            agent: None,
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.template_name, "my/custom-decompose");
    }

    #[test]
    fn test_build_decompose_params_custom_agent() {
        let options = DecomposeOptions {
            template: None,
            agent: Some(AgentType::Codex),
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.assignee, Some("codex".to_string()));
    }

    #[test]
    fn test_build_decompose_params_data_uses_target_not_epic() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params("plan.md", "target_id", "file:plan.md", &options);

        assert!(params.data.contains_key("target"));
        assert!(!params.data.contains_key("epic"));
        assert_eq!(params.data.get("target").unwrap(), "target_id");
        assert_eq!(params.data.get("plan").unwrap(), "plan.md");
    }

    #[test]
    fn test_decompose_template_uses_data_target_not_data_epic() {
        let template_content = include_str!("../../tasks/templates/core/decompose.md");
        assert!(
            template_content.contains("{{data.target}}"),
            "Decompose template must use {{{{data.target}}}}"
        );
        assert!(
            !template_content.contains("{{data.epic}}"),
            "Decompose template must NOT use {{{{data.epic}}}}"
        );
    }
}

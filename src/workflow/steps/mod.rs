use std::path::Path;

use anyhow::Result;

pub(crate) use super::WorkflowContext;
use crate::tasks::runner::{handle_session_result, task_run, task_run_on_session, TaskRunOptions};

pub(crate) mod decompose;
pub(crate) mod fix;
pub(crate) mod r#loop;
pub(crate) mod plan;
pub(crate) mod regression_review;
pub(crate) mod review;
pub(crate) mod setup_epic;
pub(crate) mod setup_fix;
pub(crate) mod setup_review;

/// A workflow-level change requested by a step.
pub enum WorkflowChange {
    /// No change to the workflow.
    None,
    /// Append additional steps after the current position.
    NextSteps(Vec<Step>),
    /// Remove matching steps from the remaining queue.
    SkipSteps(Vec<Step>),
}

/// Result returned by a single workflow step.
pub struct StepResult {
    pub message: String,
    pub task_id: Option<String>,
    pub change: WorkflowChange,
}

/// Unified step enum covering all workflow step variants.
///
/// Commands compose workflows by selecting which variants to include in their
/// step sequence. Options are read from `WorkflowContext.opts`; only runtime
/// state that varies per-step remains as variant fields.
pub enum Step {
    /// Validate plan, find/create epic, check blockers, set ctx.task_id.
    ///
    /// When `ctx.task_id` is None (plan path): validates plan, checks draft,
    /// cleans stale builds, finds or creates epic with restart handling.
    /// When `ctx.task_id` is Some (epic ID): looks up epic, extracts plan_path,
    /// checks blockers.
    SetupEpic,

    /// Find/create epic, set ctx.task_id, run decompose agent.
    Decompose,

    /// Run loop orchestrator over subtasks.
    Loop,

    /// Detect target, validate constraints, create review task, set ctx.task_id.
    ///
    /// Cheap setup step — does scope detection and task creation but does NOT
    /// run the review agent. Paired with a subsequent `Review` step.
    SetupReview,

    /// Run a pre-created review task from ctx.task_id.
    ///
    /// Always paired with a prior `SetupReview` step that creates the review
    /// task and sets ctx.task_id. Fix-after-review is handled at the workflow
    /// level by the `RegressionReview` step via dynamic step injection.
    Review,

    /// Validate review task, resolve scope/assignee/template, create fix-parent.
    ///
    /// Cheap setup step — does validation and task creation but does NOT run
    /// any fix agents. Paired with subsequent fix steps.
    /// Reads `ctx.review_id` for the review task to validate.
    SetupFix,

    /// Create fix-parent, write fix plan, and run the plan-fix task.
    /// Short-circuits if the review has no actionable issues.
    /// Reads `ctx.review_id`, `ctx.scope`, and `ctx.assignee` from context.
    Fix,

    /// Regression review — re-review original scope after a fix cycle.
    RegressionReview,

    /// Test-only step variant for unit testing workflow machinery.
    #[cfg(test)]
    _Test {
        name: &'static str,
        section: Option<&'static str>,
        handler: std::sync::Arc<dyn Fn(&mut WorkflowContext) -> Result<StepResult> + Send + Sync>,
    },
}

pub(crate) fn downstream_review_steps() -> Vec<Step> {
    vec![Step::SetupReview, Step::Review, Step::RegressionReview]
}

/// Skip set that jumps straight to `RegressionReview`, bypassing the
/// intermediate fix/decompose/loop/review steps.
///
/// Used by both the Fix step (no actionable issues → short-circuit) and the
/// Decompose step (no subtasks created during fix decomposition). Because
/// `SkipSteps` only removes steps still in the queue, emitting the full set
/// from either call-site is safe: steps that already ran are absent from the
/// queue and silently ignored.
///
/// `RegressionReview` is deliberately kept — it handles the `task_id = None`
/// case as an immediate "approved" result.
pub(crate) fn fix_skip_to_regression_review() -> Vec<Step> {
    vec![Step::Decompose, Step::Loop, Step::SetupReview, Step::Review]
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Step::SetupEpic, Step::SetupEpic) => true,
            (Step::Decompose, Step::Decompose) => true,
            (Step::Loop, Step::Loop) => true,
            (Step::SetupReview, Step::SetupReview) => true,
            (Step::Review, Step::Review) => true,
            (Step::SetupFix, Step::SetupFix) => true,
            (Step::Fix, Step::Fix) => true,
            (Step::RegressionReview, Step::RegressionReview) => true,
            #[cfg(test)]
            (Step::_Test { name: a, .. }, Step::_Test { name: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl Step {
    pub fn name(&self) -> &'static str {
        match self {
            Step::SetupEpic => "setup epic",
            Step::Decompose => "decompose",
            Step::Loop => "loop",
            Step::SetupReview => "setup review",
            Step::Review => "review",
            Step::SetupFix => "setup fix",
            Step::Fix => "fix",
            Step::RegressionReview => "review for regressions",
            #[cfg(test)]
            Step::_Test { name, .. } => name,
        }
    }

    /// Section header for this step, if any.
    ///
    /// `iteration` is the current quality-loop iteration (0 = initial build).
    /// For the Decompose step this returns "Initial Build" on iteration 0
    /// and "Iteration N" for subsequent fix cycles.
    pub fn section(&self, iteration: usize) -> Option<String> {
        match self {
            Step::Decompose => {
                if iteration == 0 {
                    Some("Initial Build".to_string())
                } else {
                    Some(format!("Iteration {}", iteration))
                }
            }
            #[cfg(test)]
            Step::_Test { section, .. } => section.map(|s| s.to_string()),
            _ => None,
        }
    }

    pub fn run(&self, ctx: &mut WorkflowContext) -> Result<StepResult> {
        match self {
            Step::SetupEpic => setup_epic::run(ctx),

            Step::Decompose => decompose::run(ctx),

            Step::Loop => r#loop::run(ctx),

            Step::SetupReview => setup_review::run(ctx),

            Step::Review => review::run(ctx),

            Step::SetupFix => setup_fix::run(ctx),

            Step::Fix => fix::run(ctx),

            Step::RegressionReview => regression_review::run(ctx),

            #[cfg(test)]
            Step::_Test { handler, .. } => handler(ctx),
        }
    }
}

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

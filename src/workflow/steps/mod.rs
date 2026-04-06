use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossbeam_channel::Receiver;

pub(crate) use super::WorkflowContext;
use super::WorkflowOutput;
use crate::error::AikiError;
use crate::tasks::runner::{
    finalize_agent_run, handle_session_result, prepare_task_run, rollback_if_still_reserved,
    task_run, task_run_on_session, TaskRunOptions,
};
use crate::tasks::listener::POLL_INTERVAL;
use crate::tasks::TaskEvent;

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

// ── Shared spawn-drain-finalize ─────────────────────────────────────

/// Step-specific event processing during the spawn-drain-finalize loop.
///
/// Each step gets exclusive access to the shared `event_rx` channel for the
/// duration of its agent run. Because `rx.try_iter()` destructively consumes
/// events, implementations only see events produced while their agent is alive.
/// This is the intended single-consumer-per-step design — see
/// `WorkflowContext::event_rx` for details.
pub(crate) trait DrainHandler {
    /// Process events from the channel. Called repeatedly while the agent is
    /// running and once after it exits. Consumes all pending events from `rx`
    /// via `try_iter()` — consumed events are not visible to later steps.
    fn drain(&mut self, rx: &Receiver<TaskEvent>);

    /// Called once after the final drain, before stderr/finalize. Use for
    /// summary output (e.g. issue counts).
    fn finish(&mut self) {}
}

/// Shared spawn-drain-finalize loop.
///
/// Spawns a monitored agent process, drains task events through the provided
/// handler while the agent runs, reads diagnostic output, and finalizes the
/// agent run. The `event_rx` channel is shared across all workflow steps
/// (see `WorkflowContext::event_rx`), but only one step's drain loop runs
/// at a time, so each handler only consumes events from its own agent.
pub(crate) fn spawn_drain_finalize(
    cwd: &Path,
    task_id: &str,
    run_options: &TaskRunOptions,
    event_rx: Option<&Receiver<TaskEvent>>,
    output: WorkflowOutput,
    handler: &mut dyn DrainHandler,
) -> crate::error::Result<()> {
    let prepared = prepare_task_run(cwd, task_id, run_options, |_| {})?;

    let mut agent_handle = match prepared.runtime.spawn_monitored(&prepared.spawn_options) {
        Ok(handle) => handle,
        Err(e) => {
            rollback_if_still_reserved(cwd, &prepared.task_id, &e);
            return Err(e);
        }
    };

    if let Some(rx) = event_rx {
        while agent_handle
            .try_wait()
            .map_err(|e| AikiError::AgentSpawnFailed(format!("try_wait failed: {}", e)))?
            .is_none()
        {
            handler.drain(rx);
            thread::sleep(Duration::from_millis(250));
        }
        // The listener thread polls JJ at POLL_INTERVAL. After the agent
        // exits there may be events written just before exit that the listener
        // hasn't picked up yet. Wait for one full poll cycle plus margin,
        // draining as new events arrive, so we don't lose tail events.
        let tail_drain = POLL_INTERVAL + Duration::from_millis(200);
        let deadline = std::time::Instant::now() + tail_drain;
        loop {
            handler.drain(rx);
            if std::time::Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        handler.finish();
    } else {
        // No event channel — still must wait for the agent to exit before
        // reading output and finalizing.
        agent_handle
            .wait()
            .map_err(|e| AikiError::AgentSpawnFailed(format!("wait failed: {}", e)))?;
    }

    // Read any diagnostic output
    let proc_output = agent_handle.read_output();
    if !proc_output.stderr.is_empty() {
        output.emit(&format!("  agent stderr: {}", proc_output.stderr));
    }

    finalize_agent_run(cwd, task_id)?;

    Ok(())
}

/// Drain handler for steps that track subtask creation under a parent task.
///
/// Buffers `Created` events and only displays them once a `LinkAdded`
/// (`subtask-of` → parent) event confirms the task is a child of the target.
/// Used by the decompose and fix steps.
pub(crate) struct SubtaskDrainHandler<'a> {
    pending_names: HashMap<String, String>,
    task_names: &'a mut HashMap<String, String>,
    parent_id: String,
    output: WorkflowOutput,
}

impl<'a> SubtaskDrainHandler<'a> {
    pub fn new(
        task_names: &'a mut HashMap<String, String>,
        parent_id: String,
        output: WorkflowOutput,
    ) -> Self {
        Self {
            pending_names: HashMap::new(),
            task_names,
            parent_id,
            output,
        }
    }
}

impl DrainHandler for SubtaskDrainHandler<'_> {
    fn drain(&mut self, rx: &Receiver<TaskEvent>) {
        for event in rx.try_iter() {
            match &event {
                TaskEvent::Created { task_id, name, .. } => {
                    self.pending_names.insert(task_id.clone(), name.clone());
                }
                TaskEvent::LinkAdded { from, to, kind, .. }
                    if kind == "subtask-of" && to == &self.parent_id =>
                {
                    if let Some(name) = self.pending_names.remove(from) {
                        self.task_names.insert(from.clone(), name.clone());
                        self.output.emit(&format!("  + {}", name));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Drain handler for review steps that count issue comments.
///
/// Counts `CommentAdded` events where `data.issue == "true"` and the comment
/// targets the review task. Used by the review and regression_review steps.
pub(crate) struct ReviewDrainHandler {
    review_id: String,
    issue_count: usize,
    output: WorkflowOutput,
}

impl ReviewDrainHandler {
    pub fn new(review_id: String, output: WorkflowOutput) -> Self {
        Self {
            review_id,
            issue_count: 0,
            output,
        }
    }
}

impl DrainHandler for ReviewDrainHandler {
    fn drain(&mut self, rx: &Receiver<TaskEvent>) {
        for event in rx.try_iter() {
            if let TaskEvent::CommentAdded { task_ids, data, .. } = &event {
                if task_ids.iter().any(|id| id == &self.review_id)
                    && data.get("issue").map(|v| v == "true").unwrap_or(false)
                {
                    self.issue_count += 1;
                }
            }
        }
    }

    fn finish(&mut self) {
        if self.issue_count > 0 {
            self.output.emit(&format!(
                "  Found {} issue{}",
                self.issue_count,
                if self.issue_count == 1 { "" } else { "s" }
            ));
        }
    }
}

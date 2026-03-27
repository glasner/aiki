//! Workflow assembly functions.
//!
//! Each public function composes a [`Workflow`] from [`Step`] variants for a
//! specific command (build, fix, review).

use std::path::{Path, PathBuf};

use crate::agents::AgentType;
use crate::commands::review::ReviewScope;

use super::{Step, Workflow, WorkflowContext};

/// Options for assembling a build workflow.
pub struct BuildOpts {
    pub restart: bool,
    pub decompose_template: Option<String>,
    pub loop_template: Option<String>,
    pub agent: Option<AgentType>,
    pub agent_str: Option<String>,
    pub review_after: bool,
    pub review_template: Option<String>,
    pub fix_after: bool,
    pub fix_template: Option<String>,
}

/// Options for building a fix workflow.
pub struct FixOpts {
    pub scope: ReviewScope,
    pub assignee: Option<String>,
    pub plan_template: String,
    pub decompose_template: Option<String>,
    pub loop_template: Option<String>,
    pub autorun: bool,
    pub cwd: PathBuf,
}

/// Assemble a build workflow for a plan path (includes Plan step).
///
/// When `opts.fix_after` is true, the static Fix step is omitted — fix iteration
/// is handled dynamically by [`drive_build`] via [`Workflow::run_build`].
pub(crate) fn build_workflow(cwd: &Path, plan_path: &str, opts: &BuildOpts) -> Workflow {
    let mut steps: Vec<Step> = vec![
        Step::Plan,
        Step::Decompose {
            restart: opts.restart,
            template: opts.decompose_template.clone(),
            agent: opts.agent.clone(),
        },
        Step::Loop {
            template: opts.loop_template.clone(),
            agent: opts.agent.clone(),
        },
    ];

    if opts.review_after {
        steps.push(Step::Review {
            scope: None, // derive from ctx.plan_path
            template: opts.review_template.clone(),
            agent: opts.agent_str.clone(),
            fix_template: None,
            autorun: false,
        });

        // Static Fix step only when NOT using drive_build (fix_after = false means
        // review-only mode; fix_after = true delegates iteration to drive_build).
        if !opts.fix_after {
            // No fix step needed in review-only mode
        }
    }

    Workflow {
        steps,
        ctx: WorkflowContext {
            task_id: None,
            plan_path: Some(plan_path.to_string()),
            cwd: cwd.to_path_buf(),
        },
    }
}

/// Assemble a build workflow for an existing epic (skips Plan step).
pub(crate) fn build_workflow_from_epic(
    cwd: &Path,
    epic_id: &str,
    plan_path: &str,
    opts: &BuildOpts,
) -> Workflow {
    let mut steps: Vec<Step> = vec![
        Step::Decompose {
            restart: false,
            template: opts.decompose_template.clone(),
            agent: opts.agent.clone(),
        },
        Step::Loop {
            template: opts.loop_template.clone(),
            agent: opts.agent.clone(),
        },
    ];

    if opts.review_after {
        steps.push(Step::Review {
            scope: None, // derive from ctx.plan_path
            template: opts.review_template.clone(),
            agent: opts.agent_str.clone(),
            fix_template: None,
            autorun: false,
        });
        // Fix iteration is handled dynamically by drive_build via run_build.
    }

    Workflow {
        steps,
        ctx: WorkflowContext {
            task_id: Some(epic_id.to_string()),
            plan_path: Some(plan_path.to_string()),
            cwd: cwd.to_path_buf(),
        },
    }
}

/// Assemble a review workflow from a pre-resolved scope and options.
pub(crate) fn review_workflow(
    cwd: PathBuf,
    scope: ReviewScope,
    template: Option<String>,
    agent: Option<String>,
    fix_template: Option<String>,
    autorun: bool,
) -> Workflow {
    Workflow {
        steps: vec![Step::Review {
            scope: Some(scope),
            template,
            agent,
            fix_template,
            autorun,
        }],
        ctx: WorkflowContext {
            task_id: None,
            plan_path: None,
            cwd,
        },
    }
}

/// Build a full fix workflow assembly for architecture tests and future drivers.
pub(crate) fn fix_workflow(review_id: &str, opts: &FixOpts) -> Workflow {
    let agent_type = opts.assignee.as_deref().and_then(AgentType::from_str);

    let steps = vec![
        Step::Fix {
            review_id: review_id.to_string(),
            scope: Some(opts.scope.clone()),
            assignee: opts.assignee.clone(),
            template: Some(opts.plan_template.clone()),
            autorun: opts.autorun,
        },
        Step::Decompose {
            restart: false,
            template: opts.decompose_template.clone(),
            agent: agent_type,
        },
        Step::Loop {
            template: opts.loop_template.clone(),
            agent: None, // fix always uses default
        },
        Step::Review {
            scope: None,
            template: None,
            agent: None,
            fix_template: None,
            autorun: false,
        },
        Step::RegressionReview {
            template: None,
            agent: None,
        },
    ];

    Workflow {
        steps,
        ctx: WorkflowContext {
            task_id: None,
            plan_path: None,
            cwd: opts.cwd.clone(),
        },
    }
}

/// Build one pass of the fix pipeline used by the current quality loop driver.
pub(crate) fn fix_pass_workflow(review_id: &str, opts: &FixOpts) -> Workflow {
    let mut wf = fix_workflow(review_id, opts);
    wf.steps.truncate(3);
    wf
}

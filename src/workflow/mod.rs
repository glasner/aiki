pub mod builders;
pub mod orchestrate;
pub mod steps;

use std::path::PathBuf;

use anyhow::Result;

use crate::agents::AgentType;
use crate::commands::{fix, review};

pub use steps::review::{
    CreateReviewParams, CreateReviewResult, Location, ReviewScope, ReviewScopeKind,
};

/// Context shared across all steps in a workflow.
pub struct WorkflowContext {
    /// Root task this workflow operates on. Set by Init/Decompose step, read by later steps.
    pub task_id: Option<String>,
    /// Plan path (if applicable). Set at construction or by Init.
    pub plan_path: Option<String>,
    /// Working directory.
    pub cwd: PathBuf,
}

/// Result returned by a single workflow step.
pub struct StepResult {
    pub message: String,
    pub task_id: Option<String>,
}

/// Unified step enum covering all workflow step variants.
///
/// Commands compose workflows by selecting which variants to include in their
/// step sequence.
pub enum Step {
    /// Validate plan file. Shows plan path on completion.
    Plan,

    /// Find/create epic, set ctx.task_id, run decompose agent.
    Decompose {
        restart: bool,
        template: Option<String>,
        agent: Option<AgentType>,
    },

    /// Run loop orchestrator over subtasks.
    Loop {
        template: Option<String>,
        agent: Option<AgentType>,
    },

    /// Run a review.
    ///
    /// When `scope` is `Some`, creates a standalone review with the given scope
    /// (used by `aiki review` and fix review steps).
    /// When `scope` is `None`, derives scope from ctx (used by `aiki build` post-build review).
    Review {
        scope: Option<ReviewScope>,
        template: Option<String>,
        agent: Option<String>,
        fix_template: Option<String>,
        autorun: bool,
    },

    /// Create fix-parent, write fix plan, and run the plan-fix task.
    /// Short-circuits if the review has no actionable issues.
    Fix {
        review_id: String,
        scope: Option<ReviewScope>,
        assignee: Option<String>,
        template: Option<String>,
        autorun: bool,
    },

    /// Regression review — re-review original scope after a fix cycle.
    RegressionReview {
        template: Option<String>,
        agent: Option<String>,
    },

    /// Test-only step variant for unit testing workflow machinery.
    #[cfg(test)]
    _Test {
        name: &'static str,
        section: Option<&'static str>,
        handler: std::sync::Arc<dyn Fn(&mut WorkflowContext) -> Result<StepResult> + Send + Sync>,
    },
}

impl Step {
    pub fn name(&self) -> &'static str {
        match self {
            Step::Plan => "plan",
            Step::Decompose { .. } => "decompose",
            Step::Loop { .. } => "loop",
            Step::Review { .. } => "review",
            Step::Fix { .. } => "fix",
            Step::RegressionReview { .. } => "review for regressions",
            #[cfg(test)]
            Step::_Test { name, .. } => name,
        }
    }

    pub fn section(&self) -> Option<&'static str> {
        match self {
            Step::Decompose { .. } => Some("Initial Build"),
            #[cfg(test)]
            Step::_Test { section, .. } => *section,
            _ => None,
        }
    }

    pub fn run(&self, ctx: &mut WorkflowContext) -> Result<StepResult> {
        match self {
            Step::Plan => steps::build::run_plan_step(ctx),

            Step::Decompose {
                restart,
                template,
                agent,
            } => steps::build::run_decompose_step(ctx, *restart, template.clone(), agent.clone()),

            Step::Loop { template, agent } => {
                if agent.is_some() {
                    steps::build::run_loop_step(ctx, template.clone(), agent.clone())
                } else {
                    let task_id = match ctx.task_id {
                        Some(ref id) => id.clone(),
                        None => {
                            return Ok(StepResult {
                                message: "skipped".to_string(),
                                task_id: None,
                            })
                        }
                    };
                    let mut opts = crate::commands::loop_cmd::LoopOptions::new();
                    if let Some(ref tmpl) = template {
                        opts = opts.with_template(tmpl.clone());
                    }
                    crate::commands::loop_cmd::run_loop(&ctx.cwd, &task_id, opts, false)?;
                    Ok(StepResult {
                        message: "subtasks executed".to_string(),
                        task_id: Some(task_id),
                    })
                }
            }

            Step::Review {
                scope: Some(scope),
                template,
                agent,
                fix_template,
                autorun,
            } => review::run_standalone_review_step(
                ctx,
                scope.clone(),
                template.clone(),
                agent.clone(),
                fix_template.clone(),
                *autorun,
            ),

            Step::Review {
                scope: None,
                template,
                agent,
                ..
            } => steps::build::run_review_step(ctx, template.clone(), agent.clone()),

            Step::Fix {
                review_id,
                scope,
                assignee,
                template,
                autorun,
            } => {
                if let Some(scope) = scope {
                    fix::run_fix_plan_step(
                        ctx,
                        review_id,
                        scope,
                        assignee,
                        template.as_deref(),
                        *autorun,
                    )
                } else {
                    steps::build::run_fix_step(ctx, review_id, template.clone(), None)
                }
            }

            Step::RegressionReview { template, agent } => {
                fix::run_regression_review_step(ctx, template.clone(), agent.clone())
            }

            #[cfg(test)]
            Step::_Test { handler, .. } => handler(ctx),
        }
    }
}

/// Count issues from a review task's data.
pub fn count_issues(
    tasks: &crate::tasks::types::FastHashMap<String, crate::tasks::Task>,
    review_task_id: &str,
) -> usize {
    crate::tasks::find_task(tasks, review_task_id)
        .map(|t| {
            t.data
                .get("issue_count")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Controls how workflow execution reports progress.
#[derive(Clone, Copy)]
pub enum RunMode {
    /// Sequential on main thread, minimal text output (eprintln status lines).
    Text,
    /// Silent — background/async processes.
    Quiet,
}

/// A sequence of steps executed against a shared context.
pub struct Workflow {
    pub steps: Vec<Step>,
    pub ctx: WorkflowContext,
}

impl Workflow {
    pub fn run(mut self, mode: RunMode) -> Result<WorkflowContext> {
        match mode {
            RunMode::Text => {
                for step in &self.steps {
                    if let Some(section) = step.section() {
                        eprintln!("\n── {} ──", section);
                    }
                    eprintln!("⠙ {}...", step.name());
                    match step.run(&mut self.ctx) {
                        Ok(result) => eprintln!("合 {} — {}", step.name(), result.message),
                        Err(e) => {
                            eprintln!("✗ {} — {}", step.name(), e);
                            return Err(e);
                        }
                    }
                }
                Ok(self.ctx)
            }
            RunMode::Quiet => {
                for step in &self.steps {
                    step.run(&mut self.ctx)?;
                }
                Ok(self.ctx)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::env;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    fn test_step(
        name: &'static str,
        value: &'static str,
        seen: Arc<Mutex<Vec<&'static str>>>,
    ) -> Step {
        Step::_Test {
            name,
            section: None,
            handler: Arc::new(move |ctx| {
                seen.lock().unwrap().push(name);
                ctx.task_id = Some(value.to_string());
                ctx.plan_path = Some(value.to_string());
                Ok(StepResult {
                    message: format!("set {value}"),
                    task_id: ctx.task_id.clone(),
                })
            }),
        }
    }

    fn test_step_with_section(
        name: &'static str,
        value: &'static str,
        section: Option<&'static str>,
        seen: Arc<Mutex<Vec<&'static str>>>,
    ) -> Step {
        Step::_Test {
            name,
            section,
            handler: Arc::new(move |ctx| {
                seen.lock().unwrap().push(name);
                ctx.task_id = Some(value.to_string());
                ctx.plan_path = Some(value.to_string());
                Ok(StepResult {
                    message: format!("set {value}"),
                    task_id: ctx.task_id.clone(),
                })
            }),
        }
    }

    fn test_step_assert_plan_path(
        name: &'static str,
        expected: &'static str,
        seen: Arc<Mutex<Vec<&'static str>>>,
    ) -> Step {
        Step::_Test {
            name,
            section: None,
            handler: Arc::new(move |ctx| {
                seen.lock().unwrap().push(name);
                assert_eq!(ctx.plan_path.as_deref(), Some(expected));
                ctx.task_id = Some("asserted".to_string());
                Ok(StepResult {
                    message: format!("saw {expected}"),
                    task_id: ctx.task_id.clone(),
                })
            }),
        }
    }

    fn test_step_fail(name: &'static str, message: &'static str) -> Step {
        Step::_Test {
            name,
            section: None,
            handler: Arc::new(move |_ctx| Err(anyhow!(message))),
        }
    }

    fn run_output_probe(case: &str) -> String {
        let exe = env::current_exe().expect("resolve current test binary");
        let output = Command::new(exe)
            .arg("--exact")
            .arg("workflow::tests::run_mode_output_probe")
            .arg("--nocapture")
            .env("AIKI_WORKFLOW_TEST_CASE", case)
            .output()
            .expect("run workflow output probe");

        assert!(
            output.status.success(),
            "probe failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stderr).to_string()
    }

    #[test]
    fn run_mode_output_probe() {
        let Some(case) = env::var("AIKI_WORKFLOW_TEST_CASE").ok() else {
            return;
        };

        let seen = Arc::new(Mutex::new(Vec::new()));
        let workflow = match case.as_str() {
            "text-sectioned" => Workflow {
                steps: vec![
                    test_step_with_section(
                        "workflow-text-with-section",
                        "first",
                        Some("Workflow Test Section"),
                        Arc::clone(&seen),
                    ),
                    test_step("workflow-text-without-section", "second", seen),
                ],
                ctx: WorkflowContext {
                    task_id: None,
                    plan_path: None,
                    cwd: PathBuf::from("."),
                },
            },
            "text-basic" => Workflow {
                steps: vec![test_step("workflow-text-step", "text", seen)],
                ctx: WorkflowContext {
                    task_id: None,
                    plan_path: None,
                    cwd: PathBuf::from("."),
                },
            },
            "quiet-basic" => Workflow {
                steps: vec![test_step("workflow-quiet-step", "quiet", seen)],
                ctx: WorkflowContext {
                    task_id: None,
                    plan_path: None,
                    cwd: PathBuf::from("."),
                },
            },
            "text-shared" | "quiet-shared" => Workflow {
                steps: vec![test_step("workflow-shared-step", "shared", seen)],
                ctx: WorkflowContext {
                    task_id: None,
                    plan_path: None,
                    cwd: PathBuf::from("."),
                },
            },
            other => panic!("unknown probe case: {other}"),
        };

        let mode = if case.starts_with("quiet") {
            RunMode::Quiet
        } else {
            RunMode::Text
        };

        workflow.run(mode).unwrap();
    }

    #[test]
    fn test_workflow_run_executes_steps_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let wf = Workflow {
            steps: vec![
                test_step("first", "first", Arc::clone(&seen)),
                test_step("second", "second", Arc::clone(&seen)),
            ],
            ctx: WorkflowContext {
                task_id: None,
                plan_path: None,
                cwd: PathBuf::from("."),
            },
        };

        let ctx = wf.run(RunMode::Quiet).unwrap();
        assert_eq!(ctx.task_id.as_deref(), Some("second"));
        assert_eq!(ctx.plan_path.as_deref(), Some("second"));
        assert_eq!(*seen.lock().unwrap(), vec!["first", "second"]);
    }

    #[test]
    fn test_workflow_run_stops_on_failure_and_returns_error() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let wf = Workflow {
            steps: vec![
                test_step("first", "first", Arc::clone(&seen)),
                test_step_fail("broken", "boom"),
                test_step("third", "third", Arc::clone(&seen)),
            ],
            ctx: WorkflowContext {
                task_id: None,
                plan_path: None,
                cwd: PathBuf::from("."),
            },
        };

        let err = match wf.run(RunMode::Quiet) {
            Ok(_) => panic!("workflow should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("boom"));
        assert_eq!(*seen.lock().unwrap(), vec!["first"]);
    }

    #[test]
    fn test_workflow_context_mutations_are_visible_to_next_step() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let wf = Workflow {
            steps: vec![
                test_step("set", "shared-state", Arc::clone(&seen)),
                test_step_assert_plan_path("assert", "shared-state", Arc::clone(&seen)),
            ],
            ctx: WorkflowContext {
                task_id: None,
                plan_path: None,
                cwd: PathBuf::from("."),
            },
        };

        let ctx = wf.run(RunMode::Quiet).unwrap();
        assert_eq!(ctx.plan_path.as_deref(), Some("shared-state"));
        assert_eq!(*seen.lock().unwrap(), vec!["set", "assert"]);
    }

    #[test]
    fn test_run_mode_text_emits_sections_only_for_steps_with_headers() {
        let output = run_output_probe("text-sectioned");
        assert!(output.contains("── Workflow Test Section ──"));
        assert_eq!(output.matches("── Workflow Test Section ──").count(), 1);
        assert!(output.contains("⠙ workflow-text-with-section..."));
        assert!(output.contains("⠙ workflow-text-without-section..."));
    }

    #[test]
    fn test_run_mode_text_produces_status_output_and_quiet_does_not() {
        let text_output = run_output_probe("text-basic");
        let quiet_output = run_output_probe("quiet-basic");

        assert!(text_output.contains("⠙ workflow-text-step..."));
        assert!(text_output.contains("合 workflow-text-step"));
        assert!(!quiet_output.contains("workflow-quiet-step"));
    }

    #[test]
    fn test_same_workflow_can_run_in_text_or_quiet_mode() {
        let text_ctx = Workflow {
            steps: vec![test_step(
                "step",
                "shared",
                Arc::new(Mutex::new(Vec::new())),
            )],
            ctx: WorkflowContext {
                task_id: None,
                plan_path: None,
                cwd: PathBuf::from("."),
            },
        }
        .run(RunMode::Text)
        .unwrap();
        let quiet_ctx = Workflow {
            steps: vec![test_step(
                "step",
                "shared",
                Arc::new(Mutex::new(Vec::new())),
            )],
            ctx: WorkflowContext {
                task_id: None,
                plan_path: None,
                cwd: PathBuf::from("."),
            },
        }
        .run(RunMode::Quiet)
        .unwrap();
        let text_output = run_output_probe("text-shared");
        let quiet_output = run_output_probe("quiet-shared");

        assert_eq!(text_ctx.task_id, quiet_ctx.task_id);
        assert_eq!(text_ctx.plan_path, quiet_ctx.plan_path);
        assert!(text_output.contains("workflow-shared-step"));
        assert!(!quiet_output.contains("workflow-shared-step"));
    }
}

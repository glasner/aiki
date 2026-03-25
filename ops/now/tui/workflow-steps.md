# Workflow Steps: Composable Build/Review/Fix Orchestration

**Date**: 2026-03-25
**Status**: Plan
**Followup**: [live-tui.md](../live-tui.md) (TUI integration, separate project)

---

## Problem

The build/review/fix commands have monolithic orchestration code:

1. **Duplicates workflow logic** — the TTY path, non-TTY sync path, and `--async` background path all sequence decompose→loop→review→fix independently
2. **Isn't composable** — `aiki review` can't reuse build's review phase; `aiki fix` rebuilds the same decompose→loop→review cycle from scratch
3. **Mixes orchestration with output** — command functions interleave println/eprintln with business logic

## Solution

Each command defines its own **step enum** (e.g. `BuildStep`, `ReviewStep`).
All step enums implement a shared **`WorkflowStep`** trait. A generic
**`Workflow<S>`** drives any step type through a shared runner that handles
text and quiet modes (live TUI is a future enhancement — see [live-tui.md](../live-tui.md)).

Steps read and write a shared **`WorkflowContext`** that carries the root
task ID and plan path — set by early steps (like Init), consumed by
later steps (Decompose, Loop, Review).

```
┌─────────────────────────────────────────────────┐
│              Workflow<S>::run(mode)              │
│                                                 │
│  match mode {                                   │
│    Text  → iterate steps + eprintln             │
│    Quiet → iterate steps, silent                │
│  }                                              │
│                                                 │
│  for step in steps {                            │
│    result = step.run(ctx)                       │
│  }                                              │
└─────────────────────────────────────────────────┘
```

## File Layout

```
cli/src/
├── workflow.rs       ← WorkflowStep trait, Workflow<S>, WorkflowContext, RunMode,
│                        StepResult                                   ~100 lines
├── commands/
│   ├── build.rs      ← BuildStep enum + run() + build_workflow()
│   │                    + CLI args + validation                      (REFACTORED)
│   ├── review.rs     ← ReviewStep enum + run() + review_workflow()
│   │                    + CLI args                                   (REFACTORED)
│   ├── fix.rs        ← FixStep enum + run() + fix_workflow()
│   │                    + CLI args                                   (REFACTORED)
│   ├── decompose.rs  ← UNCHANGED (CLI entry + run_decompose)
│   └── loop_cmd.rs   ← UNCHANGED (CLI entry + run_loop)
```

One file (`workflow.rs`) for the generic machinery — trait, runner, context.
Step enums live in the command files alongside their args and validation,
keeping everything about a command self-contained.

## Core Types

### WorkflowContext (shared across all workflows)

```rust
// workflow.rs

/// Shared mutable context passed through all steps in a workflow.
/// Early steps (Init) populate fields, later steps consume them.
pub struct WorkflowContext {
    /// Root task this workflow operates on (epic, review target, etc.).
    /// Set by Init step, read by Decompose/Loop/Review.
    pub task_id: Option<String>,
    /// Plan path (if applicable). Set at construction or by Init.
    pub plan_path: Option<String>,
    /// Working directory.
    pub cwd: PathBuf,
}
```

### WorkflowStep trait

```rust
// workflow.rs

pub struct StepResult {
    pub message: String,
    pub task_id: Option<String>,
}

pub trait WorkflowStep: Send {
    fn name(&self) -> &'static str;
    fn section(&self) -> Option<&'static str> { None }
    fn run(&self, ctx: &mut WorkflowContext) -> anyhow::Result<StepResult>;
}
```

### Workflow and RunMode

```rust
// workflow.rs

pub enum RunMode {
    /// Sequential on main thread, minimal text output
    Text,
    /// Silent — background/async processes
    Quiet,
}

pub struct Workflow<S: WorkflowStep> {
    pub steps: Vec<S>,
    pub ctx: WorkflowContext,
}

impl<S: WorkflowStep + 'static> Workflow<S> {
    pub fn run(mut self, mode: RunMode) -> Result<()> {
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
                            return Err(e.into());
                        }
                    }
                }
                Ok(())
            }
            RunMode::Quiet => {
                for step in &self.steps {
                    step.run(&mut self.ctx)?;
                }
                Ok(())
            }
        }
    }
}
```

## Build Workflow (in commands/build.rs)

Step enum, execution, and assembly colocated with CLI args.

**Step names:**
- `plan` — validate plan, show path
- `decompose` — find/create epic, set ctx.task_id, run decompose agent
- `loop` — run lane orchestrator
- `review` — run code review
- `fix` — write fix plan (only in iteration cycles)
- `review for regressions` — regression review after fix cycle

The "Initial Build" section header is emitted by the Decompose step's
`section()`.

**Future enhancement:** Live TUI rendering is planned as a followup — see
[live-tui.md](../live-tui.md).

```rust
// commands/build.rs

pub enum BuildStep {
    /// Validate plan file. Shows plan path on completion.
    Plan,
    /// Find/create epic + run decompose agent. Sets ctx.task_id.
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
    /// Run code review on completed build.
    Review {
        template: Option<String>,
        agent: Option<String>,
    },
    /// Write fix plan from review issues. Only used in iteration cycles.
    Fix {
        review_id: String,
        template: Option<String>,
        agent: Option<String>,
    },
    /// Regression review after fix cycle — checks original scope for new issues.
    RegressionReview {
        template: Option<String>,
        agent: Option<String>,
    },
}

impl WorkflowStep for BuildStep {
    fn name(&self) -> &'static str {
        match self {
            BuildStep::Plan                    => "plan",
            BuildStep::Decompose { .. }        => "decompose",
            BuildStep::Loop { .. }             => "loop",
            BuildStep::Review { .. }           => "review",
            BuildStep::Fix { .. }              => "fix",
            BuildStep::RegressionReview { .. } => "review for regressions",
        }
    }

    fn section(&self) -> Option<&'static str> {
        match self {
            BuildStep::Decompose { .. } => Some("Initial Build"),
            _ => None,
        }
    }

    fn run(&self, ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
        match self {
            BuildStep::Plan => {
                let plan_path = ctx.plan_path.as_deref().unwrap();
                validate_plan_path(&ctx.cwd, plan_path)?;
                Ok(StepResult {
                    message: plan_path.to_string(),
                    task_id: None,
                })
            }

            BuildStep::Decompose { restart, template, agent } => {
                let plan_path = ctx.plan_path.as_deref().unwrap();
                let epic_id = find_or_create_epic(&ctx.cwd, plan_path, ...)?;
                check_epic_blockers(&ctx.cwd, &epic_id)?;
                ctx.task_id = Some(epic_id.clone());

                // Skip decompose if subtasks already exist (resume case)
                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                if !get_subtasks(&graph, &epic_id).is_empty() {
                    return Ok(StepResult {
                        message: "Subtasks exist, skipping".into(),
                        task_id: None,
                    });
                }

                let opts = DecomposeOptions { template: template.clone(), agent: *agent };
                let decompose_id = run_decompose(&ctx.cwd, plan_path, &epic_id, opts, false)?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                let count = get_subtasks(&graph, &epic_id).len();
                Ok(StepResult {
                    message: format!("{} subtasks created", count),
                    task_id: Some(decompose_id),
                })
            }

            BuildStep::Loop { template, agent } => {
                let task_id = ctx.task_id.as_deref().unwrap();
                let mut opts = LoopOptions::new();
                if let Some(a) = agent { opts = opts.with_agent(*a); }
                if let Some(t) = template { opts = opts.with_template(t.clone()); }
                let loop_id = run_loop(&ctx.cwd, task_id, opts, false)?;
                Ok(StepResult {
                    message: "All lanes complete".into(),
                    task_id: Some(loop_id),
                })
            }

            BuildStep::Review { template, agent } => {
                let task_id = ctx.task_id.as_deref().unwrap();
                let plan_path = ctx.plan_path.as_deref().unwrap();

                let result = create_review(&ctx.cwd, CreateReviewParams {
                    scope: ReviewScope {
                        kind: ReviewScopeKind::Code,
                        id: plan_path.to_string(),
                        task_ids: vec![],
                    },
                    agent_override: agent.clone(),
                    template: template.clone(),
                    fix_template: None,
                    autorun: false,
                })?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                write_link_event(&ctx.cwd, &graph, "validates", &result.review_task_id, task_id)?;

                task_run(&ctx.cwd, &result.review_task_id, TaskRunOptions::new().quiet())?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                let issue_count = graph.tasks.get(&result.review_task_id)
                    .and_then(|t| t.data.get("issue_count"))
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(0);

                let message = if issue_count > 0 {
                    format!("Found {} issues", issue_count)
                } else {
                    "approved".into()
                };

                Ok(StepResult { message, task_id: Some(result.review_task_id) })
            }

            BuildStep::Fix { review_id, template, agent } => {
                let fix_task_id = run_fix_plan(&ctx.cwd, review_id, template.as_deref(), agent.as_deref())?;

                Ok(StepResult {
                    message: "plan written".into(),
                    task_id: Some(fix_task_id),
                })
            }

            BuildStep::RegressionReview { template, agent } => {
                // Same as Review but checks the original scope for regressions
                let task_id = ctx.task_id.as_deref().unwrap();
                let plan_path = ctx.plan_path.as_deref().unwrap();

                let result = create_review(&ctx.cwd, CreateReviewParams {
                    scope: ReviewScope {
                        kind: ReviewScopeKind::Code,
                        id: plan_path.to_string(),
                        task_ids: vec![],
                    },
                    agent_override: agent.clone(),
                    template: template.clone(),
                    fix_template: None,
                    autorun: false,
                })?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                write_link_event(&ctx.cwd, &graph, "validates", &result.review_task_id, task_id)?;

                task_run(&ctx.cwd, &result.review_task_id, TaskRunOptions::new().quiet())?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                let issue_count = graph.tasks.get(&result.review_task_id)
                    .and_then(|t| t.data.get("issue_count"))
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(0);

                let message = if issue_count > 0 {
                    format!("Found {} issues", issue_count)
                } else {
                    "approved".into()
                };

                Ok(StepResult { message, task_id: Some(result.review_task_id) })
            }
        }
    }
}

// ── Assembly ─────────────────────────────────────────────────────────

pub struct BuildOpts {
    pub restart: bool,  // passed to Decompose step
    pub decompose_template: Option<String>,
    pub loop_template: Option<String>,
    pub agent: Option<AgentType>,
    pub agent_str: Option<String>,
    pub review_after: bool,
    pub review_template: Option<String>,
    pub fix_after: bool,
    pub fix_template: Option<String>,
}

pub fn build_workflow(plan_path: &str, opts: BuildOpts) -> Workflow<BuildStep> {
    let mut steps = vec![];

    steps.push(BuildStep::Plan);

    steps.push(BuildStep::Decompose {
        restart: opts.restart,
        template: opts.decompose_template,
        agent: opts.agent,
    });

    steps.push(BuildStep::Loop {
        template: opts.loop_template,
        agent: opts.agent,
    });

    if opts.review_after {
        steps.push(BuildStep::Review {
            template: opts.review_template,
            agent: opts.agent_str,
        });
    }

    Workflow {
        steps,
        ctx: WorkflowContext {
            task_id: None,  // set by Decompose step
            plan_path: Some(plan_path.to_string()),
            cwd: std::env::current_dir().unwrap(),
        },
    }
}
```

## Review Workflow (in commands/review.rs)

Handles Flows 4–7 (plan review, code review, task review, session review).
Single step — the scope varies based on the review target.

```rust
// commands/review.rs

pub enum ReviewStep {
    /// Run a review. Scope determined by the review target.
    Review {
        scope: ReviewScope,
        template: Option<String>,
        agent: Option<String>,
    },
}

impl WorkflowStep for ReviewStep {
    fn name(&self) -> &'static str { "review" }

    fn run(&self, ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
        let ReviewStep::Review { scope, template, agent } = self;

        let result = create_review(&ctx.cwd, CreateReviewParams {
            scope: scope.clone(),
            agent_override: agent.clone(),
            template: template.clone(),
            fix_template: None,
            autorun: false,
        })?;

        task_run(&ctx.cwd, &result.review_task_id, TaskRunOptions::new().quiet())?;

        let graph = materialize_graph(&read_events(&ctx.cwd)?);
        let issue_count = graph.tasks.get(&result.review_task_id)
            .and_then(|t| t.data.get("issue_count"))
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0);

        let message = if issue_count > 0 {
            format!("Found {} issues", issue_count)
        } else {
            "approved".into()
        };

        Ok(StepResult { message, task_id: Some(result.review_task_id) })
    }
}

pub fn review_workflow(scope: ReviewScope, opts: ReviewOpts) -> Workflow<ReviewStep> {
    Workflow {
        steps: vec![ReviewStep::Review {
            scope,
            template: opts.template,
            agent: opts.agent,
        }],
        ctx: WorkflowContext {
            task_id: None,
            plan_path: None,
            cwd: std::env::current_dir().unwrap(),
        },
    }
}
```

## Fix Workflow (in commands/fix.rs)

Handles Flow 8 (`aiki fix <review-id>`).
Steps: fix → decompose → loop → review → regression review.

```rust
// commands/fix.rs

pub enum FixStep {
    /// Write fix plan from review issues.
    Fix {
        review_id: String,
        template: Option<String>,
        agent: Option<String>,
    },
    /// Decompose fix plan into subtasks.
    Decompose {
        template: Option<String>,
        agent: Option<AgentType>,
    },
    /// Run fix subtasks.
    Loop {
        template: Option<String>,
        agent: Option<AgentType>,
    },
    /// Review the fix changes.
    Review {
        template: Option<String>,
        agent: Option<String>,
    },
    /// Regression review — check original scope.
    RegressionReview {
        template: Option<String>,
        agent: Option<String>,
    },
}

impl WorkflowStep for FixStep {
    fn name(&self) -> &'static str {
        match self {
            FixStep::Fix { .. }              => "fix",
            FixStep::Decompose { .. }        => "decompose",
            FixStep::Loop { .. }             => "loop",
            FixStep::Review { .. }           => "review",
            FixStep::RegressionReview { .. } => "review for regressions",
        }
    }

    fn run(&self, ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
        match self {
            FixStep::Fix { review_id, template, agent } => {
                let fix_task_id = run_fix_plan(&ctx.cwd, review_id, template.as_deref(), agent.as_deref())?;
                // ctx.task_id set to the fix parent for decompose/loop
                ctx.task_id = Some(fix_task_id.clone());

                Ok(StepResult { message: "plan written".into(), task_id: Some(fix_task_id) })
            }

            FixStep::Decompose { template, agent } => {
                // Same as BuildStep::Decompose but for fix parent
                let task_id = ctx.task_id.as_deref().unwrap();
                let opts = DecomposeOptions { template: template.clone(), agent: *agent };
                let id = run_decompose(&ctx.cwd, /* fix plan path */, task_id, opts, false)?;
                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                let count = get_subtasks(&graph, task_id).len();
                Ok(StepResult { message: format!("{} subtasks created", count), task_id: Some(id) })
            }

            FixStep::Loop { template, agent } => {
                let task_id = ctx.task_id.as_deref().unwrap();
                let mut opts = LoopOptions::new();
                if let Some(a) = agent { opts = opts.with_agent(*a); }
                if let Some(t) = template { opts = opts.with_template(t.clone()); }
                let id = run_loop(&ctx.cwd, task_id, opts, false)?;
                Ok(StepResult { message: "All lanes complete".into(), task_id: Some(id) })
            }

            FixStep::Review { template, agent } => {
                // Review the fix changes
                // ... create review, link, run, check issues ...
                // (same pattern as BuildStep::Review)
                Ok(StepResult { message: "approved".into(), task_id: None })
            }

            FixStep::RegressionReview { template, agent } => {
                // Check original scope for regressions
                // (same pattern as BuildStep::RegressionReview)
                Ok(StepResult { message: "approved".into(), task_id: None })
            }
        }
    }
}

pub fn fix_workflow(review_id: &str, opts: FixOpts) -> Workflow<FixStep> {
    Workflow {
        steps: vec![
            FixStep::Fix { review_id: review_id.into(), template: opts.fix_template, agent: opts.agent.clone() },
            FixStep::Decompose { template: opts.decompose_template, agent: opts.agent_type },
            FixStep::Loop { template: opts.loop_template, agent: opts.agent_type },
            FixStep::Review { template: opts.review_template, agent: opts.agent },
            FixStep::RegressionReview { template: None, agent: opts.agent },
        ],
        ctx: WorkflowContext { task_id: None, plan_path: None, cwd: std::env::current_dir().unwrap() },
    }
}
```

**State 8.8 (no actionable issues):** Handled in `FixStep::Fix` run() —
if the review has no actionable issues, return early with
`StepResult { message: "approved — no actionable issues" }` and the
workflow completes without running Decompose/Loop/Review. This requires
either short-circuiting in `run()` or the driver skipping remaining steps
when Fix returns an early-exit signal.

## Command Files (Thin Wrappers)

### commands/build.rs

```rust
pub fn run(args: BuildArgs) -> Result<()> {
    let opts = BuildOpts {
        restart: args.restart,
        decompose_template: args.decompose_template,
        loop_template: args.loop_template,
        agent: parse_agent(&args.agent)?,
        agent_str: args.agent,
        review_after: args.review || args.review_template.is_some() || args.fix,
        review_template: args.review_template,
        fix_after: args.fix,
        fix_template: args.fix_template,
    };

    let wf = build_workflow(&target, opts);

    let mode = if args.run_async {
        spawn_aiki_background(cwd, &continue_async_args)?;
        return Ok(());
    } else {
        RunMode::Text
    };

    wf.run(mode)
}

fn run_continue_async(cwd: &Path, epic_id: &str, args: BuildArgs) -> Result<()> {
    let wf = build_workflow(&plan_path, opts);
    wf.run(RunMode::Quiet)
}
```

### commands/review.rs

```rust
pub fn run(args: ReviewArgs) -> Result<()> {
    let wf = review_workflow(&target, opts);
    wf.run(mode)
}
```

### commands/fix.rs

```rust
pub fn run(args: FixArgs) -> Result<()> {
    let wf = fix_workflow(&review_id, opts);
    wf.run(mode)
}
```

## Fix Iterations

After a Review step in `commands/build.rs` completes with issues, the
driver dynamically extends the step queue. This is handled by overriding
`drive()` behavior for `BuildStep` specifically (since only build
workflows have fix iterations):

```rust
// commands/build.rs — custom drive logic

pub fn drive_build(
    steps: &[BuildStep],
    ctx: &mut WorkflowContext,
) -> anyhow::Result<()> {
    let mut queue: VecDeque<BuildStep> = steps.iter().cloned().collect();
    let mut iteration = 1u16;

    while let Some(step) = queue.pop_front() {
        match step.run(ctx) {
            Ok(result) => {
                // After review: check for issues → inject fix cycle
                if let BuildStep::Review { template, agent, .. } = &step {
                    if let Some(ref review_id) = result.task_id {
                        if has_actionable_issues(&ctx.cwd, review_id)
                            && iteration < MAX_ITERATIONS
                        {
                            iteration += 1;
                            queue.push_back(BuildStep::Fix {
                                review_id: review_id.clone(),
                                template: None,
                                agent: agent.clone(),
                            });
                            queue.push_back(BuildStep::Decompose { ... });
                            queue.push_back(BuildStep::Loop { ... });
                            queue.push_back(BuildStep::Review {
                                template: template.clone(),
                                agent: agent.clone(),
                            });
                            queue.push_back(BuildStep::RegressionReview {
                                template: template.clone(),
                                agent: agent.clone(),
                            });
                        }
                    }
                }

                // After regression review: check for new issues → another cycle
                if let BuildStep::RegressionReview { template, agent } = &step {
                    if let Some(ref review_id) = result.task_id {
                        if has_actionable_issues(&ctx.cwd, review_id)
                            && iteration < MAX_ITERATIONS
                        {
                            iteration += 1;
                            queue.push_back(BuildStep::Fix { ... });
                            queue.push_back(BuildStep::Decompose { ... });
                            queue.push_back(BuildStep::Loop { ... });
                            queue.push_back(BuildStep::Review { ... });
                            queue.push_back(BuildStep::RegressionReview { ... });
                        }
                    }
                }
            }
            Err(e) => { return Err(e); }
        }
    }
    Ok(())
}
```

Fix iteration logic lives entirely in `commands/build.rs` — the generic
`Workflow` doesn't know about it. The `Workflow<BuildStep>::run()`
method can call `drive_build()` instead of the generic `drive()`.

**Fix iteration sequence:**
1. `fix` — write fix plan from review issues (3.13–3.14)
2. `decompose` — decompose fix plan into subtasks (3.15)
3. `loop` — run fix subtasks (3.16)
4. `review` — review the fix changes (3.17)
5. `review for regressions` — check original scope for regressions (3.18)
6. If regression review finds issues → back to step 1 (3.20)

## Behaviors to Preserve (from existing implementation audit)

These behaviors exist in the current commands and must not regress.

### Build command

1. **Epic ID target** — `aiki build <epic-id>` skips plan validation,
   stale cleanup, and epic creation. Goes straight to loop. The workflow
   assembly must detect epic ID vs plan path and adjust steps accordingly
   (skip Plan and Decompose, or have Decompose detect an existing epic
   and skip to loop). Currently handled by `run_build_epic()`.

2. **Draft plan check** — `parse_plan_metadata().draft` prevents building
   draft plans. Must be in the Plan step's `run()`.

3. **Stale build cleanup** — `cleanup_stale_builds()` closes stale
   orchestrator tasks for this plan before starting. Must be in the
   Plan or Decompose step.

4. **Epic resume** — When an incomplete epic with subtasks exists,
   `restart_epic()` stops stale in-progress subtasks before continuing.
   The Decompose step's "skip if subtasks exist" path must also do this.

5. **`--restart` undo** — Calls `undo_completed_subtasks()` to revert file
   changes before closing the old epic. Must be in Decompose (or a
   pre-step) when `restart` is set.

6. **`--output id`** — Prints bare task IDs to stdout, no status text.
   Needs its own `RunMode::Id` or a flag that suppresses status output
   and prints IDs after completion.

7. **Post-run output** — After workflow completes, `output_build_show()`
   prints a text summary. The command must call this after `wf.run()`
   returns.

8. **`--async` arg forwarding** — The `--async` path spawns a background
   process with `--_continue-async <epic-id>` plus all template/agent
   flags. The workflow plan must assemble these args from `BuildOpts`.

9. **`build show` subcommand** — Not a workflow, stays as-is.

### Fix command

10. **Two-phase review decision** — After fix-parent review passes, the
    original scope is re-reviewed for regressions. The `ReviewOutcome`
    enum (`LoopBack` / `ReReviewOriginalScope` / `Approved`) drives
    this. The workflow's `drive_build()` fix iteration logic must
    replicate this: Review step = review fix changes, RegressionReview
    step = re-review original scope.

11. **Fix-parent task creation** — `create_fix_parent()` creates a
    container task with `data.review`, `data.scope_kind`, `data.scope_id`
    and a `remediates` link. Must happen before decompose in fix flows.

12. **Plan-fix task + plan file cleanup** — Creates a plan-fix task from
    template, runs it (agent writes plan to `/tmp/aiki/plans/{}.md`),
    then deletes the plan file after decompose.

13. **`--once` flag** — Skips post-fix review. The workflow must either
    omit Review/RegressionReview steps or short-circuit in the driver.

14. **Assignee resolution** — For task-scoped reviews, assignee comes
    from the original task; for other scopes, from the review task.
    `determine_followup_assignee()` handles this.

15. **`has_actionable_issues()` short-circuit** — If review has no
    actionable issues, fix command returns immediately with "approved".

16. **MAX_QUALITY_ITERATIONS = 10** — Quality loop cap with warning
    output when exhausted.

### Review command

17. **`detect_target()` scope resolution** — Resolves CLI argument to a
    `ReviewScope` (task ID, file path, `--code`, or session). Must
    happen before workflow assembly, not inside a step.

18. **`--start` flag** — Creates review task but doesn't run it. Returns
    immediately. Not a workflow — handled in command file.

19. **`--fix` flag on review** — After review completes with issues,
    pipes into `run_fix()`. This is a review→fix composite workflow.
    Assembly: `[ReviewStep::Review] + if issues → fix_workflow`.

20. **`--autorun` flag** — Sets autorun on the `validates` link. Passed
    through to `create_review()`.

## What Changes

| File | Change |
|------|--------|
| `cli/src/workflow.rs` | **New.** WorkflowStep trait, Workflow<S>, WorkflowContext, RunMode |
| `cli/src/commands/build.rs` | **Refactored.** Add BuildStep enum + build_workflow(). Worker closure + non-TTY path → wf.run(mode) |
| `cli/src/commands/review.rs` | **Refactored.** Add ReviewStep enum + review_workflow(). → wf.run(mode) |
| `cli/src/commands/fix.rs` | **Refactored.** Add FixStep enum + fix_workflow(). → wf.run(mode) |
| `cli/src/commands/decompose.rs` | No change |
| `cli/src/commands/loop_cmd.rs` | No change |

## What We Keep

- `run_decompose()`, `run_loop()` as workhorses (steps call them)
- `output_*` functions (still used by direct CLI invocations)

## What We Remove

- 400-line worker closure in build.rs (replaced by BuildStep enum)
- 80-line non-TTY sync path in build.rs (replaced by RunMode::Text)
- 70-line `run_continue_async` body (replaced by RunMode::Quiet)
- `run_build_review` helper (replaced by BuildStep::Review)
- Duplicate sequencing logic across build/fix/review commands

## Refactor Safety Net

Pre-refactor behavioral tests were added to catch regressions during
the workflow refactor. These tests verify observable behavior (not
implementation details) and **MUST continue to pass throughout the
refactor**.

### Tests in `commands/build.rs`

| Test | Behavior | Preserves |
|------|----------|-----------|
| `test_draft_plan_rejected` | Draft plans cannot be built | #2 (draft check) |
| `test_epic_resume_skips_decompose_when_subtasks_exist` | Existing subtasks → skip decompose | #4 (epic resume) |
| `test_restart_closes_existing_epic` | `--restart` closes old epic | #5 (--restart) |
| `test_build_epic_id_skips_plan_validation` | Epic ID target skips file checks | #1 (epic ID) |

### Tests in `commands/fix.rs`

| Test | Behavior | Preserves |
|------|----------|-----------|
| `test_no_actionable_issues_returns_approved` | No issues → early exit | #15 (short-circuit) |
| `test_once_flag_skips_post_fix_review` | `--once` → no review after loop | #13 (--once) |
| `test_max_iterations_cap` | Loop stops at 10 | #16 (MAX_QUALITY) |
| `test_fix_parent_data_fields` | Fix-parent has review/scope data | #11 (fix-parent) |
| `test_two_phase_review_re_reviews_original_scope` | Pass → re-review original | #10 (two-phase) |

### Tests in integration tests (`test_async_tasks.rs`)

| Test | Behavior | Preserves |
|------|----------|-----------|
| `test_review_fix_and_start_conflict` | `--fix` + `--start` → error | #18 (--start) |
| `test_review_output_id_no_extra_output` | `--output id` → bare IDs only | #6 (--output id) |

### During implementation

When refactoring a command to use workflows:

1. **Run existing tests first** — `cargo test -p aiki-cli` should pass
   before you start. If any test above fails, fix it before proceeding.
2. **Keep helper functions** — `validate_plan_path()`, `cleanup_stale_builds()`,
   `find_or_create_epic()`, `create_fix_parent()`, `has_actionable_issues()`,
   `determine_review_outcome()` stay as-is. Steps call them.
3. **Run tests after each step** — After wiring each command to the workflow,
   all tests should still pass. If a test breaks, the workflow step is
   missing behavior from the original code.
4. **Don't delete tests** — The safety net tests should survive permanently.
   They test the command's contract, not its internals.

## Implementation Steps

1. **Verify safety net** — Run `cargo test -p aiki-cli` and confirm all
   pre-refactor behavioral tests pass on current code
2. Create `cli/src/workflow.rs` with WorkflowStep, Workflow<S>, WorkflowContext, RunMode
3. Add BuildStep enum + build_workflow() to `commands/build.rs`
4. Refactor `commands/build.rs` to use workflow (Text + Quiet paths)
   — **Run tests, verify safety net passes**
5. Add ReviewStep enum + review_workflow() to `commands/review.rs`
6. Refactor `commands/review.rs` to use workflow — **Run tests**
7. Add FixStep enum + fix_workflow() to `commands/fix.rs`
8. Refactor `commands/fix.rs` to use workflow — **Run tests**
9. Add fix iteration logic to build's drive_build()
10. Remove dead code: worker closures, run_build_review, non-TTY paths
    — **Run tests, final verification**
11. Add new tests: workflow step sequencing, RunMode switching

**Future enhancement:** Live TUI rendering — see [live-tui.md](../live-tui.md)

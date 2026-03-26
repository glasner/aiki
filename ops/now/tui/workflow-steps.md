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

A single **`Step`** enum in `workflow.rs` defines every step that any
workflow can use. Commands compose workflows by assembling different
sequences of `Step` variants — no per-command step enums, no duplicated
`run()` implementations.

A **`Workflow`** struct drives steps through a shared runner that handles
text and quiet modes (live TUI is a future enhancement — see [live-tui.md](../live-tui.md)).

Steps read and write a shared **`WorkflowContext`** that carries the root
task ID and plan path — set by early steps (like Decompose), consumed by
later steps (Loop, Review).

```
┌─────────────────────────────────────────────────┐
│              Workflow::run(mode)                 │
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
├── workflow.rs       ← Step enum, Workflow, WorkflowContext, RunMode,
│                        StepResult, all step run() impls            ~250 lines
├── commands/
│   ├── build.rs      ← build_workflow() assembly + CLI args         (REFACTORED)
│   ├── review.rs     ← review_workflow() assembly + CLI args        (REFACTORED)
│   ├── fix.rs        ← fix_workflow() assembly + CLI args           (REFACTORED)
│   ├── decompose.rs  ← UNCHANGED (CLI entry + run_decompose)
│   └── loop_cmd.rs   ← UNCHANGED (CLI entry + run_loop)
```

One file (`workflow.rs`) for the step enum and all step logic. Command
files become thin assemblers — they parse CLI args and call
`build_workflow()` / `fix_workflow()` / `review_workflow()` which return
a `Workflow` composed from the shared `Step` variants.

## Core Types (all in workflow.rs)

### WorkflowContext

```rust
/// Shared mutable context passed through all steps in a workflow.
/// Early steps (Decompose) populate fields, later steps consume them.
pub struct WorkflowContext {
    /// Root task this workflow operates on (epic, fix parent, etc.).
    /// Set by Decompose step, read by Loop/Review.
    pub task_id: Option<String>,
    /// Plan path (if applicable). Set at construction.
    pub plan_path: Option<String>,
    /// Working directory.
    pub cwd: PathBuf,
}
```

### StepResult

```rust
pub struct StepResult {
    pub message: String,
    pub task_id: Option<String>,
}
```

### Step enum (unified — used by all workflows)

One enum, one `run()` implementation. Commands compose workflows by
selecting which variants to include in their step sequence.

```rust
pub enum Step {
    /// Validate plan file. Shows plan path on completion.
    Plan,

    /// Find/create epic, set ctx.task_id, run decompose agent.
    /// Includes epic-finding logic (find_or_create_epic, check_epic_blockers,
    /// skip-if-subtasks-exist resume path).
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

    /// Run a review. Creates review task, links it, runs agent, counts issues.
    /// Used by build (post-build review), fix (post-fix review), and
    /// standalone `aiki review`.
    Review {
        scope: Option<ReviewScope>,  // Some = explicit scope (standalone review)
                                      // None = derive from ctx (build/fix review)
        template: Option<String>,
        agent: Option<String>,
    },

    /// Write fix plan from review issues. Sets ctx.task_id to fix parent.
    Fix {
        review_id: String,
        template: Option<String>,
        agent: Option<String>,
    },

    /// Regression review — re-review original scope after a fix cycle.
    /// Same logic as Review but uses original plan scope, not fix scope.
    RegressionReview {
        template: Option<String>,
        agent: Option<String>,
    },
}

impl Step {
    pub fn name(&self) -> &'static str {
        match self {
            Step::Plan                    => "plan",
            Step::Decompose { .. }        => "decompose",
            Step::Loop { .. }             => "loop",
            Step::Review { .. }           => "review",
            Step::Fix { .. }              => "fix",
            Step::RegressionReview { .. } => "review for regressions",
        }
    }

    pub fn section(&self) -> Option<&'static str> {
        match self {
            Step::Decompose { .. } => Some("Initial Build"),
            _ => None,
        }
    }

    pub fn run(&self, ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
        match self {
            Step::Plan => {
                let plan_path = ctx.plan_path.as_deref().unwrap();
                validate_plan_path(&ctx.cwd, plan_path)?;
                Ok(StepResult {
                    message: plan_path.to_string(),
                    task_id: None,
                })
            }

            Step::Decompose { restart, template, agent } => {
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

            Step::Loop { template, agent } => {
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

            Step::Review { scope, template, agent } => {
                let task_id = ctx.task_id.as_deref().unwrap();

                // Use explicit scope if provided, otherwise derive from ctx
                let review_scope = if let Some(s) = scope {
                    s.clone()
                } else {
                    let plan_path = ctx.plan_path.as_deref().unwrap();
                    ReviewScope {
                        kind: ReviewScopeKind::Code,
                        id: plan_path.to_string(),
                        task_ids: vec![],
                    }
                };

                let result = create_review(&ctx.cwd, CreateReviewParams {
                    scope: review_scope,
                    agent_override: agent.clone(),
                    template: template.clone(),
                    fix_template: None,
                    autorun: false,
                })?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                write_link_event(&ctx.cwd, &graph, "validates", &result.review_task_id, task_id)?;

                task_run(&ctx.cwd, &result.review_task_id, TaskRunOptions::new().quiet())?;

                let graph = materialize_graph(&read_events(&ctx.cwd)?);
                let issue_count = count_issues(&graph, &result.review_task_id);

                let message = if issue_count > 0 {
                    format!("Found {} issues", issue_count)
                } else {
                    "approved".into()
                };

                Ok(StepResult { message, task_id: Some(result.review_task_id) })
            }

            Step::Fix { review_id, template, agent } => {
                let fix_task_id = run_fix_plan(&ctx.cwd, review_id, template.as_deref(), agent.as_deref())?;
                ctx.task_id = Some(fix_task_id.clone());

                Ok(StepResult {
                    message: "plan written".into(),
                    task_id: Some(fix_task_id),
                })
            }

            Step::RegressionReview { template, agent } => {
                // Same as Review but always uses original plan scope
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
                let issue_count = count_issues(&graph, &result.review_task_id);

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

/// Helper — extract issue count from a review task's data.
fn count_issues(graph: &TaskGraph, review_task_id: &str) -> usize {
    graph.tasks.get(review_task_id)
        .and_then(|t| t.data.get("issue_count"))
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0)
}
```

### Workflow and RunMode

```rust
pub enum RunMode {
    /// Sequential on main thread, minimal text output
    Text,
    /// Silent — background/async processes
    Quiet,
}

pub struct Workflow {
    pub steps: Vec<Step>,
    pub ctx: WorkflowContext,
}

impl Workflow {
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

## Workflow Assembly (in command files)

Command files are thin assemblers — they parse CLI args and build a
`Vec<Step>` from the shared variants. No per-command step enums.

### build_workflow() (in commands/build.rs)

```rust
pub fn build_workflow(plan_path: &str, opts: BuildOpts) -> Workflow {
    let mut steps = vec![];

    steps.push(Step::Plan);

    steps.push(Step::Decompose {
        restart: opts.restart,
        template: opts.decompose_template,
        agent: opts.agent,
    });

    steps.push(Step::Loop {
        template: opts.loop_template,
        agent: opts.agent,
    });

    if opts.review_after {
        steps.push(Step::Review {
            scope: None,  // derive from ctx.plan_path
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

### review_workflow() (in commands/review.rs)

```rust
pub fn review_workflow(scope: ReviewScope, opts: ReviewOpts) -> Workflow {
    Workflow {
        steps: vec![Step::Review {
            scope: Some(scope),  // explicit scope for standalone review
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

### fix_workflow() (in commands/fix.rs)

```rust
pub fn fix_workflow(review_id: &str, opts: FixOpts) -> Workflow {
    Workflow {
        steps: vec![
            Step::Fix { review_id: review_id.into(), template: opts.fix_template, agent: opts.agent.clone() },
            Step::Decompose { restart: false, template: opts.decompose_template, agent: opts.agent_type },
            Step::Loop { template: opts.loop_template, agent: opts.agent_type },
            Step::Review { scope: None, template: opts.review_template, agent: opts.agent.clone() },
            Step::RegressionReview { template: None, agent: opts.agent },
        ],
        ctx: WorkflowContext { task_id: None, plan_path: None, cwd: std::env::current_dir().unwrap() },
    }
}
```

**State 8.8 (no actionable issues):** Handled in `Step::Fix` run() —
if the review has no actionable issues, return early with
`StepResult { message: "approved — no actionable issues" }` and the
workflow completes without running Decompose/Loop/Review. This requires
either short-circuiting in `run()` or the driver skipping remaining steps
when Fix returns an early-exit signal.

## Command Files (Thin Wrappers)

Each command file parses CLI args, calls the appropriate `*_workflow()`
assembler, and runs it. All step logic lives in `workflow.rs`.

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

After a Review step completes with issues, the driver dynamically extends
the step queue. This is handled by a `drive_with_iterations()` method on
`Workflow` (or a standalone function in `workflow.rs`) that knows how to
inject fix cycles:

```rust
// workflow.rs — iteration-aware driver

pub fn drive_with_iterations(
    steps: Vec<Step>,
    ctx: &mut WorkflowContext,
) -> anyhow::Result<()> {
    let mut queue: VecDeque<Step> = steps.into();
    let mut iteration = 1u16;

    while let Some(step) = queue.pop_front() {
        match step.run(ctx) {
            Ok(result) => {
                // After review: check for issues → inject fix cycle
                if let Step::Review { template, agent, .. } = &step {
                    if let Some(ref review_id) = result.task_id {
                        if has_actionable_issues(&ctx.cwd, review_id)
                            && iteration < MAX_ITERATIONS
                        {
                            iteration += 1;
                            queue.push_back(Step::Fix {
                                review_id: review_id.clone(),
                                template: None,
                                agent: agent.clone(),
                            });
                            queue.push_back(Step::Decompose { ... });
                            queue.push_back(Step::Loop { ... });
                            queue.push_back(Step::Review {
                                scope: None,
                                template: template.clone(),
                                agent: agent.clone(),
                            });
                            queue.push_back(Step::RegressionReview {
                                template: template.clone(),
                                agent: agent.clone(),
                            });
                        }
                    }
                }

                // After regression review: check for new issues → another cycle
                if let Step::RegressionReview { template, agent } = &step {
                    if let Some(ref review_id) = result.task_id {
                        if has_actionable_issues(&ctx.cwd, review_id)
                            && iteration < MAX_ITERATIONS
                        {
                            iteration += 1;
                            queue.push_back(Step::Fix { ... });
                            queue.push_back(Step::Decompose { ... });
                            queue.push_back(Step::Loop { ... });
                            queue.push_back(Step::Review { ... });
                            queue.push_back(Step::RegressionReview { ... });
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

Build's `run()` calls `drive_with_iterations()` when `--fix` is enabled;
otherwise uses the simple `Workflow::run()`. Fix command always uses
`drive_with_iterations()`. The driver lives in `workflow.rs` alongside
the `Step` enum since it operates on the shared types.

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
| `cli/src/workflow.rs` | **New.** Step enum + run() impls, Workflow, WorkflowContext, RunMode, drive_with_iterations() |
| `cli/src/commands/build.rs` | **Refactored.** Remove BuildStep enum → use shared Step. Add build_workflow() assembler. Worker closure + non-TTY path → wf.run(mode) |
| `cli/src/commands/review.rs` | **Refactored.** Remove ReviewStep enum → use shared Step. Add review_workflow() assembler. → wf.run(mode) |
| `cli/src/commands/fix.rs` | **Refactored.** Remove FixStep enum → use shared Step. Add fix_workflow() assembler. → wf.run(mode) |
| `cli/src/commands/decompose.rs` | No change (CLI entry + `run_decompose()` workhorse) |
| `cli/src/commands/loop_cmd.rs` | No change (CLI entry + `run_loop()` workhorse) |

## What We Keep

- `run_decompose()`, `run_loop()` as workhorses (steps call them)
- `output_*` functions (still used by direct CLI invocations)

## What We Remove

- 400-line worker closure in build.rs (replaced by Step enum)
- 80-line non-TTY sync path in build.rs (replaced by RunMode::Text)
- 70-line `run_continue_async` body (replaced by RunMode::Quiet)
- `run_build_review` helper (replaced by Step::Review)
- Per-command step enums (BuildStep, FixStep, ReviewStep) — unified into Step
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

### Already done (per-command step enums)

These steps implemented the trait-based workflow infrastructure with
per-command step enums. They are complete and working.

1. ~~**Verify safety net**~~ — Pre-refactor behavioral tests pass
2. ~~**Create `cli/src/workflow.rs`**~~ — `WorkflowStep` trait,
   `Workflow<S>`, `WorkflowContext`, `RunMode`, `StepResult` + tests
   (step ordering, failure handling, context mutation, RunMode output)
3. ~~**Wire up `commands/build.rs`**~~ — `BuildStep` enum,
   `build_workflow()`, custom `run_build()` driver with fix iterations
   (`drive_build`), Text + Quiet modes working
4. ~~**Wire up `commands/review.rs`**~~ — `ReviewStep` enum,
   `review_workflow()`, wired into command's `run()`
5. ~~**Wire up `commands/fix.rs`**~~ — `FixStep` enum,
   `fix_workflow()`, `fix_pass_workflow()`, iteration logic with
   Review/RegressionReview steps

### Remaining (unify into single Step enum)

These steps consolidate the three per-command enums into a single
`Step` enum to eliminate duplicated run() logic across commands.

6. **Unify step enum** — Move `BuildStep`, `ReviewStep`, `FixStep`
   into a single `Step` enum in `workflow.rs`. Merge their `run()`
   implementations (Decompose, Loop, Review, Fix, RegressionReview,
   Plan all have one implementation each). Add `count_issues()` helper.
   — **Run tests, verify safety net passes**
7. **Convert command assemblers** — Change `build_workflow()`,
   `review_workflow()`, `fix_workflow()` to return `Workflow` (not
   `Workflow<BuildStep>` etc.), assembling `Vec<Step>` from shared
   variants. `Step::Review` gets `scope: Option<ReviewScope>` for
   standalone vs embedded use.
   — **Run tests**
8. **Move `drive_with_iterations()` to `workflow.rs`** — It now
   operates on `Step` variants, not `BuildStep`. Build's `run_build()`
   and fix's iteration logic both call it.
   — **Run tests**
9. **Remove per-command enums** — Delete `BuildStep`, `ReviewStep`,
   `FixStep` and their `WorkflowStep` impls. Remove the `WorkflowStep`
   trait (no longer needed — `Step` has methods directly).
   — **Run tests, final verification**

**Future enhancement:** Live TUI rendering — see [live-tui.md](../live-tui.md)

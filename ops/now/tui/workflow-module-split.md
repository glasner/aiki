# Workflow Module Split: Move Step Implementations to `workflow/`

**Date**: 2026-03-26
**Status**: Plan
**Prerequisite**: Workflow step enum refactor (workflow-steps.md steps 1–5 complete)

---

## Problem

Step `run()` implementations live in command files (`commands/build.rs`,
`commands/fix.rs`, `commands/review.rs`) but are called from `workflow.rs`.
This creates backwards dependencies — `workflow.rs` reaches into command
modules for `run_*_step` functions, and those functions import workflow
types. The step logic belongs in the workflow namespace.

## Solution

Convert `workflow.rs` into a `workflow/` module directory. Step
implementations move into per-domain files under `workflow/steps/`.
Command files become pure CLI wrappers — they parse args, assemble
workflows, and call `Workflow::run()`.

### Target layout

```
cli/src/workflow/
├── mod.rs            ← Step enum, Workflow, WorkflowContext, RunMode,
│                        StepResult, count_issues, drive_build, tests
└── steps/
    ├── mod.rs        ← pub mod declarations
    ├── plan.rs       ← run_plan_step()
    ├── decompose.rs  ← run_decompose_step()
    ├── loop.rs       ← run_loop_step()
    ├── review.rs     ← run_standalone_review_step(), run_review_step(),
    │                    run_regression_review_step()
    └── fix.rs        ← run_fix_step(), run_fix_plan_step(),
                         run_fix_decompose_step(), run_fix_loop_step(),
                         run_fix_review_step()
```

## What Moves Where

### From `commands/build.rs` → `workflow/steps/`

| Function | Destination | Lines |
|----------|-------------|-------|
| `run_plan_step()` | `steps/plan.rs` | ~28 |
| `run_decompose_step()` | `steps/decompose.rs` | ~82 |
| `run_loop_step()` | `steps/loop.rs` | ~26 |
| `run_review_step()` | `steps/review.rs` | ~55 |
| `run_fix_step()` | `steps/fix.rs` | ~65 |

### From `commands/fix.rs` → `workflow/steps/`

| Function | Destination | Lines |
|----------|-------------|-------|
| `run_fix_plan_step()` | `steps/fix.rs` | ~48 |
| `run_fix_decompose_step()` | `steps/fix.rs` | ~40 |
| `run_fix_loop_step()` | `steps/fix.rs` | ~26 |
| `run_fix_review_step()` | `steps/fix.rs` | ~45 |
| `run_regression_review_step()` | `steps/review.rs` | ~46 |
| `create_plan_fix_task()` | `steps/fix.rs` | ~40 |
| `run_task_with_show_tui()` | `steps/fix.rs` | ~15 |

### From `commands/review.rs` → `workflow/steps/`

| Function | Destination | Lines |
|----------|-------------|-------|
| `run_standalone_review_step()` | `steps/review.rs` | ~46 |

### Stays in place

| Item | File | Why |
|------|------|-----|
| `build_workflow()`, `build_workflow_from_epic()` | `commands/build.rs` | CLI-specific assembly |
| `fix_workflow()`, `fix_pass_workflow()` | `commands/fix.rs` | CLI-specific assembly |
| `review_workflow()` | `commands/review.rs` | CLI-specific assembly |
| `drive_build()`, `has_review_issues()` | `workflow/mod.rs` | Already in workflow |
| `Workflow::run_build()` | `workflow/mod.rs` | Already in workflow |
| `output_after_workflow()`, `output_build_show()` | `commands/build.rs` | CLI output |
| `validate_plan_path()`, `cleanup_stale_builds()` | `commands/build.rs` | Also called from build_async path |
| `undo_completed_subtasks()`, `close_epic()`, `close_epic_as_invalid()` | `commands/build.rs` + `epic.rs` | Shared with build_async and epic command |
| `restart_epic()`, `check_epic_blockers()` | `commands/build.rs` | Also called from build_async/build_from_epic |
| `create_fix_parent()`, `has_actionable_issues()` | `commands/fix.rs` | Also called from `run_fix()` orchestrator |

## Step::run() Dispatch After Move

The `Step::run()` method in `workflow/mod.rs` already dispatches to
named functions. After the move, the call paths change from
`build::run_plan_step(ctx)` to `steps::plan::run(ctx)` etc:

```rust
// workflow/mod.rs
mod steps;

impl Step {
    pub fn run(&self, ctx: &mut WorkflowContext) -> Result<StepResult> {
        match self {
            Step::Plan => steps::plan::run(ctx),
            Step::Decompose { restart, template, agent } =>
                steps::decompose::run(ctx, *restart, template.clone(), agent.clone()),
            Step::Loop { template, agent } =>
                steps::r#loop::run(ctx, template.clone(), agent.clone()),
            Step::Review { scope: Some(scope), template, agent, fix_template, autorun } =>
                steps::review::run_standalone(ctx, scope.clone(), template.clone(), agent.clone(), fix_template.clone(), *autorun),
            Step::Review { scope: None, template, agent, .. } =>
                steps::review::run_build(ctx, template.clone(), agent.clone()),
            Step::Fix { review_id, scope: Some(scope), assignee, template, autorun } =>
                steps::fix::run_plan(ctx, review_id, scope, assignee, template.as_deref(), *autorun),
            Step::Fix { review_id, scope: None, template, .. } =>
                steps::fix::run_build(ctx, review_id, template.clone(), None),
            Step::RegressionReview { template, agent } =>
                steps::review::run_regression(ctx, template.clone(), agent.clone()),
            #[cfg(test)]
            Step::_Test { handler, .. } => handler(ctx),
        }
    }
}
```

## Implementation Steps

Each step ends with `cargo test -p aiki-cli` passing.

### 1. Create directory structure

- `mkdir -p cli/src/workflow/steps`
- Move `cli/src/workflow.rs` → `cli/src/workflow/mod.rs`
- Create `cli/src/workflow/steps/mod.rs` with pub mod declarations

Verify: `cargo check -p aiki-cli` passes (no functional changes yet).

### 2. Move build step functions

Move from `commands/build.rs` to `workflow/steps/`:
- `run_plan_step` → `steps/plan.rs` as `pub fn run()`
- `run_decompose_step` → `steps/decompose.rs` as `pub fn run()`
- `run_loop_step` → `steps/loop.rs` as `pub fn run()`
- `run_review_step` → `steps/review.rs` as `pub fn run_build()`
- `run_fix_step` → `steps/fix.rs` as `pub fn run_build()`

Update `Step::run()` dispatch in `workflow/mod.rs`.
Remove the functions from `commands/build.rs`.

### 3. Move fix step functions

Move from `commands/fix.rs` to `workflow/steps/`:
- `run_fix_plan_step` → `steps/fix.rs` as `pub fn run_plan()`
- `run_fix_decompose_step` → `steps/fix.rs` as `pub fn run_decompose()`
- `run_fix_loop_step` → `steps/fix.rs` as `pub fn run_loop()`
- `run_fix_review_step` → `steps/fix.rs` as `pub fn run_review()`
- `run_regression_review_step` → `steps/review.rs` as `pub fn run_regression()`

Update dispatch, remove from `commands/fix.rs`.

### 4. Move review step function

Move from `commands/review.rs` to `workflow/steps/`:
- `run_standalone_review_step` → `steps/review.rs` as `pub fn run_standalone()`

Update dispatch, remove from `commands/review.rs`.

### 5. Clean up imports and visibility

- Step functions use shared helpers that stay in command modules. These need
  `pub(crate)` visibility (most already have it):
  - `build.rs`: `validate_plan_path`, `cleanup_stale_builds`, `check_epic_blockers`,
    `undo_completed_subtasks`, `close_epic`, `close_epic_as_invalid`, `restart_epic`
  - `fix.rs`: `create_fix_parent`, `has_actionable_issues`
  - `review.rs`: `create_review`, `CreateReviewParams`, `ReviewScope`, `ReviewScopeKind`
  - `epic.rs`: `create_epic_task`
  - `decompose.rs`: `run_decompose`, `DecomposeOptions`
- Remove `pub(crate)` from deleted step functions.
- Run `cargo clippy -p aiki-cli` for unused import warnings.

### 6. Inline the Loop step's split logic

Currently `Step::Loop` in `workflow.rs` has inline logic that duplicates
`run_loop_step`. After the move, `steps::r#loop::run()` handles both
paths (with and without agent). Remove the inline logic from
`Step::run()`.

## What Changes

| File | Change |
|------|--------|
| `cli/src/workflow.rs` | **Deleted** — replaced by `workflow/` directory |
| `cli/src/workflow/mod.rs` | **New** — contents of old `workflow.rs`, dispatch updated |
| `cli/src/workflow/steps/mod.rs` | **New** — pub mod declarations |
| `cli/src/workflow/steps/plan.rs` | **New** — `run_plan_step` from build.rs |
| `cli/src/workflow/steps/decompose.rs` | **New** — `run_decompose_step` from build.rs |
| `cli/src/workflow/steps/loop.rs` | **New** — `run_loop_step` from build.rs + inline Loop logic |
| `cli/src/workflow/steps/review.rs` | **New** — review steps from build.rs + review.rs + regression from fix.rs |
| `cli/src/workflow/steps/fix.rs` | **New** — all fix steps from build.rs + fix.rs |
| `cli/src/commands/build.rs` | **Refactored** — step functions removed, assembly stays |
| `cli/src/commands/fix.rs` | **Refactored** — step functions removed, assembly stays |
| `cli/src/commands/review.rs` | **Refactored** — step function removed, assembly stays |

## What We Keep

- All `*_workflow()` assembler functions in command files
- `drive_build()` and `Workflow::run_build()` in `workflow/mod.rs`
- All helper functions in command modules (`validate_plan_path`, etc.)
- All existing tests — they test behavior, not file layout

## Risks

- **Import tangles** — Step functions use helpers from commands. May need to
  make some helpers `pub(crate)` that were previously private. Move
  incrementally (one step file at a time) to isolate compile errors.
- **Circular deps** — `workflow::steps` imports from `commands` (shared
  helpers). This is fine. But commands must NOT import from `workflow::steps`
  — only from `workflow` (the public types).

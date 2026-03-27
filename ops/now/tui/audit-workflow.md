# Audit: Workflow/Steps Migration

**Date**: 2026-03-27
**Status**: Findings
**Prerequisite**: workflow-module-split.md + workflow-commands-thin.md (partially implemented)

---

## Current State

Phase 1 (workflow-module-split) and phase 2 (workflow-commands-thin) are partially
implemented. The `workflow/` directory exists with `builders.rs`, `orchestrate.rs`,
and `steps/` containing domain types and helpers. But many step handler functions
remain in command files, creating backwards dependencies from `workflow` → `commands`.

### File Sizes (lines)

| File | Lines | Role |
|------|-------|------|
| `workflow/mod.rs` | 540 | Step enum, Workflow runner, tests |
| `workflow/builders.rs` | 196 | Workflow assembly (done) |
| `workflow/orchestrate.rs` | 1267 | Fix quality loop (done) |
| `workflow/steps/build.rs` | 327 | Build step handlers (staging area) |
| `workflow/steps/plan.rs` | 128 | Plan validation helpers (done) |
| `workflow/steps/decompose.rs` | 172 | Epic lifecycle helpers (done) |
| `workflow/steps/fix.rs` | 125 | Fix task helpers (done) |
| `workflow/steps/review.rs` | 552 | Review domain types (done) |
| `commands/build.rs` | 1879 | **Still fat** |
| `commands/fix.rs` | 520 | Partially thin |
| `commands/review.rs` | 1771 | **Still fat** |
| `commands/plan.rs` | 1036 | Separate concern (plan authoring) |
| `commands/decompose.rs` | 318 | Thin CLI + `run_decompose()` |
| `commands/loop_cmd.rs` | 233 | Thin CLI + `run_loop()` |
| `commands/epic.rs` | 579 | Thin CLI, imports from workflow |

---

## Findings

### 1. `run_standalone_review_step` still in `commands/review.rs`

**Location:** `commands/review.rs:208-253`

Workflow step handler called from `workflow/mod.rs:155` via
`review::run_standalone_review_step`. Creates backwards dependency
`workflow/mod.rs` → `commands::review`.

**Fix:** Move to `workflow/steps/review.rs` as `pub(crate) fn run_standalone()`.

### 2. Fix step handlers still in `commands/fix.rs`

**Location:** `commands/fix.rs:26-226`

Five step functions remain:
- `run_fix_plan_step` (26-73)
- `run_fix_decompose_step` (76-112)
- `run_fix_loop_step` (115-140)
- `run_fix_review_step` (143-183)
- `run_regression_review_step` (186-226)

Dispatched from `workflow/mod.rs:179` and `workflow/orchestrate.rs:16,294,317,438,464`.

**Fix:** Move all five to `workflow/steps/fix.rs`.

### 3. `workflow/mod.rs` dispatches via backwards imports

**Location:** `workflow/mod.rs:10`

```rust
use crate::commands::{fix, review};
```

`Step::run()` dispatches to `review::run_standalone_review_step` (line 155),
`fix::run_fix_plan_step` (line 179), `fix::run_regression_review_step` (line 193).

**Fix:** After findings 1-2, update dispatch to `steps::` paths. Remove
`commands::{fix, review}` import.

### 4. `workflow/orchestrate.rs` imports step handlers from `commands/fix`

**Location:** `orchestrate.rs:16`

```rust
use crate::commands::fix::{run_fix_review_step, run_regression_review_step};
```

Quality loop calls these directly (lines 294, 317, 438, 464).

**Fix:** Update imports after finding 2 move.

### 5. Inline `Step::Loop` logic duplicates `run_loop_step`

**Location:** `workflow/mod.rs:124-147`

When `agent.is_none()`, 20 lines of inline logic duplicate what `run_loop_step`
does (construct `LoopOptions`, call `run_loop`). The `agent.is_some()` path
delegates to `steps::build::run_loop_step`.

**Fix:** `run_loop_step` should handle both cases. Remove inline branch.

### 6. `steps/build.rs` is a grab-bag, not a step

**Location:** `workflow/steps/build.rs` (327 lines)

Contains functions for every step domain, not just "build":
- `run_plan_step` → belongs in `steps/plan.rs`
- `run_decompose_step` → belongs in `steps/decompose.rs`
- `run_loop_step` → belongs in new `steps/loop.rs`
- `build_review_scope` + `run_review_step` → belongs in `steps/review.rs`
- `run_fix_step` → belongs in `steps/fix.rs`
- `output_after_workflow` → CLI output, belongs in `commands/build.rs`

**Fix:** Dissolve `steps/build.rs` entirely. Each function moves to its domain
step file.

### 7. `has_review_issues` duplicates `count_issues` / `has_actionable_issues`

**Location:** `commands/build.rs:109-121`

Three places check `data.issue_count > 0`:
- `commands/build.rs::has_review_issues`
- `workflow/mod.rs::count_issues`
- `workflow/orchestrate.rs::has_actionable_issues`

**Fix:** Use `orchestrate::has_actionable_issues` everywhere. Delete duplicates.

### 8. `drive_build` and `Workflow::run_build` in `commands/build.rs`

**Location:** `commands/build.rs:128-203`

`drive_build` is the dynamic fix-iteration runner. `run_build` is a `Workflow`
method. Both use workflow types (`Step`, `WorkflowContext`, `RunMode`) and are
workflow execution logic, not CLI parsing.

**Fix:** Move `drive_build`, `has_review_issues`, `MAX_BUILD_ITERATIONS`, and
`impl Workflow { fn run_build }` to `workflow/mod.rs`.

### 9. `commands/review.rs` re-exports create unnecessary coupling

**Location:** `commands/review.rs:32-38`

```rust
pub(crate) use crate::workflow::steps::review::{
    CreateReviewParams, CreateReviewResult, Location, ReviewScope, ReviewScopeKind,
};
pub(crate) use crate::workflow::steps::review::{
    build_async_review_args, create_review, detect_target, ...
};
```

Other modules import via `commands::review::*` instead of `workflow::*`.

**Fix:** Update callers to import from `workflow` re-exports or
`workflow::steps::review` directly. Remove re-exports from `commands/review.rs`.

### 10. `commands/decompose.rs::run_decompose` is shared domain logic

**Location:** `commands/decompose.rs:83-134` (52 lines)

Called from 4 places: CLI, `epic.rs`, `fix.rs`, `steps/build.rs`. Writes link
events, creates template tasks, runs decompose agent. Domain logic, not CLI.

**Fix:** Move `run_decompose()` and `DecomposeOptions` to
`workflow/steps/decompose.rs`. CLI becomes thin wrapper.

### 11. `commands/loop_cmd.rs::run_loop` is shared domain logic

**Location:** `commands/loop_cmd.rs:122-197` (76 lines)

Called from 4 places: CLI, `fix.rs`, `steps/build.rs`, inline `workflow/mod.rs`.
Same pattern as decompose.

**Fix:** Move `run_loop()` and `LoopOptions` to new `workflow/steps/loop.rs`.
CLI wrapper stays.

### 12. `commands/build.rs` async path duplicates plan/decompose logic

**Location:** `commands/build.rs:380-477`

The `--async` path manually inlines Plan step logic (validate plan, check draft,
cleanup stale builds) and Decompose step logic (find-or-create epic). Same logic
as `run_plan_step` + `run_decompose_step`.

**Fix:** After steps are modular, refactor async path to call step functions
directly. Low priority — functional duplication but not architectural.

---

## Migration Completeness

| Step | Status | What Remains |
|------|--------|-------------|
| **Plan** | 70% | `run_plan_step` in `steps/build.rs` → `steps/plan.rs` |
| **Decompose** | 60% | `run_decompose_step` in `steps/build.rs`, `run_decompose()` in `commands/decompose.rs` → `steps/decompose.rs` |
| **Loop** | 30% | No `steps/loop.rs`. Logic scattered: `steps/build.rs`, `commands/loop_cmd.rs`, inline `workflow/mod.rs` |
| **Review** | 80% | Domain types done. `run_standalone_review_step` in `commands/review.rs`, `run_review_step` in `steps/build.rs` |
| **Fix** | 40% | Helpers done. All 5 step handlers in `commands/fix.rs`, `run_fix_step` in `steps/build.rs` |
| **Build orchestration** | 50% | `drive_build` + `run_build` in `commands/build.rs` → `workflow/mod.rs` |
| **builders.rs** | Done | |
| **orchestrate.rs** | 90% | Imports step handlers from `commands/fix` (backwards dep) |

---

## Implementation Steps

Each step ends with `cargo test -p aiki-cli` passing.

### 1. Dissolve `steps/build.rs`

Move each function to its domain step file:
- `run_plan_step` → `steps/plan.rs`
- `run_decompose_step` → `steps/decompose.rs`
- `run_loop_step` → new `steps/loop.rs`
- `build_review_scope` + `run_review_step` → `steps/review.rs`
- `run_fix_step` → `steps/fix.rs`
- `output_after_workflow` → `commands/build.rs`

Delete `steps/build.rs`, remove from `steps/mod.rs`.

### 2. Move fix step handlers

From `commands/fix.rs` to `steps/fix.rs`:
- `run_fix_plan_step` → `run_plan()`
- `run_fix_decompose_step` → `run_decompose()`
- `run_fix_loop_step` → `run_loop()`
- `run_fix_review_step` → `run_review()`
- `run_regression_review_step` → `run_regression()`

### 3. Move `run_standalone_review_step`

From `commands/review.rs` to `steps/review.rs` as `run_standalone()`.

### 4. Move `run_decompose`

From `commands/decompose.rs` to `steps/decompose.rs`.
Move `DecomposeOptions` with it.

### 5. Move `run_loop`

From `commands/loop_cmd.rs` to `steps/loop.rs`.
Move `LoopOptions` with it.

### 6. Move `drive_build`

From `commands/build.rs` to `workflow/mod.rs`:
- `drive_build()`
- `has_review_issues()` (deduplicate with `has_actionable_issues`)
- `MAX_BUILD_ITERATIONS`
- `impl Workflow { fn run_build }`

### 7. Clean up imports

- Remove re-exports from `commands/review.rs`
- Update all callers to import from `workflow` or `workflow::steps::*`
- Remove `use crate::commands::{fix, review}` from `workflow/mod.rs`

### 8. Deduplicate `has_review_issues`

Use `orchestrate::has_actionable_issues` in `drive_build` (now in `workflow/mod.rs`).
Delete `has_review_issues` and `count_issues`.

### 9. Consolidate inline Loop step

Remove inline `Step::Loop` logic from `workflow/mod.rs:124-147`.
`steps::loop::run()` handles both with-agent and without-agent paths.

---

## Target Layout

```
cli/src/workflow/
├── mod.rs            ← Step enum, Workflow, drive_build, run_build, tests
├── builders.rs       ← build_workflow, fix_workflow, review_workflow (done)
├── orchestrate.rs    ← run_fix, run_quality_loop, ReviewOutcome (done)
└── steps/
    ├── mod.rs        ← pub mod declarations
    ├── plan.rs       ← run(), validate_plan_path, cleanup_stale_builds
    ├── decompose.rs  ← run(), run_decompose, DecomposeOptions, epic lifecycle
    ├── loop.rs       ← run(), run_loop, LoopOptions
    ├── review.rs     ← run_standalone(), run_build(), run_regression(),
    │                    create_review, ReviewScope, Location, etc.
    └── fix.rs        ← run_plan(), run_decompose(), run_loop(), run_review(),
                         run_build(), run_regression(), create_fix_parent, etc.
```

Commands become thin shells:
- `commands/build.rs` — Clap args, dispatch, async spawn, output formatting
- `commands/fix.rs` — Clap args, stdin parsing, delegates to `orchestrate::run_fix`
- `commands/review.rs` — Clap args, dispatch, async spawn, output formatting
- `commands/decompose.rs` — Clap args, calls `steps::decompose::run_decompose`
- `commands/loop_cmd.rs` — Clap args, calls `steps::loop::run_loop`
- `commands/epic.rs` — Clap args, calls workflow step helpers

# Rename aiki/implement → aiki/loop, Restructure Fix, Extract aiki resolve

## Problem

The `aiki/implement` template is misnamed. It doesn't implement anything — it orchestrates subtask execution via lanes. Meanwhile, the epic is the thing that actually "implements" the plan (via the `implements-plan` edge). This naming collision is confusing.

The loop logic should be a reusable building block. Both `aiki build` and `aiki fix` need the same core behavior: "given a parent task with subtasks, run them to completion via lanes."

Currently `aiki fix` works differently from `aiki build` — it creates a single fix task from `aiki/fix.md`, and the agent manually creates subtasks and loops through them in one session. This should mirror build's architecture with shared composable stages.

## Design

### Composable stages

Every pipeline is built from the same stages:

| Stage | Input | Output | Template |
|---|---|---|---|
| **plan** | varies by variant | plan file | `aiki/plan/*` |
| **decompose** | plan file | subtasks under parent | `aiki/decompose` |
| **loop** | parent with subtasks | executed work | `aiki/loop` |

Plan is a family of templates, each producing a plan from different inputs:

| Template | Input | Output |
|---|---|---|
| `aiki/plan/epic` | goal/prompt (current `aiki/plan`) | epic plan file |
| `aiki/plan/fix` | review issues | fix plan file |

Pipelines compose these stages:

```
BUILD:  plan/epic → decompose → loop
FIX:    plan/fix  → decompose → loop
BUG:    debug → plan/fix → decompose → loop   (future)
```

Each stage has one job and one interface. `decompose` doesn't know or care whether the plan came from a human, `plan/fix`, or `debug`.

### New template: `aiki/loop`

Extract the lane-based subtask execution loop from `aiki/implement` into `aiki/loop`.

**Contract:** Given `data.target` (a parent task ID), loop through its subtask lanes until all are complete.

```yaml
# .aiki/templates/aiki/loop.md
---
version: 2.0.0
type: orchestrator
---
```

Body: the lane-based execution logic from current `implement.md` (Steps 1-2, failure handling, completion). No `spawns:` config — callers handle post-completion behavior in Rust.

### New command: `aiki loop`

Top-level command for orchestrating a parent task's subtasks via lanes.

```bash
aiki loop <parent-id> [--async] [--agent <agent>]
```

Internally:
1. Create loop task from `aiki/loop` template (data.target = parent task ID)
2. Write `orchestrates` link: loop task → parent
3. `task_run(loop_task)`

The loop task still appears in the task graph (the agent needs a task to claim). `aiki loop` automates creating and wiring it up.

**Separation from `task run`:**
- `aiki task run <id>` — primitive: spawn one agent on one task
- `aiki loop <parent-id>` — high-level: orchestrate all subtasks of a parent via lanes

`build.rs` and `fix.rs` call the same Rust function (`run_loop`) internally:

```rust
// Shared function used by aiki loop, build.rs, fix.rs
fn run_loop(cwd: &Path, parent_id: &str, options: LoopOptions) -> Result<String>
```

### New template: `aiki/plan/fix`

Reads review issues and produces a fix plan file that `decompose` can consume.

**Contract:** Given `data.review` (a review task ID) and `data.target` (the fix-parent task ID), read the review's issues and write a fix plan file.

```yaml
# .aiki/templates/aiki/plan/fix.md
---
version: 1.0.0
---
```

No special type needed — just a regular task.

**Plan output path:** `/tmp/aiki/plans/{{task.id}}.md` — keyed to the plan-fix task's own ID (unique, deterministic, no collisions). The Rust caller reads this path after the task completes and passes it to decompose.

**Lifecycle:** Plan files are ephemeral intermediates. Rust deletes the plan file after decompose completes (the plan's content lives on as subtasks). On error, the file is left for debugging and overwritten on retry.

Body instructs the agent to:
1. Read issues via `aiki review issue list {{data.review}}`
2. Write a fix plan to `/tmp/aiki/plans/{{task.id}}.md` covering:
   - What each issue is and where it is
   - How to fix it
   - Dependencies between fixes (e.g., two issues in the same file)
3. Close the plan-fix task

The output plan is a standard plan file that `decompose` reads — same format as any hand-written plan.

### `aiki plan` becomes a subcommand family

The plan command gains subcommands matching the template namespace:

```
aiki plan [args]            → shortcut for aiki plan epic [args]
aiki plan epic [args]       → aiki/plan/epic template (current behavior)
aiki plan fix <review-id>   → aiki/plan/fix template
```

**`aiki plan epic`** — existing behavior, creates an epic plan from a goal/prompt. `aiki plan` without a subcommand defaults to this.

**`aiki plan fix <review-id>`** — new subcommand. Creates a fix plan from review issues:
1. Creates plan-fix task from `aiki/plan/fix` template (data.review = review ID, data.target = review ID as default)
2. Runs the task (agent reads issues, writes fix plan)
3. Returns the plan-fix task ID and plan file path (`/tmp/aiki/plans/<plan-fix-task-id>.md`)

When called from `fix.rs`, `data.target` is set to the fix-parent ID. When called standalone, `data.target` defaults to the review ID (the plan file path uses `task.id` regardless, so `data.target` is only used by the agent for context about what's being fixed).

This makes each pipeline stage independently invocable:
```bash
aiki plan fix <review-id>       # stage 1: plan
aiki decompose <plan-path>      # stage 2: decompose into subtasks
aiki loop <parent-id>           # stage 3: execute via lanes
```

Or `aiki fix <review-id>` runs all three automatically.

**Template rename:** Move `.aiki/templates/aiki/plan.md` → `.aiki/templates/aiki/plan/epic.md`.

### Fix command simplified

With merge-conflict resolution extracted to `aiki resolve` (see below), `aiki fix` becomes a clean pipeline command with one job: fix review issues.

```bash
aiki fix <review-id> [--async] [--once]
```

The `has_jj_conflicts` auto-detection is removed — `aiki fix` always means "fix review issues." If you have a merge conflict, use `aiki resolve`.

**Default behavior: fix → review → fix loop.** After the fix pipeline completes, Rust automatically reviews the fix. If the review finds new issues, it runs the fix pipeline again against the new review. This repeats until the review approves or a max iteration limit is reached. The quality loop that was previously orchestrated by `fix/quality.md` + `spawns:` is now driven entirely by Rust.

**`--once` disables the review loop.** Just runs the fix pipeline once and returns. Useful when calling fix from contexts that manage their own review cycle.

```
run_fix(review_id, once=false):
  loop (max_iterations=10):
    1. plan/fix → decompose → loop  (the fix pipeline)
    2. if once: break
    3. review the fix
    4. if review approved: break
    5. review_id = new_review  (loop back with new review)
```

The fix pipeline (one iteration):

```
fix pipeline (Rust):
1. Validate review task, check for issues (existing logic)
2. Short-circuit if no actionable issues (no parent/decompose/loop churn)
3. Create fix-parent task (container, like an epic)
4. Create plan-fix task from aiki/plan/fix (data.review = review ID, data.target = fix-parent ID)
5. task_run(plan-fix) → agent writes plan to task-scoped temp path
6. Create decompose task from aiki/decompose (data.plan = fix plan path, data.epic = fix-parent)
7. task_run(decompose) → agent creates subtasks under fix-parent
8. run_loop(fix-parent) → orchestrate subtasks via lanes
```

Similarly, `build.rs` simplifies:

```
build.rs (Rust):
1. find_or_create_epic (existing: decompose → epic with subtasks)
2. run_loop(epic_id) → orchestrate subtasks via lanes
```

Both call the shared `run_loop()` function — same code path as `aiki loop` CLI.

This replaces the current flow where a single `aiki/fix` task does planning, decomposition, and execution in one session. The quality loop that was previously orchestrated by `fix/quality.md` + `fix/loop.md` + `spawns:` is now a simple Rust loop.

### New command: `aiki resolve`

Top-level command for resolving JJ merge conflicts. Extracted from the old `aiki fix` conflict path.

```bash
aiki resolve <change-id> [--async] [--start] [--agent <agent>]
```

Internally:
1. Create task from `aiki/resolve` template (data.conflict_id = change ID)
2. `task_run` (blocking), `task_run_async` (`--async`), or start inline (`--start`)

`--start` is needed by the workspace absorption hook — when absorption introduces conflicts, the hook calls `aiki resolve <change-id> --start` so the current agent resolves inline without spawning a new session.

**Why a separate command:** Merge-conflict resolution is fundamentally different from fixing review issues. Review fixes go through the pipeline (plan → decompose → loop) with parallelizable subtasks. Conflict resolution is a single holistic task — one agent needs full context of both sides to merge correctly. Cramming both under `fix` with subcommands would muddy what "fix" means.

**Template is at `aiki/resolve`.** The template path matches the command name.

### `--async` semantics

**Blocking (default):** Rust calls each pipeline stage sequentially. Returns when all subtasks are complete.

**Async:** Rust creates the parent task, then spawns itself as a detached process to run the pipeline in background. Returns the parent task ID immediately. Caller uses `aiki task wait <parent-id>`.

**Mechanism: spawn-self.** The async path re-invokes the same command with an internal `--_resume <parent-id>` flag as a detached process. The resumed invocation skips parent creation (already exists) and runs the pipeline stages synchronously in the background process. Same code path for blocking and async — only difference is whether it runs inline or detached.

```
Blocking path (aiki fix):
  loop:
    1. Create fix-parent
    2. task_run(plan-fix) — synchronous
    3. task_run(decompose) — synchronous
    4. run_loop(fix-parent) — synchronous
    5. if --once: break
    6. review the fix — synchronous
    7. if approved: break
    8. review_id = new review (loop back)
  Returns when done

Async path (aiki fix --async):
  1. Spawn detached: aiki fix --_resume <review-id>
  2. Return immediately
  Caller does: aiki task wait

Resumed path (aiki fix --_resume):
  Same as blocking path (runs synchronously in background process)
```

**Benefits:**
- Pipeline stages (plan, decompose) are driven by Rust, not by an agent interpreting markdown — the loop task is the only agent-driven orchestration node in the graph
- Same code path for blocking and async

This pattern applies to `aiki build --async` too (multi-stage pipeline). `aiki loop --async` is simpler — it's a single stage, so it just uses normal `task_run_async(loop_task)`.

### Remove `--start` from fix and review

The `--start` flag ("caller takes over inline") is removed from `fix` and `review`. It remains on `resolve` (needed by workspace absorption hook) and `explore` (used by review template).

**Why:** With the new pipeline, `aiki fix` always runs plan/fix → decompose → loop. There's no "inline" mode — the pipeline is the only path. The `--start` usage in `fix/quality.md` goes away because quality.md itself is deleted.

For `review`, no template references `aiki review --start` after quality.md is deleted. Reviews are either blocking (default) or `--async`.

**Commands after this change:**

| Command | Modes |
|---|---|
| `aiki fix` | blocking (default, review loop), `--once` (single pass), `--async` |
| `aiki resolve` | blocking (default), `--async`, `--start` |
| `aiki review` | blocking (default), `--async` |
| `aiki explore` | blocking (default), `--async`, `--start` |

## Changes

### 1. Create `.aiki/templates/aiki/loop.md`

Move the lane-based execution logic from current `implement.md`:
- Step 1: Understand the work (`aiki task show`, `aiki task lane --all`)
- Step 2: Lane loop (get ready lanes → start sessions → wait → repeat)
- Failure handling
- Completion

Keep `data.target` as the interface. Remove `spawns:` config.

### 2. Delete `.aiki/templates/aiki/implement.md`

Replaced by `aiki/loop.md`.

### 3. Delete `.aiki/templates/aiki/build.md`

Dead template (was already replaced by implement, now loop handles it). Build orchestration lives in `build.rs`.

### 4. Move `.aiki/templates/aiki/plan.md` → `.aiki/templates/aiki/plan/epic.md`

Rename the current plan template into the plan family namespace.

### 5. Create `.aiki/templates/aiki/plan/fix.md`

New template:
- No special type
- Reads `data.review` (review task ID) and `data.target` (fix-parent task ID)
- Agent runs `aiki review issue list {{data.review}}` to get issues
- Writes a fix plan to `/tmp/aiki/plans/{{task.id}}.md`
- Rust reads the plan path after task completes, deletes it after decompose succeeds
- Plan includes: issue descriptions, locations, fix approach, dependencies between fixes
- Closes plan-fix task

### 6. Create `cli/src/commands/loop.rs` (`aiki loop` command)

New top-level command:

```bash
aiki loop <parent-id> [--async] [--agent <agent>]
```

Core function `run_loop()`:
1. Validate parent task exists and has subtasks
2. Create loop task from `aiki/loop` template (data.target = parent ID)
3. Write `orchestrates` link: loop task → parent
4. `task_run(loop_task)` (or `task_run_async` if `--async`)
5. Return loop task ID

This is the shared entry point used by `build.rs`, `fix.rs`, and the CLI.

### 7. Update `cli/src/commands/build.rs`

Replace manual loop task creation with `run_loop()`:
- Remove `create_build_task()` — no longer needed
- Remove `aiki/implement` references
- `run_build_plan` and `run_build_epic` call `run_loop(epic_id)` after epic is ready
- Move `spawns:` behavior (post-build review) — already handled by `run_build_review()` in Rust
- Update tests

### 8. Restructure `cli/src/commands/plan.rs`

Add subcommand dispatch:
- `aiki plan` (no subcommand) → defaults to `aiki plan epic`
- `aiki plan epic [args]` → existing plan logic, template default `aiki/plan/epic`
- `aiki plan fix <review-id>` → new: creates plan-fix task from `aiki/plan/fix`, runs it, returns task ID + plan path

Update default template from `aiki/plan` to `aiki/plan/epic`.

### 9. Update `cli/src/tasks/templates/resolver.rs`

Update tests referencing `aiki/implement` → `aiki/loop`.

### 10. Restructure `cli/src/commands/fix.rs`

`fix.rs` becomes a clean pipeline command (review issues only) with a Rust-driven quality loop. Merge-conflict handling moves to `resolve.rs`.

**Keep unchanged:**
- Input parsing, stdin piping
- Review task validation (`is_review_task`)
- Issue detection (structured issues vs backward-compat comments)
- Scope detection, assignee determination

**Keep (repurposed):**
- `--once` flag — disables the post-fix review loop (single pass)

**Remove:**
- `--start` flag and all `start: bool` parameters (pipeline is the only path)
- `output_followup_started()` (only used by `--start` path)
- `has_jj_conflicts` auto-detection and `handle_conflict_fix()` (moved to `resolve.rs`)
- All `output_conflict_fix_*` helpers (moved to `resolve.rs`)

**New flow (Rust-driven quality loop):**
```
loop (max_iterations=10):
  1. Short-circuit if no actionable issues (returns before creating any tasks)
  2. Create fix-parent task (container, like an epic)
  3. Create plan-fix task from aiki/plan/fix (data.review = review ID, data.target = fix-parent ID)
  4. task_run(plan-fix) — agent writes fix plan to /tmp/aiki/plans/<plan-fix-task-id>.md
  5. Read plan path, create decompose task from aiki/decompose (data.plan = plan path, data.epic = fix-parent ID)
  6. task_run(decompose) — agent creates subtasks under fix-parent
  7. Delete plan file (content now lives as subtasks)
  8. run_loop(fix-parent) — orchestrate subtasks via lanes
  9. if --once: break
  10. Create review task scoped to fix-parent's changes
  11. task_run(review) — agent reviews the fix
  12. if review approved: break
  13. review_id = new review task ID (loop back)
```

**Edge relationships:**
- fix-parent `remediates` review task (existing)
- fix-parent `fixes` reviewed targets (existing)
- loop task `orchestrates` fix-parent (created by `run_loop`)

### 10b. Create `cli/src/commands/resolve.rs` (`aiki resolve` command)

New top-level command extracted from `fix.rs`:

```bash
aiki resolve <change-id> [--async] [--start] [--agent <agent>]
```

Move from `fix.rs`:
- `has_jj_conflicts()` → validation (error if change has no conflicts)
- `handle_conflict_fix()` → core logic
- All `output_conflict_fix_*` helpers → renamed to `output_resolve_*`

The logic is the same — create task from `aiki/resolve`, run it. Only the entry point changes.

Update the workspace absorption hook context to reference `aiki resolve` instead of `aiki fix`.

### 11. Delete `aiki/fix.md`, `aiki/fix/quality.md`, `aiki/fix/once.md`, `aiki/fix/loop.md`

All remaining fix templates are deleted:

- **`aiki/fix.md`** — fix orchestration is now entirely in Rust (the pipeline). No longer referenced.
- **`aiki/fix/quality.md`** — the review-fix quality loop is now a Rust loop in `fix.rs`. No longer referenced.
- **`aiki/fix/once.md`** — was an alternative template for single-pass fixes. The `--once` flag now controls the Rust loop directly. No longer referenced.
- **`aiki/fix/loop.md`** — was the spawns-based re-entry point (`aiki fix {{parent.id}}`). The Rust loop replaces this. No longer referenced.

### 15. Remove `--start` from `cli/src/commands/review.rs`

Remove the `--start` flag, `start: bool` parameter, and `output_review_started()`. Reviews are either blocking (default) or `--async`. No template uses `aiki review --start` after quality.md is deleted.

### 16. Update `agents_template.rs`

Remove all `--start` references from the CLAUDE.md agent template (the section about `aiki review --start`). Update to reflect that `aiki review` is blocking by default.

### 17. Update `cli/docs/sdlc.md`

Update the SDLC overview doc to reflect that `build` and `fix` are now pipeline commands composed from the same stages (plan → decompose → loop):
- Fix description: remove "quality loop" language, describe as pipeline (plan/fix → decompose → loop)
- Build description: mention the pipeline stages explicitly
- Add a "Composable Stages" section explaining plan, decompose, loop as the shared building blocks
- Update the `aiki loop` command as a new standalone entry point

### 18. Cleanup references

- `cli/docs/sdlc/build.md` — update references to `aiki/implement`
- `cli/docs/sdlc/fix.md` — update to reflect pipeline architecture

## Task Graph

```
1.  Create aiki/loop.md (extract from implement.md)
2.  Delete aiki/implement.md
    └── depends-on: 1
3.  Delete aiki/build.md (dead template)
4.  Move aiki/plan.md → aiki/plan/epic.md
5.  Create aiki/plan/fix.md
6.  Create loop.rs (aiki loop command + run_loop function)
    └── depends-on: 1
7.  Update build.rs (use run_loop)
    └── depends-on: 6
8.  Update plan.rs (aiki/plan → aiki/plan/epic)
    └── depends-on: 4
9.  Update resolver.rs tests
    └── depends-on: 2
10. Restructure fix.rs (pipeline + Rust-driven quality loop, keep --once, remove --start/conflict)
    └── depends-on: 5, 6
11. Create resolve.rs (extract conflict path from fix.rs)
    └── depends-on: 10
12. Delete fix templates (fix.md, fix/quality.md, fix/once.md, fix/loop.md)
    └── depends-on: 10
13. Remove --start from review.rs
14. Update agents_template.rs (remove --start references, add aiki resolve docs)
    └── depends-on: 11, 13
15. Update cli/docs/sdlc.md (pipeline language for build/fix, composable stages, aiki resolve)
    └── depends-on: 7, 10, 11
16. Cleanup references (sdlc/build.md, sdlc/fix.md, absorption hook context)
    └── depends-on: 11
17. Verify build (cargo build + cargo test)
    └── depends-on: 7, 8, 9, 10, 11, 12, 13, 14, 15, 16
```

## Future

- [`aiki debug` command](../next/debug-command.md) — investigate bug reports and pipe findings into the fix pipeline (debug → plan/fix → decompose → loop).

## Out of Scope

- Backwards compatability
- Changing the `orchestrates` edge name (still correct)
- Changing `implements-plan` edge name (still correct)
- Refactoring the decompose template or `aiki/decompose.md`
- Changes to `aiki/review.md` template (review command itself is updated to remove `--start`)
- Changes to `aiki/resolve.md` template content (template was moved from `aiki/fix/merge-conflict` to `aiki/resolve`)
- Removing `--start` from `aiki explore` (still used by review template)

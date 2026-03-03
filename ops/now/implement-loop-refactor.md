# Rename aiki/implement → aiki/loop, Restructure Fix

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

### Fix command restructured

`fix.rs` mirrors `build.rs` but with a plan/fix phase before decompose:

```
fix.rs (Rust):
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

This replaces the current flow where a single `aiki/fix` task does planning, decomposition, and execution in one session.

### `--async` semantics

**Blocking (default):** Rust calls each pipeline stage sequentially. Returns when all subtasks are complete.

**Async:** Rust creates the parent task, then spawns itself as a detached process to run the pipeline in background. Returns the parent task ID immediately. Caller uses `aiki task wait <parent-id>`.

**Mechanism: spawn-self.** The async path re-invokes the same command with an internal `--_resume <parent-id>` flag as a detached process. The resumed invocation skips parent creation (already exists) and runs the pipeline stages synchronously in the background process. Same code path for blocking and async — only difference is whether it runs inline or detached.

```
Blocking path (fix.rs):
  1. Create fix-parent
  2. task_run(plan-fix) — synchronous
  3. task_run(decompose) — synchronous
  4. run_loop(fix-parent) — synchronous
  Returns when done

Async path (fix.rs):
  1. Create fix-parent
  2. Spawn detached: aiki fix --_resume <fix-parent-id> <review-id>
  3. Return fix-parent ID immediately
  Caller does: aiki task wait <fix-parent-id>

Resumed path (fix.rs --_resume):
  1. Skip parent creation (already exists)
  2. task_run(plan-fix) — synchronous
  3. task_run(decompose) — synchronous
  4. run_loop(fix-parent) — synchronous
  5. Close fix-parent
```

**Benefits:**
- Pipeline stages (plan, decompose) are driven by Rust, not by an agent interpreting markdown — the loop task is the only agent-driven orchestration node in the graph
- Same code path for blocking and async

This pattern applies to `aiki build --async` too (multi-stage pipeline). `aiki loop --async` is simpler — it's a single stage, so it just uses normal `task_run_async(loop_task)`.

### Remove `--start` from fix and review

The `--start` flag ("caller takes over inline") is removed from `fix` and `review`. It remains only on `explore`, where the review template uses it to explore inline before reviewing.

**Why:** With the new pipeline, `aiki fix` always runs plan/fix → decompose → loop. There's no "inline" mode — the pipeline is the only path. The `--start` usage in `fix/quality.md` goes away because quality.md itself is deleted.

For `review`, no template references `aiki review --start` after quality.md is deleted. Reviews are either blocking (default) or `--async`.

**Commands after this change:**

| Command | Modes |
|---|---|
| `aiki fix` | blocking (default), `--async` |
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

Refactor `run_fix()` to mirror `build.rs` pattern:

**Keep unchanged:**
- Input parsing, stdin piping, conflict detection (`has_jj_conflicts`)
- `handle_conflict_fix()` (merge conflict path)
- Review task validation (`is_review_task`)
- Issue detection (structured issues vs backward-compat comments)
- Scope detection, assignee determination

**Remove:**
- `--start` flag and all `start: bool` parameters
- `--once` flag and all `once: bool` parameters (was only used with old `aiki/fix` template)
- `output_followup_started()` (only used by `--start` path)

**Change the default (blocking) and `--async` paths:**

Old flow:
1. Create single fix task from `aiki/fix` template
2. `task_run(fix_task)` — agent does everything in one session

New flow:
1. Short-circuit if no actionable issues (existing check, but now returns before creating any tasks)
2. Create fix-parent task (container, like an epic)
3. Create plan-fix task from `aiki/plan/fix` (data.review = review ID, data.target = fix-parent ID)
4. `task_run(plan-fix)` — agent reads issues, writes fix plan to `/tmp/aiki/plans/<plan-fix-task-id>.md`
5. Read plan path, create decompose task from `aiki/decompose` (data.plan = plan path, data.epic = fix-parent ID)
6. `task_run(decompose)` — agent creates subtasks under fix-parent
7. Delete plan file (content now lives as subtasks)
8. `run_loop(fix-parent)` — orchestrate subtasks via lanes

**Edge relationships:**
- fix-parent `remediates` review task (existing)
- fix-parent `fixes` reviewed targets (existing)
- loop task `orchestrates` fix-parent (created by `run_loop`)

### 11. Delete `aiki/fix.md`

Fix orchestration is now entirely in Rust. The `aiki/fix.md` template is no longer referenced by any code path.

### 12. Delete `aiki/fix/quality.md`

The self-spawning review-fix quality loop is replaced by Rust-level post-fix review (like build already does). No template references `aiki review --start` or `aiki fix --start` after this deletion.

### 13. Delete `aiki/fix/once.md`

Was an alternative fix template for single-pass fixes. With the new pipeline, all fixes go through plan/fix → decompose → loop. The `--once` flag is removed from fix.rs.

### 14. Update `aiki/fix/loop.md`

Currently calls `aiki fix {{parent.id}}`. This still works — `aiki fix` will use the new plan/fix → decompose → loop flow internally.

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
1. Create aiki/loop.md (extract from implement.md)
2. Delete aiki/implement.md
   └── depends-on: 1
3. Delete aiki/build.md (dead template)
4. Move aiki/plan.md → aiki/plan/epic.md
5. Create aiki/plan/fix.md
6. Create loop.rs (aiki loop command + run_loop function)
   └── depends-on: 1
7. Update build.rs (use run_loop)
   └── depends-on: 6
8. Update plan.rs (aiki/plan → aiki/plan/epic)
   └── depends-on: 4
9. Update resolver.rs tests
   └── depends-on: 2
10. Restructure fix.rs (remove --start/--once, plan/fix → decompose → run_loop)
    └── depends-on: 5, 6
11. Delete aiki/fix.md, aiki/fix/quality.md, aiki/fix/once.md
    └── depends-on: 10
12. Remove --start from review.rs
13. Update agents_template.rs (remove --start references)
    └── depends-on: 12
14. Update cli/docs/sdlc.md (pipeline language for build/fix, composable stages)
    └── depends-on: 7, 10
15. Cleanup references (sdlc/build.md, sdlc/fix.md)
16. Verify build (cargo build + cargo test)
    └── depends-on: 7, 8, 9, 10, 11, 12, 13, 14, 15
```

## Future

- [`aiki debug` command](../next/debug-command.md) — investigate bug reports and pipe findings into the fix pipeline (debug → plan/fix → decompose → loop).

## Out of Scope

- Backwards compatability
- Changing the `orchestrates` edge name (still correct)
- Changing `implements-plan` edge name (still correct)
- Refactoring the decompose template or `aiki/decompose.md`
- Changes to `aiki/review.md` template (review command itself is updated to remove `--start`)
- Changes to `aiki/fix/merge-conflict.md` (merge conflict path is unchanged)
- Removing `--start` from `aiki explore` (still used by review template)

---
draft: false
---

# Cleanup Loop and Fix Commands

**Date**: 2026-03-23
**Updated**: 2026-03-25
**Status**: Ready
**Purpose**: Rename CLI commands to be more self-documenting and clean up docs

**Related Documents**:
- [cli/src/commands/loop_cmd.rs](../../cli/src/commands/loop_cmd.rs) - Current loop command (→ `code_cmd.rs`)
- [cli/src/commands/fix.rs](../../cli/src/commands/fix.rs) - Fix command (already uses workflow steps)
- [cli/src/commands/build.rs](../../cli/src/commands/build.rs) - Build command (already uses workflow steps)
- [cli/src/workflow.rs](../../cli/src/workflow.rs) - Workflow step infrastructure (done)
- [workflow-steps.md](tui/workflow-steps.md) - Workflow steps plan (completed)

---

## Executive Summary

The workflow-steps refactor is complete — `build.rs`, `fix.rs`, and `review.rs` now use `WorkflowStep` enums driven by `Workflow<S>`. What remains is:

1. **`aiki loop` → `aiki code`** — Rename the command, module, structs, template, flags, step names, and docs.
2. **Extract shared `run_plan_fix()`** — `plan.rs::run_fix()` and `fix.rs::create_plan_fix_task()` are two separate implementations of plan-fix creation. Extract a single shared function so `aiki plan fix` and `aiki fix` (via `FixStep::Fix`) use the same code path.
3. **Docs and label cleanup** — Update all references from "loop" to "code" in docs, help text, and task labels.

The `fix` orchestrator refactor originally planned here was completed by the workflow-steps work.

---

## User Experience

### `aiki code` (renamed from `aiki loop`)

```bash
# Execute subtasks under a parent task
aiki code <parent-task-id>
aiki code <parent-task-id> --async
aiki code <parent-task-id> --agent codex
```

No behavior change — just the command name and help text.

### `aiki plan` (subcommand dispatch exists, shared function needed)

```bash
# Interactive epic plan authoring (shortcut — aiki plan = aiki plan epic)
aiki plan "Build authentication system"
aiki plan epic "Build authentication system"

# Produce fix plan from review issues
aiki plan fix <review-task-id>

# Future: produce fix plan from bug report
aiki plan bug <bug-task-id>
```

The dispatch routing exists in `plan.rs:70-91` (`epic`, `fix`, fallthrough). But `plan.rs::run_fix()` and `fix.rs::create_plan_fix_task()` are separate implementations that need to be unified into a shared `run_plan_fix()` function.

### CLI help (updated categories)

```
For Humans:
  plan        Create a plan (interactive, or subcommands: epic, fix, bug)
  build       Build from a plan file (decompose and execute all subtasks)
  review      Create and run code review tasks
  fix         Fix issues from reviews or bug reports

For Agents:
  epic        Manage epics
  task        Manage tasks
  explore     Explore a scope
  decompose   Decompose a plan into subtasks
  code        Execute a parent task's subtasks via lanes
  resolve     Resolve JJ merge conflicts
```

---

## Scope of Changes

### What's already done (workflow-steps)

The following are **complete** and not part of this plan:

- `workflow.rs` — `WorkflowStep` trait, `Workflow<S>`, `WorkflowContext`, `RunMode`
- `BuildStep` enum in `build.rs` with `build_workflow()` and `drive_build()`
- `FixStep` enum in `fix.rs` with `fix_workflow()` and quality loop
- `ReviewStep` enum in `review.rs` with `review_workflow()`
- Fix iteration logic in build's `drive_build()`

### What remains (this plan)

1. Rename `loop` → `code` (mechanical)
2. Extract shared plan-fix function (two implementations → one)
3. Docs cleanup

---

## Implementation Plan

### Phase 1: Rename `loop` → `code` (clean break)

All items below are mechanical renames — no behavior changes.

**Module + structs:**
1. Rename file: `cli/src/commands/loop_cmd.rs` → `cli/src/commands/code_cmd.rs`
2. Update `cli/src/commands/mod.rs`: `pub mod loop_cmd;` → `pub mod code_cmd;`
3. Rename structs: `LoopArgs` → `CodeArgs`, `LoopOptions` → `CodeOptions`
4. Rename function: `run_loop()` → `run_code()`

**Command registration (main.rs):**
5. `Commands::Loop(commands::loop_cmd::LoopArgs)` → `Commands::Code(commands::code_cmd::CodeArgs)`
6. Help text: `"Orchestrate a parent task's subtasks via lanes"` → `"Execute a parent task's subtasks via lanes"`
7. Match arm: `Commands::Loop(args) => commands::loop_cmd::run(args)` → `Commands::Code(args) => commands::code_cmd::run(args)`

**Callers — build.rs:**
8. Import: `use super::loop_cmd::{run_loop, LoopOptions};` → `use super::code_cmd::{run_code, CodeOptions};`
9. CLI flag: `--loop-template` → `--code-template` (both `BuildArgs` and `BuildOpts`)
10. `BuildStep::Loop` → `BuildStep::Code` (enum variant + all match arms)
11. Step name: `"loop"` → `"code"`
12. `run_loop_step()` → `run_code_step()`; body uses `CodeOptions` and `run_code()`

**Callers — fix.rs:**
13. Import: `use super::loop_cmd::{run_loop, LoopOptions};` → `use super::code_cmd::{run_code, CodeOptions};`
14. `FixStep::Loop` → `FixStep::Code` (enum variant + all match arms)
15. Step name: `"loop"` → `"code"`
16. All `LoopOptions` → `CodeOptions`, `run_loop()` → `run_code()` in fix pipeline functions
17. `loop_template` field names → `code_template` in `FixOpts` and related structs

**Callers — main.rs (build subcommand inline args):**
18. `--loop-template` flag in `Commands::Build` → `--code-template`
19. `loop_template` field → `code_template` in the inline build args

**Template:**
20. Rename `cli/src/tasks/templates/core/loop.md` → `cli/src/tasks/templates/core/code.md`
21. Update default template name in `code_cmd.rs`: `"loop"` → `"code"`

**Task labels:**
22. Update task kind/name patterns: anywhere that generates `"Loop: X"` → `"Code: X"`

**Tests:**
23. Update test assertions that reference `"loop"` step name → `"code"`
24. Update test helper field names: `loop_template` → `code_template`
25. Run `cargo test -p aiki-cli` — all tests must pass

### Phase 2: Extract shared `run_plan_fix()`

Currently there are two separate implementations of plan-fix task creation:
- `plan.rs::run_fix()` (line 700) — used by `aiki plan fix <review-id>`
- `fix.rs::create_plan_fix_task()` (line 1069) — used by `FixStep::Fix` in the quality loop

These do similar but different things. Extract a single shared function.

**Extract shared function:**
1. Create `pub fn run_plan_fix(cwd, review_id, fix_parent_id, assignee, template, ...)` — either in `fix.rs` (where the fix-parent context lives) or in a shared module
2. This function: creates plan-fix task from template, runs it, returns plan file path
3. Must handle: template resolution, data injection (review ID, fix-parent ID), task creation, agent run, plan file path construction

**Update callers:**
4. `fix.rs` `FixStep::Fix::run()` — replace inline `create_plan_fix_task()` + `run_task_with_show_tui()` with call to `run_plan_fix()`
5. `plan.rs::run_fix()` — replace its bespoke implementation with call to `run_plan_fix()`
6. `plan.rs` dispatch at line 84 still calls its local `run_fix()` wrapper (which handles output formatting), but the core logic delegates to the shared function

**Cleanup:**
7. Remove `plan.rs::run_fix()` body (replaced by thin wrapper around shared function)
8. Remove `fix.rs::create_plan_fix_task()` if fully subsumed
9. Verify `aiki plan fix <review-id>` and `aiki fix <review-id>` both work end-to-end

### Phase 3: Docs cleanup

1. Rename `cli/docs/sdlc/loop.md` → `cli/docs/sdlc/code.md`
2. Update references in: `cli/docs/sdlc.md`, `cli/docs/sdlc/build.md`, `cli/docs/sdlc/decompose.md`, `cli/docs/sdlc/fix.md`, `cli/docs/getting-started.md`, `cli/docs/aiki-for-clawbots.md`, `cli/docs/tasks/templates/spawn.md`
3. Update `AGENTS.md` references (if any)
4. Update any task name patterns that reference "Loop:" in docs

---

## Resolved Questions

1. ~~**Deprecation period for `loop`?**~~ No — clean break, no alias.
2. ~~**Shared function vs subprocess?**~~ Shared function (`run_code()`). Consistent pattern, supports `ScreenSession` passthrough.
3. ~~**Bug report input format**~~ Deferred. v1 only supports `plan fix` (review tasks). Bug reports are future `plan bug` subcommand — just a new clap variant + template.
4. ~~**New command vs extend existing?**~~ Subcommands under `plan`. The output is a plan — `aiki plan fix` reads naturally.
5. ~~**Fix orchestrator refactor?**~~ Completed by workflow-steps. `FixStep` enum + `Workflow<FixStep>` in `fix.rs` handles the full pipeline.
6. ~~**Plan subcommand parsing refactor?**~~ Dispatch routing already exists in `plan.rs`. The real work is extracting a shared `run_plan_fix()` from the two separate implementations in `plan.rs` and `fix.rs`.

---

---
draft: false
---

# Cleanup Loop and Fix Commands

**Date**: 2026-03-23
**Status**: Draft
**Purpose**: Rename CLI commands to be more self-documenting and separate concerns cleanly

**Related Documents**:
- [cli/src/commands/loop_cmd.rs](../../cli/src/commands/loop_cmd.rs) - Current loop command
- [cli/src/commands/fix.rs](../../cli/src/commands/fix.rs) - Current fix command (orchestrator + planning)
- [cli/src/commands/build.rs](../../cli/src/commands/build.rs) - Build command (reference pure orchestrator)
- [cli/src/commands/plan.rs](../../cli/src/commands/plan.rs) - Current plan command (interactive authoring)

---

## Executive Summary

Two renames and one refactor to the CLI command surface:

1. **`aiki loop` → `aiki code`** — The "execute subtasks" stage gets a name that describes its purpose (coding) rather than its mechanism (looping).
2. **`aiki plan` keeps its current subcommand support but is cleaned up** — `aiki plan fix <review-id>` already exists; this work makes that interface explicit in clap/help, keeps the default authoring mode as `epic`, and preserves backward compatibility for `aiki plan <path-or-text...>`.
3. **`aiki fix` becomes a thinner orchestrator** — Like `build`, it chains shared stages, but it must still create and manage the fix-parent container that carries remediation links and review context.

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

### `aiki plan` (extended with subcommands)

```bash
# Interactive epic plan authoring (shortcut — aiki plan = aiki plan epic)
aiki plan "Build authentication system"
aiki plan epic "Build authentication system"

# Produce fix plan from review issues (v1)
aiki plan fix <review-task-id>

# Future: produce fix plan from bug report
aiki plan bug <bug-task-id>
```

**Output**: A plan file at `/tmp/aiki/plans/<task-id>.md` that `decompose` can consume.

**Subcommand dispatch**:
- `epic` (default when no subcommand) → interactive plan authoring
- `fix` → fix-plan generation from review issues (existing behavior, cleaned up)
- `bug` → `plan/bug` template (investigate, diagnose, produce plan) (future)

**Compatibility requirement**:
- `aiki plan <path-or-text...>` must keep working exactly as it does today
- Unknown first arguments must continue to be treated as plan input, not rejected as unknown clap subcommands

### `aiki fix` (refactored to pure orchestrator)

```bash
# From a review (existing path, same UX)
aiki fix <review-task-id>
aiki fix <review-task-id> --once
```

Pipeline: `create fix-parent → plan fix → decompose → code → review` (with quality loop unless `--once`).

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

## How It Works

### Design principle: `build` and `fix` are mostly symmetric orchestrators

Both orchestrators follow the same pattern and should share building-block functions where possible. The main difference is that `fix` must first create a remediation container task that carries links back to the originating review and reviewed targets.

| Stage | `build` | `fix` |
|---|---|---|
| **Prepare** | None | `create_fix_parent()` creates remediation container + links |
| **Plan** | Already exists (plan file) | `run_plan_fix()` produces one for the fix-parent |
| **Decompose** | `run_decompose()` | `run_decompose()` |
| **Execute** | `run_code()` | `run_code()` |
| **Review** | `run_review()` (optional, `--review`) | `run_review()` (built-in, quality loop) |

Both call the same shared Rust functions — no subprocesses between stages. The `ScreenSession` (live TUI) is threaded through all stages for a seamless experience.

### Current pipeline (fix)

```
aiki fix <review-id>
  └─ [inline] read review issues, check actionable
  └─ [inline] create plan-fix task from "fix" template → run it
  └─ [inline] decompose plan into subtasks
  └─ run_loop(fix-parent)               ← calls loop internally
  └─ [inline] create review, run quality loop
```

### New pipeline (fix as pure orchestrator)

```
aiki fix <review-id>
  └─ create_fix_parent(<review-id>)     ← preserves remediates/fixes/subtask-of links
  └─ run_plan_fix(<review-id>, <parent>)← shared function, produces plan for fix-parent
  └─ run_decompose(<plan>, <parent>)    ← unchanged
  └─ run_code(<parent>)                 ← renamed from run_loop
  └─ run_review(<parent>)               ← unchanged
  └─ quality loop (repeat if issues)
```

### `plan` subcommand dispatch

```
aiki plan ...
  ├─ epic (default in help/docs; bare `aiki plan ...` still routes here)
  │   └─ interactive plan authoring (existing behavior)
  │
  ├─ fix <review-task-id>
  │   └─ run_plan_fix(): use fix-plan template
  │      (read issues, group by file, produce structured plan)
  │
  ├─ bug <task-id> (future)
  │   └─ run_plan_bug(): use "plan/bug" template
  │      (investigate, reproduce, diagnose, produce plan)
  │
  └─ anything else
      └─ treat as epic-plan input for backward compatibility
```

---

## Implementation Plan

### Phase 1: Rename `loop` → `code` (clean break)

1. Rename `cli/src/commands/loop_cmd.rs` → `cli/src/commands/code_cmd.rs`
2. Rename structs: `LoopArgs` → `CodeArgs`, `LoopOptions` → `CodeOptions`
3. Rename `run_loop()` → `run_code()`
4. Update `main.rs`: `Commands::Code`, no alias
5. Update help text: "Execute a parent task's subtasks via lanes"
6. Update all callers in `build.rs` and `fix.rs`
7. Rename `--loop-template` flag → `--code-template` in `build.rs` and `fix.rs`
8. Rename template: `core/loop.md` → `core/code.md`
9. Update task type labels (spawner creates "Loop: X" → "Code: X")
10. Update `CLAUDE.md` references

### Phase 2: Add `plan` subcommands

1. Refactor `plan.rs` command parsing carefully:
   - Preserve bare `aiki plan <path-or-text...>` behavior
   - Support documented `epic` and `fix` subcommands
2. `aiki plan` (no subcommand) remains equivalent to epic authoring via compatibility dispatch, not strict clap-only parsing
3. Extract `run_plan_fix()` shared function: takes review task ID and fix-parent ID, produces a plan file for that parent
   - Derived from existing `plan.rs::run_fix()` plus `fix.rs::create_plan_fix_task()`
   - v1: only `fix`; future `plan bug` is just a new subcommand + template
4. If introducing `core/plan/fix.md`, update call sites to use the new template name as part of the refactor
5. Output: plan file path, compatible with `decompose` input

### Phase 3: Refactor `fix` to pure orchestrator

1. Keep `create_fix_parent()` in `fix.rs` or move it to shared code, but preserve its current links and task semantics
2. Remove only the duplicated plan-fix task creation logic from `fix.rs`
3. Replace it with a call to `run_plan_fix(review_id, fix_parent_id, ...)`
4. Quality loop stays in `fix.rs` (that's the orchestration logic)
5. Remove `core/fix.md` once all in-tree call sites have been migrated to the new template location/name

### Phase 4: Cleanup

1. Update docs, AGENTS.md references
2. Update any task name patterns that reference "Loop:" or "Fix:"

---

## Resolved Questions

1. ~~**Deprecation period for `loop`?**~~ No — clean break, no alias.
2. ~~**Shared function vs subprocess?**~~ Shared function (`run_plan_fix()`, `run_code()`). Consistent pattern, supports `ScreenSession` passthrough.
3. ~~**Bug report input format**~~ Deferred. v1 only supports `plan fix` (review tasks). Bug reports are future `plan bug` subcommand — just a new clap variant + template.
4. ~~**New command vs extend existing?**~~ Subcommands under `plan`. The output is a plan — `aiki plan fix` reads naturally. Explicit subcommands avoid input-type ambiguity.

---

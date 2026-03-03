# `aiki pipeline` Command

## Problem

Multi-stage commands like `aiki fix` and `aiki build` both need the same pattern: create a parent task, run a sequence of stages, handle `--async` via spawn-self/resume. This logic is currently duplicated in each command's Rust code.

As more multi-stage workflows emerge (e.g., `aiki debug` → plan/fix → decompose → loop), the duplication grows. A general-purpose pipeline primitive would let any sequence of aiki commands be composed into an async-capable pipeline.

## Design

### User-facing command

```bash
aiki pipeline <parent-id> [--async] -- <stage1> -- <stage2> -- <stage3>
```

Each stage is an aiki command. Stages run sequentially. The parent task is the contract boundary — `--async` returns the parent ID, caller waits on it.

### Examples

```bash
# Fix pipeline (what aiki fix does internally)
aiki pipeline <fix-parent> -- \
  "plan fix <review-id>" -- \
  "decompose <plan-path> --parent <fix-parent>" -- \
  "loop <fix-parent>"

# Build pipeline (what aiki build does internally)
aiki pipeline <epic-id> -- \
  "loop <epic-id>"

# Debug pipeline (future)
aiki pipeline <fix-parent> -- \
  "debug <bug-report>" -- \
  "plan fix <review-id>" -- \
  "decompose <plan-path> --parent <fix-parent>" -- \
  "loop <fix-parent>"

# Async — returns parent ID immediately
aiki pipeline <fix-parent> --async -- \
  "plan fix <review-id>" -- \
  "decompose <plan-path> --parent <fix-parent>" -- \
  "loop <fix-parent>"
```

### Async mechanism

Same spawn-self pattern as fix/build, but generalized:

```
Blocking:
  Run each stage sequentially in-process
  Return when last stage completes

Async:
  1. Serialize stage list
  2. Spawn detached: aiki pipeline --_resume <parent-id> --stages <serialized>
  3. Return parent ID immediately

Resumed:
  1. Deserialize stages
  2. Run each stage sequentially
  3. Close parent when done
```

### Stage variables

Stages may need to reference outputs from previous stages (e.g., decompose needs the plan path from plan/fix). Options:

- **Convention-based paths** — stages write to predictable locations (e.g., task-scoped temp dir)
- **Stage output capture** — pipeline captures stdout from each stage and makes it available as `$STAGE_N_OUTPUT`
- **Task data** — stages write results to task data fields, subsequent stages read them

Convention-based paths is simplest and matches the current approach.

## Relationship to implement-loop-refactor

The [loop refactor plan](../now/implement-loop-refactor.md) uses Level 1 (shared Rust function) for the spawn-self/resume pattern. This plan is Level 2 — promoting that internal pattern to a user-facing command.

The refactor should land first with the shared Rust function. This command can be layered on top later if more pipelines emerge.

## Trigger

Consider building this when:
- A third multi-stage workflow is needed (e.g., `aiki debug`)
- Users want to compose custom workflows from aiki primitives
- The spawn-self/resume logic in fix.rs and build.rs starts diverging

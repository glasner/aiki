---
draft: false
---

# Run Task Templates

**Date**: 2026-03-06
**Status**: Draft
**Purpose**: Add `--template` flag to `aiki task run` so users can create-and-spawn a task from a template in one command.

**Related Documents**:
- [Task CLI commands](../../cli/src/commands/task.rs) - CLI argument parsing and handlers
- [Task runner](../../cli/src/tasks/runner.rs) - Agent spawn logic
- [Template resolver](../../cli/src/tasks/templates/resolver.rs) - Template lookup and parsing

---

## Executive Summary

Currently, running a task from a template requires two steps: `aiki task add --template X` then `aiki task run <id>`. This plan adds `--template` (and `--data`) to `aiki task run` so it can create and spawn in one atomic command. This is a small, focused CLI change — the template creation and task runner infrastructure already exist.

---

## User Experience

### Before (two commands)

```bash
aiki task add --template aiki/review --data scope="@"
# → Added: abc123 — Review: ...
aiki task run abc123
```

### After (one command)

```bash
aiki task run --template aiki/review --data scope="@"
# → Added: abc123 — Review: ...
# → (agent spawns and runs)
```

### Flags

| Flag | Type | Description |
|------|------|-------------|
| `--template` | `String` | Template name (e.g., `aiki/review`) |
| `--data` | `Vec<String>` | Key=value pairs passed to template (e.g., `scope="@"`) |

When `--template` is provided, `id` becomes optional (a task ID is generated from the template). When `--template` is absent, `id` is required (existing behavior).

### Combined with existing flags

```bash
# Template + async
aiki task run --template aiki/review --data scope="@" --async

# Template + agent override
aiki task run --template aiki/plan --agent codex
```

---

## How It Works

1. User runs `aiki task run --template X --data key=val`
2. CLI validates: either `--template` or `id` must be provided (not both? or allow both?)
3. If `--template` is present:
   a. Call `create_from_template()` (same code path as `task add --template`)
   b. Get back the created task ID
   c. Pass that ID into the existing `run_task_with_output()` / `run_task_async_with_output()` flow
4. Agent spawns and runs the task as normal

The only new code is in `run_run()` — a ~15-line block before the existing run logic that conditionally creates from template first.

---

## Implementation Plan

### Phase 1: CLI changes (single PR)

1. **Add `--template` and `--data` fields to the `Run` variant** in `TaskCommand` enum (`task.rs` ~line 690)
   - `template: Option<String>` with `#[arg(long)]`
   - `data: Option<Vec<String>>` with `#[arg(long)]`

2. **Update `run_run()` handler** (`task.rs` ~line 5327)
   - At the top of the function, check if `--template` is provided
   - If yes: call `create_from_template()` to create the task, use returned ID
   - If no: require `id` as before
   - Make `id` field `Option<String>` (or use a clap group to make `id` and `--template` mutually exclusive)

3. **Validation**
   - Error if neither `id` nor `--template` provided
   - Decide: error if both provided, or allow `--template` to override?

### Phase 2: Documentation

4. **Update CLAUDE.md** — add `task run --template` to the quick reference and delegation sections

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Template not found | Error before spawn: `Error: template 'X' not found` |
| Template creation fails | Error before spawn: propagate template error |
| Spawn fails after creation | Task exists in "ready" state; user can `aiki task run <id>` to retry (same as current behavior when `task add` succeeds but `task run` fails) |
| Neither `id` nor `--template` | CLI error: `Error: provide either a task ID or --template` |

---

## Decisions

1. `id` and `--template` are mutually exclusive.
2. `--data` without `--template` is an error.

---

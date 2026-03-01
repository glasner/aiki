# Consolidate `aiki wait` into `aiki task wait`

## Problem

We have two wait commands with overlapping functionality:

- **`aiki wait`** — top-level command, single task ID, stdin support, pipe-friendly (outputs plain task ID)
- **`aiki task wait`** — task subcommand, multiple IDs, `--any` flag, rich markdown output

`aiki wait` exists solely for pipe chains like `aiki review --async | aiki wait | aiki fix`. But it's confusing to have two commands, and the functionality belongs in the `task` namespace.

## Goal

Single command `aiki task wait` with all features from both. Remove `aiki wait`.

## Prerequisites

**Must be implemented first:**
- `ops/now/simplify-non-tty-output.md` — Creates `output_utils` module with `output_collection()` helper

This consolidation depends on the shared output utilities to avoid duplicating TTY detection and formatting logic.

---

## Changes

### 1. Make IDs optional in `aiki task wait`

**File:** `cli/src/main.rs` (clap struct) + `cli/src/commands/task.rs`

Change `<IDS>...` from required to optional. When no IDs are provided, read from stdin (port the stdin logic from `cli/src/commands/wait.rs`).

Include the `extract_task_id()` helper that parses XML `task_id="..."` attributes from piped input — this is what makes `aiki task run --async | aiki task wait` work.

### 2. Use output_collection helper for consistent output

**File:** `cli/src/commands/task.rs` (`run_wait`)

**Prerequisites:** Implement `ops/now/simplify-non-tty-output.md` first (creates `output_utils::output_collection`).

Use the new `output_collection()` helper from `output_utils` to handle TTY detection and output formatting:

```rust
use crate::output_utils::output_collection;

// After tasks complete
let completed_tasks = /* ... gather completed task objects ... */;
output_collection(&completed_tasks, format_wait_results);
```

This automatically:
- Outputs rich markdown table to stderr when TTY
- Outputs plain task IDs to stdout when piped (enables `| aiki fix` downstream)
- Skips expensive formatting when fully piped (performance optimization)

### 3. Port exit code semantics

**File:** `cli/src/commands/task.rs` (`run_wait`)

Current `aiki task wait` always returns `Ok(())` when tasks reach terminal state. Port the exit code logic from `aiki wait`:
- Exit 0 if all tasks closed with `done` outcome
- Exit non-zero if any task was `stopped` or closed as `wont_do`

This matters for pipe chains where downstream commands should not run on failure.

### 4. Adopt exponential backoff from `aiki wait`

**File:** `cli/src/commands/task.rs` (`run_wait`)

Current `aiki task wait` uses fixed 500ms polling. `aiki wait` uses exponential backoff (100ms → 200ms → 400ms → ... → 2000ms cap). Adopt the exponential approach — it's more responsive for fast tasks and lighter on I/O for slow ones.

### 5. Remove `aiki wait`

- Delete `cli/src/commands/wait.rs`
- Remove `Wait` variant from `Commands` enum in `cli/src/main.rs`
- Remove `mod wait` from `cli/src/commands/mod.rs`

### 6. Update references

| File | Change |
|------|--------|
| `README.md:221` | `aiki wait` → `aiki task wait` |
| `CLAUDE.md` | Already uses `aiki task wait`; verify no `aiki wait` refs |
| `cli/tests/test_async_tasks.rs` | Update test helper to use `aiki task wait` |
| `ops/done/review-and-fix.md` | `aiki wait` → `aiki task wait` (many occurrences, but it's a done doc — low priority) |
| `ops/done/background-run.md` | Same |
| `ops/now/review-loop-plugin.md` | Same |

## Result

```bash
# Rich output (TTY)
aiki task wait abc123 def456

# Pipe-friendly (stdin + stdout passthrough)
aiki task run abc --async | aiki task wait | aiki fix

# Multiple tasks, return on first completion
aiki task wait abc123 def456 --any

# Single task, piped
aiki review --async | aiki task wait | aiki fix
```

## Non-goals

- No new flags or features beyond what both commands already support
- No changes to `aiki task run --async` output format

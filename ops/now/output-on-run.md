# Add `--output` flag to `aiki task run`

## Prerequisites

- [session-run-command.md](session-run-command.md) — Moves `--next-session`
  and `--lane` out of `task run` into `aiki session next`. After that lands,
  `task run` is simple (just a task ID) and `--output id` unambiguously
  means the task ID.

## Goal

Support `--output id` and `--output summary` on `aiki task run`, matching the
pattern used by `task add`, `task start`, `task show`, and `task wait`.

## Constraints

| Format    | `--async` | Sync (blocking) |
|-----------|-----------|-----------------|
| `id`      | ✅        | ✅               |
| `summary` | ❌ (error) | ✅              |

- `--output id` — print bare task ID to stdout, nothing else. Works in both
  sync and async modes. Primary scripting use case:
  `task_id=$(aiki task run <id> --async --output id)`.
  In sync mode, prints the ID then runs the agent (suppressing markdown and TUI).
- `--output summary` (sync only) — after the agent completes, print the task's
  effective summary to stdout instead of markdown or TUI.
- `--output summary --async` → error: "--output summary requires sync mode
  (async runs return before the task completes)"

## Current state

- `Run` variant in `TaskCommands` (task.rs:694) has no `output` field.
- `run_run()` (task.rs:5298) doesn't accept output format.
- `run_task_async_with_output()` (runner.rs:549) always prints markdown.
- `run_task_with_output()` (runner.rs:445) always prints markdown.
- `task_run()` (runner.rs:249) runs TUI on TTY, `spawn_blocking` otherwise.
- `handle_session_result()` (runner.rs:337) prints summary to stderr when sync
  run completes (only in non-quiet mode).

### TUI interaction (new Elm architecture)

The sync path in `task_run()` (runner.rs:249-290) checks `stdout().is_terminal()`:
- **TTY:** spawns agent in background, runs `tui::app::run()` (Elm event loop
  with `Viewport::Inline` on stdout), then maps TUI `Effect` to
  `AgentSessionResult` via `map_tui_effect()`.
- **Non-TTY (piped):** calls `spawn_blocking()` directly, no TUI.

Since `--output id` and `--output summary` write bare text to stdout, they
**must bypass the TUI** — the TUI and structured output cannot share stdout.
The TUI is already gated on `is_terminal()`, and when `--output` is set the
caller is scripting, so skipping the TUI is the correct behavior.

### `--next-session` / `--lane` (moved to `session next`)

These flags are being moved to `aiki session next` (see
[session-run-command.md](session-run-command.md)). After that lands,
`task run` always takes a direct task ID — no resolution logic. This makes
`--output id` unambiguous: it always means the task ID you passed in.

## Changes

### 1. Use `TaskOutputFormat` on `Run` (task.rs)

Add `output: Option<TaskOutputFormat>` to the `Run` variant (reuse the enum
from `Show` which already has `Id` and `Summary`).

```rust
Run {
    // ... existing fields ...

    /// Output format (id: bare task ID, summary: completion summary)
    #[arg(long, short = 'o', value_name = "FORMAT")]
    output: Option<TaskOutputFormat>,
}
```

Thread it through the match arm into `run_run()`.

### 2. Validate combinations in `run_run()` (task.rs)

At the top of `run_run()`, before any task resolution:

```rust
// Validate --output combinations
if run_async && matches!(output_format, Some(TaskOutputFormat::Summary)) {
    return Err(AikiError::InvalidArgument(
        "--output summary requires sync mode (incompatible with --async)".into()
    ));
}
```

### 3. Handle `--output id` (task.rs)

`--output id` works in both async and sync modes — prints bare task ID,
suppresses markdown output and TUI.

**Async path:** call `task_run_async()` directly, print bare ID, return.

```rust
if run_async && matches!(output_format, Some(TaskOutputFormat::Id)) {
    let handle = task_run_async(cwd, &actual_id, options)?;
    println!("{}", handle.task_id);
    return Ok(());
}
```

**Sync path:** print bare ID, run agent via `spawn_blocking` (no TUI, no
markdown), then handle the session result quietly.

```rust
if !run_async && matches!(output_format, Some(TaskOutputFormat::Id)) {
    println!("{}", actual_id);
    // Force quiet + non-TUI path: stdout is for the ID, not the TUI
    options = options.with_quiet(true);
    task_run(cwd, &actual_id, options)?;
    return Ok(());
}
```

The `quiet` flag on `TaskRunOptions` already suppresses `handle_session_result`
output. Combined with `--output id`, `task_run()` will take the non-TUI path
because `show_tui = is_terminal() && !quiet` evaluates to false.

### 4. Handle `--output summary` in sync path (task.rs)

When `output_format == Some(Summary)` and sync:

Same approach — suppress TUI via `quiet`, run the agent, then reload and print
the summary.

```rust
if !run_async && matches!(output_format, Some(TaskOutputFormat::Summary)) {
    options = options.with_quiet(true);
    task_run(cwd, &actual_id, options)?;
    // Reload task to get summary written by the agent
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    if let Ok(task) = find_task(&graph.tasks, &actual_id) {
        if let Some(summary) = task.effective_summary() {
            println!("{}", summary);
        }
    }
    return Ok(());
}
```

### 5. No changes to runner.rs

The existing `run_task_with_output` and `run_task_async_with_output` wrapper
functions stay as-is — they're still used for the default (no `--output`)
code path. The TUI suppression works through the existing `quiet` flag on
`TaskRunOptions`, which already gates `show_tui` in `task_run()`.

## Testing

- `aiki task run <id> --async --output id` → prints bare 32-char ID, exits 0
- `aiki task run <id> --output id` (sync) → prints bare ID, runs agent without TUI
- `aiki task run <id> --output summary` (sync) → runs agent without TUI, prints summary
- `aiki task run <id> --async --output summary` → error message
- `aiki task run <id> --async` (no --output) → existing markdown output unchanged
- `aiki task run <id>` (no flags) → existing behavior unchanged (TUI on TTY)

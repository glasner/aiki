# Cleanup Extra Output: Remove Context Banner & Sanitize Task Names

**Date**: 2026-03-19
**Status**: Draft
**Priority**: P0 (regression — breaks `task show` and `task list` readability)

---

## Problem

Two issues compound to make task CLI output broken:

### 1. Task name contains embedded help text (data bug)

Task `ynsrusvzsqvzknwkvyoupzqllzsrqnup` ("Improve AI code review engine") has the entire `aiki --help` output embedded in its name field. When any command renders this task's name (e.g., in the In Progress list), the help text spills into the output, making it look like `aiki` is printing its own help mid-stream.

**Root cause:** An agent created a task with a multi-line name that captured `aiki --help` output. No validation exists to prevent this.

### 2. Context banner is noisy and redundant (design issue)

Every task CLI command appends a "context banner" — an In Progress + Ready section — to its output. This made sense early on when there were few tasks, but now:

- `task show` appends 80+ in-progress tasks after the task details
- `task list` already shows the task list — the banner duplicates it
- `task stop` / `task close` append a full task dump after the action confirmation
- The banner has no cap on in-progress tasks (ready is capped at 5)

The context banner was designed for agents (so they know what to work on next). But it's being shown on human-facing CLI output too, where it's just noise.

---

## What Gets Removed / Changed

### Phase 1: Remove context banner from CLI task commands

**File: `cli/src/commands/task.rs`**

Remove `build_context()` calls from these commands:

| Command | Current behavior | New behavior |
|---------|-----------------|--------------|
| `task show` | Task details + full In Progress + Ready | Task details only |
| `task stop` | Action confirmation + `---` + In Progress + Ready + footer | Action confirmation only |
| `task close` | Action confirmation + `---` + In Progress + Ready + footer | Action confirmation only |

**Keep the context banner for:**
- `task list` / `task` (default) — this IS the task list, the banner is the output

**File: `cli/src/tasks/md.rs`**

| Function | Action |
|----------|--------|
| `build_context()` | Keep (still used by `build_list_output` and hooks) |
| `build_transition_context()` | Remove entirely (only used by stop/close) |
| `MdBuilder::build()` | Remove the `build_context` call — callers that need context can add it explicitly |

### Specific code changes in `task.rs`

**`run_show` (~line 4237-4242):** Remove the context footer block:

```rust
// DELETE these lines:
let scope_set = get_current_scope_set(&graph);
let ready = get_ready_queue_for_scope_set(&graph, &scope_set);
let in_progress = get_in_progress(&tasks);
let in_progress_refs: Vec<_> = in_progress.iter().map(|t| *t).collect();
content.push_str(&build_context(&in_progress_refs, &ready));
```

**`run_stop` (~line 2799):** Remove `build_transition_context` call:
```rust
// DELETE:
output.push_str(&build_transition_context(&in_progress_refs, &ready_refs));
```

**`run_close` (~line 3601):** Remove `build_transition_context` call:
```rust
// DELETE:
output.push_str(&build_transition_context(&in_progress_refs, &ready_refs));
```

### Phase 2: Sanitize task names on creation

**File: `cli/src/commands/task.rs`**, in `run_add` (the task creation path):

Add name sanitization before writing the task event:

```rust
/// Sanitize a task name: collapse to single line, truncate to 120 chars.
fn sanitize_task_name(name: &str) -> String {
    let single_line = name
        .lines()
        .next()
        .unwrap_or(name)
        .trim();

    if single_line.len() > 120 {
        format!("{}...", &single_line[..117])
    } else {
        single_line.to_string()
    }
}
```

Apply to `run_add` and `run_start` (quick-start path) before the task event is written.

---

## What This Does NOT Change

- **Hook context** (`flows/state.rs::build_context`, `hooks.yaml`): The task count injected into agent session/turn hooks (`Tasks (N ready)`) is unaffected. That's a separate system and serves a different purpose (agent awareness, not CLI output).
- **`task list` output**: The default command still shows In Progress + Ready + footer. That's its job.
- **`build_context()` in `tasks/md.rs`**: The function stays — it's still used by `build_list_output` for `task list`.

---

## Implementation Order

1. **Phase 3 first** — fix the corrupted task data (one command, immediate relief)
2. **Phase 1** — remove context banner from show/stop/close (small code deletions)
3. **Phase 2** — add name sanitization (prevent recurrence)

Total estimated scope: ~20 lines deleted, ~15 lines added.

---

## Verification

After implementation:

```bash
# task show should print ONLY task details, no In Progress/Ready section
aiki task show <id> | grep -c "In Progress:"   # expect: 0

# task close should print ONLY the confirmation line
aiki task close <id> --summary "test" | wc -l   # expect: 1-3 lines

# task list should still show the full list
aiki task list | grep "Ready"                    # expect: match

# Long task names should be truncated
aiki task add "$(python3 -c 'print("x" * 200)')"
aiki task list | grep "xxx"                      # expect: truncated with ...
```

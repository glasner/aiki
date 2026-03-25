# Fix: Loop orchestrator never closes parent task

**Date**: 2026-03-24
**Status**: Proposed
**Priority**: P1 — causes infinite loops when parent stays InProgress forever

---

## Problem

The loop orchestrator (`aiki loop`) never closes the parent task it orchestrates. After all subtasks complete and the loop exits, the parent stays `InProgress` indefinitely. If the loop was called from `build` or `fix`, this blocks the pipeline. If called standalone, the parent is orphaned as permanently in-progress.

### What happens today

```
aiki loop <parent>
  └─ Creates loop task (orchestrates → parent)
  └─ Loop agent runs: starts lanes, waits, repeats
  └─ All lanes complete → agent *should* close itself
  └─ run_loop() returns
  └─ Parent is still InProgress — nobody closes it
```

### Why autostart doesn't help

The parent autostart in `run_close` (task.rs:3373-3423) checks whether all subtasks are closed and the parent is not yet InProgress. But the parent **is** already InProgress (started by `run_loop()` at loop_cmd.rs:143-145), so the guard at line 3381 skips it. This is correct — autostart was designed for the manual agent workflow, not the orchestrated workflow.

---

## Root Cause

1. **Template gap**: The loop template (`core/loop.md`) only instructs the agent to close itself (`aiki task close {{id}}`). It never mentions closing the parent (`{{data.target}}`).

2. **Missing autostart guard**: While the InProgress guard currently prevents autostart from firing for orchestrated parents, there's no explicit check for orchestration — it relies on the parent already being InProgress as a side effect.

---

## Plan

### Step 1 — Update loop template to close parent

**File:** `cli/src/tasks/templates/core/loop.md`

Update the Completion section to close the parent first, then close itself:

```markdown
## Completion

When all lanes are complete:

    aiki task close {{data.target}} --summary "All subtasks completed"
    aiki task close {{id}} --summary "All lanes completed"
```

Closing parent before self ensures the parent is closed even if the agent session ends right after the first close command.

### Step 2 — Skip autostart for orchestrated parents

**File:** `cli/src/commands/task.rs` — `run_close()`, around line 3373

Add an explicit orchestrator check to both autostart blocks. This protects against edge cases where the parent isn't InProgress (e.g., it was stopped, or a race condition):

```rust
for parent_id in &unique_parent_ids {
    if all_subtasks_closed(&graph, parent_id) {
        if let Some(parent) = graph.tasks.get_mut(parent_id) {
            if parent.status == TaskStatus::Closed {
                continue;
            }
            if parent.status == TaskStatus::InProgress {
                continue;
            }

            // NEW: Skip autostart if parent has an active orchestrator
            let orchestrators = graph.edges.referrers(parent_id, "orchestrates");
            let has_active_orchestrator = orchestrators.iter().any(|orch_id| {
                graph.tasks.get(orch_id)
                    .map_or(false, |t| t.status != TaskStatus::Closed)
            });
            if has_active_orchestrator {
                continue;
            }

            // ... existing auto-start logic ...
        }
    }
}
```

Same guard for the next-subtask autostart block (line 3431):

```rust
for parent_id in &unique_parent_ids {
    if all_subtasks_closed(&graph, parent_id) {
        continue;
    }

    // NEW: Skip next-subtask autostart if parent is orchestrated
    let orchestrators = graph.edges.referrers(parent_id, "orchestrates");
    let has_active_orchestrator = orchestrators.iter().any(|orch_id| {
        graph.tasks.get(orch_id)
            .map_or(false, |t| t.status != TaskStatus::Closed)
    });
    if has_active_orchestrator {
        continue;
    }

    // ... existing next-subtask auto-start logic ...
}
```

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/tasks/templates/core/loop.md` | Add `aiki task close {{data.target}}` to Completion section |
| `cli/src/commands/task.rs` | Add orchestrator guard to both autostart blocks in `run_close()` |

---

## Verification

```bash
# Unit test: orchestrated parent should not be auto-started
# Create parent with orchestrator, close all subtasks, verify parent is NOT auto-started

# Integration test:
aiki task add "Test parent"
aiki task add --subtask-of <parent> "Sub A"
aiki task add --subtask-of <parent> "Sub B"
aiki task start <parent>

# Run loop (sync)
aiki loop <parent>

# After loop completes:
aiki task show <parent>   # Should be Closed
aiki task show <loop-id>  # Should be Closed
```

---

## Future Improvement

- [Autoreply safety net for orchestrators](../next/autoreply-for-orchestrators.md) — A `turn.completed` hook with two-strike behavior: autoreply once asking the agent to close its target, then hard-stop if it doesn't. Catches cases where the agent doesn't follow the template (context limit, crash, misinterpretation).

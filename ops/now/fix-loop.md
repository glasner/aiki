# Fix: Loop orchestrator bugs

## Prerequisites

- [session-run-command.md](session-run-command.md) — Moves orchestration
  logic into `aiki session next` / `aiki session wait` and updates the loop
  template (fixes Bug 3).

## Problem

The loop orchestrator (`aiki loop`) has three bugs that cause it to
malfunction when managing concurrent lane execution.

### Bug 1 — InProgress lanes reported as "ready"

**Severity: high**
**File:** `cli/src/tasks/lanes.rs` — `is_lane_ready_with_decomposition`

`is_lane_ready_with_decomposition` finds the first uncompleted session in a
lane and checks whether any task in it is *blocked* (link-based). But
`is_blocked()` only checks edge constraints (blocked-by, depends-on, etc.) —
it does **not** check task status. A lane whose head task is already
`InProgress` passes the readiness check.

**Consequence:** `aiki task lane <parent>` (no `--all`) lists in-progress
lanes as ready. The loop orchestrator then calls
`aiki task run <parent> --next-session --lane <lane> --async`, which
internally calls `resolve_next_session_in_lane`. That function filters for
`status == Open`, finds nothing (the task is `InProgress`), and returns
`Blocked(unclosed)` → error. The orchestrator gets an error each iteration
for every already-running lane.

### Bug 2 — `lane_status` ignores cross-lane dependencies

**Severity: medium**
**File:** `cli/src/tasks/lanes.rs` — `lane_status`

```rust
pub fn lane_status(lane: &Lane, graph: &TaskGraph) -> LaneStatus {
    ...
    if is_lane_ready(lane, graph) { ... }
    ...
}
```

`is_lane_ready` delegates to `is_lane_ready_with_decomposition(lane, graph,
&[])` — empty `all_lanes`. The predecessor-lane check at line 362–363 skips
validation when `all_lanes` is empty, so a lane that depends on an incomplete
predecessor is reported as `Ready` instead of `Blocked`.

**Consequence:** `aiki task lane <id> --all` shows incorrect status for lanes
with cross-lane `depends-on` edges. The orchestrator's "understand the work"
step (`aiki task lane <target> --all`) gives a misleading picture.

### ~~Bug 3 — Loop template bash example doesn't match CLI output~~

Moved to [session-run-command.md](session-run-command.md) (Step 4).

---

## Plan

### Step 1 — Fix `is_lane_ready_with_decomposition` (Bug 1)

**File:** `cli/src/tasks/lanes.rs`

In the `Some(session)` arm (≈line 380), add an InProgress check before the
blocked check:

```rust
Some(session) => {
    // A session with InProgress tasks is already running, not "ready"
    let any_in_progress = session.task_ids.iter().any(|tid| {
        graph
            .tasks
            .get(tid)
            .map_or(false, |t| t.status == TaskStatus::InProgress)
    });
    if any_in_progress {
        return false;
    }
    // No task in the next session should be blocked
    !session.task_ids.iter().any(|tid| graph.is_blocked(tid))
}
```

**Tests:**
- Add `test_lane_ready_in_progress_not_ready` — single-task lane with
  InProgress head → not ready.
- Add `test_lane_ready_in_progress_chain` — needs-context chain where head
  is InProgress → not ready.

### Step 2 — Fix `lane_status` (Bug 2)

**File:** `cli/src/tasks/lanes.rs`

Change `lane_status` signature to accept the full decomposition:

```rust
pub fn lane_status(lane: &Lane, graph: &TaskGraph, all_lanes: &[Lane]) -> LaneStatus {
    if is_lane_failed(lane, graph) { return LaneStatus::Failed; }
    if is_lane_complete(lane, graph) { return LaneStatus::Complete; }
    if is_lane_ready_with_decomposition(lane, graph, all_lanes) {
        return LaneStatus::Ready;
    }
    LaneStatus::Blocked
}
```

Remove the now-unnecessary `is_lane_ready` wrapper (or keep it as a
convenience that passes `&[]` — check callers).

**Callers to update:**
- `cli/src/commands/task.rs` `run_lane` (the `--all` branch, ≈line 5579):
  pass `&decomp.lanes`.
- Grep for other `lane_status` calls and update.

**Tests:**
- Add `test_lane_status_blocked_by_predecessor` — lane B depends on lane A,
  A not complete → B status is `Blocked`.

---

## Verification

```bash
# Unit tests
cargo test -p aiki -- lanes::tests

# Integration: create a parent with two independent subtasks,
# start one, confirm `aiki task lane` does NOT list it as ready
aiki task add "parent"
aiki task add --subtask-of <parent> "A"
aiki task add --subtask-of <parent> "B"
aiki task start <parent>
aiki task start <A>
aiki task lane <parent>        # Should show only B as ready
aiki task lane <parent> --all  # A should be ▶ in-progress, not ● ready
```

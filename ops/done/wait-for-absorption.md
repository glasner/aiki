---
draft: false
---

# Fix Race Condition: `aiki task wait` Returns Before Workspace Absorption

**Date**: 2026-03-04
**Status**: Draft
**Purpose**: Ensure `aiki task wait` does not return until delegated agents' code changes are visible in the parent workspace's filesystem.

---

## Executive Summary

When an agent finishes a delegated task (`aiki task run <id> --async`), the parent uses `aiki task wait` to block until the agent is done. Today, `wait` returns as soon as the **task status** flips to `Closed` — but workspace absorption (the step that moves the agent's file changes into the parent workspace) happens *after* that, during the agent's `turn.completed` / `session.ended` hook. This creates a race window where the parent reads stale files.

This was discovered during the 10-agent isolation stress test: Agent S4 correctly replaced its section in `shared-sections.txt`, but when the parent ran `cat shared-sections.txt` right after `wait` returned, S4's edit wasn't there yet. It appeared ~7 seconds later when absorption completed.

---

## Background: How Workspace Isolation Works

When `aiki task run` spawns a subagent, the lifecycle is:

### Workspace creation (subagent startup)
1. `session.started` hook fires → `core.workspace_ensure_isolated` runs
2. If concurrent sessions detected, creates an isolated JJ workspace: `jj workspace add /tmp/aiki/<repo-id>/<session-id>/ --name aiki-<session-id> -r @-`
3. Agent's `cwd` is set to the isolated workspace directory
4. Agent works in isolation — file writes go to workspace dir, JJ tracks changes under `aiki-<session-id>` workspace

### Task completion (subagent work done)
5. Agent finishes work
6. Agent runs `aiki task close <id> --summary "..."` → writes a `Closed` event (with `session_id`) to the `aiki/tasks` branch
7. Turn completes → `turn.completed` hook fires → `workspace_absorb_all` absorbs changes and writes `Absorbed` event
8. **`aiki task wait` sees `Closed` status AND `Absorbed` event → returns to parent**

### Workspace cleanup (subagent session teardown)
9. Agent session ends → `session.ended` hook fires
10. `core.workspace_absorb_all` runs again (catches any stragglers), writes another `Absorbed` event
11. `jj workspace forget aiki-<session-id>` cleans up the workspace
12. Workspace directory is deleted

### The gap (today, without this fix)

Today `wait` returns at step 6 (sees `Closed`), before step 7 absorbs the changes. In the stress test, this gap was ~7 seconds.

---

## Problem

`run_wait()` in `cli/src/commands/task.rs` polls task status:

```rust
let is_terminal = |id: &str| -> bool {
    find_task(&tasks, id)
        .map(|t| matches!(t.status, TaskStatus::Closed | TaskStatus::Stopped))
        .unwrap_or(false)
};
```

It has no awareness of workspace absorption. The parent gets back control, reads files, and sees pre-agent content.

This is particularly bad because CLAUDE.md instructs agents to always `wait` then `show` results before responding — but `wait` returning doesn't mean results are actually visible in the filesystem.

---

## Proposed Fix: `Absorbed` Task Event + `session_id` on Terminal Events

Two changes:

1. **Add `session_id` to `Closed` and `Stopped` events** — so the absorption code can find which tasks a session completed by looking at `Closed` events directly.
2. **New `Absorbed` event with `task_ids`** — emitted by `workspace_absorb_all` after absorption, referencing the tasks whose changes are now visible.

### New event type

```rust
/// Changes from these tasks have been absorbed into the parent workspace
Absorbed {
    task_ids: Vec<String>,
    session_id: String,
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
}
```

Task-scoped, not workspace-scoped. The event answers "which tasks' changes are now visible?" — a question that belongs in the task event stream. `session_id` is included for provenance and debugging (which session's absorption produced this event).

### Updated terminal events

```rust
Closed {
    task_ids: Vec<String>,
    outcome: TaskOutcome,
    summary: Option<String>,
    session_id: Option<String>,  // ← NEW
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
}

Stopped {
    task_ids: Vec<String>,
    reason: Option<String>,
    session_id: Option<String>,  // ← NEW
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
}
```

`session_id` on terminal events is independently useful as provenance — knowing *which session* closed or stopped a task is good data regardless of absorption.

### How it works

1. Agent closes task → `Closed` event written with `session_id` and `turn_id`
2. `turn.completed` hook fires → `workspace_absorb_all` runs
3. `workspace_absorb_all` reads task events, finds `Closed` events with matching `turn_id`
4. Absorption completes (or determines nothing to absorb)
5. `workspace_absorb_all` writes `Absorbed { task_ids }` for the closed tasks
6. `run_wait` polls task events; after finding all tasks terminal, checks that each task_id appears in an `Absorbed` event
7. Once all waited tasks are absorbed, `run_wait` returns

### Implementation

#### Step 1: Add `session_id` to `Closed` and `Stopped` events

In `cli/src/tasks/types.rs`:

```rust
Closed {
    task_ids: Vec<String>,
    outcome: TaskOutcome,
    summary: Option<String>,
    session_id: Option<String>,  // ← NEW
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
},

Stopped {
    task_ids: Vec<String>,
    reason: Option<String>,
    session_id: Option<String>,  // ← NEW
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
},
```

Update serialization/deserialization in `cli/src/tasks/md.rs`. The `session_id` is `Option` so existing events without it deserialize cleanly.

Wire `session_id` through the close/stop code paths in `cli/src/commands/task.rs` — read it from the `AIKI_SESSION_UUID` env var (already available to agents).

#### Step 2: Add `Absorbed` to `TaskEvent` enum

In `cli/src/tasks/types.rs`:

```rust
/// Changes from these tasks have been absorbed into the parent workspace
Absorbed {
    task_ids: Vec<String>,
    session_id: String,
    turn_id: Option<String>,
    timestamp: DateTime<Utc>,
},
```

Update serialization/deserialization in `cli/src/tasks/md.rs`.

#### Step 3: Emit `Absorbed` from `workspace_absorb_all`

In `cli/src/flows/core/functions.rs`, after absorption completes:

```rust
// Find tasks closed in this turn
let closed_task_ids: Vec<String> = events.iter()
    .filter_map(|e| match e {
        TaskEvent::Closed { task_ids, turn_id: Some(tid), .. }
            if tid == &this_turn_id => Some(task_ids.clone()),
        _ => None,
    })
    .flatten()
    .collect();

if !closed_task_ids.is_empty() {
    let absorbed_event = TaskEvent::Absorbed {
        task_ids: closed_task_ids,
        session_id: this_session_id.clone(),
        turn_id: current_turn_id.clone(),
        timestamp: chrono::Utc::now(),
    };
    write_event(repo_root, &absorbed_event)?;
}
```

**Emitted unconditionally** whenever `workspace_absorb_all` runs and there are closed tasks for the session — regardless of whether there were file changes to absorb. This avoids a failure mode where `run_wait` hangs because the agent's final turn had no file changes.

Note: `workspace_absorb_all` receives `session_id` from the hook context and obtains `turn_id` from the session history. It reads task events to find `Closed` events matching that turn_id.

#### Step 4: Update `run_wait` to check for `Absorbed`

After the task-status poll loop finds all tasks terminal:

```rust
if done {
    // Check that all waited tasks have been absorbed
    let absorbed_tasks: HashSet<&str> = events.iter()
        .filter_map(|e| match e {
            TaskEvent::Absorbed { task_ids, .. } =>
                Some(task_ids.iter().map(|s| s.as_str())),
            _ => None,
        })
        .flatten()
        .collect();

    // Only check absorption for tasks that have a session_id on their Closed event
    // (no session_id = not an isolated agent, no absorption needed)
    let needs_absorption: Vec<&str> = ids.iter()
        .filter(|id| events.iter().any(|e| matches!(e,
            TaskEvent::Closed { task_ids, session_id: Some(_), .. }
                if task_ids.iter().any(|t| t == *id)
        )))
        .map(|s| s.as_str())
        .collect();

    let all_absorbed = needs_absorption.iter().all(|id| absorbed_tasks.contains(id));
    if !all_absorbed {
        std::thread::sleep(Duration::from_millis(delay_ms));
        delay_ms = (delay_ms * WAIT_BACKOFF_MULTIPLIER).min(WAIT_MAX_DELAY_MS);
        continue;
    }
    // ... existing output logic ...
}
```


---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/tasks/types.rs` | Add `session_id` to `Closed`/`Stopped`; add `Absorbed` variant |
| `cli/src/tasks/md.rs` | Serialization/deserialization for `session_id` fields and `Absorbed` event |
| `cli/src/commands/task.rs` | Wire `session_id` through close/stop paths; update `run_wait` with absorption check |
| `cli/src/flows/core/functions.rs` | In `workspace_absorb_all`: find closed tasks by session_id, write `Absorbed` event |

---

## Alternatives Considered

### 1. Poll `jj workspace list` for workspace removal

After detecting tasks are terminal, poll `jj workspace list` to check if the agent's workspace has been forgotten.

**Rejected**: Spawns a `jj` subprocess on every poll iteration (expensive). Couples `wait` to absorption's implementation details.

### 2. Poll for workspace directory deletion

Check if `/tmp/aiki/<repo-id>/<session-id>/` still exists. Cheaper than JJ subprocess (single syscall).

**Rejected**: Still couples `wait` to filesystem implementation details. Fragile if cleanup step ordering changes.

### 3. Workspace-scoped event (`WorkspaceAbsorbed` with `session_id`)

Emit a workspace-level event instead of a task-level event.

**Rejected**: Workspace-level events feel out of place in the task event stream. `Absorbed` with `task_ids` is more natural — it answers "which tasks' changes are visible?" which is the question `run_wait` actually asks.

### 4. Move absorption before task close

Have the agent absorb its workspace before closing the task, so `Closed` implies absorbed.

**Rejected**: Absorption runs in hooks (`turn.completed` / `session.ended`) which fire *after* the agent's action. Restructuring this would require fundamental changes to the hook lifecycle.

---

## Edge Cases

1. **Solo sessions (no workspace)**: `session_id` on the `Closed` event will be `None` (or absent). `run_wait` skips the absorption check for tasks without a `session_id` — no workspace means no absorption needed.

2. **Crashed agents**: If an agent crashes, its `session.ended` hook still fires and `workspace_absorb_all` still runs, so `Absorbed` should still be emitted. If the hook itself fails, `wait` will continue polling. Orphaned workspace recovery handles eventual cleanup.

3. **Non-async `aiki task run`**: Synchronous runs already block until the agent process exits, which includes hooks running. The race only exists with `--async` where the parent proceeds immediately after task close.

4. **Multiple tasks from same session**: If subtasks run in one agent session, they share one workspace. Multiple tasks may appear in a single `Absorbed` event's `task_ids`. `run_wait` checks each waited task individually — all must appear in at least one `Absorbed` event.

5. **Multiple `Absorbed` events per session**: Both `turn.completed` and `session.ended` may emit `Absorbed`. That's fine — `run_wait` only needs each task_id to appear in *at least one* `Absorbed` event.

6. **Task closed without session_id**: Tasks closed by humans or non-isolated agents won't have `session_id`. `run_wait` treats these as not needing absorption — correct, since no workspace exists.

---

## Test Plan

1. Re-run the Phase 2 isolation test (5 agents editing different sections of same file) and verify all 5 sections are present when `cat` runs after `wait` returns
2. Verify `wait` returns promptly when `Closed` events have no `session_id` (solo sessions, human-closed tasks)
3. Verify `--any` mode: if waiting for any-of-N, only check absorption for the completed task(s)
4. Verify multiple `Absorbed` events per session don't cause issues
5. Verify `session_id` is correctly populated on `Closed`/`Stopped` events from agent sessions

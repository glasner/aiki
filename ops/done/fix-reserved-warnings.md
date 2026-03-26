# Fix: Spurious "Released event for task in status in_progress" warnings

## Problem

After certain spawn failures, a `Released` event is written for a task that has already transitioned from `Reserved` to `InProgress`. This produces a warning on every `materialize_graph` call:

```
warn: Released event for task lqswmu in status in_progress, expected Reserved — skipping
```

The warning is harmless (the invalid event is correctly skipped), but it fires on every `aiki task` invocation because the event is permanently in the JJ history.

## Root cause

Race condition in `commands/run.rs` lines 295-305:

```
1. run.rs:135-140    — Reserve task (Open → Reserved)
2. spawn_and_discover — Spawn agent process
   2a. Agent starts → session hook runs `aiki task start` → Started event (Reserved → InProgress)
   2b. Spawn function returns error (e.g., timeout discovering session UUID)
3. run.rs:296-303    — Rollback: emit Released event
   → But task is already InProgress (from step 2a), not Reserved!
```

The rollback at step 3 doesn't check the current task status before emitting the `Released` event. If the agent successfully started (emitting `Started`) but the spawn discovery failed, the release is invalid.

## Fix

### 1. Check status before rollback release

**File:** `cli/src/commands/run.rs` (lines 295-305)

Before emitting the `Released` event, re-read the task graph and check the current status:

```rust
if result.is_err() {
    if let Some(ref cid) = claimed_id {
        // Re-read to check if the agent already started the task
        let events = read_events(cwd).ok();
        let current_status = events
            .map(|e| materialize_graph(&e))
            .and_then(|g| g.tasks.get(cid).map(|t| t.status));

        // Only release if still Reserved (agent hasn't started yet)
        if current_status == Some(TaskStatus::Reserved) {
            let rollback_event = TaskEvent::Released {
                task_ids: vec![cid.clone()],
                reason: Some("Spawn failed, rolling back claim".to_string()),
                timestamp: chrono::Utc::now(),
            };
            let _ = write_event(cwd, &rollback_event);
        }
    }
}
```

### 2. Same guard in runner.rs (if applicable)

**File:** `cli/src/tasks/runner.rs`

Check if `prepare_task_run` or `task_run_async` have similar rollback paths. From the code, `prepare_task_run` (line 230-241) emits the `Reserved` event but doesn't have a rollback on failure — the caller handles it. `task_run_async` (line 634) delegates to `prepare_task_run` but the rollback is in `run.rs`, not here. So only `run.rs` needs the fix.

### 3. Consider: should materialize_graph warn or silently skip?

The `Released` event on a non-Reserved task is a no-op either way. The warning is useful for debugging but noisy when it fires on every invocation. Options:

- **Keep as warning** (current) — correct but noisy for historical events
- **Downgrade to debug_log** — only visible with `AIKI_DEBUG=1`
- **Keep warning but only on first occurrence per task** — less noise

Recommendation: downgrade to `debug_log`. The guard correctly skips the invalid event; the warning adds noise without actionability (you can't fix historical events).

## Testing

- Unit test: spawn fails after agent started → no Released event emitted
- Unit test: spawn fails before agent started → Released event emitted correctly
- Verify the existing `test_released_event_on_non_reserved_task_is_skipped` test still passes

## Not in scope

- Fixing existing invalid events in JJ history (they're permanently recorded; the fix prevents new ones)
- Changing the reserve/start protocol itself (the two-phase Reserve → Started design is correct)

# Task Branch Fan-Out: 30K Heads Killing JJ Performance

**Date**: 2026-03-19
**Status**: Proposed
**Priority**: P0 — repo is nearly unusable

---

## Problem

Every task event (`aiki task start`, `close`, `comment add`, `link`, etc.) writes a JJ change via:

```rust
jj new aiki/tasks --no-edit --ignore-working-copy -m "<metadata>"
```

This forks a new change **as a child of the bookmark**, but the bookmark never advances. The result is a massive fan-out — 30,780 sibling heads all hanging off the same parent:

```
aiki/tasks (root@)
    ├── task event 1  (head)
    ├── task event 2  (head)
    ├── task event 3  (head)
    ├── ...
    └── task event 30,780  (head)
```

JJ must track every head. Operations like `jj log`, `jj status`, workspace creation, and rebase slow to a crawl because JJ's internal algorithms scale with head count.

The same pattern exists for `aiki/conversations` (9 heads currently — small but will grow).

### Impact

- `jj log` takes noticeably longer
- Workspace absorption (rebase) is slow — it must reconcile against all heads
- `jj workspace add` is slow
- Any revset evaluation touches head tracking
- The repo will only get worse over time (linear growth with every task event)

---

## Root Cause

`storage.rs` line 33:
```rust
.args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
```

This creates a new change as a **child** of `aiki/tasks`, but never moves the bookmark forward. So every event is a sibling, not a chain. JJ considers every one a head.

---

## Solution: Chain Events and Advance Bookmark

Instead of fanning out from the bookmark, chain events linearly and advance the bookmark after each write:

```
aiki/tasks
    → event 1 → event 2 → event 3 → ... → event N (bookmark)
```

### Step 1: Change `write_event()` to chain + advance

After creating the new change, get the change ID and advance the bookmark to point at it. The next `write_event` call will then fork from the new head, creating a chain.

#### Getting the change ID reliably

Generate a unique marker and include it in the commit message. Then resolve the new
change directly via template output:

1. Create a random marker like `aiki-task-event-id=<random_hex>` in memory.
2. Run `jj new ... -m "<metadata>\nMARKER"`.
3. Resolve the new full change id using template output:
   `jj log -r "description(substring:'MARKER')" -T "change_id ++ \"\\n\""`
4. Move the bookmark with the returned full change id.

```rust
fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;
    let marker = new_jj_write_marker(TASKS_MARKER_PREFIX);
    let metadata = append_write_marker(&event_to_metadata_block(&event), &marker);

    // Create child of current bookmark head
    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to create task event: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to write task event: {}",
            stderr
        )));
    }

    let new_id = resolve_change_id_by_marker(cwd, &marker)?;
    jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", &new_id, "--ignore-working-copy"])
        .output()?;

    Ok(())
}
```

### Step 1b: Batch writes (`write_events_batch`)

`write_events_batch` already packs N events into a **single** `jj new` call — one commit with multiple `[aiki-task]...[/aiki-task]` blocks concatenated in the commit message. So the batch path already does 1 JJ invocation for N events today.

After the chaining change, `write_events_batch` needs the same bookmark-advance treatment as `write_event`, but only **once at the end** — not per event. The change is minimal:

```rust
pub fn write_events_batch(cwd: &Path, events: &[TaskEvent]) -> Result<()> {
    if events.is_empty() { return Ok(()); }
    if events.len() == 1 { return write_event(cwd, &events[0]); }

    ensure_tasks_branch(cwd)?;
    let marker = new_jj_write_marker(TASKS_MARKER_PREFIX);
    let metadata = append_write_marker(
        &events
            .iter()
            .map(|e| event_to_metadata_block(e))
            .collect::<Vec<_>>()
            .join("\n"),
        &marker,
    );

    // Single jj new for all events (existing behavior)
    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
        .output()?;

    if !result.status.success() { return Err(...); }

    // Single bookmark advance at the end (new)
    let new_id = resolve_change_id_by_marker(cwd, &marker)?;

    jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", &new_id, "--ignore-working-copy"])
        .output()?;

    Ok(())
}
```

**Cost:** 2 JJ invocations total per batch (1 `jj new` + 1 `bookmark set`), same as a single-event write. No per-event overhead.

**Note:** The `write_event` → `write_events_batch` delegation for single events (line `if events.len() == 1`) means the bookmark advance logic could be extracted into a shared `advance_bookmark` helper called by both paths, avoiding duplication.

### Step 2: Concurrency safety

Multiple agents may write task events concurrently. With fan-out, this was safe because each `jj new` created an independent sibling. With chaining, two concurrent writes could both fork from the same bookmark position, creating a temporary fork.

**This is acceptable.** JJ handles divergent bookmarks gracefully. We handle this by:

1. Using `--allow-backwards` on `bookmark set` (in case of rebase)
2. Accepting temporary forks — they resolve naturally as one agent's next write builds on the other's

Alternatively: reuse the existing `acquire_absorb_lock()` pattern from `isolation.rs` to serialize task writes. This is the safest approach if task writes are infrequent enough (they are — humans type slowly).

### Step 3: Migrate existing heads (one-time cleanup)

Task state is ephemeral — reconstructed from event descriptions each time `read_events()` runs. So we can just abandon the old fan-out heads:

```bash
# Export current task state as backup
aiki task > /tmp/task-backup.txt

# Abandon all old fan-out heads
jj abandon 'children(ancestors(aiki/tasks)) & description(substring:"[aiki-task]")' --ignore-working-copy
```

### Step 4: Apply same fix to conversation storage

`history/storage.rs` has the same pattern. Apply the same chain + advance approach to `aiki/conversations`.

---

## Read Path Impact

The reader at `storage.rs:315` uses:
```
children(ancestors(aiki/tasks)) & description(substring:"[aiki-task]")
```

This already works for both fan-out and chain models (it finds all descendants). No read-path changes needed.

After migration, reads will actually be **faster** because JJ doesn't need to enumerate 30K heads.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/tasks/storage.rs` | `write_event()`: add bookmark advance after write; `write_events_batch()`: same, single advance at end (not per-event); extract shared `advance_bookmark` helper |
| `cli/src/history/storage.rs` | Same pattern for conversation events |
| `cli/src/jj/mod.rs` | Add marker helper + marker-based change-id resolution by template |
| `cli/tests/` | Keep/extend `new_jj_write_marker` coverage as needed |

---

## Risks

- **Concurrent writes**: Two agents writing simultaneously could create a temporary fork. Mitigated by lock or by accepting JJ's natural fork resolution.
- **Migration data loss**: Abandoning old heads loses the JJ change metadata. But task events are parsed from descriptions into the TaskGraph in memory — the graph reconstruction doesn't depend on JJ change structure, just on the description content being queryable.
- **Bookmark advancement failure**: If the `bookmark set` fails (e.g., JJ lock contention), the event is still written — it just creates a fan-out head. Next write will pick it up. Degraded but not broken.
- **Marker mismatch**: If marker generation collided (extremely unlikely with random hex token) or if marker propagation is filtered by `jj` in a future output change, bookmark movement can fail. Full-id resolution by template query still surfaces the mismatch and fails with a clear single-match error.

---

## Success Criteria

- Head count drops from ~31K to ~1 per branch (aiki/tasks, aiki/conversations)
- `jj log` and `jj status` return to sub-second performance
- Task operations (`aiki task start`, `close`, etc.) remain correct
- Existing task state is preserved through migration
- Regression monitored via existing test suite

---

## Abandoned Ideas

### Replay migration (re-chain all events)

Instead of simply abandoning old heads, re-chain them:
1. Read all existing events via `read_events()`
2. Abandon all existing task-branch children
3. Replay events as a chain by calling the new `write_event()` for each

**Rejected:** Unnecessary complexity. Task state is ephemeral — `read_events()` reconstructs the graph from descriptions regardless of change structure. Simply abandoning the fan-out heads is sufficient. The revset query works the same on a chain as on a fan-out.

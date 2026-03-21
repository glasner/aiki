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

After creating the new change, capture its change ID from `jj new` and advance
the bookmark to point at it. The next `write_event` call will then fork from
the new head, creating a chain.

#### Use the change ID emitted by `jj new`

`jj new` already tells us which change it created, so we can use that directly.
In a scratch repo, this exact command:

```bash
jj new aiki/tasks --no-edit --ignore-working-copy -m $'[aiki-task]\nkind=start\n[/aiki-task]'
```

produced:

```text
# stdout
(empty)

# stderr
Created new commit xlulsuvp 9ca401f0 (empty) [aiki-task]
```

So the parser should be concrete:

1. Run `jj new ... -m "<metadata>"`.
2. Read `stderr`, not `stdout`.
3. Find the line starting with `Created new commit `.
4. Split that line on ASCII whitespace. The fields are:
   `Created`=`1`, `new`=`2`, `commit`=`3`, `<change_id>`=`4`, `<commit_id>`=`5`, ...
   so use the **4th whitespace-separated token** as `<new_change_id>`.
5. Validate that the parsed token matches JJ's short change-id shape (`[a-z]{8}` today), and fail if the line is missing or malformed.
6. Move the bookmark with `jj bookmark set ... -r <new_change_id>`.

```rust
fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;
    let metadata = event_to_metadata_block(&event);

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

    let new_id = parse_new_change_id(&result.stderr)?;
    advance_bookmark(cwd, TASKS_BRANCH, &new_id)?;

    Ok(())
}

/// Advance bookmark to new_id. If it fails (concurrent writer moved the
/// bookmark forward), rebase our change onto the new bookmark tip and retry.
/// No --allow-backwards — we always move forward.
fn advance_bookmark(cwd: &Path, branch: &str, new_id: &str) -> Result<()> {
    for _ in 0..3 {
        let bm = jj_cmd()
            .current_dir(cwd)
            .args(["bookmark", "set", branch, "-r", new_id, "--ignore-working-copy"])
            .output()?;

        if bm.status.success() {
            return Ok(());
        }

        // Bookmark moved — rebase our event onto the (now-advanced) bookmark tip
        let rebase = jj_cmd()
            .current_dir(cwd)
            .args(["rebase", "-r", new_id, "--onto", branch, "--ignore-working-copy"])
            .output()?;

        if !rebase.status.success() {
            let stderr = String::from_utf8_lossy(&rebase.stderr);
            return Err(AikiError::JjCommandFailed(format!(
                "Failed to rebase orphaned event: {}", stderr
            )));
        }
    }

    // After retries, give up. Event is written (just orphaned as a head).
    // Next writer or read_events() can clean it up.
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
    let metadata = events
        .iter()
        .map(|e| event_to_metadata_block(e))
        .collect::<Vec<_>>()
        .join("\n");

    // Single jj new for all events (existing behavior)
    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
        .output()?;

    if !result.status.success() { return Err(...); }

    // Single bookmark advance at the end (new)
    let new_id = parse_new_change_id(&result.stderr)?;
    advance_bookmark(cwd, TASKS_BRANCH, &new_id)?;

    Ok(())
}
```

**Cost:** 2 JJ invocations total per batch (1 `jj new` + 1 `bookmark set`), same as a single-event write. No per-event overhead.

**Note:** The `write_event` → `write_events_batch` delegation for single events (line `if events.len() == 1`) means the bookmark advance logic could be extracted into a shared `advance_bookmark` helper called by both paths, avoiding duplication.

### Step 2: Concurrency safety (rebase-on-conflict)

Multiple agents may write task events concurrently. With fan-out, this was safe because each `jj new` created an independent sibling. With chaining, two concurrent writes could both fork from the same bookmark position, creating a temporary fork.

We handle this with **rebase-on-conflict** — no `--allow-backwards`:

1. Agent A and Agent B both fork from bookmark at commit X, creating siblings A and B.
2. Agent A advances bookmark to A (succeeds — A is a child of X).
3. Agent B tries `bookmark set` to B — **fails** because B is not a descendant of A.
4. Agent B detects the failure, rebases B onto the new bookmark tip (A): `jj rebase -r B -d aiki/tasks`.
5. Agent B retries `bookmark set` — now succeeds because B is a descendant of A.

This is implemented in the shared `advance_bookmark` helper (see Step 1) with a bounded retry loop (3 attempts). If all retries fail (extremely unlikely — requires 3+ concurrent writers within the same millisecond window), the event is still written as an orphaned head. The read path finds it regardless, and the next writer's retry loop will clean it up.

**Why not `--allow-backwards`?** It creates last-writer-wins semantics where concurrent writers silently clobber each other's bookmark advancement, producing orphaned heads with no error signal. Rebase-on-conflict is strictly better: you either advance forward correctly or detect the conflict and fix it.


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
| `cli/src/tasks/storage.rs` | `write_event()` and `write_events_batch()`: call shared `advance_bookmark` helper after write; helper does `bookmark set` with rebase-on-conflict retry (no `--allow-backwards`) |
| `cli/src/history/storage.rs` | Same pattern for conversation events |
| `cli/src/jj/mod.rs` | Add marker helper + marker-based change-id resolution by template |
| `cli/tests/` | Keep/extend `new_jj_write_marker` coverage as needed |

---

## Risks

- **Concurrent writes**: Two agents writing simultaneously fork from the same parent. The second writer's `bookmark set` fails; it rebases onto the new tip and retries (up to 3 times). In the pathological case (3+ concurrent writers within the same millisecond), the event is still written as an orphaned head — the read path finds it regardless, and the next writer cleans it up.
- **Migration data loss**: Abandoning old heads loses the JJ change metadata. But task events are parsed from descriptions into the TaskGraph in memory — the graph reconstruction doesn't depend on JJ change structure, just on the description content being queryable.
- **Bookmark advancement failure**: If all retry attempts fail (lock contention, extreme concurrency), the event is still written — it just remains as a fan-out head. Next write's retry loop will rebase it. Degraded but not broken.
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

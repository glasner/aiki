# Fix JJ Event-Write Race Condition

## Problem

When two `aiki` processes write task events concurrently (e.g., `aiki task comment` and `aiki task close` fired in parallel by Codex), the second one fails:

```
Error: jj command failed: Failed to find newly created task change
```

### Root Cause

`write_event()` maintains events as a **linear chain** with a manually-managed bookmark:

```
root ← event1 ← event2 ← event3 ← aiki/tasks (bookmark = head)
```

Each write does **three non-atomic JJ commands**:

1. `jj new aiki/tasks --no-edit -m "<metadata>"` — create child of head
2. `jj log -r "children(aiki/tasks) & ..."` — query to find the new change_id
3. `jj bookmark set aiki/tasks -r <change_id>` — advance the bookmark

When two processes race:
- Both create children of the **same** parent (the old head)
- Process A advances the bookmark to its change
- Process B's query now searches `children(aiki/tasks)` where `aiki/tasks` has moved — B's change is a child of the **old** head, not the new one
- B gets an empty result → error

Even when both succeed, one event becomes an **orphaned sibling** unreachable from the `root()..aiki/tasks` ancestor chain:

```
root ← ... ← old_head ← event_A    (orphaned — not in ancestor chain)
                       ← event_B ← aiki/tasks
```

### Current Damage

Verified in this repo: **9 orphaned task events** already exist from past races (spanning Jan 18 – Feb 10, 2026). These include lost comments, a close event, started events, and created events — real data that was silently dropped:

```
$ jj log -r '(heads(all()) & description(substring:"[aiki-task]")) ~ aiki/tasks'
# 9 orphaned events found, all children of old chain positions
```

### Affected Code

The same pattern exists in **two** event stores:

| Store | Write | Read |
|-------|-------|------|
| `cli/src/tasks/storage.rs` | 3-step write (new → query → bookmark set) | `root()..aiki/tasks` ancestor traversal |
| `cli/src/history/storage.rs` | 2-step write with retry (new → bookmark set via revset) | `root()..aiki/conversations` ancestor traversal |

The history store already has retry/backoff but shares the same fundamental flaw. (Currently 0 orphaned conversation events — the retry logic masks but doesn't fix the issue.)

---

## Solution: Flat Sibling Model

Stop moving the bookmark. Freeze it as a fixed anchor. New events accumulate as children:

```
                        ← event_A (was orphaned, now recovered!)
root ← ... ← event_N ← aiki/tasks (frozen — never moves again)
                 ├── new_event1
                 ├── new_event2
                 └── new_event3  (concurrent writes = just more siblings)
```

### Why This Works

- **Write becomes one atomic command**: `jj new aiki/tasks --no-edit -m "<metadata>"` — no query, no bookmark update
- **JJ handles concurrent `jj new` natively**: two processes creating children of the same parent is normal concurrent operation; the op log merges cleanly
- **No bookmark contention**: the bookmark never moves, so there's no race

### The Key Revset

Instead of `root()..aiki/tasks` (ancestor chain only), use:

```
children(ancestors(aiki/tasks)) & description(substring:"[aiki-task]")
```

This single revset finds **everything** because:
- Every chain event is a child of its predecessor (an ancestor of the bookmark)
- Every orphaned event is a child of some old chain position (also an ancestor)
- Every new flat event is a child of the current bookmark target (also an ancestor)
- `root()` itself is in `ancestors()`, so even the first-ever event (child of root) is found

**Verified empirically**: 7199 events in 202ms vs current 232ms — actually **faster** than the current approach.

No union of two revsets needed. No special anchor change needed. No migration needed.

---

## Implementation Plan

### Step 1: Simplify `write_event()` — tasks/storage.rs

**Before** (3 commands, lines 49–131):
```rust
jj new aiki/tasks --no-edit -m "<metadata>"
jj log -r "children(aiki/tasks) & ..." -T change_id --limit 1  // find it
jj bookmark set aiki/tasks -r <change_id>                       // advance head
```

**After** (1 command):
```rust
pub fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;
    let metadata = event_to_metadata_block(event);

    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to create task event: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!("Failed to write task event: {}", stderr)));
    }
    Ok(())
}
```

Delete the `jj log` query (lines 72–106) and `jj bookmark set` (lines 108–128) entirely.

### Step 2: Update `read_events()` — tasks/storage.rs

**Before** (line 156):
```rust
let revset = format!("root()..{}", TASKS_BRANCH);
```

**After**:
```rust
let revset = format!(
    "children(ancestors({})) & description(substring:\"{}\")",
    TASKS_BRANCH, METADATA_START,
);
```

This handles all three scenarios with zero migration:

| Repo State | What the revset finds |
|------------|-----------------------|
| **New repo** (bookmark at root, no events) | Empty (correct) |
| **Existing repo** (chain model, no new writes) | All chain events |
| **Existing repo** (chain + new flat writes) | Chain events + new flat events |
| **Existing repo** (chain + old orphans) | Chain events + **recovered orphans** |

**Add explicit timestamp sorting** after reading. The chain model guaranteed order via position; the flat model doesn't. Sort events by their embedded `timestamp` field in Rust:

```rust
events.sort_by_key(|e| e.timestamp());
```

This requires adding a `timestamp()` accessor to `TaskEvent` (or extracting timestamps during parse).

### Step 3: Update `read_events_with_ids()` — tasks/storage.rs

Same revset change as Step 2. Same timestamp sorting. The template already extracts `change_id` per event, which works identically for flat siblings.

### Step 4: No changes to `ensure_tasks_branch()`

The current initialization (bookmark at `root()`) works correctly with the new revset:
- `ancestors(root()) = {root()}`
- `children({root()})` includes only direct children of root
- `& description(substring:"[aiki-task]")` filters to task events only

No anchor change needed. No migration needed.

### Step 5: Apply same fix to `history/storage.rs`

The conversation event store (`aiki/conversations` branch) has the same pattern:

1. **`write_event()`**: Remove the `jj bookmark set` step (lines 111–135) and the retry logic (constants + `write_event_inner` wrapper, lines 17–90). The retry was masking the race; with a single atomic command it's unnecessary.

2. **`read_events()`**: Change revset from `root()..aiki/conversations` to:
   ```rust
   children(ancestors(aiki/conversations)) & description(substring:"[aiki-conversation]")
   ```

3. **`get_current_turn_info()`**: Update revset from `ancestors(branch) & description(...)` to:
   ```rust
   children(ancestors(branch)) & description(...)
   ```
   (Same pattern — `children(ancestors(x))` ⊇ `ancestors(x) - {root}`)

4. **Other query functions** (`get_latest_turn_number`, etc.): Same revset migration.

5. **No changes to `ensure_conversations_branch()`** — same reasoning as tasks.

### Step 6: Tests

1. **Existing unit tests in storage.rs**: These test serialization/deserialization only (no JJ interaction). Should pass unchanged.
2. **Integration tests**: Update any that assert on chain structure or bookmark advancement.
3. **Run full test suite**: `cargo test --manifest-path cli/Cargo.toml --lib`

---

## Edge Cases Considered

### Timestamp ordering
The chain model guaranteed total event ordering via position. The flat model relies on timestamps (RFC 3339 with nanosecond precision from chrono). Same-nanosecond collisions are theoretically possible but practically impossible across processes (different clock reads) and impossible within a process (sequential writes). Add explicit timestamp sorting in Rust for safety.

### Orphan recovery
The 9 existing orphans are children of old chain positions. Since old chain positions are in `ancestors(aiki/tasks)`, the orphans are in `children(ancestors(aiki/tasks))`. They'll be **automatically recovered** — no special migration step needed. These are real events (comments, closes, creates) that were silently lost. Recovering them is a net positive; `materialize_tasks()` processes events idempotently so late-arriving events are absorbed correctly.

### Bookmark-at-root for new repos
When `aiki/tasks` points to `root()`, `children(ancestors(root())) = children({root()})` could include many changes in a large repo. The `description(substring:"[aiki-task]")` filter ensures correctness. For repos with task events, the first `write_event()` moves the bookmark to event1 under the old code — so this only affects truly empty repos (negligible cost since there are no events to match).

**Post-fix**: new repos never move the bookmark, so it stays at root. `children(root())` is scanned but filtered. For most JJ repos, `children(root())` is small (only the first change in each branch). Performance impact is negligible.

### Performance at scale
With 7188 chain events, the new revset (202ms) is faster than the current approach (232ms). The cost is O(total events) — same asymptotic complexity as the chain model. Over time, new events accumulate as children of the frozen bookmark head. JJ handles nodes with many children efficiently (reverse index lookup).

### Old binary + new binary concurrency
If an old binary (with bookmark advancement) and new binary (without) run concurrently, the old binary could move the bookmark to a wrong target. This is a transitional risk during upgrade. In practice, all agents in a session use the same binary version. After upgrade, all use the new code.

---

## Out of Scope

- **Event compaction/archival** — the flat model accumulates events indefinitely, same as the current chain model. Cleanup is a separate concern.
- **Conflicted bookmark repair** — if past races left `aiki/tasks` in a conflicted state (multiple targets), `jj new aiki/tasks` creates a merge change. Currently no conflicted bookmarks exist in this repo. Could add a pre-flight check if needed.

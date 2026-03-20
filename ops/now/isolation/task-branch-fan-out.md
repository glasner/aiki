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

```rust
// Before (fan-out):
jj new aiki/tasks --no-edit --ignore-working-copy -m "<metadata>"

// After (chain):
jj new aiki/tasks --no-edit --ignore-working-copy -m "<metadata>"
jj bookmark set aiki/tasks -r <new-change-id> --ignore-working-copy
```

After creating the new change, resolve its change ID and advance the bookmark to point at it. The next `write_event` call will then fork from the new head, creating a chain.

**Implementation detail:** `jj new` prints the new change ID to stderr. Parse it, or use `jj log -r 'children(aiki/tasks) & description(substring:"[aiki-task]")' --limit 1` to find the just-created change, then `jj bookmark set`.

A simpler approach: use `jj commit` semantics. Instead of `jj new --no-edit`, we can:
1. `jj new aiki/tasks --no-edit --ignore-working-copy` (creates child)
2. Parse the new change ID from output
3. `jj bookmark set aiki/tasks -r <id> --ignore-working-copy`

Or even simpler — use `jj describe` + `jj new` on the bookmark directly:
1. `jj describe aiki/tasks -m "<metadata>" --ignore-working-copy --no-edit` — NO, this overwrites the previous event.

**Recommended approach:** Wrap the existing `jj new` + add a bookmark advance:

```rust
fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;
    let metadata = event_to_metadata_block(event);

    // Create child of current bookmark head
    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", TASKS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", &metadata])
        .output()?;

    if !result.status.success() { return Err(...); }

    // Advance bookmark to the new change
    // Find the just-created child (most recent child of aiki/tasks)
    advance_tasks_bookmark(cwd)?;

    Ok(())
}

fn advance_tasks_bookmark(cwd: &Path) -> Result<()> {
    // Find the newest child of the current bookmark
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log", "-r",
            &format!("children({}) & description(substring:\"{}\")", TASKS_BRANCH, METADATA_START),
            "--no-graph", "-T", "change_id", "--limit", "1",
            "--ignore-working-copy",
        ])
        .output()?;

    let new_head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if new_head.is_empty() {
        return Ok(()); // No child found, skip
    }

    jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", &new_head, "--ignore-working-copy"])
        .output()?;

    Ok(())
}
```

### Step 2: Concurrency safety

Multiple agents may write task events concurrently. With fan-out, this was safe because each `jj new` created an independent sibling. With chaining, two concurrent writes could both fork from the same bookmark position, creating a temporary fork.

**This is acceptable.** JJ handles divergent bookmarks gracefully. The next `write_event` call from either agent will see both children and the `children()` revset will return multiple results. We can handle this by:

1. Using `--allow-backwards` on `bookmark set` (in case of rebase)
2. Accepting temporary forks — they resolve naturally as one agent's next write builds on the other's

Or: reuse the existing `acquire_absorb_lock()` pattern from `isolation.rs` to serialize task writes. This is the safest approach if task writes are infrequent enough (they are — humans type slowly).

### Step 3: Migrate existing heads (one-time cleanup)

After deploying the fix, run a one-time migration to collapse the 30K existing heads:

```bash
# Abandon all task event heads (their metadata is in descriptions,
# but we need to preserve it)
#
# BETTER: squash all children into a single summary change
jj log -r 'children(root()) & description(substring:"[aiki-task]")' \
    --no-graph -T 'change_id ++ "\n"' | head -5
```

**Migration strategy:**
1. Read all existing events via `read_events()` (already works — reads from descriptions)
2. Abandon all existing task-branch children: `jj abandon 'children(ancestors(aiki/tasks)) & description(substring:"[aiki-task]")'`
3. Replay events as a chain by calling the new `write_event()` for each
4. Or: just abandon them — the task graph is rebuilt from events on read, and we can export a snapshot first

**Simplest migration:** Just abandon the old heads. Task state is ephemeral (reconstructed from events each time). If we need history, export it first.

```bash
# Export current task state
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
| `cli/src/tasks/storage.rs` | `write_event()` and `write_events_batch()`: add bookmark advance after write |
| `cli/src/history/storage.rs` | Same pattern for conversation events |
| `cli/src/jj.rs` (maybe) | Add `advance_bookmark()` helper if shared |

---

## Risks

- **Concurrent writes**: Two agents writing simultaneously could create a temporary fork. Mitigated by lock or by accepting JJ's natural fork resolution.
- **Migration data loss**: Abandoning old heads loses the JJ change metadata. But task events are parsed from descriptions into the TaskGraph in memory — the graph reconstruction doesn't depend on JJ change structure, just on the description content being queryable.
- **Bookmark advancement failure**: If the `bookmark set` fails (e.g., JJ lock contention), the event is still written — it just creates a fan-out head. Next write will pick it up. Degraded but not broken.

---

## Success Criteria

- Head count drops from ~31K to ~1 per branch (aiki/tasks, aiki/conversations)
- `jj log` and `jj status` return to sub-second performance
- Task operations (`aiki task start`, `close`, etc.) remain correct
- Existing task state is preserved through migration

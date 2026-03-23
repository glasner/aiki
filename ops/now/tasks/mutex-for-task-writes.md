# Mutex for Task Branch Writes

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: Infrastructure — eliminate bookmark contention on `aiki/tasks`

---

## Problem

`write_event` and `write_events_batch` in `cli/src/tasks/storage.rs` write to the `aiki/tasks` branch via:

```
jj new aiki/tasks --no-edit -m <event>
jj bookmark set aiki/tasks -r <change_id>
```

With N concurrent agents, the bookmark set races. `advance_bookmark` (jj/mod.rs:142) retries 3 times (rebase + re-set), and `set_tasks_bookmark` silently swallows failures:

```rust
fn set_tasks_bookmark(cwd: &Path, change_id: &str) -> Result<()> {
    if let Err(err) = advance_bookmark(cwd, TASKS_BRANCH, change_id) {
        eprintln!("Warning: ...");  // swallowed
    }
    Ok(())  // always succeeds
}
```

### Consequences

1. **Orphaned events** — Commits exist in the DAG but aren't ancestors of the bookmark. The `children(ancestors(aiki/tasks))` revset usually finds them, but edge cases exist.
2. **DAG forks** — Concurrent `jj new aiki/tasks` creates sibling commits. `jj log --reversed` may order forked siblings non-chronologically, causing `materialize_graph` to apply events out of order (e.g., close before create → close silently dropped).
3. **Stale reads** — A write succeeds but the bookmark doesn't advance. Subsequent reads see a stale graph.
4. **`build --restart` failure** — The race caused the "Parent task has no subtasks" error that prompted this plan.

### What we already tried

- `advance_bookmark`: 3-retry loop with rebase — helps but doesn't eliminate the race
- `resolve_bookmark_conflict`: Detects conflicted bookmarks — reactive, not preventive
- `build --restart` direct `create_epic` bypass — defense-in-depth, doesn't fix root cause

---

## Fix: Serialize writes with fd-lock

Wrap the two-step write (jj new + bookmark set) in an `fd-lock` mutex. The lock is held only for the duration of the write (~10ms), so contention is minimal.

### Why fd-lock

- Already in the dependency tree (step 1a)
- `acquire_named_lock` in `session/isolation.rs` provides the exact API we need
- Kernel-level lock: released on process death (no stale locks)
- Blocks in kernel (no spin-wait, no polling)

---

## Implementation

### Step 1: Add `write_event_locked` to `storage.rs`

Replace the body of `write_event` with a locked version:

```rust
pub fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;

    // Serialize all task writes to prevent bookmark contention
    let _lock = acquire_task_write_lock(cwd)?;

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

    let change_id = parse_change_id_from_stderr(&result.stderr)?;

    // Bookmark set is now guaranteed uncontested — no retry needed
    let bm = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", &change_id, "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to set bookmark: {}", e)))?;

    if !bm.status.success() {
        let stderr = String::from_utf8_lossy(&bm.stderr);
        return Err(AikiError::JjCommandFailed(format!("Failed to advance bookmark: {}", stderr)));
    }

    Ok(())
    // _lock drops here → fd-lock released
}
```

Same treatment for `write_events_batch`.

### Step 2: Add `acquire_task_write_lock` helper

In `storage.rs`:

```rust
use crate::session::isolation::acquire_named_lock;

fn acquire_task_write_lock(cwd: &Path) -> Result<fd_lock::RwLockWriteGuard<'static, std::fs::File>> {
    // cwd may be a workspace — resolve to repo root for lock path
    let repo_root = crate::jj::get_repo_root(cwd)?;
    acquire_named_lock(&repo_root, "task-event-write")
}
```

Lock file lives at `/tmp/aiki/<repo-id>/.task-event-write.lock` — same directory as the workspace absorption lock.

### Step 3: Remove `advance_bookmark` and lock both branch writers

`advance_bookmark` is used by two callers — both have the same race condition:

| Caller | Branch | File |
|--------|--------|------|
| `set_tasks_bookmark` | `aiki/tasks` | `cli/src/tasks/storage.rs` |
| `set_conversations_bookmark` | `aiki/conversations` | `cli/src/history/storage.rs` |

Both use the same pattern: `advance_bookmark` with silent error swallowing.

**Action:**

1. **Delete `advance_bookmark`** from `jj/mod.rs` — the retry/rebase loop is no longer needed
2. **Lock `history/storage.rs` too** — add `acquire_named_lock(&repo_root, "conversation-event-write")` to `write_conversation_event`, same pattern as task writes
3. **Replace both `set_*_bookmark` functions** with a direct `jj bookmark set` that propagates errors (see step 4)
4. **Remove the `advance_bookmark` import** from both `tasks/storage.rs` and `history/storage.rs`

### Step 4: Make `set_tasks_bookmark` propagate errors

The current silent swallowing was a workaround for contention. Under the mutex, bookmark set should always succeed. Propagate failures:

```rust
fn set_tasks_bookmark(cwd: &Path, change_id: &str) -> Result<()> {
    let bm = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "set", TASKS_BRANCH, "-r", change_id, "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to set bookmark: {}", e)))?;

    if !bm.status.success() {
        let stderr = String::from_utf8_lossy(&bm.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to advance '{}': {}", TASKS_BRANCH, stderr.trim()
        )));
    }
    Ok(())
}
```

### Step 5: Verify `acquire_named_lock` works from workspaces

`acquire_named_lock` takes `repo_root: &Path` and derives the lock path from the repo ID. When called from a JJ workspace (e.g., `/tmp/aiki/<repo-id>/<session-id>`), we need to resolve back to the repo root. Verify that `jj workspace root` or equivalent gives the right path.

If not, `acquire_task_write_lock` should resolve the repo root from `cwd`:

```rust
fn acquire_task_write_lock(cwd: &Path) -> Result<fd_lock::RwLockWriteGuard<'static, std::fs::File>> {
    let repo_root = crate::jj::get_repo_root(cwd)?;
    acquire_named_lock(&repo_root, "task-event-write")
}
```

Existing `get_repo_root` (or `jj workspace root`) handles workspaces correctly.

---

## What does NOT change

- **`read_events`** — No lock needed for reads. JJ's revset queries are read-only.
- **`children(ancestors())` revset** — Still correct, now just redundant safety.
- **`build --restart` direct `create_epic`** — Keep as defense-in-depth.
- **`resolve_bookmark_conflict`** — Handles pre-existing conflicts from before the mutex.

---

## Testing

1. **Unit test**: Write two events concurrently from threads, verify both appear in `read_events` with correct order.
2. **Integration test**: Spawn 5 parallel `aiki task add` processes, verify all 5 tasks appear.
3. **Manual test**: Run `aiki build --restart` with background agents active — no more "no subtasks" error.

---

## Risk

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Lock blocks agent for too long | Low — writes take ~10ms | Lock scope is minimal (2 jj commands) |
| Deadlock with absorption lock | None — different lock names, no nesting | Lock names are distinct |
| Lock file stale on crash | None — fd-lock uses flock(2), kernel releases on exit | Kernel-guaranteed cleanup |
| `get_repo_root` fails in workspace | Low — jj handles this | Test from workspace paths |

---

## Files to modify

| File | Change |
|------|--------|
| `cli/src/tasks/storage.rs` | Add lock in `write_event` and `write_events_batch`; replace `set_tasks_bookmark` with direct bookmark set; remove `advance_bookmark` import |
| `cli/src/history/storage.rs` | Add lock in conversation write path; replace `set_conversations_bookmark` with direct bookmark set; remove `advance_bookmark` import |
| `cli/src/jj/mod.rs` | Delete `advance_bookmark` (42 lines); add `get_repo_root` if not present |

## Files to keep (defense-in-depth)

| File | What stays |
|------|-----------|
| `cli/src/commands/build.rs` | `None if restart => create_epic()` bypass |
| `cli/src/jj/mod.rs` | `resolve_bookmark_conflict` (handles pre-existing conflicts) |

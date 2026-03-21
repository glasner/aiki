# Step 1a: Replace Bespoke File Locking with `fd-lock`

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 1 — Stop the bleeding (prevent data loss now)
**Source**: Stale absorption locks investigation (2026-03-19) — Scenarios 1+3, Fix 1
**Depends on**: Nothing

---

## Problem

The current mutex in `isolation.rs:249-311` is a bespoke file-based lock built on `O_CREAT|O_EXCL` with manual PID tracking, a 100ms spin-wait loop, and a 30s timeout. It works but has known issues:

1. **Stale locks on process death** — If a process is killed (SIGKILL, laptop sleep, terminal close), the lock file remains. Recovery depends on PID-liveness checks that only trigger after 30s, causing all concurrent agents to stall. This was the primary cause of the "files not appearing" incident (2026-03-19).

2. **~65 lines of reimplemented OS primitive** — The `AbsorbLock` struct, `acquire_absorb_lock`, `read_lock_owner_pid`, and `is_pid_alive` reimplement what `flock(2)` does natively with automatic cleanup.

3. **Spin-wait overhead** — 100ms polling in a tight loop is wasteful compared to kernel-level blocking.

4. **Generalizing is more code** — The session-start race fix (step-1b/1c) needs a generalized `acquire_named_lock`. Building that on the current mechanism means duplicating the same fragile pattern.

### What this fixes from the original incident

| Issue | Status |
|---|---|
| **Fix 1: Stale lock detection** | **Superseded.** `flock(2)` locks are released by the kernel on process exit, even SIGKILL. Stale locks become impossible — no PID checking needed. |
| **Scenario 1: Session killed mid-absorption** | **Fixed.** The lock file can't go stale, so the 30s delay that caused absorptions to be skipped is eliminated. |
| **Scenario 3: Slow absorption timeout** | **Improved.** `flock` blocks in the kernel until available instead of timing out after 30s, so concurrent sessions wait rather than silently giving up. |

The remaining issues (Fix 2: session file TTL, Scenario 2: orphaned workspaces) are session lifecycle problems unrelated to locking, tracked in [step-4-session-lifecycle.md](step-4-session-lifecycle.md).

---

## Fix: Replace with `fd-lock`

Replace the bespoke locking with [`fd-lock`](https://crates.io/crates/fd-lock), a thin wrapper over `flock(2)` (Unix) / `LockFileEx` (Windows).

### Why `fd-lock`

- **No stale locks** — The kernel releases `flock` locks when the process exits, even on SIGKILL. The entire stale-lock bug class disappears.
- **No polling** — `flock` blocks in the kernel until the lock is available. No spin loop, no timeout tuning.
- **RAII guard** — `fd-lock` returns a guard that releases on drop, same pattern we have now but without the manual PID-matching cleanup.
- **Minimal** — ~200 lines, zero transitive dependencies. Just wraps the OS syscall.
- **Well-maintained** — ~7M downloads, maintained by Yoshua Wuyts (tokio contributor).

### Why not `fs2`

`fs2` (used by Cargo, ~50M downloads) also wraps `flock`, but it extends `File` with `.lock_exclusive()` / `.unlock()` — no RAII guard. We'd still need to write our own guard struct. `fd-lock` gives us `RwLock<File>` with a guard out of the box.

### New `acquire_named_lock` function

Delete:
- `struct AbsorbLock` and its `Drop` impl (lines 211–225)
- `fn is_pid_alive` (line 227–229)
- `fn read_lock_owner_pid` (lines 231–235)
- `fn acquire_absorb_lock` (lines 249–311)

Replace with a generalized named-lock function (supports step-1b/1c mutex primitive):

```rust
use fd_lock::RwLock;
use std::fs::File;

/// Acquire a named file lock for the given repo.
///
/// Uses OS-level `flock(2)` via `fd-lock`. The lock is automatically
/// released when the returned guard drops — even on panic or SIGKILL.
/// Blocks until the lock is available (no timeout, no polling).
pub fn acquire_named_lock(repo_root: &Path, name: &str) -> Result<fd_lock::RwLockWriteGuard<'static, File>> {
    let repo_id = repos::ensure_repo_id(repo_root)?;
    let lock_dir = workspaces_dir().join(&repo_id);
    let _ = fs::create_dir_all(&lock_dir);
    let lock_path = lock_dir.join(format!(".{}.lock", name));

    let file = File::create(&lock_path).map_err(|e| {
        AikiError::WorkspaceAbsorbFailed(format!("Failed to create lock file: {}", e))
    })?;

    // RwLock needs to be 'static for the guard to outlive this function.
    // Box::leak is fine — the file is tiny and the lock path is reused.
    let lock = Box::leak(Box::new(RwLock::new(file)));

    let guard = lock.write().map_err(|e| {
        AikiError::WorkspaceAbsorbFailed(format!("Failed to acquire {} lock: {}", name, e))
    })?;

    debug_log(|| format!("Acquired '{}' lock at {}", name, lock_path.display()));
    Ok(guard)
}
```

> **Note on `Box::leak`:** This is a pragmatic choice — `flock` locks are acquired infrequently (once per absorption or session start) and the leaked `RwLock` is just a file descriptor wrapper. An alternative is to return a wrapper struct that owns both the `RwLock` and the guard, but that requires self-referential types or `unsafe`. The leak approach is simpler and the "cost" is one fd per lock acquisition per process lifetime, which is negligible.

### Update `absorb_workspace` call site

**File:** `cli/src/session/isolation.rs` (line 405–406)

```rust
// Before:
let lock_path = absorb_lock_path(repo_root)?;
let _lock = acquire_absorb_lock(&lock_path)?;

// After:
let _lock = acquire_named_lock(repo_root, "workspace-absorption")?;
```

This renames the lock file from `.absorb.lock` to `.workspace-absorption.lock`, matching the named-lock convention used by step-1b/1c.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/Cargo.toml` | Add `fd-lock = "4.0"` |
| `cli/src/session/isolation.rs` | Delete `AbsorbLock`, `is_pid_alive`, `read_lock_owner_pid`, `acquire_absorb_lock`, `absorb_lock_path`. Add `acquire_named_lock`. Update call site. |
| `ops/future/task-write-locking.md` | Update function name reference |

---

## Implementation Steps

1. Add `fd-lock = "4.0"` to `cli/Cargo.toml`
2. Open `cli/src/session/isolation.rs`
3. Delete `AbsorbLock` struct + `Drop` impl, `is_pid_alive`, `read_lock_owner_pid`, `acquire_absorb_lock`, `absorb_lock_path`
4. Add `acquire_named_lock` function (see code above)
5. Update `absorb_workspace` call site to use `acquire_named_lock(repo_root, "workspace-absorption")`
6. Update doc reference in `ops/future/task-write-locking.md`
7. Run `cargo test` to verify no regressions
8. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`

---

## What This Does NOT Change

- **Lock semantics** — Still exclusive, still blocks until acquired, still RAII release. Same behavior, better mechanism.
- **Lock file location** — Still under `/tmp/aiki/{repo-id}/`. Renamed from `.absorb.lock` to `.workspace-absorption.lock`.
- **The `mutex` YAML primitive** — That's step-1b. This step provides the underlying lock function it will call.
- **The two-step absorption** — Still two rebases under one lock. This step only changes the lock mechanism, not the absorption algorithm.

## Risk

**Low.** `flock(2)` is the standard Unix file locking primitive. `fd-lock` is a thin wrapper with no logic of its own. The behavioral change is strictly positive: stale locks become impossible, and blocking is handled by the kernel instead of a spin loop.

One edge case: `flock` locks are per-file-descriptor, not per-file-path. Two opens of the same path get independent locks. This is fine for our usage — each caller opens the lock file, acquires the lock, does work, and drops. There's no scenario where the same process opens the same lock file twice.

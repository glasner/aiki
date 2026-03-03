# Isolation & Absorption Bug Fixes

**Date**: 2026-03-01
**Status**: Plan
**Source**: Code review of `cli/src/session/isolation.rs`, `cli/src/flows/core/functions.rs`, `cli/src/flows/core/hooks.yaml`

---

## Bug 1: `cleanup_orphaned_workspaces` runs against wrong repo root

**Severity**: High
**File**: `cli/src/flows/core/functions.rs:1295`

### Problem

After absorption, `workspace_absorb_all` calls:

```rust
if let Some(repo_root) = isolation::find_jj_root(&std::env::current_dir().unwrap_or_default()) {
    if let Err(e) = isolation::cleanup_orphaned_workspaces(&repo_root) {
```

The agent's cwd is the isolated workspace at `/tmp/aiki/<repo-id>/<session-uuid>/`. `find_jj_root` walks up from there and finds the workspace's own `.jj/`, returning the workspace path as the "repo root." `cleanup_orphaned_workspaces` then runs `jj workspace list` in the workspace directory — which lists workspace-local state, not the real repo's workspaces.

**Result**: Orphaned workspace cleanup is effectively a no-op for every isolated session. Stale JJ workspace entries accumulate indefinitely.

### Fix

The absorption loop already resolves the real repo root via `find_repo_root_from_workspace`. Pass that through instead of re-deriving from cwd.

```rust
// Collect repo roots encountered during absorption
let mut seen_repo_roots: Vec<PathBuf> = Vec::new();

// ... inside the absorption loop, after successful absorb:
if !seen_repo_roots.contains(&repo_root) {
    seen_repo_roots.push(repo_root.clone());
}

// After the loop, clean up orphans using the REAL repo roots
for repo_root in &seen_repo_roots {
    if let Err(e) = isolation::cleanup_orphaned_workspaces(repo_root) {
        debug_log(|| format!("[workspace] Orphaned workspace cleanup failed: {}", e));
    }
}
```

Remove the current `find_jj_root(cwd)` call at line 1295.

### Files

| File | Change |
|------|--------|
| `cli/src/flows/core/functions.rs` | Replace cwd-based repo root with loop-collected roots |

---

## Bug 2: `jj workspace update-stale` failure silently ignored

**Severity**: High
**File**: `cli/src/session/isolation.rs:442-445`

### Problem

```rust
let _ = jj_cmd()
    .current_dir(&target_dir)
    .args(["workspace", "update-stale"])
    .output();
```

If `workspace update-stale` fails (e.g., JJ internal error, disk full, corrupted operation log), the target's filesystem remains stale — it doesn't reflect the absorbed changes. The next JJ snapshot will see old file content and compute a diff that **reverts the absorbed work**.

This is the same class of bug that caused the "bad absorption" post-mortem (`ops/done/bad-absorbtion.md`): the commit lands correctly in JJ, but the working tree diverges from HEAD.

### Fix

Check the output status and propagate as an error. Absorption succeeded in the JJ graph, so we should still return `Absorbed`, but log a prominent warning and include it in the result so the caller knows the filesystem is stale.

```rust
let update_output = jj_cmd()
    .current_dir(&target_dir)
    .args(["workspace", "update-stale"])
    .output();

match update_output {
    Ok(o) if o.status.success() => {}
    Ok(o) => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        eprintln!(
            "[aiki] WARNING: workspace update-stale failed after absorption — \
             filesystem may be stale. Run `jj workspace update-stale` manually.\n\
             stderr: {}",
            stderr.trim()
        );
    }
    Err(e) => {
        eprintln!(
            "[aiki] WARNING: workspace update-stale failed to execute: {} — \
             filesystem may be stale.",
            e
        );
    }
}
```

### Files

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Check `update-stale` output, log warning on failure |

---

## Bug 3: Stale lock removal can break legitimate slow absorptions

**Severity**: Medium
**File**: `cli/src/session/isolation.rs:233-274`

### Problem

`acquire_absorb_lock` removes the lock file after 30 seconds assuming it's stale. But if a legitimate absorption is taking >30 seconds (large repo, slow disk, heavy I/O), the lock gets forcibly removed while the holder is still operating.

**Race scenario:**
1. Session A acquires lock, begins slow absorption
2. Session B waits 30s, removes A's lock as "stale"
3. Session B creates new lock, begins its own absorption
4. Session A finishes, drops RAII guard → `fs::remove_file` removes B's lock
5. Both sessions now operate unlocked; next absorber sees no lock

### Fix

Write the current PID and timestamp into the lock file. Before removing as stale, read the file, check if the PID is still alive. Only remove if the PID is dead.

```rust
fn acquire_absorb_lock(lock_path: &Path) -> Result<AbsorbLock> {
    let max_wait = Duration::from_secs(30);
    let poll_interval = Duration::from_millis(100);
    let start = std::time::Instant::now();
    let my_pid = std::process::id();

    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                // Write PID so other processes can check liveness
                use std::io::Write;
                let _ = write!(file, "{}", my_pid);
                return Ok(AbsorbLock {
                    path: lock_path.to_path_buf(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if start.elapsed() > max_wait {
                    // Check if lock holder is still alive
                    let holder_alive = fs::read_to_string(lock_path)
                        .ok()
                        .and_then(|s| s.trim().parse::<u32>().ok())
                        .map(|pid| {
                            // Check PID liveness
                            unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
                        })
                        .unwrap_or(false);

                    if holder_alive {
                        // Holder is alive but slow — extend the wait
                        debug_log(|| "Absorb lock holder still alive, continuing to wait");
                        // Reset the timer to give another 30s
                        // (or just keep polling — the holder will finish eventually)
                        std::thread::sleep(poll_interval);
                        continue;
                    }

                    // Holder is dead — safe to remove
                    debug_log(|| "Absorb lock holder dead, removing stale lock");
                    let _ = fs::remove_file(lock_path);
                    continue;
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return Err(AikiError::WorkspaceAbsorbFailed(format!(
                    "Failed to acquire absorb lock: {}", e
                )));
            }
        }
    }
}
```

Also update the `AbsorbLock` drop to only remove the file if it still contains our PID (prevents removing another session's lock):

```rust
impl Drop for AbsorbLock {
    fn drop(&mut self) {
        let my_pid = std::process::id().to_string();
        // Only remove if we still own the lock
        if let Ok(content) = fs::read_to_string(&self.path) {
            if content.trim() == my_pid {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
```

### Files

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Write PID to lock file, check liveness before stale removal, guard drop |

---

## Bug 4: Short change IDs from `jj workspace list` may be ambiguous

**Severity**: Medium
**File**: `cli/src/session/isolation.rs:707-743`

### Problem

`find_workspace_change_id` parses `jj workspace list` output and extracts the **short** change ID (first token after `workspace_name:`). Short change IDs (typically 8-12 chars) can collide in repos with many changes. When passed to `jj rebase -b <ws_head>`, JJ errors: "Revset resolved to more than one revision."

### Fix

Use a custom template to get the full change ID:

```rust
pub fn find_workspace_change_id(repo_root: &Path, workspace_name: &str) -> Result<Option<String>> {
    // Use jj log with the workspace's @ to get the full change ID
    // The @workspace_name revset resolves the workspace's working copy
    let at_workspace = format!("{}@", workspace_name);
    let output = jj_cmd()
        .current_dir(repo_root)
        .args([
            "log", "-r", &at_workspace, "--no-graph",
            "-T", "change_id", "--limit", "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::WorkspaceAbsorbFailed(format!("Failed to query workspace: {}", e))
        })?;

    if !output.status.success() {
        // Workspace might not exist — fall back to parsing workspace list
        return find_workspace_change_id_from_list(repo_root, workspace_name);
    }

    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if change_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(change_id))
}
```

Note: The `name@` revset syntax may not exist in all JJ versions. If it doesn't work, an alternative is to use `jj workspace list` with a custom template (if JJ supports it), or append the full change ID lookup as a second query when the short ID is found.

**Fallback approach** (safer, no new revset syntax):

Keep the current `jj workspace list` parsing but add a follow-up `jj log -r <short_id> -T change_id` to resolve to the full ID. If the revset returns multiple results, error clearly rather than letting the rebase fail cryptically.

### Files

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Get full change ID instead of short ID from workspace list |

---

## Bug 5: Snapshot-before-lock gap allows target drift

**Severity**: Medium
**File**: `cli/src/session/isolation.rs:326-379`

### Problem

`absorb_workspace` runs two snapshots before acquiring the lock:

```
1. jj status (workspace snapshot)      ← outside lock
2. jj status (target snapshot)         ← outside lock
3. acquire_absorb_lock()               ← lock acquired
4. jj rebase -b ws_head -d @-          ← step 1
5. jj rebase -s @ -d ws_head           ← step 2
```

Between step 2 and step 3, another session can absorb, moving `@-` forward. Step 4 then rebases onto a stale `@-`.

### Analysis

This is actually **not a correctness bug** in most cases because:
- The `-b` rebase moves the workspace chain. Even if `@-` moved, the rebase inserts the chain relative to whatever `@-` is at the time.
- Step 4 runs `jj rebase -b ws_head -d @-` from `target_dir`. JJ resolves `@-` fresh at execution time, using the lock-holding JJ operation. Since the lock is held, no other absorption can interleave between steps 4 and 5.

The real concern is the **target snapshot** (step 2). If `@` changed between the snapshot and the lock, the snapshot captured stale state. But `jj status` writes the snapshot into the operation log atomically. The subsequent rebase in step 4 operates on whatever the latest state is.

**Verdict**: This is mostly a theoretical concern. The JJ operation log handles concurrent operations correctly. However, the target snapshot *could* race with the user making changes in the target directory between snapshot and lock. The fix would be to move the target snapshot inside the lock.

### Fix (optional, low-priority)

Move the target `jj status` snapshot inside the lock:

```rust
let _lock = acquire_absorb_lock(&lock_path)?;

// Snapshot target INSIDE lock to ensure we capture the latest state
let _ = jj_cmd()
    .current_dir(&target_dir)
    .args(["status"])
    .output();

// Step 1 & 2 ...
```

The workspace snapshot (step 1) can stay outside the lock since it only captures the workspace's own files.

### Files

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Move target snapshot inside lock (optional) |

---

## Bug 6: `session.ended` absorption doesn't warn about conflicts

**Severity**: Low (by design, but improvable)
**File**: `cli/src/flows/core/hooks.yaml:318`

### Problem

```yaml
session.ended:
    - call: self.workspace_absorb_all
```

Uses `call:` (fire-and-forget) instead of `let:` + conflict check. If absorption introduces conflicts, there's no mechanism to notify the user. The session is ending, so `autoreply` won't reach anyone.

### Fix

Check the result and print a user-visible warning to stderr:

```yaml
session.ended:
    - let: absorb_result = self.workspace_absorb_all
    - if: absorb_result != "ok" and absorb_result != "0" and absorb_result
      then:
          - log: |
                WARNING: Workspace absorption at session end introduced conflicts.
                Conflicted files:
                {{absorb_result}}
                Run `jj resolve --list` to see conflicts and resolve them.
```

Alternatively, handle this in the Rust code — `workspace_absorb_all` could print to stderr when conflicts are detected during session.ended (detectable by checking if autoreply is available or by a flag).

### Files

| File | Change |
|------|--------|
| `cli/src/flows/core/hooks.yaml` | Change `call:` to `let:` + warn on conflicts |

---

## Implementation Order

| Priority | Bug | Effort | Impact |
|----------|-----|--------|--------|
| 1 | Bug 1: Wrong repo root for orphan cleanup | ~15 min | Fixes orphan accumulation |
| 2 | Bug 2: Silent `update-stale` failure | ~10 min | Prevents silent data reversion |
| 3 | Bug 3: Stale lock PID check | ~30 min | Prevents concurrent absorption corruption |
| 4 | Bug 4: Short change ID ambiguity | ~20 min | Prevents rebase failures in large repos |
| 5 | Bug 6: Session end conflict warning | ~10 min | UX improvement |
| 6 | Bug 5: Target snapshot inside lock | ~5 min | Marginal correctness improvement |

**Total estimated changes**: ~120 lines modified across 2 files + 1 YAML file.

## Files Changed Summary

| File | Bugs |
|------|------|
| `cli/src/session/isolation.rs` | #2, #3, #4 |
| `cli/src/flows/core/functions.rs` | #1 |
| `cli/src/flows/core/hooks.yaml` | #6 |

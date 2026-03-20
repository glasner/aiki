# Bug: Stale Absorption Locks Block Workspace Absorption

**Date**: 2026-03-19
**Status**: Draft
**Priority**: P0 (primary cause of "files not appearing in main workspace")
**Related**: [bug-absorption-concurrency.md](bug-absorption-concurrency.md) (secondary bug, already patched)

---

## Discovery

While investigating why file writes weren't appearing in the main workspace, we initially diagnosed a rebase topology bug (concurrent absorptions stranding commits on side branches). We patched that (post-absorption ancestry check in `isolation.rs`). But files STILL weren't appearing.

Running `AIKI_DEBUG=1 aiki session list` revealed the actual primary cause:

```
[aiki] Absorb lock timed out after 30s, removing stale lock
```

A stale `.absorb.lock` file from a crashed/killed session was blocking all subsequent absorptions. Sessions that couldn't acquire the lock within 30 seconds silently skipped absorption, leaving their workspace changes permanently stranded.

## How the Lock Works

**File:** `cli/src/session/isolation.rs:249` (`acquire_absorb_lock`)

```
Lock path: /tmp/aiki/{repo-id}/.absorb.lock
Mechanism: O_CREAT|O_EXCL (atomic file creation)
Content: PID of the lock holder
Timeout: 30 seconds
Poll interval: 100ms
```

After 30s timeout:
1. Read PID from lock file
2. Check if PID is alive (`kill(pid, 0)`)
3. If dead → remove stale lock, retry
4. If alive → `continue` (retry indefinitely, re-hitting the 30s check each iteration)
5. If PID unreadable → treat as dead, remove

## Why Absorptions Fail

### Scenario 1: Session killed mid-absorption (most common)

1. Session A starts absorption, acquires lock
2. Session A is killed (SIGKILL, terminal closed, laptop sleep, etc.) mid-rebase
3. Lock file remains on disk with dead PID
4. Session B starts, tries to absorb on turn.completed
5. Session B waits 30s, then detects stale lock, removes it, retries
6. BUT: Session B's turn.completed already timed out or the agent moved on — the 30s delay caused the absorption to run too late or be skipped entirely

### Scenario 2: session.ended never fires

1. Agent process is killed (not graceful shutdown)
2. `session.ended` hook never fires
3. `workspace_absorb_all` never runs for that session
4. Workspace is orphaned
5. `prune_dead_pid_sessions` should clean it up on next session start, but with 161 stale sessions, it's overwhelmed

### Scenario 3: Slow absorption on huge graph

With 3,685 empty commits in `@`'s ancestry, `jj rebase -b` operations are slow. A single absorption can take >30s, causing concurrent sessions waiting for the lock to time out and give up.

## Evidence from the Incident

```
# 161 stale session files (none cleaned up)
$ ls ~/.aiki/sessions/ | wc -l
161

# 12 orphaned workspaces (after cleaning 44 earlier in the same session)
$ jj workspace list | grep -c aiki
12

# Debug output shows stale lock detected
[aiki] Absorb lock timed out after 30s, removing stale lock
[aiki] Acquired absorb lock at /tmp/aiki/7f50e063/.absorb.lock

# Also: Rhai condition evaluation bug in hooks.yaml
[aiki] Warning: condition evaluation failed (defaulting to false):
  `absorb_result != "ok" and absorb_result != "0" and absorb_result`
  — Data type incorrect: i64 (expecting bool) (line 1, position 50)
```

## Proposed Fixes

### Fix 1: Aggressive stale lock detection (immediate relief)

Check PID liveness IMMEDIATELY on first lock failure, not after 30s:

```rust
fn acquire_absorb_lock(lock_path: &Path) -> Result<AbsorbLock> {
    let max_wait = Duration::from_secs(30);
    let poll_interval = Duration::from_millis(100);
    let start = std::time::Instant::now();
    let mut checked_stale = false;

    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => { /* ... existing success path ... */ }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Check for stale lock on FIRST failure, not after 30s
                if !checked_stale {
                    checked_stale = true;
                    let holder_alive = read_lock_owner_pid(lock_path)
                        .map(is_pid_alive)
                        .unwrap_or(false);

                    if !holder_alive {
                        debug_log(|| "Removing stale absorb lock (holder PID dead)");
                        let _ = fs::remove_file(lock_path);
                        continue;  // Retry immediately
                    }
                }

                if start.elapsed() > max_wait {
                    // Only reach here if holder was alive at first check
                    // Re-check PID in case it died during our wait
                    let holder_alive = read_lock_owner_pid(lock_path)
                        .map(is_pid_alive)
                        .unwrap_or(false);

                    if !holder_alive {
                        debug_log(|| "Removing stale absorb lock after timeout");
                        let _ = fs::remove_file(lock_path);
                        continue;
                    }
                    // Holder still alive — keep waiting (it's doing real work)
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => { /* ... existing error path ... */ }
        }
    }
}
```

**Impact:** Stale locks from dead PIDs are removed in <100ms instead of 30s. Live holders still get the full wait.

### Fix 2: Session file TTL cleanup

Add a maximum age check to session files. Any session file older than 24h with a dead PID should be cleaned up regardless of `prune_dead_pid_sessions` throughput:

```rust
fn cleanup_stale_sessions() {
    let sessions_dir = sessions_dir();
    for entry in fs::read_dir(&sessions_dir).into_iter().flatten().flatten() {
        if let Ok(metadata) = entry.metadata() {
            let age = metadata.modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .unwrap_or_default();

            if age > Duration::from_secs(86400) {
                // 24h old — remove regardless
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
```

### Fix 3: Fix the Rhai condition evaluation bug

The `hooks.yaml` condition:
```yaml
- if: absorb_result != "ok" and absorb_result != "0" and absorb_result
```

Fails with "Data type incorrect: i64 (expecting bool)" when `absorb_result` is an integer. The trailing `and absorb_result` is treated as a truthy check but Rhai doesn't auto-coerce i64 to bool. Fix:

```yaml
- if: absorb_result != "ok" and absorb_result != "0" and absorb_result != ""
```

This is in the conflict detection path — not critical for absorption itself, but produces noisy warnings.

## Priority Order

1. **Fix 1** — immediate stale lock detection (biggest impact, simplest change)
2. **Fix 2** — session file TTL cleanup (prevents accumulation)
3. **Fix 3** — Rhai condition fix (cosmetic, reduces log noise)

## Manual Cleanup Commands

Until fixes are deployed, clean up manually:

```bash
# Remove stale lock
rm -f /tmp/aiki/7f50e063/.absorb.lock

# Forget orphaned workspaces
jj --ignore-working-copy workspace list | grep '^aiki-' | \
  awk '{print $1}' | sed 's/:$//' | \
  while read ws; do jj --ignore-working-copy workspace forget "$ws"; done

# Clean workspace temp dirs
rm -rf /tmp/aiki/7f50e063/*/

# Clear stale session files
rm -rf ~/.aiki/sessions/*

# Recover a stranded file
jj log -r 'files("path/to/file.md")' --no-graph
jj squash --from <change_id> --into @
```

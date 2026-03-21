# Step 4: Session Lifecycle Fixes

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 4 — Operational hygiene
**Source**: Stale absorption locks investigation (2026-03-19) — Scenario 2, Fix 2
**Depends on**: Step 1a (acute fix first, then prevention)

---

## Problem

When agent processes die without graceful shutdown (SIGKILL, laptop sleep, terminal close), two cleanup problems occur that are **independent of the locking mechanism** (which is fixed by [step-1a-fd-lock-mutex.md](step-1a-fd-lock-mutex.md)):

1. **Orphaned workspaces** — `session.ended` never fires, so `workspace_absorb_all` never runs. The JJ workspace is left behind with uncommitted changes stranded on a side branch. `prune_dead_pid_sessions` should clean these up on next session start, but it doesn't scale — during the original incident, 161 stale session files accumulated and the pruner was overwhelmed.

2. **Stale session file accumulation** — Session files in `~/.aiki/sessions/` pile up indefinitely when the owning process dies. There's no TTL or age-based cleanup, so the directory grows unbounded.

### Evidence from the incident

```
$ ls ~/.aiki/sessions/ | wc -l
161
```

These files are from sessions whose PIDs are long dead, but `prune_dead_pid_sessions` runs only on session start and can't process them all fast enough.

---

## Fixes

### Fix 1: Session file TTL cleanup

**File:** `cli/src/session/mod.rs` (near `prune_dead_pid_sessions`)

Add a maximum age check to session files. Any session file older than 36h should be removed regardless of PID status. This provides a hard upper bound on accumulation.

```rust
fn cleanup_stale_sessions() {
    let sessions_dir = sessions_dir();
    for entry in fs::read_dir(&sessions_dir).into_iter().flatten().flatten() {
        if let Ok(metadata) = entry.metadata() {
            let age = metadata.modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .unwrap_or_default();

            if age > Duration::from_secs(36 * 3600) {
                // 36h old — remove regardless of PID status
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
```

Call this at session start, before `prune_dead_pid_sessions`. The TTL provides a backstop — even if the PID-based pruner is overwhelmed or skips entries, old sessions are always cleaned.

### Fix 2: Orphaned workspace cleanup

**File:** `cli/src/session/isolation.rs` (near workspace pruning)

When a stale session file is removed (either by TTL or PID-death pruning), also forget the associated JJ workspace if it still exists. Currently `prune_dead_pid_sessions` removes the session file but doesn't always clean up the workspace.

Ensure the cleanup path is:
1. Read session file to get workspace name
2. `jj workspace forget <name> --ignore-working-copy`
3. Remove workspace temp dir from `/tmp/aiki/{repo-id}/`
4. Remove session file

### Fix 3: Limit `prune_dead_pid_sessions` batch size

**File:** `cli/src/session/mod.rs`

During the incident, 161 stale sessions overwhelmed the pruner (which runs synchronously at session start). Add a batch limit — prune at most N sessions per startup (e.g., 20), and let subsequent session starts catch up. This prevents a single startup from blocking for minutes.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/session/mod.rs` | Add `cleanup_stale_sessions` TTL function, batch-limit pruner |
| `cli/src/session/isolation.rs` | Ensure workspace forget + temp dir cleanup on session removal |

---

## Implementation Steps

1. Open `cli/src/session/mod.rs` and find `prune_dead_pid_sessions`
2. Add `cleanup_stale_sessions()` function — iterate session files, check age > 36h, remove expired ones
3. Call it from session start, before `prune_dead_pid_sessions`
4. Update session removal paths (both TTL and PID-death) to also forget JJ workspaces and clean temp dirs
5. Add batch limit (e.g., 20) to `prune_dead_pid_sessions` loop
6. Run `cargo test` to verify no regressions
7. Test manually: create fake stale session files older than 36h, start a new session, verify cleanup

---

## What This Does NOT Change

- **Locking** — That's handled by [step-1a-fd-lock-mutex.md](step-1a-fd-lock-mutex.md).
- **The session.ended hook** — We can't prevent process death. These fixes ensure cleanup happens on the *next* session start even when `session.ended` never fires.
- **The absorption algorithm** — This step only addresses cleanup of orphaned resources.

## Risk

**Low.** TTL cleanup and batch limiting are defensive measures. The 36h threshold is conservative — no legitimate session runs that long. Workspace forget with `--ignore-working-copy` is safe for dead workspaces.

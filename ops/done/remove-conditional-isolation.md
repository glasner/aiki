# Remove Conditional Isolation — Always Isolate

**Date**: 2026-02-26
**Status**: Ready
**Source**: [workspace-isolation-improvements.md](workspace-isolation-improvements.md) issue #1

## Summary

Remove the "only isolate if concurrent sessions" logic. Every session gets an isolated
JJ workspace unconditionally. This eliminates 4 race conditions and the asymmetric
solo/concurrent design, removes ~140 lines of session-counting machinery, and makes all
sessions symmetric.

## Problem

The conditional `count_sessions_in_repo > 1` check has compounding race conditions:

1. **Unregister between turns** — `workspace_absorb_all` unregisters from by-repo at turn end.
   Between unregister and re-register at next turn start, other sessions see count=1 and
   skip isolation.

2. **Simultaneous startup** — Two sessions starting within milliseconds can both read count=1
   before the other's register is on disk.

3. **Solo→concurrent gap** — Session A starts solo (count=1). Session B starts mid-turn
   (count=2, gets workspace). A is still in repo root. B absorbs into main where A is
   actively editing.

4. **Asymmetric design** — Session 1 never gets isolated. It's the absorption target.
   The lock only protects the rebase, not Session 1's broader edit window.

5. **Mid-turn collision** — Even after isolation kicks in, the solo→concurrent transition
   leaves a gap within the current turn. If s1 is solo and s2 starts, s2 gets a workspace
   but s1 is still writing to repo root for the rest of its turn. When s2 absorbs at turn
   end, `jj debug snapshot` captures s1's in-flight writes, but the ordering can be
   surprising and conflict-prone. No mid-turn migration exists — isolation only happens at
   turn boundaries.

## Changes

### 1. Simplify `workspace_create_if_concurrent` → `workspace_ensure_isolated`

**File**: `cli/src/flows/core/functions.rs:1093-1179`

Remove:
- `register_session_in_repo` call (line 1128)
- `count_sessions_in_repo` call (line 1130)
- `workspace_path.exists()` check (lines 1132-1135)
- The conditional skip block (lines 1137-1150)

The function becomes:

```rust
pub fn workspace_ensure_isolated(
    session: &crate::session::AikiSession,
    cwd: &Path,
) -> Result<ActionResult> {
    use crate::session::isolation;

    let repo_root = match isolation::find_jj_root(cwd) {
        Some(root) => root,
        None => {
            debug_log(|| "[workspace] Not in a JJ repo, skipping workspace creation");
            return Ok(ActionResult::success());
        }
    };

    match isolation::create_isolated_workspace(&repo_root, session.uuid()) {
        Ok(ws) => {
            debug_log(|| format!("[workspace] Workspace '{}' ready at {}", ws.name, ws.path.display()));
            Ok(ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: ws.path.to_string_lossy().to_string(),
                stderr: String::new(),
            })
        }
        Err(e) => {
            eprintln!("[aiki] Warning: workspace creation failed, continuing in main workspace: {}", e);
            Ok(ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: String::new(),
                stderr: format!("fallback: {}", e),
            })
        }
    }
}
```

Note: `ensure_repo_id` call is also removed — `create_isolated_workspace` calls it internally.

### 2. Rename all references

**`workspace_create_if_concurrent` → `workspace_ensure_isolated`**

| File | Line(s) | Change |
|------|---------|--------|
| `cli/src/flows/core/functions.rs` | 1093 | Function definition |
| `cli/src/flows/core/mod.rs` | 19 | Re-export |
| `cli/src/flows/engine.rs` | 1996, 1998 | Match arm + call (first occurrence) |
| `cli/src/flows/engine.rs` | 2205, 2207 | Match arm + call (second occurrence) |
| `cli/src/flows/core/hooks.yaml` | 41, 63, 95, 124, 287 | `self.workspace_create_if_concurrent` → `self.workspace_ensure_isolated` |

### 3. Remove unregister calls from `workspace_absorb_all`

**File**: `cli/src/flows/core/functions.rs:1189-1315`

Remove these `unregister_session_from_repo` calls:
- Line 1272: in `Absorbed` branch
- Line 1291: in `Skipped` branch
- Line 1302: in `Err` branch
- Lines 1311-1312: unconditional unregister at end of function

These existed because the session was registered at turn start and unregistered at turn end.
With always-isolate, there's no registration, so no unregistration needed.

### 4. Update `cleanup_orphaned_workspaces` liveness check

**File**: `cli/src/session/isolation.rs:700-774`

Replace the by-repo sidecar check (line 728: `find_session_repo(uuid).is_some()`) with a
session-file-based liveness check:

```rust
// OLD: Check if session has a by-repo sidecar
if find_session_repo(uuid).is_some() {
    continue; // Session is still active, skip
}

// NEW: Check if session file exists at ~/.aiki/sessions/{uuid}
let session_file = crate::global::global_sessions_dir().join(uuid);
if session_file.exists() {
    continue; // Session is still active, skip
}
```

Session files are created at session.started and removed at session.ended. They contain
`parent_pid=...` for PID-based liveness — but `prune_dead_pid_sessions` already handles
that. The file-exists check is sufficient for orphan cleanup.

### 5. Update `detect_repo_transition`

**File**: `cli/src/events/change_completed.rs:401-468`

This function uses by-repo sidecars to detect when an agent writes to a different repo.
With always-isolate, each session always has a workspace. Repo transitions need a
different tracking mechanism.

**Option A (simple)**: Track the current repo ID in the session file itself. `AikiSessionFile`
already has `add_repo()` (line ~40 in session_started.rs). Read the repo from the session
file instead of scanning by-repo dirs.

**Option B (simplest)**: Track current repo in a field on the workspace. When
`workspace_ensure_isolated` is called with a different cwd, compare against the existing
workspace's repo root. If different, fire repo.changed.

**Recommended**: Option A — the session file already stores repo info. Replace:
```rust
// OLD
let previous_repo_id = find_session_repo(&session_uuid);
register_session_in_repo(&new_repo_id, &session_uuid);
if let Some(ref old_repo_id) = previous_repo_id {
    unregister_session_from_repo(old_repo_id, &session_uuid);
}

// NEW
let previous_repo_id = session_file.read_repo_id();
session_file.update_repo_id(&new_repo_id);
```

Also need to update the sidecar-exists early return (line 438-446) to check the session
file's stored repo ID instead.

### 6. Delete by-repo sidecar functions

**File**: `cli/src/session/isolation.rs`

Delete these functions and their tests:

| Function | Lines | Tests |
|----------|-------|-------|
| `by_repo_dir()` | 789-793 | (none) |
| `count_sessions_in_repo()` | 780-786 | `test_count_sessions_in_repo_empty` (965-971), `test_count_sessions_in_repo_with_sessions` (974-984) |
| `register_session_in_repo()` | 799-808 | `test_register_session_in_repo` (987-999) |
| `unregister_session_from_repo()` | 815-825 | `test_unregister_session_from_repo` (1002-1017) |
| `find_session_repo()` | 831-843 | `test_find_session_repo` (1020-1037) |

**Total**: ~65 lines of code + ~75 lines of tests = ~140 lines deleted.

### 7. Update docs

**File**: `cli/docs/session-isolation-workflow.md`

- Update Phase 2 diagram: remove "count == 1 → Skip" branch. All sessions create workspace.
- Update state machine: remove "SOLO MODE" state.
- Remove by-repo sidecar from architecture diagram.
- Update Key Files table.
- Update Design Decisions table: remove "by-repo sidecar" row, update "zero overhead for solo sessions" to note it was removed and why.

## Subtask Checklist

1. [ ] Rewrite `workspace_create_if_concurrent` → `workspace_ensure_isolated` (functions.rs)
2. [ ] Rename all references in engine.rs, mod.rs, hooks.yaml
3. [ ] Remove unregister calls from `workspace_absorb_all` (functions.rs)
4. [ ] Update `cleanup_orphaned_workspaces` to use session-file liveness (isolation.rs)
5. [ ] Update `detect_repo_transition` to use session file instead of by-repo sidecar (change_completed.rs)
6. [ ] Delete `by_repo_dir`, `count_sessions_in_repo`, `register/unregister_session_from_repo`, `find_session_repo` + tests (isolation.rs)
7. [ ] Update session-isolation-workflow.md docs
8. [ ] `cargo build` — verify no dead code warnings from removed functions
9. [ ] `cargo test` — verify tests pass (removed tests won't fail, but callers might)

## Impact

- **Removes**: ~140 lines (isolation.rs functions + tests), ~25 lines (functions.rs conditional + unregisters), 5 race conditions
- **Modifies**: ~30 lines across functions.rs, engine.rs, hooks.yaml (renames), ~20 lines in change_completed.rs
- **Risk**: Low — the isolated path is already the well-tested code path. We're removing the skip-isolation path.

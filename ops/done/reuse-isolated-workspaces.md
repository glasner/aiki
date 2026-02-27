# Reuse Isolated Workspaces Between Turns

**Date**: 2026-02-26
**Status**: Ready
**Source**: [workspace-isolation-improvements.md](workspace-isolation-improvements.md) issue #2
**Prerequisite**: [remove-conditional-isolation.md](remove-conditional-isolation.md) (issue #1)

## Summary

Stop destroying and recreating JJ workspaces on every turn. After absorption, keep the
workspace alive and rebase it to the new fork point on the next turn start. This replaces
19 unnecessary destroy/create cycles (in a 20-turn session) with a single rebase per turn.

## Problem

Every successful turn runs this sequence:

1. **turn.completed**: `workspace_absorb_all` absorbs changes, then `cleanup_workspace`:
   - `jj workspace forget` (modifies JJ repo metadata)
   - `rm -rf /tmp/aiki/{repo}/{session}/` (filesystem delete)

2. **turn.started** (next turn): `workspace_ensure_isolated` recreates from scratch:
   - `mkdir -p /tmp/aiki/{repo}/{session}/` (filesystem create)
   - `jj workspace add` (modifies JJ repo metadata + copies working tree)

Steps 1-2 of `cleanup_workspace` are purely wasted — the workspace will be immediately
recreated. For a session doing 20 turns, that's 19 unnecessary destroy/create cycles.
Each cycle touches JJ repo metadata twice and does a full filesystem delete + working tree
copy.

**Files**: `cli/src/flows/core/functions.rs:1271`, `cli/src/session/isolation.rs:58`

## Fix

Instead of destroying the workspace after absorption, keep it alive and **rebase it to the
new fork point** on the next turn start.

The workspace already exists (the idempotent check at `isolation.rs:77` would return early),
but its content is stale — it still points at the old fork point from before absorption.
Adding a rebase step refreshes it:

```rust
// In workspace_ensure_isolated, after detecting workspace already exists:
if workspace_path.exists() {
    // Rebase workspace to current @- to pick up other sessions' absorbed changes.
    // IMPORTANT: Do NOT use --ignore-working-copy here. JJ must update the
    // filesystem to reflect concurrent session advances. Without this, the next
    // snapshot would see the old content and create a diff that reverts other
    // sessions' absorbed changes.
    let _ = jj_cmd()
        .current_dir(&workspace_path)
        .args(["rebase", "-r", "@", "-d", &current_target_at_minus])
        .output();
    return Ok(workspace_path);
}
```

This replaces the full destroy/create cycle with a single `jj rebase`.

## Changes

### 1. Remove `cleanup_workspace` from the `Absorbed` branch

**File**: `cli/src/flows/core/functions.rs` (in `workspace_absorb_all`)

In the `Absorbed` match arm (~line 1271), remove the `cleanup_workspace` call. The workspace
stays alive after absorption.

Keep `cleanup_workspace` for:
- **`Skipped`** — workspace had no changes, safe to destroy (avoids stale empty workspaces)
- **`Err`** — workspace is in a broken state, destroy it to force a clean recreation

```rust
// BEFORE
AbsorbResult::Absorbed => {
    // ... log success ...
    cleanup_workspace(&repo_root, session.uuid())?;
}

// AFTER
AbsorbResult::Absorbed => {
    // ... log success ...
    // Workspace stays alive — will be rebased on next turn start
}
```

### 2. Add rebase-to-current in workspace reuse path

**File**: `cli/src/session/isolation.rs` (in `create_isolated_workspace`)

When the workspace already exists (idempotent check), rebase it to the current `@-` to pick
up any changes absorbed by other sessions since the last turn:

```rust
// In create_isolated_workspace, existing idempotent check at ~line 77:
if workspace_path.exists() {
    // Workspace survived from previous turn — rebase to current fork point
    // so it picks up other sessions' absorbed changes.
    //
    // IMPORTANT: Do NOT use --ignore-working-copy here. JJ must update the
    // workspace's filesystem to reflect changes absorbed by concurrent sessions.
    // Without the working copy update, the next JJ snapshot would see stale files
    // and create a diff that reverts other sessions' absorbed changes.
    let target = resolve_at_minus(&repo_root)?;
    let output = jj_cmd()
        .current_dir(&workspace_path)
        .args(["rebase", "-r", "@", "-d", &target])
        .output()?;

    if !output.status.success() {
        // Rebase failed — fall through to destroy + recreate
        debug_log(|| format!("[workspace] Rebase failed, recreating workspace"));
        cleanup_workspace(&repo_root, session_uuid)?;
    } else {
        debug_log(|| format!("[workspace] Rebased existing workspace to {}", &target[..12]));
        return Ok(WorkspaceInfo { name, path: workspace_path });
    }
}
```

The `resolve_at_minus` helper resolves the current `@-` (parent of the main workspace's `@`)
which is the target fork point after absorption.

### 3. Keep `cleanup_workspace` in `session.ended` handler

**File**: `cli/src/flows/core/functions.rs` or `hooks.yaml` (session.ended hook)

The final cleanup at session end still destroys the workspace. This is the only place where
the workspace should be fully removed — when the session is completely done.

### 4. Add `resolve_at_minus` helper

**File**: `cli/src/session/isolation.rs`

```rust
fn resolve_at_minus(repo_root: &Path) -> Result<String> {
    let output = jj_cmd()
        .current_dir(repo_root)
        .args(["log", "-r", "@-", "--no-graph", "-T", "change_id", "--ignore-working-copy"])
        .output()?;
    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if change_id.is_empty() {
        anyhow::bail!("Could not resolve @- in {}", repo_root.display());
    }
    Ok(change_id)
}
```

## Why rebase instead of destroy/create?

| Operation | JJ metadata writes | Filesystem ops | Working tree copy |
|-----------|--------------------|----------------|-------------------|
| Destroy + create | 2 (`forget` + `add`) | `rm -rf` + `mkdir` | Full copy |
| Rebase | 1 (`rebase`) | None | Incremental update |

The rebase is cheaper in every dimension. JJ's rebase updates only the changed files in the
working tree (incremental), while `workspace add` copies the entire tree.

## Edge cases

**Stale workspace after failed absorption**: If absorption fails (`Err` branch), the workspace
is destroyed as before. On next turn start, `create_isolated_workspace` creates a fresh one.
No change in behavior.

**Workspace drift**: If the workspace's `@` has drifted (e.g., due to manual JJ operations),
the rebase handles it — it moves `@` to the current fork point regardless of where it was.

**Rebase failure**: If the rebase itself fails (should be rare with JJ's conflict model), we
fall through to the destroy/recreate path. This is a safe fallback — identical to current
behavior.

**Working copy update on rebase (critical)**: The rebase MUST NOT use `--ignore-working-copy`.
After rebasing to a new `@-` that includes other sessions' absorbed changes, JJ must update the
workspace's filesystem to reflect those changes. Without the working copy update, the filesystem
retains stale content from the previous turn. On the next JJ snapshot, this stale content
would appear as a diff that reverts the concurrent session's changes — effectively undoing
their work when this session absorbs.

## Subtask Checklist

1. [ ] Remove `cleanup_workspace` from `Absorbed` branch in `workspace_absorb_all` (functions.rs)
2. [ ] Add rebase-to-current-`@-` in workspace reuse path (isolation.rs)
3. [ ] Add `resolve_at_minus` helper (isolation.rs)
4. [ ] Keep `cleanup_workspace` in `session.ended` and `Skipped`/`Err` branches
5. [ ] `cargo build` — verify no warnings
6. [ ] `cargo test` — verify tests pass
7. [ ] Manual test: run a multi-turn session, verify workspace is reused (not destroyed/recreated)

## Impact

- **Removes**: `cleanup_workspace` call from `Absorbed` branch (~5 lines)
- **Adds**: rebase path in `create_isolated_workspace` (~20 lines), `resolve_at_minus` helper (~10 lines)
- **Performance**: Eliminates N-1 destroy/create cycles per session (where N = number of turns)
- **Risk**: Low — fallback to destroy/create on rebase failure preserves current behavior

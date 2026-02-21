# Move Isolated Workspaces to /tmp

## Problem

Isolated workspaces live at `~/.aiki/workspaces/<repo-id>/<session-uuid>/`. This clutters the home directory with ephemeral data that only needs to survive for the duration of a session. Sessions are short-lived — the durability argument for `~/.aiki` doesn't justify the cost.

## Design

**New path:** `/tmp/aiki/<repo-id>/<session-uuid>/`

Short and flat. `/tmp/aiki/` is the workspace root — aiki won't use `/tmp` for anything else, so no need for a `workspaces/` subdirectory or user namespacing (single-user tool).

### What about crash recovery?

`recover_orphaned_workspaces` scans the workspaces directory for orphaned sessions and absorbs them back. This still works — it just scans `/tmp/aiki/` instead of `~/.aiki/workspaces/`.

On reboot, `/tmp` is wiped, but:
1. A reboot also kills all sessions — there's nothing to recover *for*
2. JJ still knows about the workspace (via `jj workspace list`), so `cleanup_orphaned_workspaces` will `jj workspace forget` the dangling entry on next startup
3. Sessions are short-lived — a session surviving a reboot is not a real scenario

## Implementation

### Step 1: Add `workspaces_dir()` helper

**File:** `cli/src/session/isolation.rs`

Extract the workspace base path into a single function so all call sites use the same root:

```rust
/// Base directory for isolated workspaces: /tmp/aiki/
pub fn workspaces_dir() -> PathBuf {
    PathBuf::from("/tmp/aiki")
}
```

No user namespacing needed — single-user tool. No `workspaces/` subdirectory — `/tmp/aiki/` is exclusively for workspaces.

### Step 2: Replace all `global_aiki_dir().join("workspaces")` calls

There are 6 call sites across 2 files:

**`cli/src/session/isolation.rs`** (4 sites):
- Line 59: `create_isolated_workspace` — workspace path construction
- Line 220: `absorb_workspace` — parent workspace path lookup
- Line 320: `recover_orphaned_workspaces` — scan for orphans
- Line 475: `cleanup_orphaned_workspaces` — cleanup directory lookup

**`cli/src/flows/core/functions.rs`** (2 sites):
- Line 1134: `workspace_create_if_concurrent` — workspace path construction
- Line 1196: `cleanup_session_workspace` — cleanup directory lookup

All become `workspaces_dir().join(...)`.

### Step 3: Update doc comments

Update the module-level doc comment and struct doc comments:
- `//! Workspace paths follow: ~/.aiki/workspaces/...` → `//! Workspace paths follow: /tmp/aiki/<repo-id>/<session-uuid>/`
- `IsolatedWorkspace::path` doc: `/// Workspace path: ~/.aiki/workspaces/...` → `/// Workspace path: /tmp/aiki/<repo-id>/<session-uuid>/`

### Step 4: Update CLAUDE.md workspace isolation docs

The CLAUDE.md references `~/.aiki/workspaces/` in the workspace isolation section. Update to `/tmp/aiki/`.

### Step 5: Handle `/tmp` cleanup on startup

Add a check to `cleanup_orphaned_workspaces`: if `/tmp/aiki/<repo-id>/` is empty after cleaning, remove it. This prevents empty directory accumulation in `/tmp`.

Already partially handled — `cleanup_workspace` does `fs::remove_dir_all` on the workspace dir. Just need to also clean up empty parent dirs.

## Files to Modify

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Add `workspaces_dir()`, replace 4 call sites, update docs |
| `cli/src/flows/core/functions.rs` | Replace 2 call sites |
| `CLAUDE.md` | Update workspace path references |

## Testing

1. Existing isolation tests use `AIKI_HOME` override which routes through `global_aiki_dir()` — these will need updating since `workspaces_dir()` no longer depends on `global_aiki_dir()`. Use a test-only env var or make `workspaces_dir()` accept an override.
2. Manual: run two concurrent sessions, verify workspaces appear in `/tmp/aiki/<repo-id>/`
3. Manual: end a session, verify workspace is cleaned up from `/tmp`
4. Manual: verify `jj workspace list` doesn't accumulate stale entries after reboot

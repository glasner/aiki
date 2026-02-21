# Fix: JJ Workspace Isolation

Findings from live testing of the workspace isolation implementation (`ops/now/jj-workspaces.md`).

**Test method**: Launched 2 concurrent `aiki task run --async` tasks and monitored `~/.aiki/workspaces/`, `jj workspace list`, and by-repo sidecars throughout the lifecycle.

## What Works

- `workspace_create_if_concurrent` fires correctly on `turn.started`
- Session registration in `~/.aiki/sessions/by-repo/<repo-id>/` sidecars works
- `create_isolated_workspace` creates JJ workspace entries and workspace directories
- Idempotent workspace creation (no-op if already exists) works

## Issues

### 1. Subagent CWD is main repo — isolated workspace never used

**Severity**: Critical — isolation is completely ineffective.

The spawned agent's `cwd` is set to `std::env::current_dir()` (the main repo root) via `AgentSpawnOptions::new(cwd, task_id)` in `runner.rs:125`. The claude/codex subprocess runs `.current_dir(&options.cwd)` which is the main repo. All `jj` commands target the `default` workspace.

The plan says "hooks work automatically (they resolve workspace from cwd)" but this is wrong — the agent's CWD never changes to the workspace path. The isolated workspace is created but sits empty while both sessions race on `default`.

**Files**: `cli/src/tasks/runner.rs:125`, `cli/src/agents/runtime/mod.rs:182-186`, `cli/src/agents/runtime/claude_code.rs:40`

**Fix — context-based CWD redirect**: Use the `context:` hook action to tell the agent to `cd` into the workspace directory. This applies to both main agents and subagents — both get the same `turn.started` hook.

1. **`workspace_create_if_concurrent` returns workspace path** (not just "created"/"skipped"):
   - Return the absolute workspace path as stdout when a workspace is created or already exists
   - Return empty string when skipped (solo session)

2. **hooks.yaml injects context conditionally**:
   ```yaml
   turn.started:
     - let: ws_path = self.workspace_create_if_concurrent
     - if: ws_path
       then:
         - context: |
             WORKSPACE ISOLATION: An isolated JJ workspace has been created for your session.
             You MUST `cd {{ws_path}}` before any file operations and work from that directory.
             All file reads, writes, and edits must use paths relative to {{ws_path}}.
             This ensures your changes don't conflict with other concurrent sessions.
   ```

3. **Why this chains correctly for hooks**: Once the agent cd's to the workspace and writes files there, `change.completed` events will have file paths inside the workspace. The engine's `find_jj_root()` resolves the workspace's `.jj/` dir, and `jj:` commands in hooks naturally run from the workspace directory. No engine changes needed for the hooks' own JJ commands.

4. **Why this works for both main and subagents**: The `turn.started` hook fires for every session, regardless of how it was started (interactive, `aiki task run`, IDE). The context message is injected into the agent's prompt/system context. Both Claude Code and Codex respect context messages and can `cd`.

**Note**: This is NOT a subagent-only problem. The current session (main agent in an interactive terminal) also gets an isolated workspace that it never uses. The context approach fixes both cases uniformly.

### 2. `workspace_id()` revset doesn't exist in JJ 0.38

**Severity**: Critical — absorb always fails.

`absorb_workspace()` at `cli/src/session/isolation.rs:143-153` uses:
```
jj log -r 'workspace_id("aiki-<uuid>").parents()' -T change_id --no-graph -l 1
```

The `workspace_id()` revset function does not exist in JJ 0.38.0:
```
Error: Failed to parse revset: Function `workspace_id` doesn't exist
```

This means absorb always fails with a parse error. Changes can never be merged back into the main workspace.

**Fix**: Use an alternative approach to find the workspace head. Options:
- Parse `jj workspace list` output to get the change ID for a named workspace
- Use `jj log --at-workspace <name>` if available
- Track the workspace's change ID when creating it and store it

### 3. Workspace forks from `root()` instead of `main@-`

**Severity**: High — workspace is disconnected from repo history.

`create_isolated_workspace` runs:
```
jj workspace add <path> --name <name> -r @- --ignore-working-copy
```

But the resulting workspace has `root()` (zzzzzzzz/00000000) as its parent instead of the main workspace's parent. Observed: workspace `wkvpolnq` with parent `root()`, while `default@` was at `nmpmzzrw` with a full change chain.

Likely cause: `--ignore-working-copy` changes how `@` resolves, or `@` resolves to a different workspace's working copy in a multi-workspace context. With 5+ existing aiki workspaces, `@` may be ambiguous.

**Fix**: Use an explicit change ID instead of `@-`. Resolve the default workspace's parent before creating the new workspace:
```
WS_PARENT=$(jj log -r '@-' -T change_id --no-graph --limit 1)
jj workspace add <path> --name <name> -r $WS_PARENT
```

### 4. By-repo sidecar leak for solo sessions

**Severity**: Medium — causes cascading workspace creation.

Sessions that register in `by-repo/` via `register_session_in_repo()` but skip workspace creation (because `count == 1`, solo session) never get their sidecar cleaned up. `workspace_absorb_all` only calls `unregister_session_from_repo` inside the workspace-dir iteration loop — if no workspace dir exists, unregistration is skipped.

**Observed**: 5 orphaned sidecars in `~/.aiki/sessions/by-repo/7f50e06.../` from dead sessions. This inflates `count_sessions_in_repo()` so it always returns >1, forcing ALL new sessions to create workspaces (even when they're the only live session).

**Files**: `cli/src/flows/core/functions.rs:1278-1283` — unregister only happens inside workspace iteration loop.

**Fix**: `workspace_absorb_all` (or `session.ended`) must always unregister the session from by-repo, regardless of whether a workspace dir exists. Add an unconditional `unregister_session_from_repo` call after the workspace iteration loop. Also add cleanup of stale sidecars in `prune_dead_pid_sessions`.

### 5. Orphaned JJ workspaces never forgotten

**Severity**: Medium — JJ workspace list grows unbounded.

4 JJ workspace entries exist for sessions that no longer have session files or workspace directories:
- `aiki-2d56cd44-...`
- `aiki-90f289a7-...`
- `aiki-d49add27-...`
- `aiki-ff2a42b9-...`

These were either:
- Created before cleanup was fully implemented
- Had their workspace dirs removed but `jj workspace forget` was never called

**Fix**: `cleanup_workspace` should be more robust. `prune_dead_pid_sessions` should scan `jj workspace list` for `aiki-*` entries that don't correspond to live sessions and forget them.

### 6. Missing cleanup path for sessions without workspaces

**Severity**: Low — related to #4.

`workspace_absorb_all` iterates `~/.aiki/workspaces/*/<session-uuid>/` directories. Sessions that registered in by-repo but never created a workspace dir are invisible to this scan. The by-repo unregistration at `functions.rs:1281-1283` only runs inside this loop, so these sessions are never cleaned up.

**Fix**: Same as #4 — add unconditional by-repo cleanup outside the workspace iteration loop.

## Immediate Cleanup Needed

Before fixing the code, clean up the current stale state:

```bash
# Clean orphaned JJ workspaces
jj workspace forget aiki-2d56cd44-6f83-5851-bb9c-ec1c88483ce0
jj workspace forget aiki-90f289a7-dad3-5915-a691-4426303c3779
jj workspace forget aiki-d49add27-75bb-57e5-9f4a-315edf6011e9
jj workspace forget aiki-ff2a42b9-b0be-504c-a634-10e1161ac5be

# Clean orphaned by-repo sidecars
rm ~/.aiki/sessions/by-repo/7f50e06340e462ecc2f0b28447d532bfe719267c/2d56cd44-6f83-5851-bb9c-ec1c88483ce0
rm ~/.aiki/sessions/by-repo/7f50e06340e462ecc2f0b28447d532bfe719267c/90f289a7-dad3-5915-a691-4426303c3779
rm ~/.aiki/sessions/by-repo/7f50e06340e462ecc2f0b28447d532bfe719267c/d49add27-75bb-57e5-9f4a-315edf6011e9
rm ~/.aiki/sessions/by-repo/7f50e06340e462ecc2f0b28447d532bfe719267c/e73c2199-4fa8-52c2-b3ef-a5433c66ab59
rm ~/.aiki/sessions/by-repo/7f50e06340e462ecc2f0b28447d532bfe719267c/ff2a42b9-b0be-504c-a634-10e1161ac5be
```

## Fix Approach

The core fix is **context-based CWD redirect** — use the existing `context:` hook action to tell agents to `cd` into their isolated workspace. This is not a subagent-only problem; main agents are affected too. The `turn.started` hook fires for all sessions uniformly.

### Chain of events (after fix)

```
turn.started
  └─► workspace_create_if_concurrent → returns workspace path (or empty if skipped)
  └─► if ws_path: context: "cd to {{ws_path}} for all file operations"

agent reads context, cd's to workspace path

agent writes file at <workspace_path>/cli/src/foo.rs
  │
  ▼
change.completed fires with file_path = <workspace_path>/cli/src/foo.rs
  └─► engine: find_jj_root(file_path) → resolves workspace .jj/ dir ✅
  └─► jj metaedit (runs from workspace dir, targets workspace) ✅
  └─► jj new (runs from workspace dir) ✅

turn.completed
  └─► workspace_absorb_all → merges workspace changes into main
```

### Implementation changes

| File | Change |
|------|--------|
| `cli/src/flows/core/functions.rs` | `workspace_create_if_concurrent` returns workspace path (not "created"/"skipped") |
| `cli/src/flows/core/hooks.yaml` | Add conditional `context:` after workspace creation in `turn.started` |
| `cli/src/session/isolation.rs` | Fix `absorb_workspace` to not use `workspace_id()` revset; fix `-r @-` resolution |
| `cli/src/flows/core/functions.rs` | `workspace_absorb_all` unconditionally unregisters by-repo sidecar |

## Priority Order

1. **#1 + #3 — Context redirect + fix fork point** (make isolation actually work end-to-end)
2. **#2 — Fix `workspace_id()` revset** (absorb is completely broken)
3. **#4 + #6 — Fix by-repo sidecar leak** (causes cascading false positives)
4. **#5 — Fix orphaned JJ workspace cleanup** (unbounded growth)

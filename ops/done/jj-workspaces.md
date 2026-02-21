# Plan: JJ Workspace Isolation for Agent Sessions

## Context

When multiple agent sessions run concurrently in the same repo (e.g., one initiated by `aiki task run`, another by a user directly in their IDE), both share the same JJ workspace. The `change.completed` hook runs `jj metaedit` and `jj new` after every file write. Two processes running these concurrently race on the working copy, causing provenance metadata to be silently lost. `aiki task diff` shows no changes for tasks worked on by the affected agent.

**Goal**: Each agent session gets its own JJ workspace, automatically — regardless of how the session was initiated. Hooks work automatically (they resolve workspace from cwd). Changes are absorbed back into the main workspace when the session ends. Sessions that don't overlap with another session have zero overhead — workspace creation is skipped entirely when no other session is active.

## Approach

Use JJ's built-in workspace feature (`jj workspace add`) to give each agent session an isolated working copy. JJ workspaces share the same repo and operation log, so task events and provenance queries work across workspaces. When the session ends, rebase the main workspace on top of the session's changes and clean up.

**Key design decisions:**

1. **Workspace lifecycle via core hooks** (`session.started` / `session.ended`) — applies to all agent sessions universally, no per-runner changes needed.

2. **Lazy workspace creation per repo** — workspaces are created on demand, keyed by `(session-uuid, repo-root)`. A session that touches N repos gets N workspaces. The engine detects repo transitions and creates workspaces before any JJ writes occur. If only one session is active at `session.started` time, workspace creation is skipped entirely (the session runs in the main workspace). If a second session starts later, it creates a workspace for itself — the first session continues in main, unaffected.

3. **Filesystem as registry** — workspace paths follow `~/.aiki/workspaces/<repo-id>/<session-uuid>/`. No workspace fields stored in session files. Crash recovery scans this directory tree.

4. **`repo.changed` as a first-class engine event** — the engine fires `repo.changed` before dispatching `change.completed` when it detects the changed file belongs to a different repo than the session's current one. `change.completed` assumes the workspace is already correct.

## Event Flow

```
session.started
  └─► workspace.create_if_concurrent(initial_repo_root)
        session_file.add_repo(repo_id)  // write repo= to current session file first
        count_sessions_in_repo(repo_id) — count session files containing repo=<repo_id>
        if count == 1: skip (only us in this repo — zero overhead)
        if count > 1:
          creates ~/.aiki/workspaces/<repo-id>/<session-uuid>/
          sets session.current_repo_root = initial_repo_root
          jj workspace add ... -r @-

agent writes file(s)
  │
  ▼
engine receives change event (file_path)
  ├─ find_jj_root(file_path)   // walk up dirs looking for .jj/, microseconds
  │
  ├─ [if new_root != session.current_repo_root]
  │    fire repo.changed(prev_root, new_root)
  │      └─► workspace.create_if_concurrent(new_root)
  │            session_file.add_repo(new_repo_id)
  │            count_sessions_in_repo(new_repo_id)
  │            if count == 1 AND no workspace exists yet: skip
  │            else: creates ~/.aiki/workspaces/<repo2-id>/<session-uuid>/
  │    update session.current_repo_root = new_root
  │
  └─ fire change.completed(file_path)
       ├─ jj metaedit  (workspace already correct, no workspace logic here)
       └─ jj new

session.ended
  └─► for each workspace in ~/.aiki/workspaces/*/<session-uuid>/
        workspace.absorb(repo_root, workspace)
          if session.parent_session_uuid exists AND parent workspace exists:
            jj rebase -b @ -d <ws_head>  (run from parent workspace dir)
          else:
            jj rebase -b @ -d <ws_head>  (run from repo_root → targets main)
        workspace.cleanup(repo_root, workspace)
          jj workspace forget <name>
          rm -rf <workspace_path>

[crash recovery — fires at next session.started or aiki task start]
  prune_dead_pid_sessions() detects dead PID for session S:
  └─► scan ~/.aiki/workspaces/*/<S-uuid>/
        for each orphaned workspace:
          absorb_workspace(repo_root, workspace)   // normal absorb path
          cleanup_workspace(repo_root, workspace)
        unblock in-progress tasks claimed by session S
        emit info: "Recovered N changes from crashed session for task <id>"

  [fallback — if workspace commits have no recognizable task=<id>]
        jj bookmark create aiki/recovered/<name> -r <ws_head>
        warn: "Orphaned workspace <name> had untagged changes at aiki/recovered/<name>"
```

## Implementation

### 1. New module: `cli/src/session/isolation.rs`

Add `pub mod isolation;` to `cli/src/session/mod.rs`.

```rust
pub struct IsolatedWorkspace {
    pub name: String,         // "aiki-<session-uuid>"
    pub path: PathBuf,        // ~/.aiki/workspaces/<repo-id>/<session-uuid>/
    pub repo_root: PathBuf,   // project root this workspace belongs to
    pub session_uuid: String,
}

/// Create an isolated JJ workspace for a repo/session pair.
/// Idempotent: no-op (returns existing) if workspace directory already exists.
///
/// - workspace_name: "aiki-<session-uuid>"
/// - workspace_path: ~/.aiki/workspaces/<repo-name>/<session-uuid>/
/// - Forks from repo's main workspace @- (parent of working copy, starts clean)
pub fn create_isolated_workspace(
    repo_root: &Path,
    session_uuid: &str,
) -> Result<IsolatedWorkspace>

/// Absorb workspace changes into the target workspace (parent session's workspace,
/// or main if no parent session or parent workspace no longer exists).
///
/// 1. Resolve workspace head:
///    jj log -r 'workspace_id("aiki-<uuid>").parents()' -T change_id --no-graph -l 1
/// 2. If empty (no changes), return early.
/// 3. Resolve absorb target:
///    - If parent_session_uuid is set AND ~/.aiki/workspaces/<repo-id>/<parent-uuid>/ exists:
///        run jj rebase -b @ -d <ws_head> from the parent workspace directory
///    - Otherwise:
///        run jj rebase -b @ -d <ws_head> from repo_root (targets main workspace)
pub fn absorb_workspace(
    repo_root: &Path,
    workspace: &IsolatedWorkspace,
    parent_session_uuid: Option<&str>,
) -> Result<()>

/// Forget workspace in JJ and delete its directory.
pub fn cleanup_workspace(
    repo_root: &Path,
    workspace: &IsolatedWorkspace,
) -> Result<()>

/// Find and recover all workspaces for a dead session across all repos.
/// Scans ~/.aiki/workspaces/*/<session-uuid>/ (where * is repo-id).
/// For each: absorb into main (normal absorb path), then cleanup.
/// Fallback: if commits have no task=<id> tag, bookmark-and-warn instead of absorb.
/// Called from prune_dead_pid_sessions() for each dead session UUID.
pub fn recover_orphaned_workspaces(session_uuid: &str)

/// Walk up from path looking for .jj/ directory. Returns repo root or None.
/// Delegates to JJWorkspace::find() from cli/src/jj/workspace.rs — do not reimplement.
pub fn find_jj_root(path: &Path) -> Option<PathBuf>
// impl: JJWorkspace::find(path).ok().map(|ws| ws.workspace_root().to_path_buf())
```

**`create_isolated_workspace` details:**
1. Compute workspace path: `global_aiki_dir() / "workspaces" / repo_id / session_uuid`
   - `repo_id` is read from `<repo_root>/.aiki/repo-id` (the same file used by `RepoRef::id`)
2. If path already exists: reconstruct and return `IsolatedWorkspace` (idempotent)
3. Create parent directories
4. Run from `repo_root`: `jj workspace add <path> --name aiki-<session-uuid> -r @-`
5. Return `IsolatedWorkspace`

**`absorb_workspace` details (run from `repo_root`):**
1. `jj log -r 'workspace_id("aiki-<uuid>").parents()' -T change_id --no-graph -l 1` → `$WS_HEAD`
2. If `$WS_HEAD` is the same as `root()` or is an empty/root change: return early (no commits made in workspace)
3. `jj rebase -b @ -d $WS_HEAD`
   - `-b @` rebases the full branch containing main's working copy (everything in `(WS_HEAD..@)::`)
   - `-d $WS_HEAD` makes the session's last non-empty commit the new parent
   - This is what `jj rebase` defaults to (`-b @`) but we spell it out explicitly
4. If conflicts: JJ marks them inline, user resolves later

### 2. New event: `repo.changed`

New file `cli/src/events/repo_changed.rs`:

```rust
/// A reference to a repo, used in repo.changed payload
pub struct RepoRef {
    /// Last path component of the repo root (e.g., "aiki")
    pub root: String,
    /// Full absolute path to the repo root
    pub path: PathBuf,
    /// Internal identifier from <repo>/.aiki/repo-id
    pub id: String,
}

/// repo.changed event payload
///
/// Fires when the engine detects the session has moved to a different JJ repo,
/// based on the repo root of the file being changed. Always fires before
/// change.completed for the triggering file.
pub struct AikiRepoChangedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// The repo the session moved into
    pub repo: RepoRef,
    /// The repo the session was previously in (None if no prior repo)
    pub previous_repo: Option<RepoRef>,
}
```

Add `repo_changed.rs` to `cli/src/events/mod.rs` and `prelude.rs`.

### 3. Engine: repo transition detection

In the change event dispatch path (before firing `change.completed`):

```rust
fn dispatch_change(engine: &mut Engine, file_path: &Path) {
    if let Some(new_root) = find_jj_root(file_path) {
        if new_root != engine.session.current_repo_root {
            let previous_repo = engine.session.current_repo_root
                .as_ref()
                .map(|r| RepoRef::from_path(r));
            let repo = RepoRef::from_path(&new_root); // reads .aiki/repo-id
            engine.fire(Event::RepoChanged(AikiRepoChangedPayload {
                session: engine.session.clone(),
                cwd: engine.session.cwd.clone(),
                timestamp: Utc::now(),
                repo,
                previous_repo,
            }));
            engine.session.current_repo_root = Some(new_root);
        }
    }
    engine.fire(Event::ChangeCompleted { file_path });
}
```

`find_jj_root` is pure Rust (no subprocess), so this is microseconds in the common case (no repo change).

### 4. New core hook actions

Implemented in `cli/src/flows/core/functions.rs`:

```
workspace.create_if_concurrent
  1. session_file.add_repo(repo_id) — record this repo in the current session file
     (idempotent; ensures the current session is counted in step 2).
  2. count_sessions_in_repo(repo_id) — scan global sessions dir, count files
     that contain repo=<repo_id>. O(n) file scan, typically single-digit ms.
     count == 1 means "only us in this repo"; count > 1 means concurrent in same repo.
  3. If count == 1 AND no workspace exists yet for this session: skip (zero overhead).
     Two sessions in different repos don't trigger isolation of each other.
  4. If count > 1 OR a workspace already exists (session moved to a new repo
     where it's already isolated): create workspace.
  Creation is idempotent — no-op if workspace directory already exists.
  Updates session.current_repo_root.

workspace.absorb_all
  Iterates ~/.aiki/workspaces/*/<session-uuid>/, absorbs and cleans up each.
  No-op if no workspaces exist (solo session that never needed isolation).
  Absorbs into parent session's workspace if parent_session_uuid is set and
  parent workspace exists; otherwise absorbs into main.
  Called from session.ended.
```

### 5. Modify `cli/src/flows/core/hooks.yaml`

```yaml
session.started:
  - call: self.workspace_create_if_concurrent   # skipped if no concurrent sessions
  - jj: new
  # ... rest of existing session.started hooks unchanged ...

repo.changed:
  - call: self.workspace_create_if_concurrent   # creates workspace if concurrent (or already isolated)
  - jj: new                                # start fresh change in new workspace

session.ended:
  - call: self.workspace_absorb_all   # no-op if no workspaces were created
  # ... any existing session.ended content ...
```

### 6. No session file changes

Workspace state is fully derivable:
- `workspace_name` = `aiki-<session-uuid>` (deterministic)
- `workspace_path` = `~/.aiki/workspaces/<repo-id>/<session-uuid>/` (deterministic)
- All workspaces for a session = `glob ~/.aiki/workspaces/*/<session-uuid>/` (where `*` matches repo-id)

No `workspace_name=`, `workspace_path=`, or `main_cwd=` fields in session files.

**Crash recovery**: `prune_dead_pid_sessions()` calls `recover_orphaned_workspaces(dead_session_uuid)` which scans `~/.aiki/workspaces/*/<session-uuid>/` (where `*` is repo-id) directly — no session file fields needed.

### 7. Modify `cli/src/tasks/runner.rs`

One change only: when spawning a subagent, pass the current session's UUID as `AIKI_PARENT_SESSION_UUID` env var (or equivalent session init field). The spawned agent's `session.started` reads this and records it in session state, so `workspace.absorb_all` can target the parent's workspace at session end.

No other runner changes — workspace lifecycle is fully handled by the hook/event system.

### 8. Add error variants to `cli/src/error.rs`

```rust
#[error("Failed to create isolated workspace: {0}")]
WorkspaceCreationFailed(String),

#[error("Failed to absorb workspace changes: {0}")]
WorkspaceAbsorbFailed(String),
```

### 9. No changes to `change.completed` hook

`change.completed` runs `jj metaedit` / `jj new` via `jj_cmd().current_dir(state.cwd())`. By the time it fires, the engine has already ensured the workspace exists (via `repo.changed` or `session.started`). No workspace logic in this hook.

**Note on `--ignore-working-copy`:** JJ snapshots the working copy at the start of every command. In `change.completed`, which fires after every file write, this means:
- `jj metaedit` — modifies commit metadata only; a fresh snapshot is not needed. Using `--ignore-working-copy` here is safe and avoids re-hashing the working tree.
- `jj new` — must snapshot the current working copy to commit the just-written files before starting the next empty change. `--ignore-working-copy` would be incorrect here.

Adding `--ignore-working-copy` to `jj metaedit` in `change.completed` is a worthwhile optimization (avoids an O(files) hash scan on every file write), but is out of scope for this plan. Tracked separately.

### 10. Task diff (`aiki task diff`) — no changes needed

`build_task_revset_pattern()` searches `description(substring:"task=<id>")` across the entire repo. Since workspaces share the same repo, changes made in any workspace are findable. The `~ ::aiki/tasks` exclusion still works correctly.

## File Change Summary

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | **NEW** — workspace lifecycle (create, absorb, cleanup, orphan recovery, find_jj_root) |
| `cli/src/session/mod.rs` | Add `pub mod isolation;` |
| `cli/src/events/repo_changed.rs` | **NEW** — `RepoRef`, `AikiRepoChangedPayload`, `handle_repo_changed` |
| `cli/src/events/mod.rs` | Register `repo_changed` module and `EventType::RepoChanged` |
| `cli/src/flows/engine.rs` | Add repo transition detection in change dispatch path, fire `repo.changed` |
| `cli/src/flows/core/functions.rs` | Add `workspace.create` and `workspace.absorb_all` built-in actions |
| `cli/src/flows/core/hooks.yaml` | Add `workspace.create` to `session.started`, new `repo.changed` hook, `workspace.absorb_all` to `session.ended` |
| `cli/src/tasks/status_monitor.rs` (or wherever prune lives) | Call `recover_orphaned_workspaces` for dead sessions, unblock claimed tasks |
| `cli/src/tasks/runner.rs` | Pass `AIKI_PARENT_SESSION_UUID` env var when spawning subagents |
| `cli/src/error.rs` | Add workspace error variants |

**Not changed:**
- `cli/src/agents/runtime/mod.rs`
- `cli/src/session/mod.rs` — no new fields
- `cli/src/jj/workspace.rs` — reused as-is; `find_jj_root` in `isolation.rs` delegates to `JJWorkspace::find()`
- Change hooks (`change.permission_asked`, `change.completed`)

## Merge-Back Algorithm (detailed)

```
Before session.started:
  main @ ─── (empty working copy)
  main @- ── (last provenance-tracked change)

After workspace.create (jj workspace add -r @-):
  main @- ── (shared parent)
     ├── main @ (empty, user's terminal stays here)
     └── ws @ (empty, agent session works here)

After agent works in workspace:
  main @- ── (shared parent)
     ├── main @ (empty, unchanged)
     └── ws_c1 → ws_c2 → ... → ws_cn → ws_@ (empty)
         (each ws_c has [aiki] provenance with task=<id>)

After workspace.absorb (jj rebase -b @ -d ws_cn):
  main @- → ws_c1 → ws_c2 → ... → ws_cn → main_@ (rebased)
  (main workspace now descends from all session changes)

After cleanup (jj workspace forget):
  main @- → ws_c1 → ws_c2 → ... → ws_cn → main_@
  (workspace forgotten, directory deleted, changes remain in repo)
```

## Edge Cases

1. **Agent crashes mid-work**: `prune_dead_pid_sessions()` detects the dead PID, scans `~/.aiki/workspaces/*/<dead-session-uuid>/`, and absorbs each orphaned workspace into main via the normal absorb path. In-progress tasks claimed by the dead session are unblocked automatically. The next `aiki task start <task-id>` picks up where the crashed session left off — the recovered changes are already in the main workspace. No manual steps required.

   **Fallback**: if an orphaned workspace's commits have no `task=<id>` tag (shouldn't happen under normal operation), a bookmark `aiki/recovered/<name>` is created and the user is warned. This is purely defensive.

2. **Subagent sessions**: When `aiki task run` spawns a subagent, the runner passes `AIKI_PARENT_SESSION_UUID` to the child. The child's `workspace.absorb_all` at `session.ended` targets the parent's workspace instead of main — so the parent agent sees the subagent's changes in its own workspace as soon as the subagent finishes. Chains of nested subagents work the same way recursively. If a subagent outlives its parent (possible with `--async` + crash), the parent workspace directory is gone and absorb falls back to main.

3. **Cross-repo session**: Each repo the agent writes to gets its own workspace, created lazily on first write via `repo.changed`. `session.ended` absorbs all of them. The filesystem glob `~/.aiki/workspaces/*/<session-uuid>/` (where `*` is repo-id) finds all of them regardless of how many repos were touched.

4. **Single agent, no concurrency**: `workspace.create_if_concurrent` checks `count_sessions_in_repo(repo_id)` — an O(n) scan over the global sessions dir filtering on `repo=<repo_id>`. If count == 1 (only this session in this repo), no workspace is created and `session.ended` is a no-op. Zero JJ overhead for solo sessions. Two sessions working in different repos are fully independent and don't isolate each other.

5. **Main workspace moved forward**: If the user made changes while an agent session was running, `jj rebase` may produce conflicts. JJ handles this gracefully — conflict markers in working copy, user resolves later.

6. **Multiple concurrent sessions**: Each gets its own workspace per repo. `workspace.absorb` runs as each session ends. Subsequent absorbs rebase on top of the prior chain. JJ's operation log is **lock-free** (not serialized via a mutex): two simultaneous `jj rebase -b @ -d <ws_head>` calls won't corrupt the repo, but the second one may produce a divergent operation that JJ surfaces as a warning on next `jj log`. In practice this is harmless — the rebased working-copy commits are still correct — but the plan should account for it: `absorb_workspace` should detect a divergent-operation warning in stderr and emit an `info`-level log so the user is aware. No retry mechanism is needed; JJ's divergence is self-healing via `jj op log` and `jj rebase` will succeed either way.

7. **Workspace creation fails**: Fall back to running in the main workspace (current behavior). Log a warning. Never a blocker.

8. **Agent writes via absolute path to another repo without cd-ing**: `find_jj_root(file_path)` derives the repo root from the file path directly, so `repo.changed` fires correctly even without a shell `cd`. The `.jj` walk is the ground truth.

## Verification

1. **Unit tests** in `cli/src/session/isolation.rs`:
   - `test_create_isolated_workspace` — creates workspace, verifies dir exists and JJ recognizes it
   - `test_create_idempotent` — calling create twice returns same workspace, no error
   - `test_absorb_workspace` — creates workspace, makes a change, absorbs, verifies main descends from it
   - `test_absorb_empty_workspace` — no changes made, absorb is a no-op
   - `test_cleanup_workspace` — creates and cleans up, verifies dir gone and workspace forgotten
   - `test_recover_orphaned_workspaces` — creates workspace with dead session uuid, verifies it gets absorbed into main and cleaned up
   - `test_find_jj_root` — given a nested path, returns correct repo root

2. **Integration test**: Fire `session.started`, write a file in repo A, write a file in repo B (triggers `repo.changed`), fire `session.ended`, verify `aiki task diff` finds changes in both repos' main workspaces.

3. **Manual test**: Start any agent session, verify:
   - `jj workspace list` shows `aiki-<uuid>` workspace
   - Changes have `[aiki]` provenance with `task=<id>`
   - After session ends, changes visible in main workspace
   - Workspace cleaned up (`jj workspace list` normal, directory gone)
   - Works identically for `aiki task run`, IDE session, CLI session

## Update: Turn-Level Workspace Lifecycle

**Problem with session-level lifecycle:** When workspaces are created on `session.started` and absorbed on `session.ended`, the user's main workspace doesn't see the agent's changes until the entire session ends. For interactive sessions (agent waits for user input between turns), this means the user can't inspect changes in their main repo between turns — they're invisible until the session fully terminates.

**Fix:** Move workspace creation to `turn.started` and absorption to `turn.completed`. This way, after each agent turn, changes are absorbed back into the main workspace. The user can always see the latest state in their repo between turns.

### Changes to hooks.yaml

```yaml
session.started:
  # REMOVED: workspace_create_if_concurrent (moved to turn.started)
  - jj: new
  # ... rest unchanged ...

turn.started:
  - call: self.workspace_create_if_concurrent   # create/reuse workspace at start of each turn
  # ... existing task context injection ...

turn.completed:
  - call: self.workspace_absorb_all   # absorb changes back into main after each turn

repo.changed:
  - call: self.workspace_create_if_concurrent   # unchanged — still needed mid-turn
  - jj: new

session.ended:
  - call: self.workspace_absorb_all   # KEPT as safety net for crash/abnormal termination
```

### Why this works

1. **`turn.started`** creates the workspace (idempotent — no-op if already exists from a prior turn that didn't absorb, or creates fresh after a prior turn absorbed). The agent then works in isolation for the duration of the turn.

2. **`turn.completed`** absorbs the workspace changes back into main and cleans up. The user immediately sees all changes from that turn in their main workspace.

3. **`session.ended`** still absorbs as a safety net — if the session crashes or terminates without a clean `turn.completed`, orphaned workspaces are recovered here (or by the existing crash recovery path).

4. **`repo.changed`** still creates workspaces mid-turn when the agent crosses repo boundaries — unchanged from the original design.

### Tradeoff

This adds a small amount of overhead per turn (workspace create + absorb + cleanup) vs. once per session. But for concurrent sessions, correctness and visibility are more important than the milliseconds saved. For solo sessions, `workspace_create_if_concurrent` skips entirely (count == 1), so both create and absorb are no-ops — zero overhead is preserved.

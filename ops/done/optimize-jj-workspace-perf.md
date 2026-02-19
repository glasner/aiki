# JJ Workspace Isolation: Performance Optimizations

Source: [ops/now/jj-workspaces.md](jj-workspaces.md)

## Summary

The workspace isolation implementation is sound. The "zero overhead for solo sessions" design goal is met — `workspace_create_if_concurrent` short-circuits when `count_sessions_in_repo == 1`. The opportunities below target the **concurrent case** (where isolation is active) and the **per-change-event hot path** that runs regardless.

---

## 1. `find_jj_root` calls `canonicalize()` on every invocation — expensive syscall on hot path

**Where:** `JJWorkspace::find()` in `cli/src/jj/workspace.rs:28`

```rust
let mut current = path.canonicalize().context("Failed to resolve path")?;
```

`canonicalize()` resolves all symlinks via `realpath(3)`, which does multiple `stat()` + `readlink()` syscalls per path component. This runs on every `change.completed` event via `detect_repo_transition` → `find_jj_root`.

**Impact:** High. Every file write triggers this. On macOS with APFS, `canonicalize` on a typical 5-component path costs ~50-100µs. For a session with 200 file writes, that's 10-20ms of pure syscall overhead.

**Fix:** Remove the `canonicalize()` call in `JJWorkspace::find()`. The `.jj` directory check uses `is_dir()`, which does not require a canonicalized path — walking up with raw path components works correctly for all non-symlink cases. Symlink-to-repo-root is an edge case not worth the syscall cost.

```rust
pub fn find(path: &Path) -> Result<Self> {
    let mut current = path.to_path_buf();  // was: path.canonicalize()?
    loop {
        let jj_dir = current.join(".jj");
        if jj_dir.is_dir() {
            return Ok(Self::new(current));
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => anyhow::bail!("Not in a JJ workspace (no .jj directory found)"),
        }
    }
}
```

**Why not a cache:** Each hook is a separate `aiki` process invocation — in-process statics (`SESSION_REPO_ROOTS`, thread-locals) are reset on every call. A cache in process memory provides zero benefit across turns. The existing `SESSION_REPO_ROOTS` static in `change_completed.rs` was intended for this purpose but is ineffective for the same reason (see also: broken repo-transition detection below).

**Estimated savings:** Eliminates all `realpath(3)` syscalls on every `find_jj_root` call. Drops from O(path components × stat+readlink) to O(path components × is_dir).

---

## 2. `count_sessions_in_repo` reads + parses every session file on every `turn.started`

**Where:** `cli/src/session/isolation.rs:385-412`

```rust
for entry in entries.flatten() {
    if let Ok(content) = fs::read_to_string(&path) {
        if content.lines().any(|line| line.trim() == repo_line) {
            count += 1;
        }
    }
}
```

This reads every file in `~/.aiki/sessions/`, parsing their full content to check for `repo=<id>`. Called on every `turn.started` via `workspace_create_if_concurrent`.

**Impact:** Medium. Typically single-digit session files, but the full `read_to_string` + line iteration is wasteful. More importantly, this is I/O on the hot path at the start of every turn.

**Fix:** Place each session's sidecar in a per-repo directory: `~/.aiki/sessions/by-repo/<repo-id>/<uuid>`. Then `count_sessions_in_repo` is just `read_dir().count()` — no file content parsing, just a directory listing.

This is also the sidecar used by `detect_repo_transition` (bug #7 fix). When a session transitions to a new repo, the old sidecar is removed from the old repo's directory and a new one is created in the new repo's directory — keeping the counts accurate.

```
~/.aiki/sessions/by-repo/<repo-id>/<uuid>   # presence = active session in this repo
```

Cleanup: session end removes `~/.aiki/sessions/by-repo/<repo-id>/<uuid>`.

**Estimated savings:** O(1) `read_dir().count()` vs O(n × file-size) scan.

---

## 3. `absorb_workspace` makes 3 sequential JJ subprocess calls

**Where:** `cli/src/session/isolation.rs:136-243`

The absorb path runs:
1. `jj log -r 'workspace_id(...)...' ...` — query workspace head (~30ms)
2. `jj log -r '...ws_head...' ...` — verify changes exist (~30ms)
3. `jj rebase -b @ -d <ws_head>` — actual rebase (~50ms)

Each JJ subprocess call has ~15-30ms of startup overhead (process fork + JJ init + repo load).

**Impact:** Medium-high on turn-level lifecycle. This runs at every `turn.completed`. Three subprocess calls = ~90-110ms minimum per turn end.

**Fix:** Combine the two `jj log` queries into a single call using JJ's template language:

```rust
// Single call: get workspace head AND check if it differs from fork point
let revset = format!(
    "workspace_id(\"{}\").parents() ~ roots(workspace_id(\"{}\").parents())",
    workspace.name, workspace.name
);
jj_cmd().args(["log", "-r", &revset, "-T", "change_id", "--no-graph", "--ignore-working-copy"])
```

If output is empty → no changes, skip rebase. If non-empty → the first line is the workspace head. This eliminates one subprocess call.

Even better: use `--ignore-working-copy` on the rebase too (the workspace is being torn down — there's no need to snapshot its working copy). Currently the rebase call doesn't use it.

**Estimated savings:** ~30-40ms per absorb (one fewer subprocess + `--ignore-working-copy` on rebase).

---

## 4. `jj metaedit` in `change.completed` snapshots working copy unnecessarily

**Where:** `cli/src/flows/core/hooks.yaml:110` and the plan's section 9

```yaml
- with_author_and_message: metadata
  jj: metaedit --message "{{message}}"
```

`jj metaedit` only changes commit metadata (description/author). But JJ auto-snapshots the working copy at the start of every command. For `metaedit`, this snapshot is pure waste — it re-hashes the entire working tree (O(files)) just to update a description string.

**Impact:** High. This is the most expensive operation on the critical path. A repo with 1,000 tracked files costs ~50-200ms per snapshot. This runs on _every single file write_.

**Fix:** Add `--ignore-working-copy` to the `metaedit` call in the hook:

```yaml
- with_author_and_message: metadata
  jj: metaedit --message "{{message}}" --ignore-working-copy
```

This is explicitly called out as safe in the plan (section 9): "jj metaedit — modifies commit metadata only; a fresh snapshot is not needed."

The subsequent `jj new` must NOT use `--ignore-working-copy` because it needs to snapshot the just-written files.

**Estimated savings:** 50-200ms per file write depending on repo size. This is likely the single highest-value optimization in this list.

---

## 5. `workspace_create_if_concurrent` re-derives `repo_root` and `repo_id` redundantly

**Where:** `cli/src/flows/core/functions.rs:1092-1147`

On every `turn.started`, this function:
1. Calls `find_jj_root(cwd)` — directory walk + canonicalize
2. Calls `read_repo_id(&repo_root)` — file read of `.aiki/repo-id`
3. Calls `count_sessions_in_repo(&repo_id)` — scans all session files
4. Checks `workspace_path.exists()` — stat call

For solo sessions (the common case), all of this happens just to return "skipped". The `cwd` and repo root almost never change between turns.

**Impact:** Medium. ~1-2ms per turn start for the common solo case. Not catastrophic, but it's pure overhead every turn.

**Fix:** An in-process static cache does not work — each `turn.started` hook is a separate process. With the `by-repo/<repo-id>/<uuid>` sidecar from #2/#7, `count_sessions_in_repo` becomes a `read_dir().count()` and the full session-file scan is eliminated. The remaining overhead is `find_jj_root` (fixed by #1), `read_repo_id` (one small file read), and the `read_dir` count. That's already the fast path.

**Estimated savings:** Subsumed by #2 (medium-term) and #1. No additional work needed once those are implemented.

---

## 6. `jj workspace add` and `jj workspace forget` don't use `--ignore-working-copy`

**Where:** `cli/src/session/isolation.rs:94-108` (create) and `cli/src/session/isolation.rs:252-262` (cleanup)

Both `jj workspace add` and `jj workspace forget` trigger a working copy snapshot of the repo they operate on. During workspace creation, the main workspace's working copy gets snapshotted unnecessarily. During forget, the workspace being cleaned up gets snapshotted.

**Impact:** Low-medium. These happen once per turn (at most), not per file write. But the snapshot cost is still O(tracked files).

**Fix:** Add `--ignore-working-copy` to both calls:

```rust
// create
.args(["workspace", "add", &path, "--name", &name, "-r", "@-", "--ignore-working-copy"])

// forget
.args(["workspace", "forget", &workspace.name, "--ignore-working-copy"])
```

Safe because: workspace add doesn't need the current working copy state (it forks from `@-`), and workspace forget is tearing down the workspace.

**Estimated savings:** 50-200ms per workspace create/forget, depending on repo size.

---

## 7. `SESSION_REPO_ROOTS` cache in `detect_repo_transition` is always empty — repo-transition detection is broken

**Where:** `cli/src/events/change_completed.rs:13` and `detect_repo_transition` function

```rust
static SESSION_REPO_ROOTS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);
```

`detect_repo_transition` uses this static to track the last known repo root per session, comparing it against the new root to detect cross-repo transitions. The intent: if the session wrote a file in repo A on turn N and writes a file in repo B on turn N+1, fire `repo.changed`.

**The bug:** Each `change.completed` hook is a separate `aiki` process invocation. The static is always `None` at startup, so `previous_root` is always `None`. The transition check is:

```rust
let previous_root = match previous_root {
    Some(ref prev) if prev != &new_root => prev.clone(),
    // None falls through here — no transition fired
    _ => return,
};
```

`previous_root` is always `None`, so the function always returns early. **`repo.changed` never fires from `detect_repo_transition`** regardless of which repo the file was written to.

**Fix:** Use the `by-repo/<repo-id>/<uuid>` sidecar (introduced in #2) to track the current repo per session. On each `detect_repo_transition` call:

1. Derive `new_repo_id` from `new_root` via `read_repo_id`
2. Check whether a sidecar already exists for this session under a *different* repo directory — if so, it's a transition
3. Move (or delete + create) the sidecar from the old repo dir to the new repo dir
4. Call `add_repo` on the session file for provenance

```rust
fn detect_repo_transition(payload: &AikiChangeCompletedPayload) {
    // ... find new_root as before ...
    let new_repo_id = match read_repo_id(&new_root) { ... };

    let sessions_by_repo = global_aiki_dir().join("sessions").join("by-repo");
    let new_sidecar = sessions_by_repo.join(&new_repo_id).join(payload.session.uuid());

    if new_sidecar.exists() {
        return; // Already in this repo, no transition
    }

    // Find previous sidecar (scan by-repo dirs for this uuid — typically 1-2 dirs)
    let previous_root = find_previous_repo_root(&sessions_by_repo, payload.session.uuid());

    // Move sidecar to new repo dir (creates dir if needed)
    fs::create_dir_all(new_sidecar.parent().unwrap())?;
    fs::write(&new_sidecar, "")?;
    // Remove old sidecar if found
    if let Some((old_repo_id, _)) = &previous_root {
        let _ = fs::remove_file(sessions_by_repo.join(old_repo_id).join(payload.session.uuid()));
    }

    // Record new repo in session file for provenance (idempotent)
    let _ = payload.session.file().add_repo(&new_repo_id);

    if let Some((_, prev_root)) = previous_root {
        // fire repo.changed with RepoRef::from_path(&prev_root)
    }
}
```

The `find_previous_repo_root` scan is cheap — `by-repo/` typically has 1-2 repo dirs, and we're looking for a file named after the session UUID. This runs only on actual transitions (which are rare), not on every invocation.

For the common case (no transition), `new_sidecar.exists()` is a single stat — O(1), no scan.

`add_repo` is called on transition so the session file accumulates all repos touched — important for provenance.

**Impact:** This is a correctness bug, not just a performance issue. Repo-transition events are silently dropped. For multi-repo workflows, workspace isolation will not trigger correctly.

**Note:** Consolidating into the `by-repo/<repo-id>/<uuid>` structure unifies the sidecar for #2, #5, and #7 into a single design.

---

## 8. `RepoRef::from_path` does a synchronous file read on every `repo.changed` event

**Where:** `cli/src/events/repo_changed.rs:21-22`

```rust
let id = std::fs::read_to_string(repo_root.join(".aiki/repo-id"))
    .unwrap_or_else(|_| format!("local-{}", root))
    .trim()
    .to_string();
```

This reads `.aiki/repo-id` every time a `RepoRef` is constructed, including for `previous_repo` (which we already knew about). Note: `repo.changed` is currently never fired due to bug #7, so this is moot until that's fixed.

**Impact:** Low. `repo.changed` is rare (only on cross-repo transitions). But it's an easy fix.

**Fix:** Accept pre-resolved repo_id as a parameter (passed in from the caller that already has it), avoiding the extra `read_to_string` entirely. Implemented as part of Phase 2.

---

## Implementation Phases

### Phase 1: One-liner JJ flag fixes

Ship these together — each is a one-line change to a JJ command invocation, no architectural impact.

1. **#4** — Add `--ignore-working-copy` to `jj metaedit` in `hooks.yaml` (**50-200ms** saved per file write)
2. **#6** — Add `--ignore-working-copy` to `jj workspace add` and `jj workspace forget` in `isolation.rs` (**50-200ms** saved per workspace create/forget)
3. **#3 (partial)** — Add `--ignore-working-copy` to the `jj rebase` call in `absorb_workspace` (**~15-30ms** saved per turn end)
4. **#1** — Remove `path.canonicalize()` from `JJWorkspace::find()` in `workspace.rs` (**~50-100µs** saved per file write)

### Phase 2: `by-repo` sidecar (bug fix + O(1) session counting)

A single implementation that fixes three issues: the broken repo-transition detection bug (#7), the expensive session file scan (#2), and the redundant per-turn work (#5).

1. On `session.started`: create `~/.aiki/sessions/by-repo/<repo-id>/<uuid>` (empty marker file)
2. In `detect_repo_transition`: check `new_sidecar.exists()` (O(1) stat) — return early if present; on transition, move sidecar to new repo dir and call `add_repo` on session file
3. In `count_sessions_in_repo`: replace file-content scan with `read_dir().count()` on `by-repo/<repo-id>/`
4. On session end: remove `by-repo/<repo-id>/<uuid>`
5. **#8** — Update `RepoRef::from_path` to accept pre-resolved repo_id, avoiding a redundant `.aiki/repo-id` read on `repo.changed`

### Phase 3: Merge absorb JJ log queries

1. **#3 (remaining)** — Combine the two `jj log` calls in `absorb_workspace` into one, eliminating a subprocess invocation (**~30ms** saved per turn end)

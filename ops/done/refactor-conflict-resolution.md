# Replace Force-Absorb + Retry Counter with JJ-Native Conflict Handling

**Date**: 2026-02-26 (updated 2026-02-27)
**Status**: Ready
**Source**: [workspace-isolation-improvements.md](workspace-isolation-improvements.md) issues #3 and #4

## Summary

Remove ~290 lines of hand-rolled conflict detection (pre-rebase check, retry counter,
force-absorb) and replace with ~50 lines that lean on JJ's first-class conflict model.
Absorption always succeeds; conflicts are detected afterward using the `conflicts() & @`
revset in turn.completed. This also eliminates issue #3 (target drift) — there is no
pre-rebase check left to go stale.

## Background: JJ's first-class conflict model

JJ stores conflicts as **tree algebra** in the commit, not as text markers in files. A
conflicted three-way merge is stored as `A+(C-B)` — an ordered list of tree objects. The
markers you see in files are just a **materialization** that JJ round-trips: it writes markers
to the working copy, and parses them back on snapshot.

Key capabilities this gives us:
- `jj log -r 'conflicts() & ::@'` — find all conflicted ancestors of `@` (revset)
- `jj resolve --list -r <revision>` — list conflicted files in any revision
- `jj log -r @ -T conflict` — boolean: is `@` itself conflicted?
- **Rebases always succeed** in JJ, even with conflicts — the conflict is recorded in the rebased commit
- **Resolution propagates** — fixing a conflict in a parent auto-resolves it in descendants

Sources: [JJ Conflicts docs](https://docs.jj-vcs.dev/latest/conflicts/), [JJ Technical: Conflicts](https://docs.jj-vcs.dev/latest/technical/conflicts/), [JJ Working Copy](https://docs.jj-vcs.dev/latest/working-copy/)

## Problems

### Problem 1: Three layers of conflict handling fight JJ's design

The current code has three layers that are all unnecessary given JJ's model:

1. **Step 0 pre-rebase** (`isolation.rs:326-449`) — rebases outside the lock to detect
   conflicts early, runs `jj resolve --list`, tries `jj resolve --all` for auto-resolution.
   ~120 lines.

2. **Retry counter** (`isolation.rs:414-446`) — `.conflict_retries` file in workspace dir,
   counts to 3, defers absorption on each retry by returning `AbsorbResult::Conflicts`.

3. **Force-absorb** (`isolation.rs:421-426`) — after 3 retries, falls through to normal
   absorption. Conflict markers end up in the target working copy.

This is unnecessary because **JJ rebases always succeed** — a conflicted rebase doesn't fail,
it records the conflict as first-class data. We don't need to detect conflicts before
absorbing, retry, or force-absorb. We just absorb, and if the result is conflicted, JJ knows.

### Problem 2: Step 0 target drift (former issue #3)

Step 0 runs a pre-rebase conflict check **outside the absorption lock**. Between step 0 and
steps 1-2 (which run inside the lock), the target `@-` can change — another session may have
absorbed in the gap. This means step 0 checks conflicts against a stale fork point:

```
Step 0: Check conflicts against @- = commit A  (no lock)
            ... another session absorbs, @- is now commit B ...
Steps 1-2: Rebase against @- = commit B         (under lock)
```

The conflict check in step 0 may pass (no conflicts against A), but steps 1-2 rebase against
B, which could introduce new conflicts. Or step 0 may report conflicts against A that don't
exist against B.

**This problem is eliminated entirely by removing step 0.** With no pre-rebase check, there is
no stale target to drift. Conflicts are detected post-absorption using the `conflicts() & @`
revset, which always reflects the actual state.

## Fix — Let JJ handle conflicts natively

### 1. Simplify `absorb_workspace`

**File**: `cli/src/session/isolation.rs`

Remove step 0 entirely (pre-rebase, conflict check, auto-resolve, retry counter). Absorption
becomes just the locked steps 1-2:

```rust
pub fn absorb_workspace(repo_root, workspace, parent_session_uuid) -> Result<AbsorbResult> {
    let ws_head = /* resolve workspace change_id */;

    // Snapshot workspace working copy to capture files written since last snapshot.
    // Uses `jj status` (stable API) — NOT `jj debug snapshot` (unstable, may
    // disappear between JJ releases). `jj status` triggers a snapshot as a side
    // effect. Output is discarded.
    jj_cmd().current_dir(&workspace.path).args(["status", "--ignore-working-copy"]).output();
    // NOTE: --ignore-working-copy on jj status skips the snapshot. We actually
    // need the snapshot, so run WITHOUT --ignore-working-copy:
    jj_cmd().current_dir(&workspace.path).args(["status"]).output();

    // Acquire lock
    let _lock = acquire_absorb_lock(&lock_path)?;

    // Step 1: Rebase workspace chain onto target @-
    // Uses -b (branch mode): in JJ, `-b <rev>` rebases <rev> and all its
    // ancestors up to the nearest common ancestor with the destination. This
    // correctly moves the entire workspace chain (multi-commit or single)
    // onto @-. This matches the current pre-refactor behavior.
    jj_cmd().current_dir(&target_dir)
        .args(["rebase", "-b", &ws_head, "-d", "@-", "--ignore-working-copy"])
        .output()?;

    // Step 2: Rebase @ onto workspace head
    // Uses --ignore-working-copy here too — we don't want JJ to snapshot the
    // target's (stale) working copy before rebasing. After step 1 rewrote the
    // graph with --ignore-working-copy, JJ's tracking of @ is stale. Without
    // --ignore-working-copy, step 2 would auto-snapshot into the wrong commit.
    // Instead, we reconcile the filesystem separately after the rebase.
    jj_cmd().current_dir(&target_dir)
        .args(["rebase", "-s", "@", "-d", &ws_head, "--ignore-working-copy"])
        .output()?;

    // Reconcile working copy: tell JJ to update the filesystem to match the
    // new @ position. `workspace update-stale` is the stable API for this.
    jj_cmd().current_dir(&target_dir)
        .args(["workspace", "update-stale"])
        .output()?;

    // JJ handles conflicts natively — rebase always succeeds
    Ok(AbsorbResult::Absorbed)
}
```

### 2. Remove `AbsorbResult::Conflicts`

**File**: `cli/src/session/isolation.rs`

Absorption now always returns `Absorbed` or `Skipped`. Delete the `Conflicts` variant
(~line 234-244) and all match arms that handle it.

### 3. Remove conflict branch from `workspace_absorb_all`

**File**: `cli/src/flows/core/functions.rs` (~line 1277-1288)

The `AbsorbResult::Conflicts` match arm in `workspace_absorb_all` deferred absorption and
returned early. With that variant gone, remove the branch.

### 4. Move conflict detection into `workspace_absorb_all` (post-absorption)

**File**: `cli/src/flows/core/functions.rs`

**Why not a separate function?** The original plan proposed a standalone
`detect_conflicts_at_head(cwd)` called from the hook. This doesn't work because of a
**cwd mismatch**: after absorption, the agent is still cwd'd in the **isolated workspace**.
Conflicts are in the **target's** `@` (repo root or parent workspace), not the workspace's
`@`. The workspace's `@` is an empty change from the last `jj new` in `change.completed` —
it hasn't been rebased yet (that happens on the next `turn.started`). So `conflicts() & @`
run from the workspace would always return empty.

Instead, integrate the conflict check into `workspace_absorb_all`, which already has the
target directory. After absorption, check the target's `@` for conflicts and return the
result. This preserves the existing hook structure (`absorb_result` check) while swapping
the detection mechanism.

Delete `detect_workspace_conflicts` (lines 1271-1369) and replace with a post-absorption
check inside `workspace_absorb_all`:

```rust
// Inside workspace_absorb_all, after successful absorption:

// Post-absorption: check if the target's @ is now conflicted.
// Uses `conflicts() & @` — not `::@`. JJ propagates conflicts through
// rebases, so if any ancestor is conflicted and wasn't resolved in a
// descendant, @ itself will be conflicted. Checking @ avoids false
// positives from historical conflicts resolved in later commits.
let conflict_check = jj_cmd()
    .current_dir(&target_dir)
    .args([
        "log", "-r", "conflicts() & @", "--no-graph",
        "-T", r#"change_id ++ "\n""#,
        "--ignore-working-copy",
    ])
    .output()?;

let conflicted = String::from_utf8_lossy(&conflict_check.stdout);
if !conflicted.trim().is_empty() {
    // @ is conflicted — get the file list for the autoreply.
    // NOTE: `jj resolve --list` output stream varies across JJ versions.
    // Capture both stdout and stderr defensively.
    let files_output = jj_cmd()
        .current_dir(&target_dir)
        .args(["resolve", "--list", "-r", "@"])
        .output()?;
    let stdout = String::from_utf8_lossy(&files_output.stdout);
    let stderr = String::from_utf8_lossy(&files_output.stderr);
    let files = if !stdout.trim().is_empty() { stdout } else { stderr };

    has_conflicts = true;
    last_conflicted_files = files.trim().to_string();
}
```

The return value changes from the current JSON format to a simpler structure:
- `"0"` — no workspaces to absorb
- `"ok"` — absorbed, no conflicts
- Conflicted file list string — absorbed, but target `@` is conflicted

**Why `conflicts() & @` instead of `conflicts() & ::@`**: JJ propagates conflicts
through rebases. If ancestor A has a conflict in `file.txt` and descendant B doesn't
touch it, B inherits the conflict. Only an explicit resolution in an intermediate commit
clears it. So if `@` is clean, *all* ancestor conflicts have been resolved — there's
nothing for the agent to fix. Checking `::@` would flag ancestors whose conflicts were
already resolved in later commits, creating false/ongoing blockers.

### 5. Simplify hooks.yaml turn.completed

**File**: `cli/src/flows/core/hooks.yaml`

**Important**: `autoreply` is only supported in `turn.completed` events (see
`engine.rs:1259`). It queues a message that triggers a follow-up turn, forcing the agent
to address conflicts before continuing normal work. `context` (the only injection
primitive available in `turn.started`) is advisory — the agent receives the info but is
not forced to respond. For conflict resolution, we need the stronger guarantee.

Since conflict detection is now inside `workspace_absorb_all`, the hook structure stays
close to the current pattern — just check the absorb result:

```yaml
turn.completed:
    # Absorb workspace changes. Returns "ok", "0", or conflicted file list.
    # Absorption always succeeds (JJ records conflicts as tree algebra).
    # Post-absorption conflict check is done inside workspace_absorb_all
    # against the TARGET's @ (not the workspace's @).
    - let: absorb_result = self.workspace_absorb_all

    - if: absorb_result != "ok" and absorb_result != "0" and absorb_result
      then:
          # Target's @ is conflicted after absorption.
          # autoreply triggers a follow-up turn — the agent MUST address
          # the conflict before it can continue normal work. This is stronger
          # than context (advisory) and preserves the current blocking behavior.
          #
          # On the follow-up turn, turn.started runs workspace_ensure_isolated,
          # which rebases the workspace to the current @-. Since @- now contains
          # the conflicted changes, the workspace inherits the conflicts. The
          # agent sees conflict markers in their workspace files and can resolve.
          - autoreply: |
                CONFLICT RESOLUTION REQUIRED

                Absorption introduced conflicts in the working copy.
                Conflicted files:
                {{absorb_result}}

                To resolve: edit the conflicted files to remove JJ conflict
                markers, then continue working. JJ parses your edits back
                automatically on snapshot.

                JJ conflict marker format (NOT Git's format):
                  <<<<<<< Conflict N of M
                  %%%%%%% Changes from base to side #1 (this is a DIFF, not content)
                  +++++++ Contents of side #2 (this is literal content)
                  >>>>>>> Conflict N of M ends

                The %%%%%%% section shows a diff (additions/removals from base),
                NOT a middle version like Git's =======. Apply the diff's intent
                to side #2's content to produce the merged result.
```

**Behavior**: After absorption, if the target's `@` has conflicts,
`workspace_absorb_all` returns the conflicted file list. The hook fires an `autoreply`
that starts a new turn. On that turn, `workspace_ensure_isolated` rebases the workspace
to the current `@-` (which now contains the conflicted changes), so the agent sees
conflict markers in their workspace files. The agent resolves the conflicts (edits files),
and on the next `turn.completed`, absorption runs again. If `@` is now clean, no
autoreply fires and the session continues normally.

### 6. Delete `.conflict_retries` sidecar handling

**File**: `cli/src/session/isolation.rs`

Remove all code that reads/writes/increments the `.conflict_retries` file in the workspace
directory (~lines 414-446). This includes the file creation, the count check, and the
cleanup.

## Hooks Interaction Analysis

The turn lifecycle runs several JJ commands that interact with the absorption flow. This
section traces through the full lifecycle to verify no conflicts with the refactored plan.

### Turn lifecycle with workspace isolation

```
turn.started
  └→ workspace_ensure_isolated
       └→ rebases workspace @ to current @- (picks up other sessions' changes)

  [agent works in isolated workspace]

  change.permission_asked (before each file edit)
    ├→ jj diff -r @ --name-only          check for existing changes
    ├→ jj describe --message "..."        stash user edits with metadata
    └→ jj new                             give agent a clean working copy

  change.completed (after each file edit)
    ├→ jj metaedit / jj split             record provenance metadata
    └→ jj new                             separate next edit into own change

turn.completed
  └→ workspace_absorb_all
       ├→ jj status (snapshot)            capture workspace files
       ├→ [acquire lock]
       ├→ jj rebase -b ws_head -d @-      step 1: insert workspace chain
       ├→ jj rebase -s @ -d ws_head       step 2: move target @ on top
       ├→ jj workspace update-stale        reconcile target filesystem
       ├→ conflicts() & @ check            post-absorption conflict detection
       └→ [release lock]
```

### Verified: no interactions

**`change.completed`'s `jj new` and absorption** — Every `change.completed` ends with
`jj new`, so the workspace's `@` is always an empty change (no diff). `ws_head` in
absorption is this empty change. After absorption, the empty change and its ancestors
(the real work) are rebased into the target's history. The empty change persists in the
target's history but is harmless — JJ handles empty changes fine and hides them in
`jj log` by default. This is **existing behavior**, not introduced by the refactor.

**`change.permission_asked` stashing** — The stashing flow (`jj diff`, `jj describe`,
`jj new`) runs during the turn, in the workspace, before the agent edits. Absorption
runs after the turn completes. No timing conflict — these operate on different changes
at different times.

**`session.ended` absorption** — `session.ended` also calls `workspace_absorb_all`. The
simplified version works identically here. On session end, there's no follow-up turn, so
if conflicts are detected, they'll be left in the target's `@` for the user or another
agent to resolve. This is acceptable — the session is ending anyway.

### Resolved: cwd mismatch for conflict detection

**Problem**: After absorption, the agent is still cwd'd in the isolated workspace.
Running `conflicts() & @` from the workspace checks the workspace's `@` (an empty
change from `jj new`), not the target's `@` (where conflicts live).

**Solution**: Conflict detection runs inside `workspace_absorb_all`, which already has
the target directory. The `conflicts() & @` revset is evaluated against the target's `@`
after step 2 + `workspace update-stale`. See step 4 above.

### Verified: conflict resolution flow end-to-end

When conflicts are detected, the resolution flow works through the existing turn lifecycle:

```
1. turn.completed
   └→ workspace_absorb_all
        ├→ absorption succeeds (JJ records conflicts as tree algebra)
        ├→ conflicts() & @ on TARGET → finds conflicts
        └→ returns conflicted file list

2. Hook fires autoreply → new turn starts

3. turn.started
   └→ workspace_ensure_isolated
        └→ rebases workspace @ to current @-
            @- is now ws_head from the last absorption
            ws_head's ancestors include the conflicted change
            workspace @ inherits the conflict (JJ propagation)
            → agent sees conflict markers in workspace files

4. Agent edits files to resolve conflicts
   └→ change.completed runs jj new after each edit

5. turn.completed
   └→ workspace_absorb_all
        ├→ absorption picks up the resolution
        ├→ conflicts() & @ on TARGET → clean
        └→ returns "ok"
   → no autoreply → session continues normally
```

The key insight is that `workspace_ensure_isolated` (step 3) rebases the workspace to
`@-`, which is the ws_head from the last absorption. Since the conflicted change is an
ancestor of ws_head, and the workspace's `@` is a descendant, JJ propagates the conflict
into the workspace's `@`. The agent sees the conflict markers in their workspace files and
can resolve them by editing.

## What this removes

| Code | Lines | Why it's redundant |
|------|-------|--------------------|
| Step 0 pre-rebase conflict check | ~120 (`isolation.rs:326-449`) | JJ rebases always succeed with conflicts as first-class data |
| `.conflict_retries` counter | ~30 (`isolation.rs:414-446`) | No retry loop needed |
| `AbsorbResult::Conflicts` variant | ~10 (`isolation.rs:234-244`) | Absorption always succeeds |
| Conflict branch in `workspace_absorb_all` | ~15 (`functions.rs:1277-1288`) | No deferred absorption |
| Conflict handling in turn.completed hook | ~15 (`hooks.yaml:152-167`) | Replaced by simpler post-absorb check |
| `detect_workspace_conflicts` function | ~100 (`functions.rs:1356-1456`) | Replaced by `conflicts() & @` revset |

**Total removed**: ~290 lines

## What this adds

| Code | Lines | Purpose |
|------|-------|---------|
| Post-absorption conflict check in `workspace_absorb_all` | ~25 | JJ-native `conflicts() & @` on target's `@` after absorption |
| Updated turn.completed hook | ~15 | Simplified result check + expanded autoreply with JJ marker docs |

**Total added**: ~50 lines (net: **-240 lines**)

## Subtask Checklist

1. [ ] Remove step 0 (pre-rebase, conflict check, auto-resolve) from `absorb_workspace` (isolation.rs)
2. [ ] Remove `.conflict_retries` counter code (isolation.rs)
3. [ ] Replace `jj debug snapshot` with `jj status` in `absorb_workspace` (isolation.rs)
4. [ ] Add `--ignore-working-copy` to step 2 rebase + `jj workspace update-stale` (isolation.rs)
5. [ ] Remove `AbsorbResult::Conflicts` variant and all match arms (isolation.rs)
6. [ ] Remove conflict branch from `workspace_absorb_all` (functions.rs)
7. [ ] Add post-absorption `conflicts() & @` check inside `workspace_absorb_all` — runs against TARGET dir (functions.rs)
8. [ ] Delete `detect_workspace_conflicts` function (functions.rs)
9. [ ] Update hooks.yaml: simplify turn.completed to check absorb_result string (no JSON)
10. [ ] Remove `detect_workspace_conflicts` from engine.rs dispatch and core/mod.rs exports
11. [ ] `cargo build` — verify no warnings
12. [ ] `cargo test` — verify tests pass
13. [ ] Manual test: create a conflict between two sessions, verify detection + resolution flow end-to-end

## Impact

- **Removes**: ~290 lines of pre-rebase conflict detection, retry counter, force-absorb, and workspace-specific conflict handling
- **Adds**: ~40 lines of JJ-native conflict detection using `conflicts() & @` revset
- **Eliminates**: Issue #3 (target drift) — no pre-rebase check left to go stale
- **Eliminates**: `.conflict_retries` sidecar file
- **Preserves**: Blocking behavior — `autoreply` in `turn.completed` forces a follow-up turn for conflict resolution (same enforcement as current deferred-absorption approach)
- **Risk**: Low — JJ's conflict model is well-documented and battle-tested. Conflicts are still detected and surfaced to the agent; the only change is *how* (revset on `@` vs manual rebase + parse + retry counter)

## Review Notes (2026-02-27)

Changes from review feedback:

1. **Replaced `jj debug snapshot` with `jj status`** — `debug` commands are explicitly
   unstable in JJ and can disappear between releases. `jj status` triggers a snapshot as a
   side effect through the stable API. The current pre-refactor code (isolation.rs:312-315)
   uses `debug snapshot` and should be migrated as part of this work.

2. **Clarified `-b` semantics are correct** — JJ's `-b <rev>` (branch mode) rebases `<rev>`
   *and all its ancestors* up to the nearest common ancestor with the destination. This
   correctly handles both single-commit and multi-commit workspace chains. The current
   pre-refactor code already uses `-b` for step 1 (isolation.rs:489), and the simplified
   version preserves these semantics. This is different from Git's branch behavior — in JJ,
   `-b` moves the whole ancestral chain, not just the tip.

3. **Added `--ignore-working-copy` to step 2 + `workspace update-stale`** — After step 1
   rebases with `--ignore-working-copy`, JJ's working-copy tracking is stale. Without
   `--ignore-working-copy` on step 2, JJ auto-snapshots the target's working copy into the
   wrong (pre-rebase) commit before executing. Safer to use `--ignore-working-copy` on both
   steps, then reconcile with `jj workspace update-stale` afterward. The lock guarantees no
   concurrent filesystem changes, but the stale-snapshot risk is real if the target workspace
   had uncommitted edits.

4. **Capture both stdout/stderr from `jj resolve --list`** — Output stream varies across JJ
   versions. Defensive approach: check stdout first, fall back to stderr.

5. **Expanded conflict marker documentation in autoreply** — JJ uses diff-style markers
   (`%%%%%%%` = diff from base, `+++++++` = literal content), not Git's three-way markers
   (`=======` = middle version). Agents unfamiliar with JJ might misinterpret `%%%%%%%` as
   content rather than a diff. The autoreply now explains the format explicitly.

# Session Isolation & Workspace Absorption Workflow

## Overview

When multiple agent sessions run concurrently in the same repo, aiki creates
**isolated JJ workspaces** per session so agents don't stomp on each other's
files. Changes are automatically **absorbed** (rebased) back into the main
workspace at the end of each turn and when the session ends.

Key design principle: **unconditional isolation**. Every session gets its own
workspace, regardless of how many sessions are active. This eliminates race
conditions that would arise if isolation were conditional on session count.

---

## Architecture Diagram

```
 ~/.aiki/sessions/                      /tmp/aiki/
 ├── {uuid-A}          (session file)   ├── {repo-id}/
 └── {uuid-B}          (session file)   │   ├── {uuid-A}/        (isolated workspace A)
                                        │   │   ├── .jj/repo → ~/.../repo/.jj/repo
                                        │   │   └── (working tree copy)
                                        │   ├── {uuid-B}/        (isolated workspace B)
                                        │   │   └── ...
                                        │   └── .absorb.lock     (serializes absorptions)
                                        │
                                        └── {other-repo-id}/
                                            └── ...
```

---

## Complete Lifecycle

### Phase 1: Session Registration

```
                        ┌─────────────────────┐
                        │   Agent starts       │
                        │   (claude, cursor,   │
                        │    codex, etc.)       │
                        └─────────┬───────────┘
                                  │
                                  ▼
                  ┌───────────────────────────────┐
                  │   session.started event fires  │
                  │   (session_started.rs:20)       │
                  └───────────────┬───────────────┘
                                  │
               ┌──────────────────┼──────────────────┐
               ▼                  ▼                   ▼
   ┌───────────────────┐ ┌──────────────┐ ┌─────────────────────┐
   │ Prune dead PIDs   │ │ Create       │ │ Execute core hook   │
   │ (crashed agents)  │ │ session file │ │ (hooks.yaml:6-36)   │
   │                   │ │ ~/.aiki/     │ │  • jj new           │
   │                   │ │ sessions/    │ │  • aiki init --quiet│
   │                   │ │ {uuid}       │ │  • inject context   │
   └───────────────────┘ └──────────────┘ └─────────────────────┘
```

**Source:** `cli/src/events/session_started.rs` handles the event.

Session file at `~/.aiki/sessions/{uuid}` contains:
```
[aiki]
agent=claude
external_session_id=...
session_id={uuid}
started_at=2026-02-25T...
mode=interactive
parent_pid=12345
[/aiki]
```

---

### Phase 2: Workspace Creation

Triggered on every **turn start** and several other events:

```
Events that trigger isolation:
  • turn.started          (hooks.yaml:120)
  • session.resumed       (hooks.yaml:40)
  • session.compacted     (hooks.yaml:62)
  • session.cleared       (hooks.yaml:94)
  • repo.changed          (hooks.yaml:286)
```

```
 ┌──────────────────────────────────────────────────────────────┐
 │              workspace_ensure_isolated()                      │
 │              (functions.rs:1093)                              │
 └──────────────────────────────┬───────────────────────────────┘
                                │
                                ▼
                ┌───────────────────────────────┐
                │ find_jj_root(cwd)             │
                │ → walk up from cwd for .jj/   │
                └───────────────┬───────────────┘
                                │
                                ▼
                ┌───────────────────────────────┐
                │ create_isolated_workspace()   │
                │ (isolation.rs:58)             │
                │                               │
                │ 1. Check if exists (reuse)    │
                │ 2. Resolve @- change_id       │
                │    explicitly via jj log      │
                │ 3. jj workspace add           │
                │    /tmp/aiki/{repo}/{uuid}    │
                │    --name aiki-{uuid}         │
                │    -r {parent_change_id}      │
                └───────────────┬───────────────┘
                                │
                                ▼
                ┌───────────────────────────────┐
                │ Inject context to agent:      │
                │ (hooks.yaml:44-49)            │
                │                               │
                │ "WORKSPACE ISOLATION: ...     │
                │  You MUST cd {ws_path} ..."   │
                └───────────────────────────────┘
```

**Idempotent:** If workspace already exists at `/tmp/aiki/{repo-id}/{uuid}/`,
it's returned immediately without re-creating (isolation.rs:77).

---

### Phase 3: Agent Works in Isolated Workspace

```
                    Main Workspace (repo root)
                    ┌─────────────────────────┐
                    │  @- ─── @ (working copy) │
                    │   │                       │
                    │   │   (human or other     │
                    │   │    agent's changes)   │
                    └───┼───────────────────────┘
                        │
                  fork point
                        │
         ┌──────────────┼──────────────┐
         ▼                             ▼
 Isolated WS A                  Isolated WS B
 /tmp/aiki/{repo}/{uuid-A}/    /tmp/aiki/{repo}/{uuid-B}/
 ┌──────────────────────┐      ┌──────────────────────┐
 │ Agent A edits files  │      │ Agent B edits files  │
 │ in this directory    │      │ in this directory    │
 │                      │      │                      │
 │ .jj/repo → real repo│      │ .jj/repo → real repo│
 └──────────────────────┘      └──────────────────────┘
```

Each workspace is a JJ workspace sharing the same underlying repo
(`.jj/repo` points back to the original). Agents `cd` into their workspace
and perform all file operations there.

---

### Phase 4: Workspace Absorption (Turn Completion)

Triggered on:
- `turn.completed` (hooks.yaml:146) — after every agent turn
- `session.ended` (hooks.yaml:299) — when session terminates
- **Claude Code only**: `ExitPlanMode` PreToolUse (events.rs:193) — when agent requests to exit plan mode, absorbs workspace *before* showing user approval prompt so plan files are visible before user decides whether to approve

```
 ┌───────────────────────────────────────────────────────────────┐
 │                workspace_absorb_all()                         │
 │                (functions.rs:1189)                            │
 │                                                               │
 │  Scans /tmp/aiki/*/{session-uuid}/ for this session's        │
 │  workspaces. For each workspace found:                        │
 └───────────────────────────────┬───────────────────────────────┘
                                 │
                                 ▼
 ┌───────────────────────────────────────────────────────────────┐
 │                absorb_workspace()                             │
 │                (isolation.rs:269)                             │
 └───────────────────────────────┬───────────────────────────────┘
                                 │
                                 ▼
```

#### Step 0: Pre-rebase Conflict Detection (OUTSIDE lock)

```
 ┌──────────────────────────────────────────────────────────────┐
 │  STEP 0: Conflict detection (no lock held)                   │
 │  (isolation.rs:326-449)                                      │
 │                                                               │
 │  1. Get workspace change_id via jj workspace list            │
 │  2. jj debug snapshot (capture uncommitted files)            │
 │  3. Resolve target @- change_id                              │
 │  4. jj rebase -b {ws_head} -d {target_@-}                   │
 │     --ignore-working-copy                                     │
 │  5. jj debug snapshot (materialize conflict markers)         │
 │  6. jj resolve --list (check for conflicts)                  │
 └──────────────────────────┬───────────────────────────────────┘
                            │
               ┌────────────┴────────────┐
               ▼                         ▼
        No conflicts                Conflicts found
               │                         │
               │                         ▼
               │              ┌─────────────────────────┐
               │              │ Try auto-resolve:       │
               │              │ jj resolve --all        │
               │              │ jj debug snapshot       │
               │              │ jj resolve --list       │
               │              └───────────┬─────────────┘
               │                          │
               │               ┌──────────┴──────────┐
               │               ▼                     ▼
               │         All resolved          Still conflicts
               │               │                     │
               │               │                     ▼
               │               │          ┌─────────────────────┐
               │               │          │ Check retry count   │
               │               │          │ (.conflict_retries) │
               │               │          └───────┬─────────────┘
               │               │             ┌────┴────┐
               │               │             ▼         ▼
               │               │         < 3 retries  >= 3 retries
               │               │             │         │
               │               │             ▼         ▼
               │               │     Return           Force absorb
               │               │     Conflicts        (fall through)
               │               │     {id, files}
               ▼               ▼                       │
```

#### Target Snapshot + Steps 1-2: Two-Phase Rebase (INSIDE lock)

```
 ┌──────────────────────────────────────────────────────────────┐
 │  SNAPSHOT TARGET WORKING COPY (before lock)                  │
 │  (isolation.rs:371-379)                                      │
 │                                                               │
 │  jj status  (in target_dir)                                  │
 │                                                               │
 │  Captures any changes made in the target workspace (e.g.,    │
 │  user deleting files in main while agent works in isolation)  │
 │  into @'s committed tree. Without this, the rebase computes  │
 │  an empty diff for @ and silently reverts the user's changes.│
 └──────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
 ┌──────────────────────────────────────────────────────────────┐
 │  ACQUIRE ABSORB LOCK                                         │
 │  (isolation.rs:190)                                          │
 │                                                               │
 │  Path: /tmp/aiki/{repo-id}/.absorb.lock                     │
 │  Mechanism: O_CREAT|O_EXCL (atomic file creation)            │
 │  Timeout: 30 seconds (stale lock removal)                    │
 │  Poll interval: 100ms                                        │
 └──────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
 ┌──────────────────────────────────────────────────────────────┐
 │  STEP 1: Rebase workspace chain onto target @-               │
 │                                                               │
 │  jj rebase -b {ws_head} -d @- --ignore-working-copy         │
 │                                                               │
 │  Uses -b (branch): moves only workspace-specific commits,    │
 │  NOT shared ancestors. Avoids cascade rewrites to siblings.   │
 │                                                               │
 │  Before:  @- ─── @                                           │
 │            \                                                  │
 │             └─── ws_changes ─── ws_head                      │
 │                                                               │
 │  After:   @- ─── ws_changes ─── ws_head                     │
 │            \                                                  │
 │             └─── @  (still in old position)                  │
 └──────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
 ┌──────────────────────────────────────────────────────────────┐
 │  STEP 2: Rebase @ onto workspace head                        │
 │                                                               │
 │  jj rebase -s @ -d {ws_head} --ignore-working-copy          │
 │                                                               │
 │  Uses --ignore-working-copy because JJ's working-copy        │
 │  tracking is stale after step 1's rebase. Filesystem sync    │
 │  is handled by `workspace update-stale` below.               │
 │                                                               │
 │  After:   @- ─── ws_changes ─── ws_head ─── @               │
 │                                                               │
 │  The workspace's changes are now ancestors of @.             │
 └──────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
 ┌──────────────────────────────────────────────────────────────┐
 │  SYNC FILESYSTEM                                             │
 │                                                               │
 │  jj workspace update-stale                                   │
 │                                                               │
 │  Updates the filesystem to match the rebased @. Without      │
 │  this, the next snapshot would see stale files.              │
 └──────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
 ┌──────────────────────────────────────────────────────────────┐
 │  RELEASE LOCK (RAII: AbsorbLock drops)                       │
 └──────────────────────────────────────────────────────────────┘
```

**Why two phases?** Workspaces fork at different times (different @-
ancestors). A single `jj rebase -b @ -d <ws_head>` drags intermediate
default-workspace ancestors along, cascading rewrites to sibling
workspaces and creating divergent changes.

**Why a lock?** Without serialization, concurrent step-2s each move @
to their own target, disconnecting from previous absorptions. The lock
ensures chaining: each absorption builds on the last.

---

### Phase 5: Post-Absorption Results

```
 ┌──────────────────────────────────────────────────────────────┐
 │              AbsorbResult enum                               │
 │              (isolation.rs:234)                              │
 └──────────────────────────┬───────────────────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
         Absorbed       Conflicts      Skipped
              │             │             │
              ▼             ▼             ▼
     ┌──────────────┐ ┌──────────┐ ┌──────────────┐
     │ Cleanup:     │ │ Keep WS  │ │ Cleanup:     │
     │ • jj forget  │ │ alive    │ │ (same as     │
     │ • rm -rf ws  │ │ • Emit   │ │  Absorbed)   │
     │              │ │   auto-  │ │              │
     │              │ │   reply  │ │ (empty ws,   │
     │              │ │   with   │ │  root change │
     └──────────────┘ │   conflict│ │  or not      │
                      │   details│ │  found)      │
                      └──────────┘ └──────────────┘
```

When conflicts are detected, the hook emits an `autoreply` that triggers
another agent turn (hooks.yaml:156-167):

```
CONFLICT RESOLUTION REQUIRED

Workspace absorption detected conflicts during rebase.
Conflict ID: {conflict_id}
Conflicted files: {files}

To resolve: aiki fix {conflict_id}
```

---

### Phase 6: Session End & Final Cleanup

```
 ┌───────────────────────┐
 │ session.ended event   │
 │ (hooks.yaml:299)      │
 └───────────┬───────────┘
             │
             ▼
 ┌───────────────────────────────────────┐
 │ workspace_absorb_all()               │
 │ (final absorption — same as turn)     │
 └───────────┬───────────────────────────┘
             │
             ▼
 ┌───────────────────────────────────────┐
 │ Remove session file:                 │
 │ ~/.aiki/sessions/{uuid}             │
 └───────────┬───────────────────────────┘
             │
             ▼
 ┌───────────────────────────────────────┐
 │ Opportunistic orphan cleanup:        │
 │ cleanup_orphaned_workspaces()        │
 │ (isolation.rs:709)                   │
 │                                       │
 │ Scans jj workspace list for aiki-*   │
 │ entries with no active session →     │
 │ jj workspace forget + rm -rf         │
 └───────────────────────────────────────┘
```

---

### Crash Recovery

If an agent crashes before cleanup:

```
 ┌──────────────────────────────────────────────────────┐
 │ recover_orphaned_workspaces(session_uuid)            │
 │ (isolation.rs:590)                                    │
 │                                                       │
 │ Called during next session's prune_dead_pid_sessions  │
 └────────────────────────┬─────────────────────────────┘
                          │
                          ▼
 ┌──────────────────────────────────────────────────────┐
 │ Scan /tmp/aiki/*/{session_uuid}/ for dead sessions   │
 └────────────────────────┬─────────────────────────────┘
                          │
           For each orphaned workspace:
                          │
                          ▼
 ┌──────────────────────────────────────────────────────┐
 │ 1. find_repo_root_from_workspace()                   │
 │    (.jj/repo → text file or symlink → repo root)     │
 │                                                       │
 │ 2. absorb_workspace() into main                      │
 │    ├─ Success → cleanup                              │
 │    ├─ Conflicts → force cleanup (warn user)          │
 │    └─ Error → create recovery bookmark:              │
 │               aiki/recovered/{workspace_name}        │
 │               (preserves unabsorbed changes)         │
 │                                                       │
 │ 3. cleanup_workspace() (always, regardless)           │
 └──────────────────────────────────────────────────────┘
```

---

## Complete State Machine

```
                            ┌─────────┐
                            │  START  │
                            └────┬────┘
                                 │ session.started
                                 ▼
                     ┌───────────────────────┐
                     │    SESSION ACTIVE      │
                     │                        │
                     │  Session file created  │
                     └───────────┬────────────┘
                                 │ turn.started
                                 ▼
                     ┌───────────────────────┐
                     │  ENSURE ISOLATED      │
                     │  (unconditional)      │
                     └───────────┬───────────┘
                                 │
                                 ▼
                     ┌───────────────────────┐
                     │  ISOLATED MODE         │
                     │  (ws at /tmp/aiki)     │
                     └───────────┬───────────┘
                                 │
                                 │    agent works
                                 │
                                 ▼
                     ┌───────────────────────┐
                     │    turn.completed      │
                     └───────────┬───────────┘
                                 │
                    ┌────────────┼────────────┐
                    ▼            ▼            ▼
                 Absorbed     Conflicts    Skipped
                    │            │            │
                    │            ▼            │
                    │    ┌──────────────┐     │
                    │    │ auto-reply   │     │
                    │    │ "CONFLICT    │     │
                    │    │  RESOLUTION  │     │
                    │    │  REQUIRED"   │     │
                    │    └──────┬───────┘     │
                    │           │ agent resolves
                    │           │ (retry loop)
                    │           ▼             │
                    │    ┌──────────────┐     │
                    │    │ Next turn →  │     │
                    │    │ re-absorb    │     │
                    │    └──────────────┘     │
                    │                         │
                    └────────────┬────────────┘
                                 │ session.ended
                                 ▼
                     ┌─────────────────────┐
                     │  Final absorb_all   │
                     │  Remove session file│
                     │  Orphan cleanup     │
                     └──────────┬──────────┘
                                 │
                                 ▼
                            ┌─────────┐
                            │   END   │
                            └─────────┘
```

---

## Key Files

| File | Role |
|------|------|
| `cli/src/session/isolation.rs` | All workspace CRUD: create, absorb, cleanup, recovery |
| `cli/src/session/mod.rs` | AikiSession struct, session file creation, PID detection |
| `cli/src/flows/core/hooks.yaml` | Event handlers that wire everything together |
| `cli/src/flows/core/functions.rs` | Native functions: `workspace_ensure_isolated`, `workspace_absorb_all`, `detect_workspace_conflicts` |
| `cli/src/events/session_started.rs` | Session lifecycle: prune dead PIDs, create session file, run core hook |

---

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Unconditional isolation | Every session gets a workspace — no session counting, no special-casing. Eliminates race conditions where a second session could start before the first creates its workspace |
| Two-phase rebase | Single rebase drags shared ancestors, cascading rewrites to sibling workspaces |
| File-based lock | Serializes concurrent absorptions so each builds on the last |
| Conflict detection outside lock | Avoids holding lock while waiting for user/agent resolution |
| `/tmp/aiki/` for workspaces | Ephemeral by nature; reboot cleans up naturally; crash recovery handles the rest |
| `jj workspace add -r @-` | Forks from parent of working copy so agent starts with a clean working copy |
| Target snapshot before rebase | `jj status` in target dir captures user's filesystem changes (e.g., file deletions) into @'s committed tree before rebasing. Without this, the rebase computes an empty diff and silently reverts user changes |
| Both steps use `--ignore-working-copy` | After step 1 rebases the workspace chain, JJ's working-copy tracking is stale. Step 2 also uses `--ignore-working-copy`, then `workspace update-stale` syncs the filesystem |
| Retry count with force-absorb at 3 | Prevents infinite conflict loops; lets human intervene after reasonable attempts |
| `jj new --ignore-working-copy` in default workspace | `session.started` and `repo.changed` hooks run `jj new` in the default workspace, which can race with concurrent absorptions. Without `--ignore-working-copy`, `jj new` triggers a working-copy snapshot that diverges from an in-flight absorption's rebase, causing jj to reconcile divergent operations and silently revert user filesystem changes (e.g. files moved in Finder). User changes are captured later by the absorption's own `jj status` target snapshot |

---

## JJ Conflict Model References

The absorption and conflict handling in this system relies on JJ's first-class conflict model.
Understanding these fundamentals is essential for working on the isolation code:

- **[Conflicts](https://docs.jj-vcs.dev/latest/conflicts/)** — How JJ stores conflicts as first-class objects (not text markers), materialization in working copies, resolution workflow, and propagation to descendants
- **[Technical: Conflicts](https://docs.jj-vcs.dev/latest/technical/conflicts/)** — Tree algebra internals (`A+(C-B)+(E-D)`), on-demand resolution, simplification during rebase
- **[Working Copy](https://docs.jj-vcs.dev/latest/working-copy/)** — Conflict marker round-trip: materialization (commit→files) and de-materialization (files→commit on snapshot)
- **[Steve's JJ Tutorial: Conflicts](https://steveklabnik.github.io/jujutsu-tutorial/branching-merging-and-conflicts/conflicts.html)** — Practical walkthrough of conflict resolution, propagation through descendants, `jj resolve`

Key design implications for aiki:

| JJ capability | Implication for isolation |
|--------------|--------------------------|
| Rebases always succeed (conflicts recorded, not rejected) | Absorption never fails — no need for retry loops or force-absorb |
| `conflicts()` revset | Native conflict detection: `jj log -r 'conflicts() & ::@'` replaces marker files |
| `jj resolve --list -r <rev>` | Query conflicted files in any revision, not just working copy |
| Resolution propagates to descendants | Fixing a conflict in one commit auto-resolves dependent commits |
| Conflict markers are round-trippable | Agents can edit markers in files; JJ parses resolution back on snapshot |

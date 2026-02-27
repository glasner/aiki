# Workspace Isolation Improvements

**Date**: 2026-02-26
**Status**: Draft
**Context**: Found during deep trace of the session isolation lifecycle

## 1. Remove Conditional Isolation — Always Isolate

**Broken out to**: [`remove-conditional-isolation.md`](remove-conditional-isolation.md)

Eliminates 3 race conditions and ~140 lines by removing session-counting machinery and
making every session unconditionally isolated. Also eliminates issues #3 and #5.

---

## 2. Needless Workspace Destroy/Recreate Each Turn

**Broken out to**: [`reuse-isolated-workspaces.md`](reuse-isolated-workspaces.md)

Stop destroying and recreating JJ workspaces on every turn. After absorption, keep the
workspace alive and rebase it to the new fork point on next turn start. Eliminates N-1
destroy/create cycles per session. Depends on #1.

---

## ~~3. Step 0 ↔ Steps 1-2 Target Drift~~

**Status**: Eliminated by #4 — see [`refactor-conflict-resolution.md`](refactor-conflict-resolution.md)

---

## 4. Replace Force-Absorb + Retry Counter with JJ-Native Conflict Handling

**Broken out to**: [`refactor-conflict-resolution.md`](refactor-conflict-resolution.md)

Remove ~290 lines of hand-rolled conflict detection (pre-rebase check, retry counter,
force-absorb) and replace with ~50 lines using JJ's native `conflicts() & ::@` revset.
Absorption always succeeds; conflicts are detected at turn start. Also eliminates #3
(target drift).

---

## ~~5. Solo → Concurrent Transition Gap~~

**Status**: Eliminated by #1

With always-isolate, there is no solo mode. Every session gets a workspace at session start, so no session ever works directly in the repo root. The solo→concurrent transition gap cannot occur.

---

## Priority Order

| # | Issue | Severity | Effort | Recommendation |
|---|-------|----------|--------|----------------|
| 1 | Always isolate (remove conditional) | Bug + Simplification | Medium (remove ~70 lines, simplify function) | Fix now |
| 4 | JJ-native conflict handling (remove step 0 + retry + force-absorb) | Correctness + Simplification | Medium (remove ~290 lines, add ~50) | Fix now (with #1) |
| 2 | Destroy/recreate overhead | Perf | Medium (restructure absorption cleanup) | Fix now (depends on #1) |
| ~~3~~ | ~~Target drift~~ | ~~Eliminated~~ | — | Solved by #4 (step 0 removed) |
| ~~5~~ | ~~Solo→concurrent gap~~ | ~~Eliminated~~ | — | Solved by #1 |

Combined impact of #1 + #4: **remove ~360 lines, add ~50 lines, eliminate 4 race conditions / edge cases, delete 2 sidecar files (`.conflict_retries`, by-repo sidecars)**

## Files Changed

| File | Change |
|------|--------|
| `cli/src/flows/core/functions.rs` | Rewrite `workspace_create_if_concurrent` → `workspace_ensure_isolated` (#1); remove `detect_workspace_conflicts`, add `detect_conflicts_in_ancestry` using `conflicts()` revset (#4); remove cleanup on Absorbed (#2); remove conflict branch from `workspace_absorb_all` (#4) |
| `cli/src/session/isolation.rs` | Remove `count_sessions_in_repo`, `register/unregister_session_from_repo`, `find_session_repo`, `by_repo_dir` (#1); remove step 0 pre-rebase, retry counter, `AbsorbResult::Conflicts` (#4); add rebase-to-current in workspace reuse path (#2); update `cleanup_orphaned_workspaces` to use PID-based liveness (#1) |
| `cli/src/flows/core/hooks.yaml` | Update function name references (#1); replace turn.completed conflict branch with turn.started `detect_conflicts_in_ancestry` (#4) |
| `cli/src/events/session_started.rs` | Remove or simplify session registration (#1) |

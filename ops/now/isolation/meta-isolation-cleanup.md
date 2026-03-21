# Isolation Cleanup: Implementation Order

**Date**: 2026-03-21
**Status**: Plan
**Priority**: P0 — multiple data-loss paths are open until these ship

---

## The Problem

Multiple isolation bugs are open. The only fix shipped so far is a post-absorption safety check (Option B, `08c4cae`). The bugs compound — stale locks cause absorption failures, which trigger cleanup of unabsorbed files, which causes silent data loss. Fixing them in the wrong order leaves windows where partial fixes create new failure modes.

---

## Recommended Order

### Phase 1: Stop the bleeding (prevent data loss now)

#### 1a. Replace bespoke locking with `fd-lock`
**Plan:** [step-1a-fd-lock-mutex.md](step-1a-fd-lock-mutex.md)
**Effort:** ~65 lines deleted, ~20 lines added in `isolation.rs` + `Cargo.toml` dependency
**Why first:** This is the domino that starts the cascade. A stale lock blocks all absorptions for 30s, causing timeouts that lead to cleanup of unabsorbed files. Replacing the bespoke `O_CREAT|O_EXCL` + PID-tracking lock with `fd-lock` (a thin `flock(2)` wrapper) eliminates stale locks entirely — the kernel releases `flock` locks on process exit, even SIGKILL. Also generalizes to `acquire_named_lock` for use by step-1b/1c (session-start race fix).
**Depends on:** Nothing.

#### 1b. `mutex` YAML primitive for flow engine
**Plan:** [step-1b-mutex-yaml-primitive.md](step-1b-mutex-yaml-primitive.md)
**Effort:** ~30 lines across `types.rs` and `engine.rs`
**Why:** The session-start-race fix (1c) needs to wrap `jj new` in the workspace-absorption lock via YAML. This adds a `mutex` action to the flow engine that acquires a named file lock (via `acquire_named_lock` from 1a), runs nested steps, and releases on scope exit.
**Depends on:** 1a (uses `acquire_named_lock`).

#### 1c. Wrap `session.started` `jj new` in mutex
**Plan:** [step-1c-session-start-mutex.md](step-1c-session-start-mutex.md)
**Effort:** ~5 lines in `hooks.yaml`
**Why:** `session.started`'s `jj new` races with the two-step absorption — a new `@` can be created between step 1 (rebase ws_head onto @-) and step 2 (rebase @ onto ws_head), stranding all workspace changes on sibling branches. Wrapping `jj new` in `mutex: workspace-absorption:` serializes it with concurrent absorptions. Nearly 100% reproducible with concurrent agents.
**Depends on:** 1b (needs the `mutex` YAML primitive).

#### 1d. Never delete unabsorbed files
**Plan:** [step-1d-never-delete-unabsorbed.md](step-1d-never-delete-unabsorbed.md)
**Effort:** ~40 lines across `isolation.rs` and `functions.rs`
**Why:** Even after fixing stale locks and the session-start race, absorptions can still fail (JJ errors, corrupted `.jj/repo`, snapshot failures). Today, every failure path ends with `cleanup_workspace` deleting files. This change adds a `workspace_has_changes` guard before cleanup and a `fallback_copy_files` last resort. Also stops ignoring snapshot failures (Rule 4).
**Depends on:** Nothing (but most valuable after 1a–1c reduce failure frequency).

### Phase 2: Fix the absorption mechanics

#### 2a. Split `AbsorbResult::Skipped` into `Skipped` / `Empty`
**Plan:** [step-2a-split-skipped-enum.md](step-2a-split-skipped-enum.md)
**Effort:** ~20 lines across `isolation.rs`, `functions.rs`
**Why:** The current `Skipped` enum conflates "nothing to absorb" (safe) with "couldn't absorb" (not safe). Splitting it lets the caller make correct cleanup decisions. This is the structural fix that makes 1d's guards precise rather than heuristic.
**Depends on:** 1d (the guard logic needs to distinguish the two cases).

#### 2b. Fix `rebase -b` topology bug (use `-s` with root detection)
**Plan:** [step-2b-rebase-topology-fix.md](step-2b-rebase-topology-fix.md)
**Effort:** ~25 lines in `isolation.rs:absorb_workspace` Step 1
**Why:** The post-absorption safety check (Option B, already shipped) catches stranded commits after the fact, but the root cause is `rebase -b` creating sibling branches when workspaces share a fork point. Uses `-s` with explicit root detection (`roots(ws_head ~ ::@-)`) to move only workspace-specific commits. This eliminates the stranding rather than repairing it.
**Depends on:** 1a (stale locks must be fixed first, otherwise the longer absorption code path increases the window for lock contention).

### Phase 3: Performance (make everything fast enough to not timeout)

#### ~~3a. Task branch fan-out → linear chain~~ — SHIPPED

See "What's Already Shipped" below. Events now chain linearly and bookmarks advance after each write. Head count dropped from ~31K to ~1 per branch.

#### 3b. Empty change cleanup
**Plan:** [step-3b-empty-change-cleanup.md](step-3b-empty-change-cleanup.md)
**Effort:** ~5 lines in `hooks.yaml` (session.ended cleanup)
**Why:** Only ~9 empty changes remain on the main chain (the 35K fan-out heads were eliminated by 3a). Low priority — cosmetic cleanup. Add `jj abandon 'ancestors(@) & empty() & ~root()'` to `session.ended`.
**Depends on:** Nothing (3a is shipped).

### Phase 4: Operational hygiene

#### 4. Session lifecycle fixes
**Plan:** [step-4-session-lifecycle.md](step-4-session-lifecycle.md)
**Effort:** ~40 lines across `session/mod.rs` and `isolation.rs`
**Why:** 161 stale session files accumulated because `prune_dead_pid_sessions` couldn't keep up, and orphaned JJ workspaces were never cleaned up when `session.ended` didn't fire. Three fixes: (1) 36h TTL on session files, (2) orphaned workspace cleanup (forget JJ workspace + remove temp dir) when removing stale sessions, (3) batch-limit the pruner to avoid blocking startup. This is defense-in-depth — Phase 1a fixes the acute locking problem, this prevents session/workspace accumulation.
**Depends on:** 1a (acute fix first, then prevention).

---

## What's Already Shipped

| Fix | Commit | Status |
|-----|--------|--------|
| Post-absorption safety check (Option B) | `08c4cae` | Shipped — catches stranded commits after Step 2 and rebases @ to fix |
| Git guidance in workspace context | `29fbedf` | Shipped — agents told to run git commands from main repo |
| Codex `--add-dir` for shared JJ store | `dccc17e` | Shipped — sandbox no longer blocks JJ store writes |
| Task branch fan-out → linear chain (3a) | | Shipped — events chain linearly, bookmarks advance after write, head count ~31K → ~1 |

---

## Dependency Graph

```
1a (fd-lock mutex) ─→ 1b (mutex YAML) ─→ 1c (session.started fix)
       │
       ├─────────────────────────────────→ 2b (rebase -b fix)
       │
1d (never delete unabsorbed) ──────→ 2a (split Skipped enum)

3b (empty cleanup) ← no deps (3a shipped)

1a ────────────────────────────→ 4 (session lifecycle)
```

1a → 1b → 1c is the critical path: fix the lock primitive, expose it in YAML, then use it to fix the session-start race. 1d is independent and can run in parallel with 1b/1c. 3b is unblocked (3a shipped). Phase 2 builds on Phase 1. Phase 4 is cleanup after Phase 1.

---

## Estimated Total

~150 lines of new/changed code across 6 files (3a already shipped). No architectural changes — all fixes are surgical additions to existing functions.

---

## Appendix: Research & Decisions

Every finding from the original investigation is accounted for in the steps above or noted here.

### Incident: JJ repo degradation (2026-03-19)

44 orphaned workspaces, stranded file commits on side branches, divergent operation log, 3.8GB orphaned temp dirs. Manual cleanup performed (workspace forget, stranded commit recovery via `jj squash --from`, temp dir removal). Root cause: concurrent absorption topology bug (step-2b) compounded by stale locks (step-1a). Option B defensive check shipped in `08c4cae`.

### Stale absorption locks investigation (2026-03-19)

Primary cause of "files not appearing" incident. Three failure scenarios identified:
- **Scenario 1** (session killed mid-absorption): stale lock blocks all absorptions for 30s → **step-1a** eliminates via `flock(2)`
- **Scenario 2** (session.ended never fires): orphaned workspaces, 161 stale session files → **step-4** adds TTL cleanup + workspace forget
- **Scenario 3** (slow absorption on huge graph): 30s timeout causes silent skips → **step-1a** (kernel blocking) + **3a shipped** (reduced graph size)

### Absorption concurrency bug: abandoned alternatives

- **Option B** (post-absorption ancestry check): Already shipped (`08c4cae`). Catches stranded commits after the fact — kept as defense-in-depth alongside step-2b's root cause fix.
- **Option C** (`jj squash --from ws_head --into @`): Atomically moves changes but loses per-commit provenance. Abandoned.

### Session-start race: abandoned alternatives

- **Option B** (move `jj new` into absorption): Eliminates race but changes session lifecycle model — agent expects fresh `@` before starting work.
- **Option C** (single-step absorption via `jj squash`): No race window but loses fine-grained history. Same issue as absorption concurrency Option C.
- **Option D** (atomic rebase via JJ operation lock): JJ doesn't expose an operation lock to external callers.

### Empty change cleanup: deferred enhancement

Conditional `jj new` in `change.completed` (only create new change if `@` has content) would prevent empty changes at source. Deferred — with only ~9 empties on the main chain after step-3a, the session-end cleanup in step-3b is sufficient.

### Out of scope

- **Rhai int-to-bool conditional bug** — tracked separately in `ops/now/fix-rhai-int-conditionals.md`.

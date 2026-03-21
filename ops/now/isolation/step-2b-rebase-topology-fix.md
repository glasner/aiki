# Step 2b: Fix `rebase -b` Topology Bug (Use `-s` with Root Detection)

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 2 — Fix the absorption mechanics
**Source**: Absorption concurrency bug investigation (2026-03-19) — Option A
**Depends on**: Step 1a (stale locks must be fixed first, otherwise the longer absorption code path increases the window for lock contention)

---

## Problem

The post-absorption safety check (Option B, shipped in `08c4cae`) catches stranded commits after the fact, but the root cause is `rebase -b` creating sibling branches when workspaces share a fork point.

### How It Happens

**Absorption code** (`cli/src/session/isolation.rs:343`, `absorb_workspace`):
- Step 1: `jj rebase -b ws_head -d @- --ignore-working-copy` (rebase workspace chain onto target's @-)
- Step 2: `jj rebase -s @ -d ws_head --ignore-working-copy` (move @ after workspace head)

**Race scenario:**

1. B absorbs first (acquires lock):
   - Step 1: `rebase -b ttmysprq -d @-` where `@-` = `pykxpyru` → no-op (already there)
   - Step 2: `rebase -s @ -d ttmysprq` → `@` now at: `pykxpyru → ttmysprq → @`
   - Release lock

2. A absorbs second (acquires lock):
   - Step 1: `rebase -b omqoylol -d @-` where `@-` = `ttmysprq`
   - **Bug:** `-b` rebase finds that `omqoylol`'s root is `pykxpyru`, which is already an ancestor of `@-`. JJ rebases `omqoylol` but it can end up as a sibling of `ttmysprq` rather than a child, depending on how JJ resolves the branch topology.
   - Step 2: `rebase -s @ -d omqoylol` → `@` now at: `omqoylol → @`
   - BUT: `ttmysprq` is no longer in `@`'s ancestry! The chain is: `pykxpyru → omqoylol → @`, with `ttmysprq` as a sibling.

**Core issue:** `rebase -b` with a shared ancestor doesn't guarantee linear chaining. When two workspace chains share a common ancestor and are absorbed sequentially, the second rebase can create a fork instead of extending the chain.

---

## Fix: Use `-s` Instead of `-b` for Step 1

Instead of:
```
jj rebase -b ws_head -d @- --ignore-working-copy
```

Explicitly find the workspace chain's root and use `-s` (source) which rebases a specific commit and its descendants:

```rust
// Find the workspace chain root (first commit not in @-'s ancestry)
let root = jj_cmd()
    .args(["log", "-r", format!("roots({ws_head} ~ ::@-)"),
           "--no-graph", "-T", "change_id", "--limit", "1",
           "--ignore-working-copy"])
    ...;

// Rebase from the root onto @-
jj_cmd().args(["rebase", "-s", &root, "-d", "@-", "--ignore-working-copy"]);
```

**Advantage:** Precise — only moves the workspace's unique commits. Uses `roots(ws_head ~ ::@-)` to find commits that belong exclusively to this workspace.

**Note:** The existing post-absorption safety check (Option B, `08c4cae`) remains as defense-in-depth. This fix eliminates the root cause so the safety check should stop firing.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | In `absorb_workspace` Step 1: replace `rebase -b ws_head -d @-` with root detection via `roots(ws_head ~ ::@-)` followed by `rebase -s <root> -d @-` (~25 lines) |

---

## Implementation Steps

1. Open `cli/src/session/isolation.rs`, find `absorb_workspace` Step 1
2. Before the rebase, run `jj log -r "roots({ws_head} ~ ::@-)"` to find the workspace chain's exclusive root
3. If the root query returns empty (ws_head is already in @-'s ancestry), this is the idempotency case — return early (already absorbed)
4. Replace `rebase -b ws_head -d @-` with `rebase -s <root> -d @-`
5. Keep the existing post-absorption safety check (Option B) as defense-in-depth
6. Run `cargo test` to verify no regressions
7. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`

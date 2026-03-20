# Bug: Concurrent Workspace Absorption Strands Commits

**Date**: 2026-03-19
**Status**: Draft
**Priority**: P0 (data loss — file writes silently disappear from working copy)
**Discovered in**: This session — wrote `ops/now/cleanup-extra-output.md`, absorption captured it into JJ (commit `omqoylol`), but it ended up on a stranded side branch not reachable from `@`.

---

## Symptoms

- Agent writes a file in an isolated workspace
- Turn completes, absorption runs, JJ captures the file into a commit
- The commit is NOT in `@`'s ancestry — it's on a side branch
- Main workspace shows no new files
- The file silently disappears from the working copy

## Root Cause

When multiple sessions absorb concurrently (serialized by file lock but operating on the same repo), later absorptions can strand earlier ones' commits.

### The Graph After the Bug

```
@  tzkzvmkn (empty) ← default working copy (no file!)
○  tytnkqlw (empty) [aiki] — session 58c016ec
○  tpznwwqp (empty) [aiki] — session ff9ceadb
○  ttmysprq (empty) [aiki] — session ff9ceadb
│ ○  omqoylol [aiki] — session 4f92914d (FILE IS HERE, stranded)
├─╯
○  pykxpyru [aiki] — common ancestor
```

### Expected Graph

```
@  (empty) ← default working copy (has file!)
○  tytnkqlw (empty) [aiki]
○  tpznwwqp (empty) [aiki]
○  ttmysprq (empty) [aiki]
○  omqoylol [aiki] ← file commit, in @'s ancestry
○  pykxpyru [aiki]
```

### How It Happens

**Setup:** Sessions A (has file changes) and B (empty changes) both have workspaces forked from the same `@-` = `pykxpyru`.

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
   - **Bug:** `-b` rebase finds that `omqoylol`'s root is `pykxpyru`, which is already an ancestor of `@-` (`ttmysprq`). JJ rebases `omqoylol` but it can end up as a sibling of `ttmysprq` rather than a child, depending on how JJ resolves the branch topology.
   - Step 2: `rebase -s @ -d omqoylol` → `@` now at: `omqoylol → @`
   - BUT: `ttmysprq` is no longer in `@`'s ancestry! The chain is: `pykxpyru → omqoylol → @`, with `ttmysprq` as a sibling.

**Alternatively (and matching the observed graph):** A absorbs, then B absorbs second and its Step 2 moves `@` to `ttmysprq`'s branch, stranding `omqoylol`.

The core issue: **`rebase -b` with a shared ancestor doesn't guarantee linear chaining.** When two workspace chains share a common ancestor and are absorbed sequentially, the second rebase can create a fork instead of extending the chain.

---

## Analysis of `rebase -b` Behavior

`jj rebase -b <rev> -d <dest>` rebases the "branch" from `<rev>` back to (but not including) its common ancestor with `<dest>`. When:

- Workspace chain A: `P → ... → A_head` (P = common fork point)
- Workspace chain B: `P → ... → B_head` (same P)
- After A absorbs: `P → ... → A_head → @`
- B's chain still roots at P, which is an ancestor of A_head

When B absorbs with `rebase -b B_head -d @-` (where `@-` = `A_head`):
- JJ finds the path from `B_head` back to the common ancestor with `A_head`
- The common ancestor is `P`
- JJ rebases the unique commits (`P → ... → B_head` minus ancestors of `A_head`) onto `A_head`

This SHOULD work if JJ correctly identifies P as the common ancestor and rebases B's unique commits onto A_head. The bug may be in how JJ handles the case where B's entire chain is just `P → B_head` and P is already an ancestor of the destination.

---

## Proposed Fix

### Option A: Use `-s` instead of `-b` for Step 1

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

**Advantage:** Precise — only moves the workspace's unique commits.
**Risk:** If the root detection gets the wrong commit, it could move shared history.

### Option B: Post-absorption ancestry check (defensive)

After Step 2, verify that `ws_head` is in `@`'s ancestry. If not, fix it:

```rust
// After Step 2, verify ws_head is an ancestor of @
let check = jj_cmd()
    .args(["log", "-r", format!("{ws_head} & ::@"),
           "--no-graph", "-T", "change_id", "--limit", "1",
           "--ignore-working-copy"])
    ...;

if check.stdout.trim().is_empty() {
    // ws_head was stranded — rebase the default @ chain to include it
    jj_cmd().args(["rebase", "-s", "@", "-d", &ws_head, "--ignore-working-copy"]);
}
```

**Advantage:** Catches the bug regardless of root cause. Simple to implement.
**Risk:** Extra JJ command per absorption (minor perf cost).

### Option C: Single-step absorption with `jj squash` or `jj new`

Instead of the two-step rebase dance, use a different JJ primitive:

```
jj squash --from ws_head --into @
```

This merges the workspace's tree changes directly into `@` without rebasing the commit chain. The workspace commits stay where they are (as historical provenance) and their file changes land in `@`.

**Advantage:** No rebase races possible — `@` always gets the changes.
**Disadvantage:** Loses per-commit provenance in `@`'s ancestry (the workspace commits stay as side branches).

### Recommendation: Option B (defensive check)

Option B is the safest — it's a 10-line addition after Step 2, catches this specific bug, and doesn't change the fundamental absorption strategy. If the check fires, it logs a warning so we can track how often this happens.

If Option B fires frequently, we should investigate Option A as a more robust fix for the root cause.

---

## Implementation

**File:** `cli/src/session/isolation.rs`, in `absorb_workspace()`, after Step 2 (after line ~508):

```rust
// Post-absorption safety check: verify ws_head is in @'s ancestry.
// If not, a concurrent absorption stranded our commits on a side branch.
// Fix by rebasing @ onto ws_head (which includes our changes).
let verify_check = jj_cmd()
    .current_dir(&target_dir)
    .args([
        "log", "-r", &format!("{} & ::@", ws_head),
        "--no-graph", "-T", "change_id", "--limit", "1",
        "--ignore-working-copy",
    ])
    .output();

if let Ok(verify_output) = verify_check {
    if verify_output.status.success() {
        let in_ancestry = String::from_utf8_lossy(&verify_output.stdout);
        if in_ancestry.trim().is_empty() {
            // ws_head is NOT in @'s ancestry — stranded!
            debug_log(|| {
                format!(
                    "[workspace] Post-absorption: ws_head {} is stranded \
                     (not in @'s ancestry), rebasing @ to fix",
                    &ws_head[..ws_head.len().min(12)]
                )
            });
            let fix = jj_cmd()
                .current_dir(&target_dir)
                .args([
                    "rebase", "-s", "@", "-d", &ws_head,
                    "--ignore-working-copy",
                ])
                .output();
            if let Ok(fix_output) = fix {
                if !fix_output.status.success() {
                    let stderr = String::from_utf8_lossy(&fix_output.stderr);
                    eprintln!(
                        "[aiki] WARNING: post-absorption fix rebase failed: {}",
                        stderr.trim()
                    );
                }
            }
        }
    }
}
```

**Also update `workspace update-stale`** (line 514) to run AFTER the safety check, not before.

---

## Verification

```bash
# Simulate the race:
# 1. Create two workspaces from the same @-
# 2. Write a file in workspace A
# 3. Absorb workspace B first (empty)
# 4. Absorb workspace A second (has file)
# 5. Verify file is in @'s working copy

# After fix, this should always pass:
jj log -r 'files("ops/now/cleanup-extra-output.md") & ::@' --no-graph
# Should show the commit containing the file
```

---

## Related

- `cli/src/session/isolation.rs:343` — `absorb_workspace()` implementation
- `cli/src/session/isolation.rs:56` — `create_isolated_workspace()` (workspace creation, fork point)
- `cli/src/flows/core/functions.rs:1003` — `workspace_absorb_all()` (orchestrates absorption)
- `cli/src/flows/core/hooks.yaml:155` — `turn.completed` hook triggers absorption

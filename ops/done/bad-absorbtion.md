# Bad Workspace Absorption — Post-Mortem

**Date**: 2026-02-25
**Status**: Active investigation
**Severity**: Data integrity issue (no data lost, but working tree corrupted)

## What Happened

After commit `61950c3` ("Rename specs to plans, reorganize modules, and clean up completed work") on 2026-02-24, the main working tree was left in a state that **does not match HEAD**. The working tree contains the pre-reorganization file layout while HEAD contains the post-reorganization layout.

This manifests as ~176 files showing as modified/deleted/untracked in `git status`, with a diff of **-27,592 lines** that would effectively undo the commit if staged.

## Timeline (from git reflog)

```
61950c3 HEAD@{0} reset: moving to HEAD           2026-02-24 13:52:09
61950c3 HEAD@{1} commit: Rename specs to plans... 2026-02-24 13:52:09
142df83 HEAD@{2} reset: moving to HEAD           2026-02-22 17:19:01
142df83 HEAD@{3} reset: moving to HEAD           2026-02-22 17:19:01
```

Key observations:
- The commit and `reset: moving to HEAD` happen at the **exact same timestamp** (13:52:09)
- The `reset: moving to HEAD` is a no-op (HEAD didn't change), likely from workspace absorption running `git reset HEAD`
- No commits exist after `61950c3` — the commit itself landed correctly
- The commit was also pushed to origin/main at the same timestamp (`origin/main@{1} update by push`)

## Root Cause Analysis

The commit `61950c3` was a **large reorganization** that:
1. Moved flat files into directory modules (`provenance.rs` → `provenance/`, `repo.rs` → `repos/`, etc.)
2. Renamed `spec` → `plan` across templates and commands
3. Moved completed ops docs from `ops/now/` → `ops/done/`
4. Added new commands (`decompose`, `epic`)

### Likely cause: workspace absorption committed but didn't update the working tree

The workspace absorption process (`cli/src/session/isolation.rs:absorb_workspace`) creates commits via JJ operations (rebase, squash, etc.) and then syncs to git. The git commit lands correctly, but the **main working tree's files are never updated** to reflect the new commit.

This is because:
1. The workspace operates on its own JJ working copy (isolated directory)
2. When absorbed, JJ rebases the main workspace's change on top of the workspace's changes
3. The git commit is created from JJ's internal state, not from the working tree
4. The main working tree still has whatever files were there before — nobody ran `git checkout` or equivalent to sync it

For small changes (single file edits), this is invisible because the working tree usually already has the right content. But for a **massive reorganization** (files moved, renamed, deleted), the working tree diverges dramatically from HEAD.

### Why `reset: moving to HEAD` didn't help

The `git reset HEAD` that absorption runs only resets the **index** (staging area) to match HEAD. It does NOT update the **working tree**. So:
- Index matches HEAD ✓ (the reorganized layout)
- Working tree still has old layout ✗
- `git diff` shows working tree vs index = massive diff undoing the reorg

## Impact

### No data was lost
- All commits are intact in git history
- The reorganization commit `61950c3` is correct and was pushed to origin
- Working tree changes are just stale copies, not lost work

### New work exists in working tree
~1,800 lines of genuinely new work were created after the absorption, in the stale working tree:

| File | Lines | New? |
|------|-------|------|
| `cli/src/tasks/templates/data_source.rs` | 235 | Yes — never in any commit |
| `cli/docs/tasks/templates.md` | 329 | Yes — never in any commit |
| `ops/now/detect-conflicts.md` | 198 | Yes — never in any commit |
| `ops/now/spec-plan-merge.md` | 530 | Yes — never in any commit |
| `ops/now/spec-file-frontmatter.md` | 409 | Yes — was in parent commit but modified |
| `.aiki/templates/aiki/fix/once.md` | 38 | Yes — never in any commit |
| `.aiki/templates/aiki/fix/quality.md` | 49 | Yes — never in any commit |
| `ops/test-prompts.md` | 2 | Yes |
| `test-parent-propagation{2,3,4}.txt` | small | Test artifacts |

### Pre-reorganization artifacts (safe to discard)
These are old flat files from before the reorg, just sitting in the working tree:
- `cli/src/authors.rs`, `blame.rs`, `provenance.rs`, `repo.rs`, `repo_id.rs`, `interpolation.rs`
- `cli/src/commands/spec.rs`
- `.aiki/templates/aiki/spec.md`, `explore/spec.md`, `review/criteria/spec.md`
- `ops/now/` files that were moved to `ops/done/` in `61950c3`
- `ops/research/*.md` files from subdirectories

## Fix Plan

1. **Back up genuinely new files** to a temporary location
2. **Reset working tree** to match HEAD: `git checkout -- .` + remove untracked pre-reorg files
3. **Restore new files** from backup
4. **Pull** the 4 remote commits (investor pitch deck PR)
5. **Commit** the genuinely new work

## Prevention

The absorption code in `cli/src/session/isolation.rs` needs to ensure the main working tree is updated after commits land. Options:
- Run `git checkout HEAD -- .` after absorption to sync working tree
- Run `jj workspace update-stale` on the main workspace
- Detect large diffs between HEAD and working tree post-absorption and warn
- See also: `ops/now/detect-conflicts.md` (related — detecting JJ conflicts during absorption)

## Related

- `ops/now/detect-conflicts.md` — Preventing silent JJ conflicts during absorption
- `cli/src/session/isolation.rs` — Workspace absorption implementation
- Commit `9b7cfa5` — "Fix concurrent workspace absorption dropping changes" (earlier fix attempt)
- Commit `5674072` — "Fix workspace absorption losing file changes" (even earlier fix)

# JJ Repo Cleanup: 2026-03-19 Incident Report

**Date**: 2026-03-19
**Status**: Complete (manual cleanup done)
**Related**: [bug-absorption-concurrency.md](bug-absorption-concurrency.md) (root cause plan)

---

## What Happened

The repo's JJ state degraded over multiple concurrent agent sessions to the point where:

1. **44 orphaned workspaces** accumulated in `jj workspace list` — all from dead sessions with no active PIDs
2. **File writes silently disappeared** — agents wrote files, JJ captured them into commits, but those commits ended up on stranded side branches not reachable from `@`
3. **Divergent operation log** — JJ started erroring with "repo was loaded at operation X, which seems to be a sibling of the working copy's operation Y"
4. **3.8GB of orphaned workspace directories** in `/tmp/aiki/7f50e063/` from sessions that never cleaned up

## Root Cause

The workspace absorption mechanism (`cli/src/session/isolation.rs:absorb_workspace`) uses a two-step rebase to merge workspace changes back into `@`:

1. `jj rebase -b ws_head -d @-` (move workspace chain onto @-)
2. `jj rebase -s @ -d ws_head` (move @ after workspace head)

When multiple sessions absorb sequentially (serialized by file lock), **later absorptions can strand earlier ones' commits**. The `rebase -b` in Step 1 can create sibling branches instead of a linear chain when two workspace chains share the same fork point (`@-` at workspace creation time).

**Example observed:**
```
@  (empty) ← default working copy — DOES NOT have the file
○  (empty) [aiki] — session B's empty change
○  (empty) [aiki] — session B's empty change
│ ○  [aiki] — session A's FILE WRITE (stranded here!)
├─╯
○  [aiki] — common fork point
```

Session A wrote a file. Session B (empty) absorbed after A. B's Step 2 moved `@` to B's head, stranding A's commit on a side branch.

## What Was Cleaned Up (Manual Steps)

### 1. Forgot all orphaned workspaces

```bash
jj --ignore-working-copy workspace list | grep '^aiki-' | awk '{print $1}' | sed 's/:$//' | \
  while read ws; do jj --ignore-working-copy workspace forget "$ws"; done
```

Removed 44 `aiki-*` workspaces. Only `default` remained.

**Why `--ignore-working-copy`:** The divergent operation log caused `jj` commands without this flag to error out. This flag bypasses the working copy check and uses the latest operation directly.

### 2. Fixed divergent operation log

```bash
jj workspace update-stale
```

After forgetting all workspaces, this synced the working copy to the latest operation state, resolving the "sibling operation" error.

### 3. Recovered stranded file commits

Three files from this session were stranded on side branches:

| File | Stranded commit | Fix |
|------|----------------|-----|
| `ops/now/cleanup-extra-output.md` | `omqoylol` | `jj rebase -s ttmysprq -d omqoylol` (rebased empty chain onto file commit) |
| `cli/docs/session-isolation.md` | `znolpkun` | `jj rebase -s rotlzzmo -d znolpkun` (same technique) |
| `ops/now/bug-absorption-concurrency.md` | `vlvryszz` | `jj squash --from vlvryszz --into @` (squashed into working copy) |

**How to find stranded commits:**
```bash
# Find commits containing a specific file
jj log -r 'files("path/to/file.md")' --no-graph

# Check if a commit is in @'s ancestry (empty output = stranded)
jj log -r '<change_id> & ::@' --no-graph

# Show all non-empty dangling heads (potential stranded data)
jj log -r 'heads(all()) ~ @ ~ empty()' --no-graph
```

### 4. Cleaned up workspace temp directories

```bash
rm -rf /tmp/aiki/7f50e063/*/
```

Freed 3.8GB. These directories contained working tree copies for the 44 forgotten workspaces.

## State of the Repo After Cleanup

- **Workspaces:** 1 (`default` only)
- **Working copy:** Clean, all expected files present
- **Operation log:** Healthy, no divergence
- **Temp disk:** ~4KB (down from 3.8GB)
- **Dangling non-empty heads:** ~160 still exist from historical sessions (run `jj log -r 'heads(all()) ~ @ ~ empty()'` to see them). These are old workspace commits that were either already absorbed into `@`'s ancestry (duplicates) or stranded by the concurrency bug. They're harmless — JJ will garbage-collect them eventually — but they do clutter `jj log` output.

## Prevention

The root cause fix is tracked in [bug-absorption-concurrency.md](bug-absorption-concurrency.md). The recommended fix is a post-absorption ancestry check: after Step 2, verify `ws_head` is in `::@` and if not, do a corrective rebase.

Until the fix is implemented, the workarounds are:

1. **Periodically clean orphaned workspaces:**
   ```bash
   jj workspace list | grep '^aiki-' | awk '{print $1}' | sed 's/:$//' | \
     while read ws; do jj workspace forget "$ws"; done
   ```

2. **Check for stranded commits after writing files:**
   ```bash
   jj log -r 'files("path/to/your/file.md") & ::@' --no-graph
   # If empty, the file is stranded. Fix with:
   jj squash --from <stranded-change-id> --into @
   ```

3. **Fix divergent operations:**
   ```bash
   jj workspace update-stale
   # If that doesn't work:
   jj --ignore-working-copy <command>  # bypass the check
   ```

## What Agents Should Know

- **Don't trust that file writes persist across turns.** The absorption bug can strand your commits. If a file you wrote is missing, check `jj log -r 'files("path")'` — it might be on a side branch.
- **The `--ignore-working-copy` flag is your friend** when JJ errors about divergent operations. It bypasses the working copy check.
- **`jj squash --from <change> --into @`** is the safest way to recover a stranded commit into the working copy.
- **Workspace proliferation is a symptom.** If you see dozens of `aiki-*` workspaces in `jj workspace list`, the cleanup code isn't running properly. Safe to forget them all if no sessions are active.

# Empty Change Accumulation on Main Chain

**Date**: 2026-03-19
**Status**: Proposed
**Priority**: P1 — contributes to slowdown, less severe than fan-out

---

## Problem

The core hooks (`cli/src/flows/core/hooks.yaml`) run `jj new` at multiple lifecycle points to give the agent a clean working copy for the next edit. When no edit follows, the empty change persists forever.

**Current count:** ~35,442 empty changes, vs ~7,918 non-empty. That's 82% waste.

### Where empty changes are created

| Hook | Line | Trigger | Frequency |
|------|------|---------|-----------|
| `session.started` | 15 | Every session start | High |
| `session.started` | 30 | After init creates files | Low (once per repo) |
| `change.permission_asked` | 214 | Stashing user changes before write | Medium |
| `change.completed` (write) | 249 | After every file write | Very high |
| `change.completed` (delete) | 261 | After every file delete | Medium |
| `change.completed` (move) | 273 | After every file move | Low |
| `repo.changed` | 322 | When switching repos | Low |

The `change.completed` `jj new` is the biggest contributor. Every single file write creates one. If the agent writes 5 files in a session, that's 5 non-empty changes interleaved with 5 empty ones. Plus the session.started one. So a 5-write session creates at minimum 6 empty changes.

### Why this matters

- Workspace absorption (`absorb_workspace`) rebases the workspace chain onto `@-`. If the chain has hundreds of empty changes, each rebase step processes them individually.
- `jj log` output is cluttered — hard to find real changes.
- JJ operations (rebase, squash, abandon) all scale with change count.

---

## Root Cause

The `jj new` after `change.completed` is structurally necessary — it separates each file mutation into its own change for provenance tracking. The problem is that the empty change it creates **sticks around even when the next operation also creates a change**.

Example flow:
```
Write file A → change.completed fires → jj metaedit (metadata) → jj new (creates empty)
Write file B → change.permission_asked fires → ... → change.completed fires → jj metaedit → jj new (another empty)
```

The empty change between write A and write B serves no purpose. It was created as a "clean slate" but immediately got content from write B. Except write B's content goes into a *new* change (because `jj new` was called), so the empty one between them just sits there.

---

## Solution: Abandon Empty Predecessors

Instead of preventing empty changes (which would require restructuring the entire hook flow), clean them up at the point where they're no longer needed.

### Approach A: Abandon empty `@-` at session end (simplest)

Add a cleanup step to `session.ended` in hooks.yaml:

```yaml
session.ended:
    # ... existing absorb logic ...

    # Clean up empty changes left by jj new between edits
    - jj: abandon 'ancestors(@) & empty() & ~root()' --ignore-working-copy
```

This abandons all empty ancestors of the current working copy in one shot at session end. JJ's `abandon` on empty changes is safe — it just removes the change from the graph and rebases children onto its parent. Since the changes are empty, the rebase is a no-op for file content.

**Pros:** Simple, one-line addition. No risk during session.
**Cons:** Empty changes accumulate during the session. Absorption during the session still processes them. Cleanup only happens at graceful session end — crashes leave empties behind.

### Approach B: Abandon empty `@-` in `change.completed` (proactive)

After the `jj new` in `change.completed`, check if `@-` is empty and abandon it:

```yaml
change.completed:
    - if: event.write
      then:
          # ... existing metadata logic ...
          - jj: new

          # Clean up the empty predecessor if it has no content
          - jj: log -r "@-- & empty()" --no-graph -T "change_id" --limit 1
            alias: empty_predecessor
          - if: empty_predecessor
            then:
                - jj: abandon "@--" --ignore-working-copy
```

Wait — this is tricky. After `jj new`, `@` is the new empty change and `@-` is the change we just wrote metadata to (non-empty). The empty "gap" change is `@--` (grandparent). So we'd need to check `@--`.

Actually, let's trace more carefully:

1. Agent writes file → JJ snapshots it into `@`
2. `change.completed` fires
3. `jj metaedit` adds metadata to `@` (now non-empty with both file changes and metadata)
4. `jj new` creates fresh `@` (empty), old `@` becomes `@-` (non-empty)
5. Next agent write goes into this new `@`

So after step 4, the chain is: `... → @- (has content) → @ (empty)`. The empty `@` will get content when the agent writes next. There's no "gap" empty change at this point.

The gap empty changes come from a different scenario:

1. `session.started` → `jj new` → empty `@`
2. Agent writes file → content goes into `@` (now non-empty)
3. `change.completed` → `jj new` → new empty `@`, old becomes `@-` (non-empty)
4. Agent writes another file → content goes into `@` (now non-empty)
5. `change.completed` → `jj new` → new empty `@`
6. Session ends — that last empty `@` stays

So in normal operation, there's only **one trailing empty change per session** (the one left by the last `jj new`). The 35K empty changes suggest a different source — let me reconsider.

Going back to the data: the `empty()` revset found 35,442 across the whole repo. But earlier I checked:
```
jj log -r 'empty() & ~description("[aiki-task]") & ~description("[aiki-conv]")' → 9
```

Only **9 empty changes** on the main chain! The other ~35K are the task/conversation fan-out heads (which are fileless by design — metadata only in descriptions, JJ considers them "empty" because they have no file tree changes).

### Revised Understanding

The main chain is actually fine — only 9 empty changes. **The overwhelming majority of "empty" changes are the 30K+ task event fan-out heads.** Those are addressed by the task-branch-fan-out plan.

The 9 empty changes on the main chain are minor but worth cleaning up:

```yaml
session.ended:
    # Clean up trailing empty changes from jj new
    - jj: abandon 'ancestors(@) & empty() & ~root()' --ignore-working-copy
```

### Approach C: Conditional `jj new` in hooks

Only create a new change if the current `@` is non-empty:

```yaml
change.completed:
    - if: event.write
      then:
          # ... metadata logic ...

          # Only create new change if current @ has content
          - jj: diff -r @ --name-only
            alias: has_changes
          - if: has_changes
            then:
                - jj: new
```

This prevents empty changes from being created in the first place. If `@` is already empty (e.g., from a previous `jj new` that wasn't followed by a write), skip the `jj new`.

**Pros:** Prevents the problem at source.
**Cons:** More complex hook logic. Edge cases around metadata-only changes.

---

## Recommended Implementation

Given the revised data (only 9 empties on main chain), this is lower priority than the task-branch-fan-out fix. But still worth doing:

### Phase 1: Add session-end cleanup (low effort, safe)

In `cli/src/flows/core/hooks.yaml`, add to `session.ended`:

```yaml
session.ended:
    # Absorb workspace changes
    - let: absorb_result = self.workspace_absorb_all
    - if: absorb_result != "ok" and absorb_result != "0" and absorb_result
      then:
          - log: "..."

    # Clean up empty changes in ancestry
    - jj: abandon 'ancestors(@) & empty() & ~root()' --ignore-working-copy
```

### Phase 2: Conditional `jj new` (medium effort, prevents accumulation)

Change the `change.completed` handlers to only create a new change when `@` has content. This requires adding a `jj diff` check before each `jj new`:

```yaml
# After metadata is set, only create new change if @ has file content
- jj: diff -r @ --name-only --ignore-working-copy
  alias: current_has_files
- if: current_has_files
  then:
      - jj: new
```

Apply to all three `jj new` sites in `change.completed` (write, delete, move).

### Phase 3: One-time cleanup of existing empties

```bash
jj abandon 'ancestors(@) & empty() & ~root()' --ignore-working-copy
```

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/flows/core/hooks.yaml` | Add `jj abandon` to `session.ended`; optionally add conditional `jj new` to `change.completed` |

---

## Dependency

This plan is **independent of** but **less impactful than** `task-branch-fan-out.md`. The fan-out fix eliminates 30K+ heads (the primary performance problem). This plan addresses ~9 empty changes on the main chain (cosmetic/minor performance).

**Implement `task-branch-fan-out.md` first.** This plan is a nice-to-have cleanup.

---

## Risks

- `jj abandon` on empty changes is safe by definition — no file content is lost
- If `ancestors(@) & empty()` accidentally matches a change that *should* exist (e.g., a deliberately empty change used as a separator), it gets abandoned. Mitigable by adding `& ~description("[aiki]")` to skip metadata-bearing changes.
- The conditional `jj new` approach could skip necessary change boundaries in edge cases. Test thoroughly with the split/metaedit flow.

---

## Success Criteria

- Zero empty changes accumulate during normal operation
- `jj log` shows only meaningful changes
- No regressions in provenance tracking (metadata still correctly attributed)

# Step 1c: Wrap `session.started` `jj new` in Mutex

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 1 — Stop the bleeding (prevent data loss now)
**Source**: Session-start race investigation (2026-03-20)
**Depends on**: Step 1b (needs `mutex` YAML primitive)

---

## Problem

A race condition between `session.started` and workspace absorption causes all workspace changes to silently strand on sibling branches.

### The race

`absorb_workspace()` does two sequential rebases:

```
Step 1: jj rebase -b ws_head -d @-     ← move workspace chain onto @-
Step 2: jj rebase -s @ -d ws_head      ← move @ on top of workspace chain
```

`session.started` runs `jj new --ignore-working-copy` to create a fresh `@`. If a new agent session starts while another agent's absorption is between step 1 and step 2:

```
Timeline:
  Agent A absorption step 1:  rebase ws_head onto @-     ✓ works
  Agent B session.started:    jj new --ignore-working-copy
                              → creates NEW @ forking from @-
  Agent A absorption step 2:  rebase @ onto ws_head
                              → rebases the WRONG @ (or the old one is gone)
```

The absorb lock serializes concurrent absorptions, but `session.started`'s `jj new` runs **outside** the lock. The lock protects step 1 + step 2 from each other, but not from a `jj new` slipping in between.

---

## Fix: Wrap `jj new` in the Absorption Mutex

**File:** `cli/src/flows/core/hooks.yaml`

```yaml
session.started:
    # Hold the workspace-absorption lock during jj new to avoid racing
    # with concurrent workspace absorptions that modify @.
    - mutex:
        workspace-absorption:
          - jj: new --ignore-working-copy
    - shell: aiki init --quiet
```

`absorb_workspace()` already acquires the same `workspace-absorption` lock internally (via `acquire_named_lock` from Step 1a), so the two operations serialize correctly — `jj new` cannot slip between absorption step 1 and step 2.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/flows/core/hooks.yaml` | Wrap `session.started`'s `jj new` in `mutex: workspace-absorption:` |
| `cli/docs/customizing-defaults.md` | Add `mutex` to Actions table, add Concurrency section with usage example |
| `cli/docs/session-isolation.md` | Update lock file references from `.absorb.lock` to `.workspace-absorption.lock` |

---

## Implementation Steps

1. Update `hooks.yaml` to wrap `jj new` in `mutex: workspace-absorption:`
2. Update documentation references
3. Test: run two concurrent agent sessions and verify `jj new` doesn't race with absorption
4. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`

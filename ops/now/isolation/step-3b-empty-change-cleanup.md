# Step 3b: Empty Change Cleanup

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 3 — Performance (make everything fast enough to not timeout)
**Source**: Empty change accumulation investigation (2026-03-19)
**Depends on**: Nothing (Step 3a shipped — fan-out heads eliminated, only ~9 empties remain on main chain)

---

## Problem

The core hooks (`cli/src/flows/core/hooks.yaml`) run `jj new` at multiple lifecycle points to give the agent a clean working copy for the next edit. When no edit follows, the empty change persists forever.

After Step 3a fixes the 30K+ task fan-out heads, only ~9 empty changes remain on the main chain. These are cosmetic but worth cleaning up:

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

The `jj new` after `change.completed` is structurally necessary — it separates each file mutation into its own change for provenance tracking. The empty change left by the last `jj new` in a session is the residual.

---

## Fix: Abandon Empty Ancestors at Session End

Add a cleanup step to `session.ended` in `hooks.yaml`:

```yaml
session.ended:
    # ... existing absorb logic ...

    # Clean up empty changes in ancestry
    - jj: abandon 'ancestors(@) & empty() & ~root()' --ignore-working-copy
```

This abandons all empty ancestors of the current working copy in one shot at session end. JJ's `abandon` on empty changes is safe — it removes the change from the graph and rebases children onto its parent. Since the changes are empty, the rebase is a no-op for file content.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/flows/core/hooks.yaml` | Add `jj abandon` line to `session.ended` (~5 lines) |

---

## Risks

- `jj abandon` on empty changes is safe by definition — no file content is lost
- If `ancestors(@) & empty()` accidentally matches a deliberately empty change used as a separator, it gets abandoned. Mitigable by adding `& ~description("[aiki]")` to skip metadata-bearing changes if needed.

---

## Implementation Steps

1. Open `cli/src/flows/core/hooks.yaml`
2. Find the `session.ended` section
3. Add the `jj abandon` command after the existing absorb logic
4. Run `cargo test` to verify no regressions
5. Test manually: start a session, write a file, end the session, verify no trailing empty changes in `jj log`
6. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`

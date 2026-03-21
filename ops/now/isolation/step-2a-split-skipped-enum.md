# Step 2a: Split `AbsorbResult::Skipped` into `Skipped` / `Empty`

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 2 — Fix the absorption mechanics
**Source**: "Always Absorb" design (2026-03-19) — Rule 1
**Depends on**: Step 1d (the guard logic needs to distinguish the two cases)

---

## Problem

The current `AbsorbResult::Skipped` enum conflates "nothing to absorb" (safe to clean up) with "couldn't absorb" (NOT safe to clean up). Splitting it lets the caller make correct cleanup decisions. This is the structural fix that makes Step 1b's guards precise rather than heuristic.

---

## Fix: Split the Enum

Currently `Skipped` means both "nothing to absorb" (safe) and "couldn't absorb" (NOT safe). Split the enum:

```rust
pub enum AbsorbResult {
    /// Workspace changes absorbed into target
    Absorbed,
    /// Workspace had no file changes (safe to clean up)
    Empty,
    /// Absorption was skipped but workspace may have changes (NOT safe to clean up)
    Skipped { reason: String },
}
```

- `Empty` → safe to cleanup (root change, empty diff)
- `Skipped` → NOT safe to cleanup without checking for files first
- `Absorbed` → safe to cleanup (changes are in target)

### Return `Empty` for safe cases

In `absorb_workspace`, return `Empty` when:
- Workspace head is root/zero change ID (isolation.rs:381-384)
- Workspace head has no file changes (empty diff)

### Return `Skipped { reason }` for unsafe cases

Return `Skipped` with a reason when:
- Workspace not found in `jj workspace list` (isolation.rs:354-361) — workspace may have files on disk even if JJ doesn't see it
- Any other case where absorption is skipped but files might exist

### Update callers

In `functions.rs`, update the match on `AbsorbResult`:

```rust
match absorb_result {
    Ok(AbsorbResult::Absorbed) => {
        let _ = cleanup_workspace(&repo_root, &workspace);
    }
    Ok(AbsorbResult::Empty) => {
        let _ = cleanup_workspace(&repo_root, &workspace);
    }
    Ok(AbsorbResult::Skipped { reason }) => {
        // From Step 1b: check for files before cleanup
        if workspace_has_changes(&workspace.path) {
            eprintln!("[aiki] Absorption skipped ({}), preserving workspace with changes", reason);
            fallback_copy_files(&workspace.path, &repo_root)?;
        }
        let _ = cleanup_workspace(&repo_root, &workspace);
    }
    Err(e) => {
        // From Step 1b: check for files before cleanup
        if workspace_has_changes(&workspace.path) {
            eprintln!("[aiki] Absorption failed ({}), copying files as fallback", e);
            fallback_copy_files(&workspace.path, &repo_root)?;
        }
        let _ = cleanup_workspace(&repo_root, &workspace);
    }
}
```

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Add `Empty` variant to `AbsorbResult`; update return sites to use `Empty` vs `Skipped { reason }` |
| `cli/src/flows/core/functions.rs` | Update match arms on `AbsorbResult` to handle `Empty` and `Skipped { reason }` separately |

---

## Implementation Steps

1. Add `Empty` variant and `reason: String` field to `Skipped` in `AbsorbResult` enum
2. In `absorb_workspace`, change safe-skip returns to `AbsorbResult::Empty`
3. Change unsafe-skip returns to `AbsorbResult::Skipped { reason: "...".into() }`
4. Update all match arms in `functions.rs` to handle the three variants
5. Ensure `Empty` → cleanup, `Skipped` → guard check (from Step 1d) → cleanup
6. Run `cargo test` to verify no regressions
7. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`

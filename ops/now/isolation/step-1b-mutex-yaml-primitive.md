# Step 1b: `mutex` YAML Primitive for Flow Engine

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 1 — Stop the bleeding (prevent data loss now)
**Source**: Session-start race investigation (2026-03-20) — Phase 0
**Depends on**: Step 1a (needs `acquire_named_lock` from fd-lock)

---

## Problem

The session-start race fix (step-1c) requires wrapping `session.started`'s `jj new` in the workspace-absorption lock. To express this cleanly in YAML hooks, we need a `mutex` action in the flow engine that acquires a named file lock, runs nested steps, and releases on scope exit.

Without this primitive, the only way to serialize `jj new` with absorption is to hardcode it in Rust — which defeats the purpose of the YAML-driven flow engine.

---

## Fix: Add `mutex` Action to Flow Engine

A `mutex` action acquires a named file lock, runs nested steps, and releases on scope exit (success or failure). This replaces a bare `lock:`/`unlock:` pair which would be error-prone (unlock skipped on step failure).

### Syntax

```yaml
- mutex:
    workspace-absorption:
      - jj: new --ignore-working-copy
```

**Lock identity:** The key (`workspace-absorption`) maps to a named file lock at `/tmp/aiki/{repo-id}/.workspace-absorption.lock` via `acquire_named_lock` from Step 1a. Multiple distinct lock names can coexist.

### Type changes

**File:** `cli/src/flows/types.rs`

```rust
/// Lock-guarded action block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutexAction {
    /// Map of lock_name → steps. Only one key expected.
    pub mutex: HashMap<String, Vec<Action>>,

    #[serde(default)]
    pub on_failure: OnFailure,
}

// Add variant to Action enum:
// Mutex(MutexAction),
```

### Engine changes

**File:** `cli/src/flows/engine.rs`

1. Add `Action::Mutex` match arm
2. Resolve lock path from session's repo root + lock name
3. Call `acquire_named_lock(repo_root, lock_name)` (from Step 1a)
4. Execute inner steps with existing `execute_actions` recursion
5. Lock guard drops on scope exit — even on step failure
6. Propagate the last step's result as the `mutex` result

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/flows/types.rs` | Add `MutexAction` struct and `Action::Mutex` variant |
| `cli/src/flows/engine.rs` | Add `Action::Mutex` match arm, lock acquire/release around nested steps |

---

## Implementation Steps

1. Add `MutexAction` struct to `types.rs`
2. Add `Action::Mutex(MutexAction)` variant to `Action` enum
3. In `engine.rs`, add match arm for `Action::Mutex`:
   - Extract lock name and nested steps from the single-key HashMap
   - Call `acquire_named_lock(repo_root, lock_name)` to get the guard
   - Call `execute_actions` on the nested steps
   - Guard drops automatically on scope exit
4. Run `cargo test` to verify no regressions
5. Write a test hook that uses `mutex:` to verify serialization

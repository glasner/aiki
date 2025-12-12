# Fix Plan Review — 2025-12-12

Review of `ops/current/fix.md` for correctness, consistency, and completeness.

---

## Critical Issues

### 1. Fix 2a / Fix 4 API Inconsistency

**Location:** `fix.md:105-127` vs `fix.md:261,288`

Fix 2a proposes adding a `collect_timing` parameter to the existing `execute_statements`:

```rust
pub fn execute_statements(
    statements: &[FlowStatement],
    state: &mut AikiState,
    collect_timing: bool,  // New parameter
) -> Result<(FlowResult, FlowTiming)>
```

But Fix 4 still references a non-existent `execute_statements_fast`:

```rust
let result = FlowEngine::execute_statements_fast(section_fn(core_flow), &mut state)?;
```

**Impact:** Fix 4 won't compile as written.

**Resolution:** Update Fix 4 to call `execute_statements(..., false)` instead of `execute_statements_fast`.

---


### 3. Fix 2b Borrow Checker Conflict

**Location:** `fix.md:147-165`

The proposed pattern won't compile:

```rust
fn create_resolver(state: &mut AikiState) -> VariableResolver {
    // ...
    for (k, v) in state.get_event_vars() {  // &mut borrow starts here
        resolver.add_var(k.clone(), v.clone());
    }
    // &mut borrow still active if get_event_vars returns &HashMap

    for (k, v) in state.iter_variables() {  // ERROR: can't borrow again
        resolver.add_var(k.clone(), v.clone());
    }
}
```

If `get_event_vars(&mut self)` returns `&HashMap`, the mutable borrow extends through the iteration, blocking `iter_variables()`.

**Resolution options:**
1. Return owned `HashMap` from `get_event_vars()` (clone on first access, store)
2. Use `RefCell<Option<HashMap>>` for interior mutability
3. Separate the operations:
   ```rust
   let event_vars = state.get_event_vars().clone();  // Clone to release borrow
   for (k, v) in &event_vars { ... }
   for (k, v) in state.iter_variables() { ... }
   ```

---

## Medium Issues

### 4. Fix 3 Redundant with Fix 4

**Location:** `fix.md:178-209` vs `fix.md:212-386`

Fix 3 says to fix payload cloning in 3 specific files:
- `pre_file_change.rs:35-37`
- `post_file_change.rs:70-72`
- `session_end.rs:31-33`

But Fix 4 introduces `execute_flow` helpers that **replace the entire handler body**, which would also eliminate the cloning. These fixes overlap.

**Resolution:** Either:
- Merge Fix 3 into Fix 4 (preferred — helpers handle this automatically)
- Or implement Fix 3 only for handlers NOT using the helper

---

### 5. Fix 4 Missing Handler Examples

**Location:** `fix.md:308-376`

The plan lists 7 files to modify (`fix.md:378-385`) but only shows examples for 5 handlers:
- `handle_pre_file_change` ✓
- `handle_post_response` ✓
- `handle_pre_prompt` ✓
- `handle_session_start` ✓
- `handle_prepare_commit_message` ✓
- `handle_post_file_change` ✗ (missing)
- `handle_session_end` ✗ (missing)

**Resolution:** Add examples for `post_file_change` and `session_end`. Note that `post_file_change` has special logic for user edit detection (`cli/src/events/post_file_change.rs:67-81`) that may not fit the helper pattern.

---

### 6. Fix 5a VendorResponse Control Flow

**Location:** `fix.md:397-425`

The proposed `VendorResponse::print_and_exit(self) -> !` requires restructuring vendor code. Current pattern in `cli/src/vendors/claude_code.rs`:

```rust
pub fn handle() -> Result<()> {
    // ... build response ...
    response.print_json();  // Prints and returns
    Ok(())  // Returns to caller
}
```

New pattern with `-> !`:
```rust
pub fn handle() -> ! {
    // ... build response ...
    response.print_and_exit()  // Never returns
}
```

This changes the signature from `Result<()>` to `!`, affecting:
- `cli/src/commands/hooks.rs` dispatch logic
- Error handling (no `?` after the call)

**Resolution:** Either:
- Keep `-> Result<()>` and add `exit()` at call site
- Or update all callers to handle the `-> !` signature

---

### 7. Fix 1 Unused Import

**Location:** `fix.md:21`

```rust
use std::collections::HashMap;
```

This import is listed but `HashMap` is no longer used after removing `ENV_VARS` cache. The comment correctly says not to cache env vars, but the import remains.

**Resolution:** Remove the unused import from the code snippet.

---

## Minor Issues

### 8. Fix 4 Import Path

**Location:** `fix.md:256`

```rust
let core_flow = cache::get_core_flow();
```

From within `cli/src/events/mod.rs`, this should be:
```rust
let core_flow = crate::cache::get_core_flow();
```

---

### 9. Perf #6 Missing from Cross-Reference

**Location:** `fix.md:587-607`

The original `review.md` had:
> 6. **(Optional) Make benchmark output leverage the above to validate wins**

This item doesn't appear in the cross-reference table or any fix. Intentional omission or oversight?

**Resolution:** Either add to Fix 9 scope or explicitly note as "deferred/optional".

---

### 10. PostResponse Graceful Handler Should Check FlowResult

**Location:** `fix.md:324-336`

The `handle_post_response` example always returns `Decision::Allow`:
```rust
Ok(HookResult {
    context: output.context,
    decision: Decision::Allow,  // Always Allow
    failures: output.failures,
})
```

But the current code (`cli/src/events/post_response.rs:66-71`) also always returns `Allow`. This is correct — PostResponse shouldn't block. However, for consistency with other handlers, consider using `output.decision()` even though it will always be `Allow` (documents the pattern).

---

## Summary

| Severity | Count | Key Issues |
|----------|-------|------------|
| Critical | 3 | API inconsistency, signature breakage, borrow checker |
| Medium | 4 | Redundant fixes, missing examples, control flow |
| Minor | 3 | Imports, paths, cross-reference |

**Recommended actions before implementation:**

1. **Fix 2a/4:** Decide on API approach — either:
   - Add `collect_timing` param and update Fix 4 to use it
   - Or keep `execute_statements_fast` as separate function (original plan before revision)

2. **Fix 2b:** Resolve borrow checker issue with concrete solution

3. **Fix 3/4:** Merge or clarify relationship

4. **Fix 4:** Add missing handler examples, especially `post_file_change` which has special logic

5. **Fix 5a:** Clarify control flow change impact on callers

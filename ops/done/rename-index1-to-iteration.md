---
status: draft
---

# Rename loop.index1 to loop.iteration

**Date**: 2026-02-25
**Status**: Draft
**Priority**: P2

## Context

The loop system currently provides:
- `loop.index` (0-based: 0, 1, 2, ...)
- `loop.index1` (1-based: 1, 2, 3, ...)

The name `index1` is confusing and non-intuitive. It should be renamed to `iteration` to be more user-friendly and self-documenting. This aligns with the historical use of `data.loop.iteration` in some templates, which was previously an alias but should now become the canonical name.

## Goal

Rename all occurrences of `loop.index1` to `loop.iteration` throughout:
- Source code (Rust implementation)
- Documentation (ops/now/, ops/done/)
- Tests
- Comments

Keep `loop.index` (0-based) unchanged.

## Scope

### Files to Update

Based on grep results, the following files contain `index1`:

#### Source Code (2 files)
1. **cli/src/tasks/templates/parser.rs** (6 occurrences)
   - Line 106: `"loop.index1".to_string()`
   - Line 107: `"data.loop.index1 + 1".to_string()`
   - Line 132: `"loop.index1".to_string()`
   - Line 877: Comment about loop metadata
   - Line 881: `data.contains_key("loop.index1")`
   - Line 890-891: `template.defaults.data.get("loop.index1")`

2. **cli/src/tasks/templates/types.rs** (3 occurrences)
   - Line 220: Test YAML with `data.loop.index1 >= 10`
   - Line 229: Test assertion for the condition

#### Documentation (3 files)
3. **ops/now/general-loop-termination.md** (~23 occurrences)
   - Throughout the document in examples, explanations, and test descriptions

4. **ops/done/autorun-unblocked-tasks.md** (~15 occurrences)
   - Throughout the document in loop examples and metadata tables

5. **ops/done/rhai-for-conditionals.md** (2 occurrences)
   - Line 12: Comment about loop frontmatter
   - Line 185: Note about loop metadata namespaces

### Naming Convention

**Before:**
```yaml
loop:
  until: subtasks.review.approved or data.loop.index1 >= 10
```
```rust
data.insert("loop.index1".to_string(), serde_json::json!(1));
```

**After:**
```yaml
loop:
  until: subtasks.review.approved or data.loop.iteration >= 10
```
```rust
data.insert("loop.iteration".to_string(), serde_json::json!(1));
```

## Implementation Plan

### Phase 1: Source Code (Rust)

Update the core implementation files that create and use `loop.index1`:

1. **cli/src/tasks/templates/parser.rs**
   - Replace `"loop.index1"` → `"loop.iteration"` (string literals)
   - Replace `data.loop.index1` → `data.loop.iteration` (in expressions and comments)
   - Update test assertions to check for `loop.iteration` instead of `loop.index1`
   - Update comments mentioning `loop.index1`

2. **cli/src/tasks/templates/types.rs**
   - Update test YAML examples to use `data.loop.iteration`
   - Update test assertions

### Phase 2: Documentation

Update documentation files to reflect the new naming:

3. **ops/now/general-loop-termination.md**
   - Replace `loop.index1` → `loop.iteration` throughout
   - Update the note explaining the naming (line 20) to say:
     > The loop system provides `loop.index` (0-based) and `loop.iteration` (1-based). The `loop.iteration` counter is used for iteration-count comparisons since `max_iterations` is human-friendly (1-based).
   - Remove any mentions of `index1` as a historical artifact
   - Note that `loop.iteration` is now the canonical name (no longer an alias)

4. **ops/done/autorun-unblocked-tasks.md**
   - Replace `loop.index1` → `loop.iteration` throughout
   - Update metadata tables to show `iteration` instead of `index1`

5. **ops/done/rhai-for-conditionals.md**
   - Replace `data.loop.index1` → `data.loop.iteration`
   - Update explanatory notes

### Phase 3: Verification

After the main changes:
- Run `cargo build` to ensure compilation succeeds
- Run `cargo test` to ensure all tests pass
- Run `cargo clippy` to check for warnings
- Grep for `index1` (case-insensitive) to find any missed occurrences
- Check for related patterns like `index_1`, `index-1`, etc.

## Test Strategy

### Existing Tests
All existing tests should continue to pass after the rename:
- Parser tests in `cli/src/tasks/templates/parser.rs`
- Type tests in `cli/src/tasks/templates/types.rs`
- Spawn evaluator tests (if they exist in `cli/src/tasks/spawner.rs`)

### Manual Verification
After changes, verify:
1. `cargo build` succeeds
2. `cargo test` passes (all tests)
3. `cargo clippy` shows no warnings related to the changes
4. Search for `index1` returns no results (except in git history/this plan)

## Backward Compatibility

**Decision:** This will be a **breaking change** for any existing templates using `loop.index1`.

**Rationale:**
- This is pre-1.0 software (based on aiki version 1.15 in CLAUDE.md)
- The codebase is under active development
- The change improves clarity significantly
- Migration is straightforward (global search-and-replace)
- The documentation already mentions that `loop.iteration` was historically used

**Note:** The reference in general-loop-termination.md (line 137) to "template resolver's `loop.iteration` alias" suggests this alias may already exist in the template resolver. We should verify this and ensure we're making `loop.iteration` the primary/canonical name everywhere.

## Acceptance Criteria

- [ ] All occurrences of `loop.index1` in Rust source code are renamed to `loop.iteration`
- [ ] All occurrences in documentation are updated
- [ ] All tests pass (`cargo test`)
- [ ] `cargo build` succeeds
- [ ] `cargo clippy` shows no new warnings
- [ ] Search for `index1` in code/docs returns zero results (outside this plan)
- [ ] `loop.index` (0-based) remains unchanged

## Files Changed

### Source Code
- `cli/src/tasks/templates/parser.rs` - Rename field initialization and usage
- `cli/src/tasks/templates/types.rs` - Update test cases

### Documentation
- `ops/now/general-loop-termination.md` - Update all references and clarify naming
- `ops/done/autorun-unblocked-tasks.md` - Update all references
- `ops/done/rhai-for-conditionals.md` - Update all references

## Subtasks

1. Update `cli/src/tasks/templates/parser.rs` (rename all `index1` → `iteration`)
2. Update `cli/src/tasks/templates/types.rs` (update test cases)
3. Run `cargo test` to verify source code changes
4. Update `ops/now/general-loop-termination.md`
5. Update `ops/done/autorun-unblocked-tasks.md`
6. Update `ops/done/rhai-for-conditionals.md`
7. Final verification: grep for `index1`, run full test suite

## Estimated Effort

- **Phase 1 (Source Code):** 30 minutes
  - 2 Rust files, ~9 changes total
  - Run tests after each file
- **Phase 2 (Documentation):** 30 minutes
  - 3 markdown files, ~40 changes total
- **Phase 3 (Verification):** 15 minutes
  - Search for missed occurrences
  - Run full test suite

**Total:** ~75 minutes

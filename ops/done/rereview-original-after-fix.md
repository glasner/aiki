# Add original-scope re-review to quality loop

## Context

The quality loop in `aiki fix` currently re-reviews only the fix-parent's changes after a fix cycle. This verifies the fix resolves the original issues, but doesn't catch regressions introduced in the broader original scope. Adding a second review phase closes this gap: after the fix-parent review passes, re-review the original scope, and if new issues surface, feed them back into the loop.

## Changes

All changes are in `cli/src/commands/fix.rs`.

### 1. `run_quality_loop` (lines 425-436) — two-phase review

Current flow at end of loop iteration:
```
fix-parent review passes → "Approved", return
fix-parent review fails  → set review_id, loop back
```

New flow:
```
fix-parent review fails  → set review_id, continue     (explicit continue)
fix-parent review passes → create + run original-scope review
  original-scope passes  → "Approved", return
  original-scope fails   → set review_id = original-scope review (loop back)
```

Concrete: replace the `if !has_actionable_issues(new_review)` block (lines 430-436) with:
- If fix-parent review **has** issues → `*review_id = ...; continue;`
- Create review with `scope.clone()` as the scope
- Run it with `task_run`
- If no issues → `output_approved`, return
- If issues → `*review_id = original_review_result.review_task_id`

### 2. `run_fix_continue` (lines 329-341) — same two-phase treatment

The inline review + check at lines 329-341 mirrors the quality loop. Apply the same pattern: after fix-parent review passes, re-review original scope before declaring approved or entering the quality loop.

### 3. Doc comments

- Module doc (line 8): add "After fix-parent review passes, re-reviews the original scope to catch regressions"
- `run_quality_loop` doc (line 344): update to mention two-phase review

## What doesn't change

- `ReviewScope` already derives `Clone` — no type changes needed
- `MAX_QUALITY_ITERATIONS` stays at 10 (counts full loop iterations, not individual reviews)
- `--once` still skips all reviews
- No changes to `review.rs`, `runner.rs`, or any other file

## Verify

```bash
cd cli && cargo test -- fix
cargo build
```

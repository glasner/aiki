# Severity-Based Fix Loop Convergence

**Date**: 2026-03-26
**Status**: Draft
**Purpose**: Let the fix loop treat low-severity-only reviews as approved, preventing unbounded cycling on diminishing nits.

**Related Documents**:
- [wont-do-task-list.md](wont-do-task-list.md) — Complementary: surfaces prior exclusions to reviewers
- [smarter-task-diff.md](smarter-task-diff.md) — Root cause of the "unrelated file" issue that drove many low rounds

---

## Executive Summary

The quality loop in `aiki fix` uses a binary decision: any issues → loop back, zero issues → approved. This means a review that finds only low-severity nits (doc comments, naming, style) triggers the same full plan→decompose→loop→review pipeline as a review with a critical bug. In the `tnnuxwm` fix loop, rounds 4-9 were all low/medium refinement — each adding real but diminishing value while consuming agent sessions and tokens. The fix: make the loop severity-aware so low-only reviews are treated as approved.

---

## Problem

### Current behavior

In `cli/src/commands/fix.rs`:

```rust
// Line 892: Binary — any issues = actionable
pub(crate) fn has_actionable_issues(review_task: &Task) -> bool {
    if let Some(issue_count) = review_task.data.get("issue_count") {
        match issue_count.parse::<usize>() {
            Ok(n) => n > 0,  // <-- any count > 0 triggers fix loop
            ...
        }
    } else {
        !review_task.comments.is_empty()
    }
}

// Line 745: Binary decision in determine_review_outcome
if fix_parent_review_has_issues {
    return ReviewOutcome::LoopBack(fix_parent_review_id.to_string());
}
```

The loop has `MAX_QUALITY_ITERATIONS = 10` as a safety valve but no severity-based exit.

### Observed impact (epic `tnnuxwm`)

| Round | High | Medium | Low | Should have looped? |
|-------|------|--------|-----|---------------------|
| 1 | 0 | 0 | 2 | Debatable — one was "no tests" |
| 2 | 0 | 0 | 1 | Debatable — run.rs test coverage |
| 3 | 1 | 0 | 2 | **Yes** — real bug in runner.rs |
| 4 | 0 | 1 | 1 | Maybe — read_events error handling |
| 5 | 0 | 1 | 1 | Maybe — code duplication |
| 6 | 0 | 0 | 3 | **No** — doc comment, log level, unrelated file |
| 7 | 0 | 0 | 2 | **No** — dead code path, unrelated file (again) |
| 8 | 0 | 0 | 2 | **No** — edge case default, unrelated file (again) |
| 9 | 0 | 1 | 0 | Maybe — missing regression test |

Rounds 6-8 were pure overhead — only low issues, all addressable but not worth a full fix cycle. If low-only had been treated as approved, the loop would have converged at round 6 instead of round 9.

---

## Proposed Design

### Severity threshold on the quality loop

Add a **minimum severity** threshold that determines whether issues trigger a loop-back. Reviews with only issues below the threshold are treated as approved.

**Default:** `medium` — issues at `medium` or `high` trigger a loop-back; `low`-only reviews are approved.

**CLI flag:** `--min-severity <level>` on `aiki fix` to override:

```bash
# Default: low-only reviews don't trigger loop-back
aiki fix <review-id>

# Strict: even low issues trigger loop-back
aiki fix <review-id> --min-severity low

# Relaxed: only high issues trigger loop-back
aiki fix <review-id> --min-severity high
```

### Where the data lives

Review issues already carry severity in `comment.data["severity"]` (values: `"high"`, `"medium"`, `"low"`). The review task also has `data["issue_count"]` but this is a total count with no severity breakdown.

**New data fields on review task:**
```
data.issue_count_high = "1"
data.issue_count_medium = "0"
data.issue_count_low = "2"
```

These should be set by the review close flow (when the reviewer closes the review task, tally the issue comments by severity).

### Decision logic change

Replace the binary `has_actionable_issues` with severity-aware logic:

```rust
/// Check if a review has issues at or above the given severity threshold.
pub(crate) fn has_actionable_issues_at_severity(
    review_task: &Task,
    min_severity: Severity,
) -> bool {
    // Try structured counts first
    let high = review_task.data.get("issue_count_high")
        .and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let medium = review_task.data.get("issue_count_medium")
        .and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let low = review_task.data.get("issue_count_low")
        .and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);

    match min_severity {
        Severity::High => high > 0,
        Severity::Medium => high > 0 || medium > 0,
        Severity::Low => high > 0 || medium > 0 || low > 0,
    }
}
```

### What happens to low-only issues

They are **still recorded** on the review — the review summary still says "2 low issues." They just don't trigger a loop-back. This means:

- Low issues are visible in the review task's comments for anyone who checks
- They could be surfaced as suggestions in a future "cleanup" pass
- The reviewer's work isn't wasted — just deferred

### Backward compatibility

`has_actionable_issues` is called from two places:
1. `run_fix` (line 494) — initial short-circuit check before entering the loop
2. `run_quality_loop` (line 817) — loop-back decision after each review

For the initial check (1), keep the existing behavior: any issues at any severity should enter the fix pipeline. The severity threshold only applies to the loop-back decision (2).

Renaming plan:
- Keep `has_actionable_issues` as-is for the entry check
- Add `has_issues_above_threshold` for the loop-back check
- `determine_review_outcome` takes `bool` for "has issues above threshold" instead of "has any issues"

---

## Implementation Plan

### Step 1: Add per-severity issue counts to review close

- **Files:** `cli/src/commands/review.rs`
- **Changes:**
  1. When a review task is closed, tally issue comments by severity
  2. Write `issue_count_high`, `issue_count_medium`, `issue_count_low` to task data
  3. Keep existing `issue_count` as the total for backward compatibility
- **Depends on:** nothing

### Step 2: Add `--min-severity` flag to `aiki fix`

- **Files:** `cli/src/commands/fix.rs`
- **Changes:**
  1. Add `--min-severity` CLI arg (default: `medium`)
  2. Add `has_issues_above_threshold(review_task, min_severity)` function
  3. Thread `min_severity` through `run_quality_loop` and `run_fix_continue`
  4. In `run_quality_loop`, replace `has_actionable_issues(new_review)` with `has_issues_above_threshold(new_review, min_severity)` for the loop-back decision
  5. Keep `has_actionable_issues` for the initial entry check (line 494)
- **Depends on:** step 1 (needs per-severity counts)

### Step 3: Update review template to record severity counts

- **Files:** `.aiki/tasks/review/task.md`
- **Changes:**
  1. In the close instructions, add guidance to set severity counts:
     ```bash
     aiki task set {{id}} --data issue_count_high=X --data issue_count_medium=Y --data issue_count_low=Z
     aiki task close {{id}} --summary "Review complete (N issues: X high, Y medium, Z low)"
     ```
  2. Or better: make `aiki task close` on a review task auto-compute these counts from the issue comments (step 1 handles this)
- **Depends on:** step 1

---

## Testing

- Review with 1 high issue → loops back (all thresholds)
- Review with 1 medium, 0 high → loops back at default threshold; approved at `--min-severity high`
- Review with 2 low, 0 medium, 0 high → approved at default threshold; loops back at `--min-severity low`
- Review with 0 issues → approved (all thresholds)
- `--once` still skips the review entirely (unchanged)
- Initial entry check still uses any-severity (a review with only low issues still enters the pipeline on the first pass)
- Backward compat: reviews without `issue_count_high/medium/low` fall back to counting issue comments by their `data.severity` field

---

## Open Questions

1. **Should medium be the right default?** An argument for `high`: medium issues like "missing test coverage" are worth fixing but maybe not worth a full loop-back cycle. Counter-argument: medium issues like "error handling silently drops failures" (round 4) are real bugs that should loop.

2. **Should the threshold escalate per round?** E.g., round 1-2 loops on medium+, round 3+ only loops on high. This would handle the case where early medium issues are worth fixing but later medium issues are diminishing returns. Adds complexity though.

3. **Auto-compute vs template-driven counts?** Step 1 proposes auto-computing counts on close. Alternative: require the reviewer to set them explicitly via `aiki task set`. Auto-compute is more reliable but requires the close flow to walk the comments.

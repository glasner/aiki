# Regression Review Scoping

**Date**: 2026-03-26
**Status**: Draft
**Purpose**: Scope the regression review to only fix-round changes (not the full epic), and use a narrower regression-focused template to prevent re-discovering known issues.

**Related Documents**:
- [skip-low-issues.md](skip-low-issues.md) — Severity threshold for loop convergence
- [wont-do-task-list.md](wont-do-task-list.md) — Won't-do visibility for reviewers

---

## Executive Summary

The quality loop's regression review re-examines the entire original scope, including the initial implementation and inherited workspace state. This causes it to find new issues unrelated to the fix rounds, triggering more cycles. The fix: scope the regression review to only the accumulated diff of fix rounds, and use a `review/regression` template with criteria limited to regressions and correctness — not style, docs, or design.

---

## Problem

### Current flow in `run_quality_loop` (fix.rs:803-862)

```
1. Per-round review: review fix-parent's changes → pass/fail
2. If pass → regression review: re-review ORIGINAL SCOPE → pass/fail
3. If regression review finds issues → loop back
```

The regression review at step 2 calls `create_review` with the original epic's `ReviewScope`. The reviewer then runs `aiki task diff <epic-id>`, which includes:
- The initial implementation subtasks
- All fix rounds
- Inherited workspace state (e.g., unrelated files from prior sessions)

This is far broader than what the regression review needs to check. It should only see what the fix rounds changed.

### Observed impact (epic `tnnuxwm`)

Epic-level reviews (the regression reviews) kept finding:
- "Unrelated file in diff" — 5 times (inherited workspace state)
- New low-severity nits in code the per-round reviews had already vetted
- Occasionally legitimate cross-cutting concerns

The per-round reviews were tight and useful. The regression reviews were redundant and noisy.

---

## Proposed Design

### Three changes

1. **`--fixes` filter on `aiki task list`** — find tasks linked via `fixes → <target>`
2. **`aiki task diff` accepting multiple task IDs** — combine diffs from several tasks
3. **`review/regression` template** — regression-only criteria, scoped to fix diffs

### How they compose

The regression review's explore step becomes:

```bash
# Get IDs of all fix-round tasks targeting this epic
FIX_IDS=$(aiki task list --fixes {{data.scope.id}} -o id)

# Diff only those tasks' changes (not the full epic)
aiki task diff $FIX_IDS
```

This gives the reviewer exactly the surface area where regressions could exist — the fix rounds' accumulated diff — without the initial implementation or inherited workspace noise.

---

## User Experience

### `--fixes` flag on `aiki task list`

```bash
# Find all tasks that fix this epic
aiki task list --fixes tnnuxwm

# Combine with other filters
aiki task list --fixes tnnuxwm --done        # completed fixes only
aiki task list --fixes tnnuxwm --wont-do     # dismissed fixes
aiki task list --fixes tnnuxwm -o id         # bare IDs for piping
```

Follows the same pattern as `--source`: takes a target value, supports partial matching, walks the `fixes` edges in the task graph.

### Multi-task `aiki task diff`

```bash
# Diff a single task (existing)
aiki task diff abc123

# Diff multiple tasks (new — combined net diff)
aiki task diff abc123 def456 ghi789

# Piped from task list
aiki task diff $(aiki task list --fixes tnnuxwm -o id)
```

The multi-task diff produces the **combined net diff** — the union of all changes across the listed tasks, deduplicated by file. If task A changes line 10 and task B changes line 20 of the same file, the combined diff shows both.

---

## Implementation Plan

### Step 1: Add `--fixes` filter to `aiki task list`

- **Files:** `cli/src/commands/task.rs`
- **Changes:**
  1. Add `fixes: Option<String>` to the `List` CLI struct (next to `--source`)
  2. Add `matches_fixes` closure in `run_list`, identical pattern to `matches_source` but walking `"fixes"` edges instead of `"sourced-from"`
  3. Add to the filter chain alongside existing filters
  4. Update help text
- **Depends on:** nothing

### Step 2: Support multiple task IDs in `aiki task diff`

- **Files:** `cli/src/commands/task.rs` (the `diff` subcommand)
- **Changes:**
  1. Change the `ID` argument from single value to `Vec<String>`
  2. For each task ID, resolve the JJ revset for that task's changes
  3. Combine revsets with `|` (union) and run a single `jj diff` against the combined set
  4. Single-ID case unchanged (backward compatible)
- **Depends on:** nothing (parallel with step 1)

### Step 3: Create `review/regression` template

- **Files:** `.aiki/tasks/review/regression.md` (new)
- **Changes:**
  1. Same two-subtask structure as `review/task` (Explore Scope → Review & Record Issues)
  2. Explore step uses `--fixes` + multi-task diff instead of `aiki task diff {{data.scope.id}}`
  3. Review criteria limited to:
     - **Regressions** — Did the fixes break existing behavior?
     - **Correctness** — Are the fixes themselves correct?
     - **Consistency** — Do the fixes contradict each other or the original implementation?
  4. Explicitly excluded:
     - Style, naming, documentation
     - Design improvements or refactoring suggestions
     - Issues in the original implementation (already reviewed)
     - Issues previously closed as won't-do
- **Depends on:** steps 1 and 2

### Step 4: Wire regression template into the quality loop

- **Files:** `cli/src/commands/fix.rs`
- **Changes:**
  1. In `run_regression_review_step` (line 244), pass `template: Some("review/regression".to_string())` to `create_review` instead of `None`
  2. This makes the regression review always use the narrower template
  3. The per-round review continues using `review/task` (full criteria)
- **Depends on:** step 3

---

## Testing

- `aiki task list --fixes <id>` returns tasks with `fixes → <id>` links
- `aiki task list --fixes <id>` with partial ID matching works
- `aiki task list --fixes <id>` returns empty when no fixes exist
- `aiki task diff <id1> <id2>` produces combined diff
- `aiki task diff <id1> <id2> --summary` works
- `aiki task diff $(aiki task list --fixes <id> -o id)` end-to-end pipeline works
- Regression review uses `review/regression` template (not `review/task`)
- Regression review sees only fix-round changes, not full epic diff

---

## Open Questions

1. **Should `--fixes` also match transitively?** Fix round 3 might fix the epic directly, but fix round 3's subtasks fix round 3. For `aiki task list --fixes <epic-id>`, should it return only direct fixers or walk the subtree? Direct is simpler and probably sufficient since fix rounds link directly to the epic.

2. **Multi-task diff deduplication** — If two fix rounds touch the same lines, does the combined diff show the net result or each round's changes separately? Net result (single `jj diff` over the union revset) is more useful for regression review since you see the final state, not intermediate steps.

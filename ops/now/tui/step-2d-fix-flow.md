# Step 2d: `aiki fix` Flow

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 2 — Pipeline flows
**Depends on**: 2b (reuses build components), 2c (reuses review rendering)

---

## Problem

`aiki fix <review-id>` runs its own pipeline: fix-plan → decompose → loop → review → regression review. It reuses build and review components but has its own orchestration (quality loop with up to 10 iterations).

---

## Fix: Fix-specific pipeline builder

### Screen states (from [screen-states.md](screen-states.md), Flow 8)

States 8.0-8.8: starting → fix plan active → done → decompose → followup table → loop active → review of fixes → regression review → completed.

Special state 8.8 (no actionable issues):
```
[80 cols]
 合 fix (claude)                                          ← dim (completed phase)
 ⎿ ✓ approved — no actionable issues                     ← dim ⎿, green ✓, dim text
```

### Implementation

```rust
pub fn view(graph: &TaskGraph, fix_parent_id: &str, review_id: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let mut group = 0;

    // Short-circuit: no actionable issues
    if no_actionable_issues(graph, review_id) {
        lines.extend(components::phase(group, "fix", Some("claude"), vec![
            ChildLine::done("✓ approved — no actionable issues", None),
        ]));
        return lines;
    }

    // Fix plan phase
    lines.extend(components::phase(group, "fix", fix_task.agent_label(), vec![/* status */]));
    group += 1;

    // Decompose + subtask table + loop (reuses same pattern as build::view())
    // ...

    // Review of fixes
    if let Some(review) = find_fix_review(graph, fix_parent_id) {
        lines.extend(review_phase_lines(group, review));
        group += 1;
    }

    // Regression review
    if let Some(regression) = find_regression_review(graph, review_id) {
        lines.extend(components::phase(group, "review for regressions", regression.agent_label(), vec![/* status */]));
    }

    lines
}
```

### Quality loop rendering

When the fix review finds more issues, a new iteration starts. The `build::view()` function (step 2b) handles iteration headers — it's graph-driven and renders fix/iteration sections when the graph contains them:

```rust
// In build::view() (step 2b):
lines.extend(components::section_header(&format!("Iteration {}", iteration)));
lines.extend(fix::view(graph, fix_parent_id, review_id, window));
```

### Integration point

Wiring into `fix.rs` happens in Phase 3 (cutover). This step only creates the pure view function.

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/fix.rs` | New: `view()` (~120 lines) |

### Tests

- Snapshot: no-actionable-issues shortcut.
- Snapshot: single iteration (fix → decompose → loop → review approved).
- Snapshot: multi-iteration with `Iteration 2` header.
- Snapshot: max iterations warning.

# Step 2e: Static View Functions (epic show, review show)

**Date**: 2026-03-22
**Status**: Ready
**Priority**: P1
**Phase**: 2 — Pipeline flows
**Depends on**: 1c (shared components)

> **Note:** This step was formerly "build-fix flow". That logic has been absorbed into step 2b (`build::view()`) which is graph-driven — it renders fix/iteration sections when the graph contains them. No separate `build_fix_lines()` or `BuildFix` variant needed.

---

## Problem

`aiki epic show` and `aiki review show` (with `--show` or similar subcommands) render a static detail view — not a live-polling TUI. They need their own view functions and `Screen` variants because they have different exit semantics (render once and exit, vs polling until task completes).

---

## Fix: Static view functions

### `epic_show::view()`

Renders epic detail: phase header + subtask table. Used by `aiki epic show <id>`.

```rust
pub fn view(graph: &TaskGraph, epic_id: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    // Epic header phase
    lines.extend(components::phase(0, &epic_name, epic.agent_label(), vec![/* status */]));
    // Subtask table
    let data: Vec<SubtaskData> = get_subtasks(graph, epic_id).iter().map(|s| s.into()).collect();
    lines.extend(components::subtask_table(1, &short_id, &epic_name, &data, false));
    lines
}
```

### `review_show::view()`

Renders review detail: phase header + issues list. Used by `aiki review show <id>`.

```rust
pub fn view(graph: &TaskGraph, review_id: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    // Review header phase
    lines.extend(components::phase(0, "review", review.agent_label(), vec![/* status */]));
    // Issues
    if let Some(issues) = get_review_issues(graph, review_id) {
        lines.extend(components::issues(1, &issues));
    }
    lines
}
```

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/epic_show.rs` | New: `view()` (~30 lines) |
| `cli/src/tui/screens/review_show.rs` | New: `view()` (~30 lines) |

### Tests

- Snapshot: epic with all subtasks done.
- Snapshot: epic with mixed status subtasks.
- Snapshot: review with issues.
- Snapshot: review approved (no issues).

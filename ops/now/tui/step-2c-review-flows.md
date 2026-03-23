# Step 2c: `aiki review` Flows

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 2 — Pipeline flows
**Depends on**: 1a, 1b, 1c (independent of 2a/2b but easier after them)

---

## Problem

Four review scope variants (`plan`, `code`, `task`, `session`) share the same rendering structure but differ in header text and result content. Currently they're all handled by the same generic chat builder path.

---

## Fix: Single review pipeline builder with scope parameter

### Screen states (from [screen-states.md](screen-states.md), Flows 4-7)

All four scopes follow the same shape:

```
[80 cols]
 ⠹ review <scope> (<agent>)                              ← yellow spinner (active) / 合 (done), fg name, dim agent
 ⎿ <heartbeat or result>                                  ← dim ⎿, yellow text (active) / dim text (done)
                                                          ← blank row
     1. <issue>                                           ← fg number, fg text (never dimmed)
     2. <issue>                                           ← fg number, fg text
```

The differences:

| Scope | Header | Active heartbeat | Done (issues) | Done (clean) |
|-------|--------|------------------|---------------|--------------|
| Plan | `review plan` | `Reviewing ops/now/...` | `Found N issues` | `✓ approved` |
| Code | `review code` | `Reviewing diff for ...` | `Found N issues` | `✓ approved` |
| Task | `review task` | `Reviewing changes for "..."` | `Found N issue` | `✓ approved` |
| Session | `review session` | `Reviewing N completed tasks...` | `Found N issues across M tasks` | `✓ approved` |

Session scope additionally prefixes issues with `[task name]` for grouping:
```
    1. [Add get_repo_root helper] get_repo_root shells out to `jj root` on every ca
    2. [Lock task writes] acquire_named_lock uses wrong error variant
```

### Implementation

```rust
pub fn view(graph: &TaskGraph, review_id: &str, target: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let review_task = &graph.tasks[review_id];
    let issues = extract_issues(review_task);

    // Review phase — target as first child, heartbeat/result as second
    let mut children = vec![
        ChildLine::normal(target, None),  // e.g. "path.md", "path.md --code", "[id] name", "4 completed tasks"
    ];

    match review_task.status {
        TaskStatus::Open => children.push(ChildLine::active("creating isolated workspace...")),
        TaskStatus::InProgress => children.push(ChildLine::active_with_elapsed(
            &review_task.latest_heartbeat(), review_task.elapsed_str(),
        )),
        TaskStatus::Closed if !issues.is_empty() => {
            children.push(ChildLine::normal(&format!("Found {} issues", issues.len()), None));
        }
        TaskStatus::Closed => {
            children.push(ChildLine::done("✓ approved", None));
        }
        _ => {}
    }

    lines.extend(components::phase(0, "review", review_task.agent_label(), children));

    // Issue list (if any)
    if !issues.is_empty() {
        let issue_texts: Vec<String> = issues.iter().map(|i| i.title.clone()).collect();
        lines.extend(components::issues(0, &issue_texts));
    }

    lines
}
```

### Issue extraction

Issues are stored as task comments with `type: issue` in their data fields. The existing `extract_review_issues()` in `review.rs` already does this. Reuse it.

### Integration point

Wiring into `review.rs` happens in Phase 3 (cutover). The command will create a `Model` with `Screen::Review { review_id, target }` and call `tui::run()`. This step only creates the pure view function.

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/review.rs` | New: `view()` (~100 lines) |

### Tests

- `insta` snapshots: plan scope (issues, approved), code scope, task scope, session scope (grouped issues).
- Session scope issue grouping: verify `[task name]` prefix rendering.

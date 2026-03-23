# Step 2a: `aiki task run` Flow

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 2 — Pipeline flows
**Depends on**: 1a (inline renderer), 1b (data model), 1c (shared components)

---

## Problem

No dedicated TUI for `aiki task run`. Currently it either monitors via the same complex pipeline chat or just prints text. Additionally, when the task has subtasks (parent task), there's no way to see subtask progress.

---

## Fix: Task run flow with optional subtask table

Implement the `task run` flow as the first consumer of the new rendering stack. This validates the full vertical: data model → phase rendering → inline display → done/error.

When the task has subtasks, a `SubtaskTable` renders below the run phase — the same component used by build/fix flows. This makes the subtask table a shared component, not an "epic" concept.

### Screen states (from [screen-states.md](screen-states.md), Flow 1)

**Leaf task (no subtasks):**
```
[80 cols]
 State 1.0:  ⠸ task (claude)                                ← yellow spinner, fg name, dim agent
             ⎿ starting session...                          ← dim ⎿, yellow text

 State 1.1:  ⠹ task (claude)                                ← yellow spinner, fg name, dim agent
             ⎿ Reading the existing implementation...   12s  ← dim ⎿, yellow text, dim elapsed

 State 1.2:  合 task completed — Add null check        2m15  ← bold+fg text, dim elapsed
             ⎿ Added null check before token access ...     ← dim ⎿, dim text
             Run `aiki task show <id>` for details.         ← dim hint text
```

**Parent task (with subtasks):**
```
[80 cols]
 State 1.6:  ⠹ task (claude)                                ← yellow spinner, fg name, dim agent
             ⎿ 0/3 subtasks completed                  45s  ← dim ⎿, dim text, dim elapsed
             ---                                            ← dim separator
                 [xkp29m] Fix review issues                 ← dim brackets+id, fg title
                 ▸ Fix null check in auth handler       32s  ← yellow ▸, fg text, dim elapsed
                 ◌ Add missing error handling in ...        ← dim ◌, dim text
                 ○ Remove unused import in utils.rs         ← dim ○, dim text
             ---                                            ← dim separator

 State 1.8:  ---                                            ← dim separator
                 [xkp29m] Fix review issues                 ← dim brackets+id, fg title
                 ✓ Fix null check in auth handler       56s  ← green ✓, dim text, dim elapsed
                 ✓ Add missing error handling in ...   1m22  ← green ✓, dim text, dim elapsed
                 ✓ Remove unused import in utils.rs     24s  ← green ✓, dim text, dim elapsed
             ---                                            ← dim separator
             合 task completed — Fix review issues     3m42  ← bold+fg text, dim elapsed
             ⎿ 3/3 subtasks completed                      ← dim ⎿, dim text
             Run `aiki task show xkp29m` for details.       ← dim hint text
```

### Implementation

The screen builder uses components from `components.rs`:

```rust
pub fn view(graph: &TaskGraph, task_id: &str) -> Vec<Line> {
    let task = &graph.tasks[task_id];
    let subtasks = get_subtasks(graph, task_id);
    let mut lines = vec![];

    // Task phase — heartbeat or result
    let children = match task.status {
        TaskStatus::Open => vec![
            ChildLine { text: "creating isolated workspace...".into(), style: ChildStyle::Active, meta: None },
        ],
        TaskStatus::InProgress if subtasks.is_empty() => vec![
            ChildLine { text: task.latest_heartbeat(), style: ChildStyle::Active, meta: task.elapsed_str() },
        ],
        TaskStatus::InProgress => vec![
            ChildLine { text: format_progress(&subtasks), style: ChildStyle::Normal, meta: task.elapsed_str() },
        ],
        TaskStatus::Closed => vec![
            ChildLine { text: task.effective_summary(), style: ChildStyle::Done, meta: task.elapsed_str() },
        ],
        TaskStatus::Stopped => vec![
            ChildLine { text: task.stopped_reason(), style: ChildStyle::Error, meta: task.elapsed_str() },
        ],
    };
    lines.extend(components::phase(0, "task", task.agent_label(), children));

    // Subtask table (if parent task)
    if !subtasks.is_empty() {
        let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
        lines.extend(components::subtask_table(0, &task.short_id(), &task.name, &data, false));
    }

    lines
}
```

### Integration point

Wiring `task_run::view()` into `runner.rs` happens in Phase 3 (cutover). The command will create a `Model` with `Screen::TaskRun { task_id }` and call `tui::run()`. This step only creates the pure view function.

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/task_run.rs` | New: `view()` (~50 lines) |

### Tests

- Snapshot test: leaf task in Starting, Active, Done, Failed states.
- Snapshot test: parent task with subtask table showing mixed statuses.
- Snapshot test: parent task with all subtasks done.
- Snapshot test: parent task with failed subtask.

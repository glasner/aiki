# Orchestrator Task Type

**Date**: 2026-02-10
**Status**: Draft
**Purpose**: Formalize the orchestrator pattern — tasks that coordinate other tasks rather than doing direct work

**Related Documents**:
- [Task DAG Design](task-dag.md) - Edge-based task relationships
- [Task Templates](../done/task-templates.md) - Template system
- [Task Execution: aiki task run](../done/run-task.md) - Agent delegation

---

## Executive Summary

Several task types (build, review, spec) share a common pattern: they create subtasks, delegate work to agents, and coordinate execution rather than producing code changes directly. The coordination logic — particularly cascade-stop behavior — is currently hardcoded in `build.rs`. This spec formalizes an `orchestrator` task type that the task system recognizes and handles with distinct lifecycle rules.

The key behavioral change: **when an orchestrator task is stopped, its open/in-progress subtasks are automatically cascade-closed.** This mirrors the existing cascade-close behavior on `task close` (task.rs:1785-1884) but extends it to the stop path.

---

## Problem

### Hardcoded orchestration in build.rs

The build command (`cli/src/commands/build.rs`) contains ~500 lines of orchestration logic that is specific to the build workflow but represents a general pattern:

1. **Stale cleanup** — Find and close stale orchestrator tasks for the same spec (lines 441-478)
2. **Cascade stop** — When a build is stopped or its agent fails, subtasks should not remain open/in-progress
3. **Plan management** — Find/resume/restart existing plans
4. **Status display** — Show orchestrator progress with subtask breakdown

Items 1 and 3-4 are legitimately build-specific. But item 2 — cascade-close on stop — is a general orchestrator behavior that should live in the task system, not in individual commands.

### Current behavior gap

| Action | Current behavior | Desired behavior |
|--------|-----------------|------------------|
| `task close <parent>` | Cascade-closes all unclosed descendants | Same (already works) |
| `task stop <parent>` | Only stops the parent task | Cascade-close open subtasks if parent is orchestrator |
| Agent failure on `task run` | Emits `Stopped` event for the run target | Also cascade-close subtasks if target is orchestrator |

Without this, when an orchestrator agent fails mid-execution, its subtasks remain open in the ready queue — confusing for both humans and agents.

---

## Design

### The `orchestrator` type

An orchestrator task is identified by `task_type == Some("orchestrator")`. This replaces the `build` type. Templates that currently use `type: build` will use `type: orchestrator` instead.

```rust
// Method on Task type — accessible from task.rs, runner.rs, etc. without import issues
impl Task {
    pub fn is_orchestrator(&self) -> bool {
        self.task_type.as_deref() == Some("orchestrator")
    }
}
```

### Cascade-close on stop

When an orchestrator task transitions to `Stopped` or `Failed` status, all its unclosed descendants are automatically cascade-closed with outcome `WontDo`. The cascade recursively collects the full descendant subtree, not just direct children. Nested orchestrator tasks are closed as regular descendants — they do not trigger secondary cascades.

The summary varies by trigger:
- **Stopped**: `"Parent orchestrator stopped"`
- **Failed**: `"Parent orchestrator failed"`

**Concurrency note**: Collection and write are not atomic. If a subtask completes between `get_all_unclosed_descendants` and the cascade `Closed` event write, its outcome is overwritten (last-writer-wins). This matches existing `task close` cascade semantics.

### Shared helper: `cascade_close_tasks`

**Why not call `run_close` directly?** `run_close` is a command handler (~200 lines) that bundles validation, stdin reading, session ownership checks, auto-start-parent logic, and output formatting — none of which the cascade path wants. The cascade path also needs to update the *caller's* in-memory `tasks` map (for ready-queue rendering), but `run_close` materializes its own fresh map from disk.

The cascade-close pattern — write event, dispatch flow events, update in-memory state — already exists inline in `run_close` (task.rs:1871-1935). Extract it as a shared helper so `run_close`, `run_stop`, and `task_run` all use the same codepath:

```rust
/// Cascade-close a set of tasks: write Closed event, dispatch flow events, update in-memory state.
/// Used by run_close (existing cascade), run_stop (orchestrator), and task_run (orchestrator).
fn cascade_close_tasks(
    cwd: &Path,
    tasks: &mut HashMap<String, Task>,
    task_ids: &[String],
    outcome: TaskOutcome,
    summary: &str,
) -> Result<()> {
    if task_ids.is_empty() {
        return Ok(());
    }

    let close_timestamp = chrono::Utc::now();

    // 1. Write the Closed event
    let close_event = TaskEvent::Closed {
        task_ids: task_ids.to_vec(),
        outcome,
        summary: Some(summary.to_string()),
        timestamp: close_timestamp,
    };
    write_event(cwd, &close_event)?;

    // 2. Dispatch task.closed flow events for hook automation
    for id in task_ids {
        if let Some(task) = tasks.get(id) {
            let task_event = AikiEvent::TaskClosed(AikiTaskClosedPayload {
                task: TaskEventPayload {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    task_type: infer_task_type(task),
                    status: "closed".to_string(),
                    assignee: task.assignee.clone(),
                    outcome: Some(outcome.to_string()),
                    source: task.sources.first().cloned(),
                    files: None,
                    changes: None,
                },
                cwd: cwd.to_path_buf(),
                timestamp: close_timestamp,
            });
            let _ = crate::event_bus::dispatch(task_event);
        }
    }

    // 3. Update in-memory state
    for id in task_ids {
        if let Some(task) = tasks.get_mut(id) {
            task.status = TaskStatus::Closed;
            task.closed_outcome = Some(outcome);
        }
    }

    Ok(())
}
```

**Three callsites use this helper:**

**1. `run_close` cascade** (task.rs, existing code):

Refactor the existing inline cascade (task.rs:1871-1935) to call `cascade_close_tasks`. The explicit-task close event (with the user's summary) remains a separate `write_event` call — only the descendant cascade uses the helper.

```rust
// Cascade-close descendants
if !cascade_ids.is_empty() {
    cascade_close_tasks(cwd, &mut tasks, &cascade_ids, outcome, "Closed with parent")?;
}

// Close the explicitly requested tasks (separate event with user's summary)
let close_event = TaskEvent::Closed { ... };
write_event(cwd, &close_event)?;
// Flow events for explicit tasks still dispatched inline (they use the user's outcome)
```

**2. `task stop` command** (task.rs `run_stop`):

After emitting the `Stopped` event but **before building the ready queue**, cascade-close unclosed descendants. Because `cascade_close_tasks` updates the in-memory `tasks` map, the subsequent ready-queue build correctly excludes them.

```rust
// After the stop event is written, before building the ready queue...
if stopped_task.is_orchestrator() {
    let unclosed = get_all_unclosed_descendants(&tasks, &task_id);
    if !unclosed.is_empty() {
        let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
        cascade_close_tasks(cwd, &mut tasks, &cascade_ids, TaskOutcome::WontDo, "Parent orchestrator stopped")?;
    }
}
```

**3. Task runner failure path** (runner.rs `task_run`):

When a `task_run` session returns `Stopped` or `Failed` and the target task is an orchestrator, cascade-close its subtasks.

```rust
// After emitting the Stopped/Failed event for the orchestrator task...
if refreshed_task.is_orchestrator() {
    let unclosed = get_all_unclosed_descendants(&refreshed_tasks, task_id);
    if !unclosed.is_empty() {
        let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
        let summary = match outcome {
            TaskOutcome::Failed => "Parent orchestrator failed",
            _ => "Parent orchestrator stopped",
        };
        cascade_close_tasks(cwd, &mut refreshed_tasks, &cascade_ids, TaskOutcome::WontDo, summary)?;
    }
}
```

### Template changes

The `aiki/build` template changes from `type: build` to `type: orchestrator`:

```markdown
---
version: 3.0.0
type: orchestrator
---

# Build: {{data.spec}}
...
```

Other templates that coordinate subtask execution (review, plan) can optionally adopt `type: orchestrator` if they want cascade-stop behavior.

### Query changes

Code that currently filters on `task_type == "build"` (build.rs lines 360, 421, 436, 452) needs to be updated to `task_type == "orchestrator"`.

---

## What Does NOT Change

- **`task close` cascade behavior** — Already works generically for all parent tasks. No change needed.
- **`task close` with `--wont-do`** — Already cascades. No change.
- **Build-specific logic** — Plan management, stale cleanup, undo-on-restart remain in `build.rs`. These are legitimately build-specific, not generic orchestrator behavior.
- **Template system** — Templates continue to work the same way. Only the `type:` field value changes.
- **Review/spec/plan types** — These remain as-is unless they want orchestrator behavior. The `type` field is still a free-form string.

---

## Implementation Plan

### Phase 1: Add orchestrator type recognition

**Files**: `cli/src/tasks/types.rs`

Add `is_orchestrator()` as a method on the `Task` type. This keeps it accessible from `task.rs`, `runner.rs`, and anywhere else without import issues.

### Phase 2: Extract `cascade_close_tasks` helper

**Files**: `cli/src/commands/task.rs`

Extract the cascade-close pattern from `run_close` (lines 1871-1935) into a shared `cascade_close_tasks` function that handles all three steps: write event, dispatch flow events, update in-memory state. Refactor `run_close`'s existing descendant cascade to use it.

### Phase 3: Cascade-stop in `task stop`

**Files**: `cli/src/commands/task.rs` (in `run_stop`)

After emitting the stop event but **before building the ready queue**, check if the stopped task is an orchestrator and call `cascade_close_tasks` on its unclosed descendants. Because the helper updates the in-memory `tasks` map, the subsequent ready-queue build correctly excludes cascade-closed tasks.

### Phase 4: Cascade-stop in task runner

**Files**: `cli/src/tasks/runner.rs` (in `task_run`)

In the `Stopped` and `Failed` result handlers, after emitting the stop event, check if the task is an orchestrator and call `cascade_close_tasks`. Note: `cascade_close_tasks` lives in `task.rs` — either make it `pub(crate)` or move it to a shared module (e.g. `tasks/manager.rs`).

### Phase 5: Update build template and queries

**Files**:
- `.aiki/templates/aiki/build.md` — Change `type: build` to `type: orchestrator`
- `cli/src/commands/build.rs` — Change `"build"` string comparisons to `"orchestrator"`

### Phase 6: Tests

**Files**: `cli/src/commands/task.rs` (unit tests), `cli/tests/task_tests.rs`

- Test: stopping an orchestrator cascade-closes its subtasks
- Test: stopping a non-orchestrator does NOT cascade-close subtasks
- Test: cascade-close on runner failure for orchestrator tasks
- Test: build command still works with new type name

---

## Success Criteria

- Stopping an orchestrator task cascade-closes all its open/in-progress subtasks with `WontDo` outcome
- Stopping a non-orchestrator task does NOT cascade-close subtasks (existing behavior preserved)
- Agent failure during `task run` of an orchestrator cascade-closes subtasks
- `aiki build` continues to work with the renamed type
- Existing `task close` cascade behavior is unaffected

---

## Resolved Questions

1. **Should `review` and `plan` types also become orchestrators?** No — only build for now. Review/plan/spec stay as-is. Expand later if there's demand.

2. **Should the cascade-close on stop emit `WontDo` or `Done`?** `WontDo` — consistent with how `build.rs` currently handles stale cleanup and plan closure (lines 470, 529). The work was not completed.

3. **Should there be a `--no-cascade` flag on `task stop`?** No — cascade-stop is always the right behavior for orchestrators. Keep it simple. If you want to stop without cascade, don't make it an orchestrator.

4. **Race condition: what if a subtask closes between collection and cascade write?** Last-writer-wins — the cascade overwrites the subtask's outcome. This matches existing `task close` cascade semantics and is inherent to the batch event model.

5. **Cascade depth?** Fully recursive through the entire descendant subtree, not just direct children. Same function used by `task close`.

6. **Nested orchestrators?** The full subtree is collected in one pass, so nested orchestrators are closed as regular descendants. No secondary cascades are triggered.

7. **Per-subtask summaries in cascade close?** No — use a generic summary ("Parent orchestrator stopped" / "Parent orchestrator failed"), consistent with `task close` batch behavior.

8. **Failed vs Stopped distinction?** Both trigger cascade-close. The orchestrator's lifecycle has ended in either case. The cascade summary distinguishes: "stopped" vs "failed".

9. **Template migration for in-flight `type: build` tasks?** Clean break — no backwards compatibility. `is_orchestrator` only checks for `"orchestrator"`. Build tasks are short-lived enough that in-flight migration is not a concern.

10. **Where does `is_orchestrator` live?** Method on the `Task` type (`task.is_orchestrator()`), not a free function. Accessible everywhere without import issues.

11. **Display changes for orchestrator tasks?** None needed. Existing parent/child rendering in `task list` and `task show` is sufficient.

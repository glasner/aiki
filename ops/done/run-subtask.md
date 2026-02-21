# Run Subtask: `--next-subtask` flag for `aiki task run`

**Date**: 2026-02-20
**Status**: Draft
**Purpose**: Add a `--next-subtask` flag to `aiki task run` that automatically picks and runs the next ready subtask of a parent task.

**Related Documents**:
- [ops/done/run-task.md](../done/run-task.md) - Original `aiki task run` spec
- [ops/done/background-run.md](../done/background-run.md) - `--async` flag spec

---

## Executive Summary

When an agent is working through a parent task's subtasks, it currently needs to inspect the task graph, find the next ready subtask ID, and pass that specific ID to `aiki task run`. The `--next-subtask` flag eliminates this orchestration overhead by having `aiki task run <parent-id> --next-subtask` automatically resolve and run the next ready subtask.

This enables a simple agent loop pattern: keep calling `aiki task run <parent> --next-subtask` until you get an "all subtasks complete" signal, then close the parent.

---

## User Experience

### Command Syntax

```bash
aiki task run <parent-task-id> --next-subtask [--agent <name>] [--async]
```

### Examples

```bash
# Run the next ready subtask of a parent task
aiki task run xqrmnpst --next-subtask

# Run the next ready subtask with a specific agent
aiki task run xqrmnpst --next-subtask --agent codex

# Run the next ready subtask in the background
aiki task run xqrmnpst --next-subtask --async

# Agent loop pattern (in CLAUDE.md or agent instructions):
# 1. aiki task run <parent> --next-subtask   → runs subtask .1
# 2. aiki task run <parent> --next-subtask   → runs subtask .2
# 3. aiki task run <parent> --next-subtask   → "All subtasks complete" (exit 0)
# 4. aiki task close <parent> --summary "Done"
```

### Output

**When a subtask is found and run (sync):**
```
Running subtask xqrmnpst.2 (Review code changes)...
Spawning claude-code agent session for task xqrmnpstabcdefghijklmnopqrstuvwx...
Task run complete
```

**When a subtask is found and run (async):**
```
Running subtask xqrmnpst.2 (Review code changes)...
Task started asynchronously.
```

**When all subtasks are closed (exit 0):**
```
All subtasks complete for xqrmnpst
```

**When subtasks exist but none are ready - all blocked (exit 1):**
```
Error: No ready subtasks for xqrmnpst (3 subtasks blocked)
  .1 (Fix auth bug) — blocked by: xqrmnpst.3
  .2 (Add tests) — in progress
  .3 (Design schema) — blocked by: othertask
```

**When task has no subtasks (exit 1):**
```
Error: Task xqrmnpst has no subtasks
```

---

## How It Works

### Subtask Resolution Algorithm

When `--next-subtask` is provided, the runner resolves the target task ID before spawning:

```
function resolve_next_subtask(cwd, parent_id):
  # 1. Load task graph
  graph = materialize_graph(read_events(cwd))

  # 2. Validate parent exists
  parent = find_task(graph, parent_id)

  # 3. Get all subtasks of parent, excluding synthetic .0 digest
  #    .0 is excluded from ALL checks (ready, blocked, complete) to prevent
  #    a stuck .0 from blocking the "all complete" signal.
  subtasks = get_subtasks(graph, parent_id)
    .filter(|t| t.name != DIGEST_SUBTASK_NAME)

  # 4. No (non-digest) subtasks at all → error
  if subtasks is empty:
    return Error("Task {parent_id} has no subtasks")

  # 5. Filter to ready subtasks (open + unblocked)
  ready = subtasks
    .filter(|t| t.status == Open)
    .filter(|t| !graph.is_blocked(t.id))
    .sort_by(priority, then created_at)

  # 6. If ready subtask found → return it (caller handles claim + spawn)
  if ready is not empty:
    return Ok(ready[0])

  # 7. No ready subtasks - check if all closed vs blocked
  unclosed = subtasks.filter(|t| t.status != Closed)
  if unclosed is empty:
    return AllComplete  # All subtasks closed (exit 0)
  else:
    return Blocked(unclosed)  # Return blocked subtask list for diagnostics
```

### Integration with Existing Runner

The `--next-subtask` flag is resolved **before** the existing `task_run()` / `task_run_async()` functions are called. The claim (emitting `Started`) happens **after** resolution but **before** spawning, and is rolled back if the spawn fails:

```
function run_run(cwd, id, agent, run_async, subtask):
  actual_id = id

  if subtask:
    match resolve_next_subtask(cwd, id):
      Ok(subtask)      → actual_id = subtask.id
      AllComplete       → print "All subtasks complete for {id}"; return Ok(())
      Blocked(tasks)    → print_blocked_diagnostics(id, tasks); return Err(...)
      Error(msg)        → return Err(msg)

    # Claim: transition to InProgress to prevent double-pick
    print "Running subtask {short_id(actual_id)} ({subtask.name})..."
    emit(TaskEvent::Started { task_ids: [actual_id] })

  # Existing logic unchanged from here
  options = TaskRunOptions::new()
  if agent: options = options.with_agent(agent)

  result = if run_async:
    run_task_async_with_output(cwd, actual_id, options)
  else:
    run_task_with_output(cwd, actual_id, options)

  # Rollback claim on spawn failure so the subtask isn't stuck InProgress
  if result.is_err() and subtask:
    emit(TaskEvent::Stopped { task_ids: [actual_id] })
    # Subtask reverts to Open; next --next-subtask call will re-pick it

  return result
```

### Flag Compatibility

| Flag combination | Behavior |
|-----------------|----------|
| `--next-subtask` | Resolve next ready subtask, run synchronously |
| `--next-subtask --async` | Resolve next ready subtask, run in background |
| `--next-subtask --agent codex` | Resolve next ready subtask, run with codex |
| `--next-subtask --async --agent codex` | All three combined |

---

## Use Cases

### Use Case 1: Agent Subtask Loop

An orchestrator agent working through subtasks:

```bash
# Agent is working on parent task abc123 with 3 subtasks
aiki task run abc123 --next-subtask    # Runs .1 (open → in_progress → closed)
aiki task run abc123 --next-subtask    # Runs .2 (open → in_progress → closed)
aiki task run abc123 --next-subtask    # Runs .3 (open → in_progress → closed)
aiki task run abc123 --next-subtask    # "All subtasks complete for abc123" (exit 0)
aiki task close abc123 --summary "All 3 subtasks completed"
```

### Use Case 2: Parallel Subtask Execution

Launch multiple subtasks concurrently:

```bash
aiki task run abc123 --next-subtask --async   # Spawns .1 in background
aiki task run abc123 --next-subtask --async   # Spawns .2 in background (if .1 still Open, same; if claimed, picks .2)
aiki task wait abc123.1 abc123.2
```

Note: The claim (emitting `Started` in `run_run()` before spawning) prevents two concurrent `--async --subtask` calls from picking the same subtask. The second call will see the first subtask as `InProgress` and pick the next one. If a spawn fails, the claim is rolled back via `Stopped`, so the subtask becomes available again.

### Use Case 3: Review Fix Loop

A review creates followup subtasks, then iterates:

```bash
aiki review abc123 --start
# Review creates fix subtasks under followup task def456
aiki task run def456 --next-subtask    # Fix first issue
aiki task run def456 --next-subtask    # Fix second issue
aiki task run def456 --next-subtask    # "All subtasks complete" → re-review
```

---

## Implementation Plan

### Phase 1: Subtask Resolution Function

Add `resolve_next_subtask()` to `cli/src/tasks/runner.rs`:
- Loads task graph
- Validates parent has subtasks
- Filters to ready (open + unblocked) subtasks
- Returns first by priority, then creation order
- Returns distinct result for "all complete" vs "blocked" vs "no subtasks"

### Phase 2: CLI Flag Addition

Modify `cli/src/commands/task.rs`:
- Add `--next-subtask` boolean flag to `Run` variant
- Pass flag through to `run_run()`
- Call `resolve_next_subtask()` before existing logic

### Phase 3: Output Integration

- Print which subtask was selected before spawning: `"Running subtask {short_id} ({name})..."`
- Print "All subtasks complete" message on success-with-no-work
- Include subtask name in MdBuilder output

**Files modified:**
- `cli/src/commands/task.rs` - Add `--next-subtask` flag to `Run`, update `run_run()`
- `cli/src/tasks/runner.rs` - Add `resolve_next_subtask()` function

**No new files needed.**

---

## Error Handling

| Scenario | Exit Code | Message | Notes |
|----------|-----------|---------|-------|
| Parent task not found | 1 | `Error: Task not found: <id>` | |
| Parent task already closed | 1 | `Error: Task already closed: <id>` | |
| Task has no subtasks | 1 | `Error: Task <id> has no subtasks` | Excludes synthetic .0 |
| All subtasks closed | 0 | `All subtasks complete for <id>` | Excludes synthetic .0 |
| All unclosed subtasks blocked | 1 | `Error: No ready subtasks for <id> (N subtasks blocked)` | Prints per-subtask blockers (see output section) |
| Subtask found, agent spawn fails | 1 | Standard agent spawn error | Subtask rolled back to Open via `Stopped` event |
| `--next-subtask` without a task ID | 1 | Standard clap missing-argument error | |

---

## Resolved Questions

| Question | Decision | Rationale |
|----------|----------|-----------|
| **Race condition in parallel `--async --subtask`** | Claim in `run_run()` before spawn, with rollback on failure | `run_run()` emits `TaskEvent::Started` after resolution but before spawning, preventing double-pick. If the spawn fails, it emits `TaskEvent::Stopped` to revert the subtask to Open so the next `--next-subtask` call re-picks it. The spawned agent's own `aiki task start` becomes a no-op (task already started). |
| **Should `--next-subtask` skip the `.0` digest subtask?** | Yes, exclude from ALL checks | The `.0` digest subtask is excluded at step 3, before any filtering. This ensures it doesn't affect ready selection, blocked counting, or completion detection. A stuck `.0` won't prevent "All subtasks complete". |
| **No ready subtasks behavior** | Three-way exit with diagnostics | All closed → exit 0 with "All subtasks complete". All blocked → exit 1 with per-subtask blocker details. No subtasks → exit 1 with "has no subtasks". No auto-run-parent fallback. |
| **Auto-pick vs explicit subtask number** | Auto-pick only | `--next-subtask` always picks the next ready subtask. No `--next-subtask 3` syntax. Use explicit IDs for targeted execution. |

## Open Questions

(None remaining)

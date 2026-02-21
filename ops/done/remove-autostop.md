# Remove Auto-Stop from Task Start

## Problem

When `aiki task start` is called, it auto-stops all other in-progress tasks claimed by the same session. This causes cascading problems:

1. **Parent preemption bug**: `aiki explore --start` (called from a review subtask) creates a standalone explore task. Starting it auto-stops the review parent and all its subtasks because the new task isn't linked to the review tree.

2. **Increasingly complex preservation logic**: We added parent preservation, then ancestor preservation, then sibling preservation — each fix reveals a new edge case.

3. **Wrong mental model**: The auto-stop assumes "one task at a time per session," but agents naturally work on parent + subtask + helper tasks concurrently.

## Solution

Remove auto-stop entirely. Starting a task just starts it — nothing else gets stopped. Agents are responsible for explicitly stopping tasks when blocked (`aiki task stop --reason`) or closing them when done (`aiki task close --summary`).

Stale tasks from ended sessions are already handled by session cleanup.

## Changes

### 1. `cli/src/tasks/mod.rs` — `start_task_core`

- Remove the `all_in_progress_ids` computation (lines ~112-130)
- Remove the `parent_ids_to_preserve` computation (lines ~113-116)
- Remove the auto-stop `Stopped` event emission (lines ~146-155)
- Remove the `stopped` field from `StartTaskResult` entirely (internal struct we control, no serde compat needed)
- Remove `stopped` from the `TaskEvent::Started` that gets written — always pass `vec![]`

### 2. `cli/src/commands/task.rs` — `run_start`

- Remove the `all_in_progress_ids` computation (lines ~1687-1698)
- Remove the `parent_ids_to_preserve` computation (lines ~1956-1959)
- Remove the `current_in_progress_ids` filtering (lines ~1961-1965)
- Remove the stopped task output formatting (lines ~1968-1971, ~2075-2078)
- Keep the parent-with-subtasks logic (digest creation, starting both parent + digest) — that stays
- Remove the `stopped` list from the `TaskEvent::Started` event — always pass `vec![]`
- The stop event emission block can be removed entirely

### 3. `cli/src/tasks/types.rs` — `TaskEvent::Started`

- Remove the `stopped: Vec<String>` field entirely

### 4. `cli/src/tasks/graph.rs` — `materialize_graph`

- Remove the handler for `stopped` field in `TaskEvent::Started`

### 5. `cli/tests/task_tests.rs`

- Update `test_task_start_auto_stops_current` — rename to `test_task_start_does_not_stop_other_tasks` and assert that starting a second task does NOT stop the first
- Check for any other tests that rely on auto-stop behavior

### 6. `cli/src/commands/agents_template.rs` — Agent instructions template

Update `AIKI_BLOCK_TEMPLATE`:
- Bump `AIKI_BLOCK_VERSION` from `"1.14"` to `"1.15"`
- Update the Workflow section to add explicit stop guidance: "When switching tasks, explicitly stop the current task first: `aiki task stop --reason 'Switching to X'`"
- Add to Common Pitfalls: "Not explicitly stopping tasks before switching to new work"
- Remove any mention of auto-stop behavior if present


### 7. `CLAUDE.md` and `AGENTS.md`

- Update CLAUDE.md `<aiki>` block with the same changes as agents_template.rs
- Update AGENTS.md `<aiki>` block with the same changes


### 8. Output format

- The `<stopped>` XML element in start command output goes away (or is always empty)
- The "Stopped: ..." lines in start output go away
- The `Preempted by task ...` reason string is no longer generated

## What stays the same

- `aiki task stop` (explicit stop) works exactly as before
- `TaskEvent::Stopped` still exists for explicit stops
- Session cleanup still handles stale tasks
- The digest creation flow when starting a parent with subtasks is unchanged

## Migration

Breaking change: Old `TaskEvent::Started` events with `stopped` fields will fail to deserialize. Users should ensure all in-progress tasks are closed before upgrading.

## Testing

- Verify starting task B while task A is in-progress leaves A in-progress
- Verify starting multiple tasks in rapid succession (simulating agent spawning helpers) leaves all tasks in-progress
- Verify starting a subtask doesn't stop the parent
- Verify `aiki explore --start` from within a review doesn't stop the review parent
- Verify explicit `aiki task stop` still works
- Verify session cleanup still works for stale tasks

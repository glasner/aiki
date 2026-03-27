# Revset Phase 1: Remove Legacy Child-Match Clauses

**Status:** Ready for implementation

## Problem

`build_task_revset_pattern` generates two substring matches per task ID — one for the task itself and one for legacy child-ID descendants via `task={id}.`. Subtasks now use independent 32-char IDs linked via `subtask-of` edges. This doubles clause count for zero value.

### Benchmarks (22k commits)

| Query variant | 1 task | 10 tasks | 30 tasks | 50 tasks |
|---|---|---|---|---|
| Double clause (current) | 0.37s | 0.36s | 0.45s | 0.61s |
| Single clause (simplified) | 0.24s | 0.37s | 0.32s | 0.54s |

Removing the legacy child-match variant saves ~10-20%.

## Changes

### 1a. Simplify `build_task_revset_pattern`

**Before:**
```rust
fn build_task_revset_pattern(task_id: &str) -> String {
    format!(
        "(description(substring:\"task={}\") | description(substring:\"task={}.\")) ~ ::aiki/tasks",
        task_id, task_id
    )
}
```

**After:**
```rust
fn build_task_revset_pattern(task_id: &str) -> String {
    format!(
        "description(substring:\"task={}\") ~ ::aiki/tasks",
        task_id
    )
}
```

### 1b. Update test `test_build_task_revset_pattern`

Remove the assertion that checks for the dot-variant pattern (`task=abc123.`).

### Files to modify

- `cli/src/commands/task.rs` — `build_task_revset_pattern` function and its test

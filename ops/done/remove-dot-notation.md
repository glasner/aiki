# Phase 4: Remove Dot-Notation, Emit `subtask-of` Links

**Date**: 2026-02-13
**Status**: Implementation
**Related**: [Task DAG](../done/task-dag.md) — Phase 4

---

## Context

Phase 4 of the task-dag spec replaces dot-notation subtask IDs (`parent.1`, `parent.2`) with full 32-char task IDs linked via explicit `subtask-of` edges. Currently, the `subtask-of` link kind is defined and graph queries use it, but all subtask creation still uses `generate_child_id()` — the backward-compat bridge in `graph.rs:337-340` synthesizes edges from the dots. This phase flips the switch: emit links explicitly and use full IDs.

## Approach

Single implementation pass across all call sites, then update support code (sorting, parent preservation, provenance). Backward-compat bridge stays (old dot-notation IDs continue to work).

---

## Step 1: `run_add` — Manual subtask creation

**File**: `cli/src/commands/task.rs` lines 1200-1241

Replace:
```rust
let task_ids: Vec<&str> = tasks.keys().map(|s| s.as_str()).collect();
let subtask_num = get_next_subtask_number(parent_id, task_ids.into_iter());
let child_id = generate_child_id(parent_id, subtask_num);
```
With:
```rust
let child_id = generate_task_id(&name);
```

After `write_event(cwd, &event)?;` (line ~1235), add:
```rust
if let Some(ref parent_id) = parent {
    let parent_id = &find_task(tasks, parent_id)?.id.clone();
    write_link_event(cwd, &graph, "subtask-of", &task_id, parent_id)?;
}
```

Note: `graph` is already materialized at line 1190.

## Step 2: `run_start` — Planning task + parent preservation

**File**: `cli/src/commands/task.rs` lines 1525-1598

### 2a: Replace `.0` planning task (lines 1532-1587)

Replace deterministic ID `generate_child_id(&task_id, 0)` with graph-based lookup + creation:

```rust
if has_subtasks(&graph, &task_id) {
    // Find existing planning task among children
    let existing_planning = graph.edges.referrers(&task_id, "subtask-of")
        .iter()
        .find(|child_id| {
            graph.tasks.get(*child_id).is_some_and(|t|
                t.name == "Review all subtasks and start first batch"
                && t.status != TaskStatus::Closed
            )
        })
        .cloned();

    let planning_id = if let Some(id) = existing_planning {
        id
    } else {
        let id = generate_task_id("Review all subtasks and start first batch");
        // Create + write event + write subtask-of link
        // ... (same Created event as before, just with full ID)
        write_event(cwd, &planning_event)?;
        write_link_event(cwd, &graph, "subtask-of", &id, &task_id)?;
        tasks.insert(id.clone(), task);
        id
    };

    actual_ids_to_start = vec![planning_id];
    new_scope = Some(task_id);
}
```

### 2b: Replace parent preservation (lines 1592-1598)

Replace dot-notation parsing with graph edge lookup:
```rust
let parent_ids_to_preserve: HashSet<String> = actual_ids_to_start
    .iter()
    .filter_map(|id| graph.edges.target(id, "subtask-of").map(|s| s.to_string()))
    .collect();
```

## Step 3: `create_parent_from_template` — Template as subtask

**File**: `cli/src/commands/task.rs` lines 4178-4226

Replace (when `params.parent_id` is set):
```rust
let subtask_num = get_next_subtask_number(parent_id, task_ids.into_iter());
generate_child_id(parent_id, subtask_num)
```
With:
```rust
generate_task_id(&parent_name)
```

After `write_event(cwd, &parent_event)?;` (line ~4217), add:
```rust
if let Some(ref parent_id) = params.parent_id {
    write_link_event(cwd, &graph, "subtask-of", &task_id, parent_id)?;
}
```

Note: `graph` already available at line 4180.

## Step 4: Template subtask creation functions

### 4a: `create_static_subtasks` (line 4428)

**Add `graph: &TaskGraph` parameter.** Replace:
```rust
let subtask_id = generate_child_id(parent_id, i + 1);
```
With:
```rust
let subtask_id = generate_task_id(&subtask_def.name);
```

After `write_event(cwd, &subtask_event)?;` (line ~4522), add:
```rust
write_link_event(cwd, graph, "subtask-of", &subtask_id, parent_id)?;
```

Note: `subtask_id` is set in context (line 4461) before name substitution — that's fine since `generate_task_id` uses the raw name as entropy only.

### 4b: `create_dynamic_subtasks` (line 4358)

Already materializes its own graph at line 4332. Replace:
```rust
let subtask_id = generate_child_id(parent_id, i + 1);
```
With:
```rust
let subtask_id = generate_task_id(&subtask_def.name);
```

After `write_event(cwd, &subtask_event)?;` (line ~4405), add link write using the graph already materialized at line 4332:
```rust
write_link_event(cwd, &graph, "subtask-of", &subtask_id, parent_id)?;
```

Stale graph is fine (parent exists, child is new — validation passes).

### 4c: `create_subtasks_from_entries` (line 4578)

**Add `graph: &TaskGraph` parameter.** Replace:
```rust
let subtask_id = generate_child_id(parent_id, subtask_index);
```
With:
```rust
let subtask_id = generate_task_id(&format!("subtask-{}", subtask_index));
```

For Static entries: after `write_event(cwd, &subtask_event)?;`, add link write.
For Composed entries: after `write_event(cwd, &composed_event)?;` (line 4806), add link write.

Pass `graph` through the recursive call at line 4820.

### 4d: Update call sites

Update callers of these functions to pass `graph`:
- `create_parent_from_template` → pass `&graph` to `create_static_subtasks` (line 4282) and `create_subtasks_from_entries` (line 4262)
- Recursive call in `create_subtasks_from_entries` (line 4820) → pass `graph`

## Step 5: Status monitor — Sorting and display

**File**: `cli/src/tasks/status_monitor.rs`

### 5a: Subtask sorting (line 397)

Replace:
```rust
subtasks.sort_by_key(|t| get_child_number(&t.id));
```
With:
```rust
subtasks.sort_by_key(|t| t.created_at);
```

### 5b: Subtask display numbering (lines 266, 315)

Currently `get_child_number(&subtask.id)` extracts the `.N` suffix. After migration, new subtasks return `None`. Change to use the enumerated index from the sorted list instead:

Pass the loop index as the child number instead of extracting from the ID. The callers at lines 250-268 and 300-317 iterate over sorted subtasks — use `enumerate()` and pass `Some(i)` as the subtask_index.

## Step 6: Ancestor chain provenance

**File**: `cli/src/flows/core/functions.rs` lines 90-103

Replace:
```rust
fn get_in_progress_tasks_for_session(cwd: &Path, session_id: &str) -> Vec<String> {
    let events = match storage::read_events(cwd) { ... };
    let tasks = crate::tasks::graph::materialize_graph(&events).tasks;
    manager::get_in_progress_task_ids_for_session(&tasks, session_id)
}
```
With:
```rust
fn get_in_progress_tasks_for_session(cwd: &Path, session_id: &str) -> Vec<String> {
    let events = match storage::read_events(cwd) { ... };
    let graph = crate::tasks::graph::materialize_graph(&events);
    let mut task_ids = manager::get_in_progress_task_ids_for_session(&graph.tasks, session_id);

    // Expand with ancestor chain for each in-progress task
    let mut ancestors = Vec::new();
    for id in &task_ids {
        ancestors.extend(graph.ancestor_chain(id));
    }

    // Deduplicate preserving order (leaf tasks first, then ancestors)
    let mut seen = std::collections::HashSet::new();
    task_ids.extend(ancestors);
    task_ids.retain(|id| seen.insert(id.clone()));

    task_ids
}
```

Remove `#[allow(dead_code)]` from `ancestor_chain` in `graph.rs:197`.

## Step 7: Cleanup

- Remove unused imports of `generate_child_id`, `get_next_subtask_number` from `task.rs`
- Remove unused import of `get_child_number` from `status_monitor.rs` (if fully replaced)
- Keep all functions in `id.rs` — the backward compat bridge uses `get_parent_id`
- Add `#[allow(dead_code)]` to functions in `id.rs` that become unused by non-test code

---

## Files Modified

| File | Changes |
|------|---------|
| `cli/src/commands/task.rs` | 6 call sites: replace `generate_child_id` → `generate_task_id` + `write_link_event`; parent preservation |
| `cli/src/tasks/status_monitor.rs` | Sorting by `created_at`; display numbering via enumerate |
| `cli/src/flows/core/functions.rs` | Ancestor chain expansion in provenance |
| `cli/src/tasks/graph.rs` | Remove `#[allow(dead_code)]` from `ancestor_chain` |
| `cli/src/tasks/id.rs` | Add `#[allow(dead_code)]` to newly-unused functions |

## Verification

1. **Build**: `cargo build --manifest-path cli/Cargo.toml`
2. **Unit tests**: `cargo test --manifest-path cli/Cargo.toml --lib` (994 tests, all should pass)
3. **Manual smoke test**:
   - `aiki task add "Parent" && aiki task add "Child" --parent <parent-id>` → child has full 32-char ID
   - `aiki task show <child-id>` → shows `subtask-of` relationship
   - `aiki task start <parent-id>` → creates planning task with full ID
   - `aiki task start <parent-id>` again → reuses same planning task (idempotent)
   - Template with subtasks → all subtasks have full 32-char IDs with `subtask-of` links

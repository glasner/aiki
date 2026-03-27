# Revset Phase 2: Ancestor Provenance via `subtask-of` Links

**Status:** Not started (depends on Phase 1)

## Problem

`build_task_revset_pattern_with_graph` walks the `subtask-of` graph at query time and emits one `description(substring:...)` clause per descendant. For an epic with 10 subtasks each having 2 sub-subtasks, that's 31 clauses. Each clause forces jj to linear-scan all ~22k commit descriptions.

Clause count is the real scaling problem — it grows linearly with subtask depth.

## Goal

When a `subtask-of` link is created, emit `ancestor={parent_id}` (and the parent's ancestors) in the provenance block for all subsequent commits on that task. Querying an epic's descendant work becomes a single `description(substring:"ancestor={epic_id}")` clause.

## Design: `ancestor=` not `task=`

**`task=`** means "this commit was made *for* this task" — it's the direct attribution.
**`ancestor=`** means "this task is a descendant of that task" — it's structural metadata derived from `subtask-of` links.

These are semantically different. A commit on `child1` was made *for* `child1`, not *for* `epic`. But `child1` *is a descendant of* `epic`. Overloading `task=` conflates attribution with hierarchy.

**Before** (commit while working on subtask `child1` of `epic`):
```
[aiki]
...
task=child1
[/aiki]
```

**After:**
```
[aiki]
...
task=child1
ancestor=epic
[/aiki]
```

Now `description(substring:"ancestor=epic")` matches all descendant commits — no need to enumerate them.

## Changes

### 2a. Add `ancestors` field to `ProvenanceRecord`

Add a new `ancestors: Vec<String>` field alongside the existing `tasks` field. Serialize as `ancestor={id}` lines in the `[aiki]` block (same pattern as `task=`).

**Write path** (`to_description`):
```rust
// After task= lines
for ancestor_id in &self.ancestors {
    lines.push(format!("ancestor={}", ancestor_id));
}
```

**Read path** (`from_description`): Parse `ancestor=` lines into `Vec<String>`, same as `task=`.

**Builder**: Add `with_ancestors(ancestors: Vec<String>) -> Self`.

#### Files to modify

- `cli/src/provenance/record.rs` — add `ancestors` field, serialize/deserialize, builder

### 2b. Compute ancestors from task graph at provenance write time

In the provenance recording path (where `with_tasks()` is called), also compute ancestors by walking `ancestor_chain()` for each active task and call `with_ancestors()`.

```rust
fn compute_ancestors(task_ids: &[String], graph: &TaskGraph) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut seen = HashSet::new();
    for id in task_ids {
        for ancestor in graph.ancestor_chain(id) {
            if seen.insert(ancestor.clone()) {
                ancestors.push(ancestor);
            }
        }
    }
    ancestors
}
```

**Where to call this:** In the hook handler / `flows/engine.rs`, after resolving active tasks but before building the `ProvenanceRecord`. The `TaskGraph` is already loaded there.

#### Files to modify

- `cli/src/flows/engine.rs` (or hook handler) — compute ancestors and pass to `with_ancestors()`

### 2c. Remove `build_task_revset_pattern_with_graph`, update `build_task_revset_pattern`

With ancestors embedded in provenance, the graph-walk query path is dead code. Remove `build_task_revset_pattern_with_graph` entirely and update `build_task_revset_pattern` to match both `task=` and `ancestor=`:

```rust
fn build_task_revset_pattern(task_id: &str) -> String {
    format!(
        "(description(substring:\"task={t}\") | description(substring:\"ancestor={t}\")) ~ ::aiki/tasks",
        t = task_id
    )
}
```

Update all call sites that used `build_task_revset_pattern_with_graph` to use `build_task_revset_pattern` instead.

#### Files to modify

- `cli/src/commands/task.rs` — remove `_with_graph` variant, update `build_task_revset_pattern`, update call sites and tests

## Risks

- **Re-parenting** — if a task is re-parented (`subtask-of` link replaced), old commits keep the old ancestor chain. This is correct — those commits *were* made under the old parent.
- **Metadata size** — ancestor chains are typically 1-3 levels deep. Adding 1-3 extra `ancestor=` lines per commit is negligible.

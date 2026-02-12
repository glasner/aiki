# DAG Follow-up Fixes

**Date**: 2026-02-11
**Status**: Plan
**Related**: [Task DAG](../done/task-dag.md)

---

## Problems

Three bugs from the DAG implementation review:

### Bug #2: Ready queue over-counts (Medium)

`task_list_size()` and `task_list_size_for_agent()` in `functions.rs` return ready queue counts without filtering blocked tasks. These counts are injected into flow context (agent prompts), so agents see inflated "8 ready" when some are actually blocked.

### Bug #3: Auto-start can pick blocked subtasks (Medium)

When closing a subtask, the auto-start-next logic at `task.rs:2290` calls `get_scoped_ready_queue()` directly without `filter_blocked()`. If the next subtask has an unresolved `blocked-by` link, it gets auto-started anyway.

### Bug #4: Supersedes link to file path (Low)

Forward cardinality auto-replace for `implements` can emit `supersedes` pointing at a file path. At `task.rs:5059-5066`, when `implements` replaces an existing link, `old_target` is the spec file (e.g., `file:ops/now/spec.md`), so the supersedes event becomes `from=task, to=file:spec.md, kind=supersedes` — but `supersedes` is `task_only`. The reverse cardinality path (line 5107-5114) is correct because `old_from` is always a task ID.

---

## Design: Unified Ready Queue

### Root Cause

`filter_blocked()` is a separate post-filter that callers must remember to apply. The pattern `filter_blocked(get_ready_queue_for_*(...), &graph)` appears 8+ times in `task.rs` and is missed in 5+ other call sites.

### Fix

**Bake blocking into every ready queue function.** Remove the standalone `filter_blocked()`. A task with unresolved `blocked-by` links is by definition not ready — the filter belongs inside the definition of "ready," not as an optional post-processing step.

### Changes to `manager.rs`

**1. `get_scoped_ready_queue` — add `graph.is_blocked()` filter:**

Current:
```rust
pub fn get_scoped_ready_queue<'a>(
    graph: &'a TaskGraph,
    scope: Option<&str>,
) -> Vec<&'a Task> {
    graph.tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| /* scope check */)
        .collect()
}
```

New:
```rust
pub fn get_scoped_ready_queue<'a>(
    graph: &'a TaskGraph,
    scope: Option<&str>,
) -> Vec<&'a Task> {
    graph.tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| !graph.is_blocked(&t.id))    // ← NEW
        .filter(|t| /* scope check */)
        .collect()
}
```

This propagates automatically to `get_ready_queue_for_scope_set` and `get_ready_queue_for_agent_scoped` since they delegate to `get_scoped_ready_queue`.

**2. `get_ready_queue` — change signature to take `&TaskGraph`:**

Current:
```rust
pub fn get_ready_queue(tasks: &FastHashMap<String, Task>) -> Vec<&Task>
```

New:
```rust
pub fn get_ready_queue(graph: &TaskGraph) -> Vec<&Task>
```

Add `!graph.is_blocked(&t.id)` filter. Update the two callers: `functions.rs:1033` and manager.rs tests.

**3. `get_ready_queue_for_agent` — change signature to take `&TaskGraph`:**

Current:
```rust
pub fn get_ready_queue_for_agent<'a>(
    tasks: &'a FastHashMap<String, Task>,
    agent: &AgentType,
) -> Vec<&'a Task>
```

New:
```rust
pub fn get_ready_queue_for_agent<'a>(
    graph: &'a TaskGraph,
    agent: &AgentType,
) -> Vec<&'a Task>
```

Add blocking filter. Update callers (manager.rs tests only — the scoped variant is used in production).

**4. `get_ready_queue_for_human` — same pattern, take `&TaskGraph`.**

**5. Delete `filter_blocked`** — no longer needed. Remove from `manager.rs` and from `tasks/mod.rs` re-exports.

### Call Site Updates

**`task.rs` (8 sites) — remove `filter_blocked()` wrapper:**

Every occurrence of this pattern:
```rust
let ready = crate::tasks::manager::filter_blocked(
    get_ready_queue_for_scope_set(&graph, &scope_set),
    &graph,
);
```

Becomes:
```rust
let ready = get_ready_queue_for_scope_set(&graph, &scope_set);
```

Sites: lines 946-947, 953-954, 959-960, 1395-1396, 1957-1958, 2380-2381, 2810-2811, 3311-3312, 3799-3800, 3941-3942.

Also remove `use crate::tasks::manager::filter_blocked;` at line 800.

**`task.rs:2290` (bug #3) — already fixed by change #1:**

```rust
let next_subtasks = get_scoped_ready_queue(&graph, Some(parent_id));
```

Now automatically excludes blocked subtasks.

**`functions.rs:1015` (bug #2) — already fixed by change to `get_ready_queue_for_agent_scoped`:**

```rust
let ready = crate::tasks::manager::get_ready_queue_for_agent_scoped(&graph, &scope_set, agent);
```

**`functions.rs:1033` (bug #2) — update to pass `&graph`:**

```rust
let ready = crate::tasks::manager::get_ready_queue(&graph);
```

**`review.rs:372`, `spec.rs:469`, `fix.rs:174` — already fixed by inner change:**

These call `get_ready_queue_for_scope_set` which now filters blocked tasks internally.

### Test Updates

**`manager.rs` tests:**
- Tests calling `get_ready_queue(&tasks)` need to change to `get_ready_queue(&graph)` — use `make_graph()` helper
- Tests calling `get_ready_queue_for_agent(&tasks, &agent)` — same
- Existing `filter_blocked` tests (`test_filter_blocked_removes_blocked_tasks`, `test_filter_blocked_unblocks_when_blocker_closed`) — convert to test that `get_scoped_ready_queue` / `get_ready_queue` directly exclude blocked tasks
- Add a test for the auto-start path: create a parent with two subtasks where one is blocked, verify `get_scoped_ready_queue` only returns the unblocked one

---

## Bug #4 Fix: `write_link_event` with Mandatory Validation

### Problem

There are 12+ direct `write_event(cwd, &TaskEvent::LinkAdded { ... })` calls scattered across `task.rs`, `plan.rs`, and `build.rs`. The auto-replace path at `task.rs:5059-5066` bypasses `normalize_link_target` validation entirely, allowing a `supersedes` link to point at a file path despite `supersedes` being `task_only: true`.

The root cause is the same pattern as bug #2: validation is optional and callers can skip it.

### Fix

**Create a `write_link_event` function that always validates.** Same principle as baking `filter_blocked` into the ready queue — one canonical write path, impossible to bypass.

```rust
/// Write a LinkAdded event with mandatory validation.
///
/// This is the ONLY way to emit a link. All validation happens here:
/// - Target normalization (short ID resolution, file: prefix, task_only check)
/// - Idempotency (skip if link already exists)
/// - Cycle detection (for blocked-by and subtask-of)
/// - Cardinality enforcement (single-link auto-replace)
///
/// Returns Ok(true) if a new link was written, Ok(false) if it was a no-op
/// (duplicate link).
pub fn write_link_event(
    cwd: &Path,
    graph: &TaskGraph,
    kind: &str,
    from: &str,
    to: &str,
) -> Result<bool>
```

**Location:** `cli/src/tasks/storage.rs` (next to `write_event`), or a new `cli/src/tasks/links.rs` module if the logic is substantial enough.

### What moves into `write_link_event`

The function consolidates logic currently spread across `run_link` in `task.rs`:

1. **`normalize_link_target`** — resolve short IDs, add `file:` prefix, enforce `task_only`
2. **Idempotency check** — `graph.edges.has_link()`, return `Ok(false)` if duplicate
3. **Cycle detection** — `graph.would_create_cycle()` for `blocked-by` and `subtask-of`
4. **Cardinality auto-replace** — check `max_forward`/`max_reverse` from `LINK_KINDS`, emit `LinkRemoved` + `supersedes` as needed
5. **The `supersedes` guard** — when auto-replace emits a supersedes link, it calls itself recursively (or inline), so `task_only` validation applies automatically. A `supersedes` link to `file:spec.md` is rejected by `normalize_link_target` since `supersedes` is `task_only`.
6. **Write the `LinkAdded` event**

### Call sites to update

Every direct `write_event(cwd, &TaskEvent::LinkAdded { ... })` becomes `write_link_event(cwd, &graph, kind, from, to)?`:

| File | Line | Current | Kind |
|------|------|---------|------|
| `task.rs` | 1243-1249 | sourced-from on task create | `sourced-from` |
| `task.rs` | 1448-1454 | sourced-from on quick-start create | `sourced-from` |
| `task.ts` | 1859-1865 | blocked-by on stop-with-blocker | `blocked-by` |
| `task.ts` | 1868-1874 | sourced-from on blocker task | `sourced-from` |
| `task.ts` | 4241-4247 | sourced-from on template task create | `sourced-from` |
| `task.rs` | 5060-5066 | supersedes in forward auto-replace | `supersedes` |
| `task.rs` | 5108-5114 | supersedes in reverse auto-replace | `supersedes` |
| `task.rs` | 5132-5138 | the main link write in `run_link` | any |
| `plan.rs` | 450-458 | scoped-to on planning task | `scoped-to` |
| `build.rs` | 576-584 | scoped-to on orchestrator task | `scoped-to` |
| `build.rs` | 588-596 | orchestrates on orchestrator task | `orchestrates` |

**`run_link` simplification:** After extracting validation into `write_link_event`, the `run_link` function in `task.rs` shrinks to: parse flags → resolve `from` task → call `write_link_event` → print confirmation. The 90+ lines of validation, cycle detection, and cardinality handling move out.

### Cardinality auto-replace

The auto-replace logic moves into `write_link_event`. When cardinality is exceeded, it:
1. Emits `LinkRemoved` for the old link (via `write_event` directly — remove events don't need link validation)
2. For `implements`/`orchestrates`: calls `write_link_event` recursively for the `supersedes` link — which naturally validates that the target is a task ID
3. Emits the new `LinkAdded`

The recursive call handles bug #4 automatically: `write_link_event(cwd, graph, "supersedes", from, old_target)` will fail `normalize_link_target` when `old_target` is `file:spec.md` because `supersedes` is `task_only`. In that case, skip the supersedes link silently (forward cardinality replacement on `implements` means moving specs, not superseding a task).

### What about callers that know their IDs are already canonical?

Internal callers like the blocker-creation path (`task.ts:1859`) already have full 32-char task IDs. They still go through `write_link_event` — the `normalize_link_target` fast path (step 2: "already a full 32-char task ID, use it directly") is effectively free. Consistency beats micro-optimization.

### `graph` availability

Most call sites already have a materialized `&graph` in scope. The two exceptions:
- Task creation sites (lines 1243, 1448, 4241) create a task then immediately emit `sourced-from` links. The graph was materialized before task creation. The new task isn't in the graph yet, but that's fine — `sourced-from` is not `task_only`, and the `from` ID is the just-created task (already validated as a 32-char ID).
- Blocker creation (line 1859) — same situation, graph was materialized earlier.

No new `materialize_graph` calls needed.

---

## Implementation Order

1. **`write_link_event` function** — create in `storage.rs` or `links.rs`, consolidating validation + cycle detection + cardinality + write
2. **`manager.rs` changes** — bake blocking into ready queue functions, delete `filter_blocked`
3. **`task.rs` call sites** — replace all direct `LinkAdded` writes with `write_link_event`, remove all `filter_blocked()` wrappers
4. **`plan.rs` + `build.rs` call sites** — replace direct `LinkAdded` writes
5. **Tests** — update existing, add new for: blocked-subtask excluded from auto-start, supersedes skipped for file targets, `write_link_event` validation

## Success Criteria

- `filter_blocked` no longer exists — impossible to forget blocking
- `task_list_size()` and `task_list_size_for_agent()` return correct counts excluding blocked tasks
- Auto-start-next-subtask skips blocked subtasks
- `supersedes` links are never emitted with non-task targets
- All existing tests pass (with signature updates)
- New tests cover: blocked subtask excluded from auto-start, supersedes skipped for file targets

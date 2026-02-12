# DAG Followup: Gaps in task-dag.md

**Date**: 2026-02-11
**Status**: Plan
**Related**: [Task DAG](task-dag.md)

---

## 1. `--blocked` flag → `blocked-by` link (Phase 2)

The `--blocked` flag on `aiki task stop` creates a standalone P0 blocker task assigned to `human`, but no link connects the stopped task to the blocker. The stopped task has no structural dependency on the blocker — closing the blocker doesn't unblock anything.

**Current behavior** (`task.rs:1837-1879`):

```
aiki task stop --blocked "Waiting for API key"
```

Emits:
1. `Stopped { task_ids: [task_id], blocked_reason: "Waiting for API key" }` — text field, first `--blocked` only
2. `Created { task_id: blocker_id, name: "Waiting for API key", priority: P0, assignee: "human" }` — standalone task

Result: three disconnected islands (stopped task, blocker tasks). No auto-resolution.

**Fix**:

After creating each blocker task, emit two links — `blocked-by` (blocked task → blocker) and `sourced-from` (blocker → blocked task):

```rust
// After write_event(cwd, &blocker_event)?; (task.rs:1852)
let blocked_by = TaskEvent::LinkAdded {
    from: task_id.clone(),
    to: blocker_id.clone(),
    kind: "blocked-by".to_string(),
    timestamp,
};
write_event(cwd, &blocked_by)?;

let sourced_from = TaskEvent::LinkAdded {
    from: blocker_id.clone(),
    to: task_id.clone(),
    kind: "sourced-from".to_string(),
    timestamp,
};
write_event(cwd, &sourced_from)?;
```

**Drop `blocked_reason`** from `TaskEvent::Stopped` — the link carries the same information structurally. No backward compatibility needed.

**No ready queue change.** Keep `== Open` filter in all ready queue variants. Stopped tasks are not "ready" — they require explicit `aiki task start` to resume. This avoids two regressions:
- `run_stop` (task.rs:1931) already manually appends the stopped task to display output — widening the ready queue would duplicate it
- `run_close` (task.rs:2249) auto-starts the next subtask from `get_scoped_ready_queue` — widening would auto-start stopped-and-blocked subtasks

**`task stop --blocked` flow**:
1. Emit `Stopped` event (task → `Stopped`, records the agent stopped)
2. For each `--blocked` arg: emit `Created` (blocker task) + `LinkAdded` (`blocked-by` link) + `LinkAdded` (`sourced-from` link)
3. Result: task is `Stopped`, has open blockers, not in ready queue
4. When all blockers close → task stays `Stopped`, appears in display via `run_stop`-style append, requires explicit `aiki task start` to resume

**Call sites**:

| Location | Change |
|---|---|
| `task.rs:1826-1831` | Remove `blocked_reason` field from `Stopped` event construction |
| `task.rs:1622` | Remove `blocked_reason: None` from auto-stop in `task start` |
| `task.rs:1837-1879` | After each blocker `Created` event, emit two `LinkAdded` events: `blocked-by` (stopped → blocker) and `sourced-from` (blocker → stopped) |
| `tasks/types.rs` | Remove `blocked_reason` from `TaskEvent::Stopped` variant |
| `tasks/storage.rs` | Remove `blocked_reason` serialization/deserialization |
| `tasks/mod.rs:136` | Remove `blocked_reason: None` from auto-stop in `start_tasks()` |
| `runner.rs:159` | Remove `blocked_reason: None` from `Stopped` handler |
| `runner.rs:194` | Remove `blocked_reason: None` from `Failed` handler |

**Test migration**:

| Location | Change |
|---|---|
| `tasks/storage.rs:992-1025` | `test_roundtrip_stopped`: remove `blocked_reason` from construction and destructuring |
| `tasks/storage.rs:1267-1274` | `test_stopped_minimal`: remove `blocked_reason` from destructuring and assertion |

---

## 2. `get_parent_id` call sites (Phase 4)

Phase 4 replaces dot-notation `get_parent_id()` with `subtask-of` link lookups. The original plan captured only one call site (`task.rs:1905`). The complete inventory is below.

**`get_parent_id` call sites** (direct):

| Location | Purpose | Migration |
|---|---|---|
| `task.rs:1905` | Stop scope set: which parents are in-progress tasks scoped to | `graph.edges.target(&task.id, "subtask-of")` |
| `task.rs:1933` | Stop output scope: is the stopped task visible in the current scope | `graph.edges.target(&stopped_task.id, "subtask-of")` |
| `task.rs:2159` | Close auto-start: collect parent IDs of closed tasks to check if all subtasks done | `graph.edges.target(id, "subtask-of")` |
| `task.rs:2325` | Close output scope: scope computation for started tasks | `graph.edges.target(&task.id, "subtask-of")` |
| `manager.rs:557` | `get_scoped_ready_queue`: root-vs-child filter (`get_parent_id(&t.id).is_none()`) | `graph.edges.target(&t.id, "subtask-of").is_none()` |
| `manager.rs:584` | Scope set computation: extract parent IDs from in-progress tasks | `graph.edges.target(&task.id, "subtask-of")` |

**`is_direct_child_of` call sites** (wraps `get_parent_id`):

| Location | Purpose | Migration |
|---|---|---|
| `manager.rs:532` | `get_all_direct_children`: filter direct children of a parent | `graph.edges.source(parent_id, "subtask-of")` |
| `manager.rs:557` | `get_scoped_ready_queue`: match children to scope parent | `graph.edges.target(&t.id, "subtask-of") == Some(parent_id)` |
| `task.rs:2805/2824` | Review command: filter completed subtasks of plan task | `graph.edges.source(plan_id, "subtask-of")` |
| `status_monitor.rs:399` | Display: filter direct children for status output | `graph.edges.source(parent_id, "subtask-of")` |

**`get_child_number` call sites** (uses dot-notation `rsplit_once('.')`):

| Location | Purpose | Migration |
|---|---|---|
| `status_monitor.rs:267,316` | Sort subtasks by numeric suffix | Keep as-is (display-only, operates on ID syntax not structure) |
| `id.rs:251` | `get_next_subtask_number`: find max child number | Keep as-is (ID generation, not structural query) |

**Backward-compat site** (no migration needed):

| Location | Purpose | Keep? |
|---|---|---|
| `graph.rs:291` | `materialize_graph`: index dot-notation IDs as `subtask-of` edges | Yes — this is the bridge that makes Phase 4 work for old events |

**Ordering constraint**: The graph is currently materialized at `task.rs:1921`, after the first two call sites (`:1905`, `:1933`). Either:
- Move graph materialization before `:1905`, or
- Move the scope computation blocks after `:1921`

For `:2159` and `:2325`, the graph must also be in scope — verify it's available at those call sites or pass it through.

**Manager.rs migration**: Change `get_scoped_ready_queue`, `get_current_scope_set`, `has_subtasks`, and `get_subtasks` to take `&TaskGraph` instead of `&HashMap<String, Task>`. Access task data via `graph.tasks`, parent lookups via `graph.edges.target(id, "subtask-of")`. All call sites update mechanically. Depends on Phase 1 (materialize unification) so callers already have a `TaskGraph`.

---

## 3. Unify `materialize_tasks` into `materialize_graph` (Phase 1)

`materialize_tasks` and `materialize_graph` are parallel functions that replay the same event stream. `materialize_graph` builds an identical task map plus the `EdgeStore`. Currently ~45 production call sites use `materialize_tasks` (task.rs ~14, build.rs 6, runner.rs 5, flows/core/functions.rs 5, review.rs 4, fix.rs 3, plan.rs 3, spec.rs 2, wait.rs 1, status_monitor.rs 1, tasks/mod.rs 1), while ~15 production sites use `materialize_graph`. Several sites call both on the same events, replaying the stream twice.

**Fix**: Delete `materialize_tasks`. Use `materialize_graph` everywhere. Callers that don't need edges just ignore `graph.edges` — the edge computation is microseconds (HashMap inserts during replay) and not worth optimizing with lazy loading at current scale.

**Migration**:

1. Replace `materialize_tasks(&events)` → `materialize_graph(&events).tasks` at sites that only need the task map and don't hold onto events
2. At sites that need both tasks and edges, use `let graph = materialize_graph(&events)` and access `graph.tasks` directly instead of a separate `tasks` variable
3. Update `materialize_tasks_with_ids` similarly (it has a parallel implementation for comment ID tracking)
4. Update test sites (mechanical — just change the call and add `.tasks` where needed)

**Also fix**: `TaskGraph.tasks` currently uses `std::HashMap` while `EdgeStore` uses `FastHashMap`. Change `TaskGraph.tasks` to `FastHashMap<String, Task>` to match the spec. This is a natural part of the unification since `materialize_tasks` in `manager.rs` also uses `std::HashMap`.

**Timing**: Do this in Phase 1 (link infrastructure) since it's the foundation for everything else. All subsequent phases benefit from having one materialization path.

---

## 4. Cache branch existence checks per process

Every `write_event` call spawns `jj bookmark list` to check whether `aiki/tasks` exists before writing. With 29 `write_event` calls in `task.rs` and some commands emitting 3+ events (e.g., `task stop --blocked` emits stop + create blocker + link), a single user action can spawn 6+ redundant branch-existence checks. The same pattern exists for `aiki/conversations` in `history/storage.rs`.

**Current code** (`storage.rs:18-44`):

```rust
pub fn write_event(cwd: &Path, event: &TaskEvent) -> Result<()> {
    ensure_tasks_branch(cwd)?;  // ← jj bookmark list every time
    // ... jj new --no-edit ...
}
```

**Fix**: A process-level cache keyed by `(canonicalized_repo_path, branch_name)`. Keying by branch name alone is unsafe — in tests or multi-repo scenarios, `aiki/tasks` in repo A would suppress the check for repo B.

```rust
use std::sync::{Mutex, OnceLock};
use std::collections::HashSet;

static ENSURED_BRANCHES: OnceLock<Mutex<HashSet<(PathBuf, String)>>> = OnceLock::new();

fn ensure_branch(cwd: &Path, branch: &str) -> Result<()> {
    let key = (cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()), branch.to_string());
    let set = ENSURED_BRANCHES.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let guard = set.lock().unwrap();
        if guard.contains(&key) {
            return Ok(());
        }
    }
    ensure_branch_impl(cwd, branch)?;
    set.lock().unwrap().insert(key);
    Ok(())
}
```

**Call sites** — all 8 `jj bookmark list` checks that this cache eliminates:

| Location | Branch | Context |
|---|---|---|
| `tasks/storage.rs:50` | `aiki/tasks` | `write_event` → `ensure_tasks_branch` |
| `tasks/storage.rs:73` | `aiki/tasks` | `read_events` → inline branch check |
| `tasks/storage.rs:155` | `aiki/tasks` | `read_events_with_ids` → inline branch check |
| `history/storage.rs:48` | `aiki/conversations` | `write_event` → `ensure_conversations_branch` |
| `history/storage.rs:430` | `aiki/conversations` | `read_events` → inline branch check |

The `ensure_*_branch` functions (write path) and inline branch checks (read path) both use the same `jj bookmark list` call. Unify into a single `ensure_branch(cwd, branch)` used by all 5 sites. Each `(repo, branch)` pair checked at most once per process.

**Timing**: Independent of DAG phases. Can be done anytime.

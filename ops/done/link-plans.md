# Link plans to specs via `implements` edge

## Problem

`aiki plan show spec/file.md` returns the wrong task — it picks up a build subtask instead of the actual plan. Root cause: `create_static_subtasks` (task.rs:5197) clones all parent data including `data.spec`, so build orchestrator subtasks inherit `data.spec` and match `find_plan_for_spec`'s data-attribute filter.

## Solution

Use the `implements` graph edge (plan → `file:<spec>`) instead of filtering on `data.spec`. The `implements` link kind already exists in `LINK_KINDS` (graph.rs:77) with 1:1 cardinality and `task_only: false`.

Graph materialization already synthesizes `implements` edges from `data.spec` for backward compatibility (graph.rs:467-474), so existing plans will have edges in the graph. The change is to also emit explicit link events going forward and to query via the edge store rather than raw data attributes.

## Changes

### 1. Plan template — emit `implements` link

**File:** `.aiki/templates/aiki/plan.md`

Add `aiki task link $PLAN_ID implements file:{{data.spec}}` after plan task creation, before setting instructions. This makes the planning agent emit an explicit link event.

### 2. `plan.rs` — rewrite `find_plan_for_spec`

**File:** `cli/src/commands/plan.rs`

- Add import: `use crate::tasks::graph::TaskGraph;`
- Change `find_plan_for_spec` signature from `(tasks: &FastHashMap<String, Task>, spec_path: &str)` to `(graph: &TaskGraph, spec_path: &str)`
- Implementation: query `graph.edges.referrers("file:<spec>", "implements")`, resolve each ID via `graph.tasks.get(id)`, filter out `task_type == "plan"` and `task_type == "orchestrator"`, return `max_by_key(created_at)`
- No `data.spec` fallback — the backward compat synthesis in graph.rs handles older tasks
- Update callers in `run_plan` (line 119) and `run_show` (line 224) to pass `&graph` instead of `&tasks`
- Remove unused `let tasks = &graph.tasks;` in `run_plan` (line 116)

### 3. `plan.rs` — defensive `implements` link in `run_plan`

After the planning agent finishes and we find the plan task (line 187), emit an `implements` link event via `write_link_event` if one doesn't already exist. This catches cases where the agent didn't follow the template instructions.

### 4. `build.rs` — rewrite `find_plan_for_spec`

**File:** `cli/src/commands/build.rs`

Same changes as plan.rs:
- Add import: `use crate::tasks::graph::TaskGraph;`
- Change signature to take `&TaskGraph`
- Query via `graph.edges.referrers`, filter out planning/orchestrator types
- No `data.spec` fallback
- Update callers in `run_build_spec` (line 142), `run_show` (line 352), and the post-build re-read (line 219) to pass `&graph`
- Remove unused `let tasks = &graph.tasks;` in `run_build_spec` (line 139)

### 5. Tests

Update tests in both `plan.rs` and `build.rs`:
- Add `make_graph` helper that builds a `TaskGraph` with `tasks`, `edges: EdgeStore`, and empty `slug_index`
- Rewrite `find_plan_for_spec` tests to construct `EdgeStore` with `implements` edges and pass `&graph`
- Key test cases:
  - `test_find_plan_for_spec_via_implements_link` — basic happy path
  - `test_find_plan_for_spec_ignores_build_subtask_with_inherited_data` — the original bug: subtask has `data.spec` (synthesized implements edge) but is filtered because it's a child of orchestrator
  - `test_find_plan_for_spec_wrong_spec` — no match
  - `test_find_plan_for_spec_most_recent` — multiple plans, picks newest
  - `test_find_plan_for_spec_none` — empty graph
- Remove old tests that test the data.spec fallback path

## Not in scope

- Removing the backward-compat `data.spec` → `implements` synthesis from graph.rs (separate cleanup)
- Changing `find_created_plan` in plan.rs (it correctly uses `source: task:<planning_id>` for a different purpose)

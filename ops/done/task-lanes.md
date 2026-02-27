# Task Lanes

**Date**: 2026-02-26
**Status**: Draft
**Purpose**: Introduce lanes as a derived DAG concept, add `task lane` query, `--lane` filter on `--next-session`, and `task wait --any`.

**Dependencies**:
- [needs-context.md](needs-context.md) — `needs-context` link type, sessions, and `--next-session`

**Related Documents**:
- [implement-command.md.backup](implement-command.md.backup) — Orchestrator template and build command (builds on this doc)
- [runner.rs](../../cli/src/tasks/runner.rs) — Task execution runner

---

## Executive Summary

With `needs-context` links and `--next-session` in place (see [needs-context.md](needs-context.md)), we have tasks and sessions. This plan adds **lanes** — sequences of sessions derived from the subtask DAG that enable parallel orchestration.

**What this plan adds:**

1. **Lanes** — sequences of sessions derived from the subtask DAG. Lanes are independent and can run concurrently.
2. **`task lane`** — pure query that derives lanes from the DAG and shows ready lanes or full decomposition.
3. **`--lane` filter on `--next-session`** — scopes session execution to a specific lane.
4. **`task wait --any`** — wait for any of several tasks to complete.

**Three concepts (tasks and sessions from needs-context.md, lanes from this plan):**

- **Task** — a unit of work (unchanged)
- **Session** — one or more tasks in a single agent session (from `needs-context` chains). See [needs-context.md](needs-context.md).
- **Lane** — a sequence of sessions derived from the subtask DAG. Lanes are independent and can run concurrently.

**Design philosophy:** Lanes are a query-time derivation, not stored state. `task lane` computes lanes fresh each time from link structure. `--lane` scopes execution. Orchestrators (templates) consume these primitives to implement concurrency strategies.

---

## Concepts

### Lane

A **lane** is a sequence of sessions derived from the subtask DAG. Sessions within a lane execute in order. Lanes are independent of each other and can run concurrently.

Lanes are derived from link structure:
- `needs-context` chains form multi-task sessions within the lane
- `depends-on` chains within a linear path stay in one lane
- Fan-out points split into separate lanes
- Fan-in creates a new lane that waits on predecessor lanes
- Independent tasks (no links) each get their own single-session lane

**Important:** Lane structure is derived from `needs-context` and `depends-on` edges only — these are the links that define execution ordering. Other blocking link types (`blocked-by`, `validates`, `remediates`) do not affect lane *structure* but do affect lane *readiness* (see Readiness rules below).

### Lane IDs

A lane is identified by its **head task ID** — the first task in the lane. This is a regular task ID, so prefix matching works the same way as everywhere else:

```bash
aiki task run <parent> --next-session --lane xtuttn --async
```

If the prefix is ambiguous (matches heads of multiple lanes), the error is lane-specific: `"Multiple lanes match prefix 'x', be more specific."`

### Example: Fix task

```
Fix Task subtasks:
  explore (xtuttn...) ──needs-context──→ plan ──needs-context──→ implement
  review  (mqpzyp...)  (independent)

Lanes:
  xtuttn...:
    1. session: explore → plan → implement  [needs-context chain]
  mqpzyp...:
    1. session: review
```

Both lanes can run concurrently. Lane IDs are `xtuttn...` (explore's ID) and `mqpzyp...` (review's ID).

### Example: Build task with fan-out

```
Build Task subtasks:
  explore (xtuttn...) ──needs-context──→ plan
  plan ──depends-on──→ implement-frontend (nmpsp...)
  plan ──depends-on──→ implement-backend  (kyxrt...)
  implement-tests (zxrko...) ──depends-on──→ implement-frontend
  implement-tests ──depends-on──→ implement-backend

Lanes:
  xtuttn...:
    1. session: explore → plan              [needs-context chain]
  nmpsp...:                                 depends on xtuttn...
    1. session: implement-frontend
  kyxrt...:                                 depends on xtuttn...
    1. session: implement-backend
  zxrko...:                                 depends on nmpsp..., kyxrt...
    1. session: implement-tests
```

### Example: Lane with mixed sessions

```
Task subtasks:
  explore (xtuttn...) ──needs-context──→ plan
  plan ──depends-on──→ implement
  implement ──depends-on──→ test
  test ──needs-context──→ verify

Lanes:
  xtuttn...:
    1. session: explore → plan              [needs-context, multi-task]
    2. session: implement                   [standalone, single-task]
    3. session: test → verify               [needs-context, multi-task]
```

Three sessions in one lane, executed in order. `--next-session --lane xtuttn` handles this naturally — it picks the next ready session each time.

---

## Primitives

### `task lane` — Query primitive

```bash
# Show ready and running lanes with their sessions (default)
aiki task lane <parent>

# Show full lane decomposition with status
aiki task lane <parent> --all
```

**`task lane <parent>` output (ready and running lanes):**

```
Running lanes:

xtuttn...:
  1. session: explore plan implement
  2. session: review-quality

Ready lanes:

nmpsp...:
  1. session: write-tests
```

After all lanes are launched (none remaining ready):

```
Running lanes:

xtuttn...:
  1. session: explore plan implement
  2. session: review-quality
nmpsp...:
  1. session: write-tests

Ready lanes:

(none)
```

After both lanes complete, unblocking a dependent lane:

```
Running lanes:

(none)

Ready lanes:

zxrko...:
  1. session: integration verify
```

**`task lane <parent> --all` output:**

```
Lanes for <parent-id>:

xtuttn...:                                                  ✓ complete
  1. session: [explore plan implement]   needs-context      ✓ complete
  2. session: [review-quality]                              ✓ complete

nmpsp...:                                                   ▶ running
  1. session: [write-tests]                                 ▶ running

zxrko...:                                depends on xtuttn  ○ blocked
  1. session: [integration verify]       needs-context      ○ blocked
```

Another example showing a lane that has become ready after its dependencies completed:

```
Lanes for <parent-id>:

xtuttn...:                                                  ✓ complete
  1. session: [explore plan implement]   needs-context      ✓ complete
  2. session: [review-quality]                              ✓ complete

nmpsp...:                                                   ✓ complete
  1. session: [write-tests]                                 ✓ complete

zxrko...:                                depends on xtuttn  ● ready
  1. session: [integration verify]       needs-context      ● ready
```

**Status indicators:** `✓ complete`, `▶ running`, `● ready`, `○ blocked`, `✗ failed`

**Lane states:**
- **Ready** — the lane has remaining work, is not failed, all predecessor lanes have completed, and no task in the lane's next session is blocked
- **Running** — a session in this lane is currently executing (at least one task is `InProgress`)
- **Complete** — all tasks in the lane are `Closed(Done)`
- **Failed** — any task in the lane is `Stopped` or `Closed(WontDo)`. Dependent lanes stay blocked — `depends-on`/`needs-context` only unblock on `Closed(Done)` (`DONE_ONLY_UNBLOCK` semantics)
- **Blocked** — the lane has remaining work but a predecessor lane is incomplete, or a task in the next session is blocked

**Readiness rules:**
- A lane is ready only if it is **not** complete, **not** failed, **not** running, all predecessor lanes have completed, **and** no task in the lane's next session is blocked
- "Blocked" is determined by `TaskGraph::is_blocked()` — this respects **all** blocking link types (`depends-on`, `needs-context`, `blocked-by`, `validates`, `remediates`, `follows-up`), not just the links used for lane structure derivation
- Within a lane, sessions execute sequentially — `--next-session --lane` handles this
- Readiness is derived from current task statuses — no persistence needed

### `--lane` filter on `--next-session`

```bash
# Scoped to a specific lane (uses --next-session from needs-context.md)
# Lane ID = head task ID. Prefix matching supported.
aiki task run <parent> --next-session --lane <head-task-id>
aiki task run <parent> --next-session --lane <head-task-id> --async

# Example with prefix:
aiki task run <parent> --next-session --lane xtuttn --async
```

`--lane` restricts `--next-session` to only consider subtasks within the named lane. The lane is identified by its head task's ID (prefix matching supported). This is how the orchestrator drives individual lanes.

### `task wait --any`

```bash
# Wait for any of several tasks to complete
aiki task wait <id1> <id2> --any
```

The orchestrator waits on task IDs from the sessions it launched. `--any` returns when any completes, letting the orchestrator check for newly ready lanes.

**Precise semantics vs. current `wait`:**

| Behavior | `task wait <ids>` (existing) | `task wait <ids> --any` (new) |
|----------|------------------------------|-------------------------------|
| Loop predicate | `all()` — every ID is terminal | `any()` — at least one ID is terminal |
| Terminal states | `Closed` (any outcome) or `Stopped` | Same |
| Output | Table of all tasks with status/outcome/summary | Same table, but only rows for tasks that reached terminal state |
| Return | After all IDs are terminal | After first ID reaches terminal |

**Implementation note:** The change to `run_wait` is minimal — switch `ids.iter().all(...)` to `ids.iter().any(...)` when `--any` is set, and filter the output table to only terminal tasks. The caller (orchestrator template) is responsible for re-invoking `wait --any` with the remaining non-terminal IDs if it wants to continue waiting.

---

## How Lane Agents Work

The orchestrator drives each lane by calling `--next-session --lane` repeatedly:

```
# Orchestrator starts a lane (explore's ID = xtuttn...):
aiki task run <parent> --next-session --lane xtuttn --async

# Runs first session (e.g., needs-context chain: explore → plan → implement)
# Agent is scoped to session's tasks, works through them normally.
# Session ends when all tasks in the chain are closed.

# --next-session returns. Orchestrator calls again:
aiki task run <parent> --next-session --lane xtuttn --async

# Runs second session (e.g., standalone task: review-quality)
# Session ends.

# --next-session returns. Orchestrator calls again:
aiki task run <parent> --next-session --lane xtuttn
# → nothing left in this lane. Lane done.
```

The agent focuses on one session. The orchestrator drives lane-level sequencing.

---

## Implementation Plan

### Phase 1: `task lane` query

*Depends on: [needs-context.md](needs-context.md) phases 1-3 (link type, frontmatter, `--next-session`)*

1. New module: `cli/src/tasks/lanes.rs`
2. Lane derivation algorithm (structural links only: `needs-context`, `depends-on`):
   a. Identify `needs-context` chains → multi-task sessions
   b. Walk `depends-on` edges to build linear chains, keeping sessions intact
   c. At fan-out points, split into separate lanes
   d. At fan-in points, create a lane that waits on predecessor lanes
   e. Independent tasks → single-session lanes
3. Each lane is a list of sessions in execution order
4. Cross-lane dependency computation (from `depends-on` edges between lanes)
5. Lane status computation: determine each lane's state from its tasks:
   a. **Failed** — any task in the lane is `Stopped` or `Closed(WontDo)`. Short-circuit: failed lanes are never ready, and dependent lanes stay blocked (matches `DONE_ONLY_UNBLOCK` rules)
   b. **Complete** — all tasks in the lane are `Closed(Done)`
   c. **Running** — any task in the lane is `InProgress`
   d. **Blocked/Ready** — delegate to `TaskGraph::is_blocked()` for the lane's next session tasks. This respects all blocking link types (`blocked-by`, `validates`, `remediates`, `follows-up`) in addition to the structural links used for derivation. A lane is ready only if it passes all checks (not failed, not complete, not running, next session unblocked)
6. `aiki task lane <parent>` — display running and ready lanes with their sessions (default)
7. `aiki task lane <parent> --all` — display full decomposition with status

### Phase 2: `--lane` filter on `--next-session`

1. Add `--lane <lane-id>` option to `--next-session` on `task run`
2. Lane ID = head task ID of the lane. Prefix matching via existing ID resolver.
3. Restricts subtask selection to tasks within the named lane
4. Uses lane derivation from Phase 1 to determine lane membership
5. Ambiguous prefix error: "Multiple lanes match prefix '<prefix>', be more specific"
6. No match error: "No lane with head task matching '<id>' for task <parent>"

### Phase 3: `task wait --any`

1. Add `--any` flag to `task wait` CLI parsing
2. In `run_wait`: when `--any`, change loop predicate from `ids.iter().all(...)` to `ids.iter().any(...)`
3. Filter output table to only terminal tasks (omit still-running tasks)
4. Same terminal definition as existing wait: `Closed` (any outcome) or `Stopped`

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Parent has no subtasks | `task lane` returns empty. |
| All subtasks blocked | `task lane` returns no ready lanes. |
| `--lane` with unknown ID | Error: "No lane with head task matching '<id>' for task <parent>" |
| `--lane` with ambiguous prefix | Error: "Multiple lanes match prefix '<prefix>', be more specific" |
| Session fails within lane | Lane cannot proceed to next session. Dependent lanes stay blocked. |
| Task closed as won't-do or stopped | Lane is **failed** (not complete). Dependent lanes stay blocked — `depends-on`/`needs-context` only unblock on `Closed(Done)`. |
| Task blocked by non-structural link | `is_blocked()` catches it. Lane reports as not ready even if structural predecessors are done. |
| All lanes complete | `task lane` returns empty. Orchestrator closes. |

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Lane definition | Derived from DAG, not stored | Deterministic from link structure. No persistence needed. |
| Lane contents | List of sessions (multi-task or single-task) | Natural: a lane is a sequence of agent sessions. |
| `task lane` default | Shows running and ready lanes | Running lanes let callers track in-progress work without `--all`. Ready lanes show actionable. `--all` for full view. |
| Lane ID | Head task ID | Stable (head doesn't change unless DAG restructured). No new ID system. Prefix matching reuses existing task ID resolver. |
| `--lane` filter | Scopes `--next-session` to lane by head task ID | Lets orchestrator manage per-lane execution. |
| Structural vs. blocking links | Lane *structure* from `needs-context` + `depends-on` only. Lane *readiness* checks lane status (failed/complete/running) first, then delegates to `is_blocked()` (all blocking types) for remaining lanes. | Structure defines what's in a lane. Status check prevents failed/complete lanes from appearing ready. `is_blocked()` handles the remaining blocking concern. |
| Lane completion semantics | `Closed(Done)` only. `Stopped`/`WontDo` = failed lane. | Matches `DONE_ONLY_UNBLOCK` rules in `is_blocked()` for `depends-on`/`needs-context`. Prevents premature unblocking of dependent lanes. |
| `wait --any` terminal definition | Same as existing `wait`: `Closed` (any outcome) or `Stopped` | Consistent. The *orchestrator* decides how to react to outcome (e.g., abort vs. continue). |

---

## Resolved Questions

1. **Persistence** — Lane derivation is deterministic from link structure. No persistence needed. `task lane` computes fresh each time.

2. **How does the orchestrator drive a lane?** — Calls `--next-session --lane <lane-id>` repeatedly (lane ID = head task ID, prefix matching supported). Each call runs one session and returns. When no more sessions remain, the lane is done.

3. **`task lane` default behavior** — Shows running and ready lanes. Running lanes let orchestrators track in-progress work; ready lanes show what's actionable. `--all` shows full decomposition with status.

4. **Lane readiness vs. all blocking link types** — Lane *structure* is derived from `needs-context` + `depends-on` only (these define execution ordering). Lane *readiness* first checks lane status (failed lanes with `Stopped`/`WontDo` tasks are never ready; complete/running lanes are excluded), then delegates to `TaskGraph::is_blocked()` for remaining lanes, which checks all blocking link types (`blocked-by`, `validates`, `remediates`, `follows-up`, `depends-on`, `needs-context`). This means a task blocked by e.g. `validates` will correctly prevent its lane from being ready, even though `validates` doesn't affect lane structure.

5. **Lane completion outcome mapping** — "Lane complete" means all tasks `Closed(Done)`. This aligns with `DONE_ONLY_UNBLOCK` in `is_blocked()`: `depends-on` and `needs-context` links only unblock when the blocker is `Closed(Done)`. If a task in a lane is `Stopped` or `Closed(WontDo)`, the lane is failed and dependent lanes stay blocked. The orchestrator can detect this via `task lane --all` status and decide whether to intervene.

6. **`task wait --any` contract** — Minimal change to `run_wait`: swap `all()` for `any()` predicate, filter output to terminal tasks only. Terminal = `Closed` (any outcome) or `Stopped` — same as existing `wait`. The orchestrator decides how to react to the outcome (retry, abort, continue). Remaining non-terminal IDs are the caller's responsibility to re-wait on.

## Open Questions

1. **Progress reporting** — Deferred. Orchestrator has all state needed; format TBD.

2. **`task start --next`** — Convenience flag to start the next ready task in current scope without listing first. Deferred for future work.

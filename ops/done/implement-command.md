# Implement Template & Build Refactor

**Date**: 2026-02-25
**Status**: Draft
**Purpose**: Provide `aiki/implement` as the default orchestrator template and refactor `aiki build` to use it.

**Dependencies**:
- [needs-context.md](needs-context.md) — `needs-context` link type, sessions, and `--next-session`
- [task-lanes.md](task-lanes.md) — `task lane` query, `--lane` filter on `--next-session`, `task wait --any`

**Related Documents**:
- [build.rs](../../cli/src/commands/build.rs) - Current build command
- [epic.rs](../../cli/src/commands/epic.rs) - Epic creation/management
- [build.md](../../.aiki/templates/aiki/build.md) - Current build template
- [runner.rs](../../cli/src/tasks/runner.rs) - Task execution runner
- [loop-flags.md](loop-flags.md) - Review-fix workflow (simplifies fix template)

---

## Executive Summary

With `needs-context` links and `--next-session` in place (see [needs-context.md](needs-context.md)), and lane primitives provided by [task-lanes.md](task-lanes.md), we can build the orchestrator template and refactor the build command.

This plan adds:

1. **`aiki/implement` template** — default orchestrator that runs lanes in parallel, using `--next-session --lane` to drive each lane.
2. **`aiki build` refactor** — simplified to: `epic add` + create task from `aiki/implement` + run.

**Prerequisites (from other plans):**

- **Task** — a unit of work (unchanged)
- **Session** — one or more tasks in a single agent session (from `needs-context` chains). See [needs-context.md](needs-context.md).
- **Lane** — a sequence of sessions derived from the subtask DAG. See [task-lanes.md](task-lanes.md).

**Architecture:**

1. **`task run --next-session`** — execution primitive (from needs-context.md). Runs the next ready session.
2. **`task lane`** — query primitive (from task-lanes.md). Derives lanes from DAG, reports readiness.
3. **`--lane` filter on `--next-session`** — scopes execution to a lane (from task-lanes.md).
4. **`task wait --any`** — wait for any of several tasks to complete (from task-lanes.md).
5. **`aiki/implement` template** — default orchestrator (this plan). Gets ready lanes, drives each with `--next-session --lane`.
6. **`aiki build`** — convenience wrapper (this plan). `epic add` + create task from `aiki/implement` + run.

**Design philosophy:** All orchestration logic — looping, concurrency, lane management — lives in the template. The `task lane` query helps orchestrators understand work structure. `--next-session --lane` scopes execution to a lane.

---

## Orchestrator Template

### `aiki/implement` — Default orchestrator

```markdown
---
version: 2.0.0
type: orchestrator
---

# Implement: {{data.target}}

You are orchestrating the implementation of task {{data.target}}.

## Step 1: Understand the work

    aiki task show {{data.target}}
    aiki task lane {{data.target}} --all

## Step 2: Execute

Loop until all lanes are complete:

1. Get ready lanes via `aiki task lane {{data.target}}`
2. For each ready lane, start it with `--next-session --lane <lane-id> --async`
3. Collect the last task IDs from started sessions
4. Wait for any to finish with `aiki task wait <id1> <id2> ... --any`
5. Loop back — finished session may have unblocked new lanes or the next session in a lane

```bash
while true; do
  # Get ready lanes
  ready=$(aiki task lane {{data.target}})
  [ -z "$ready" ] && break

  # Start ready lanes, collect last task IDs for waiting
  wait_ids=()
  for lane in $ready; do
    last_id=$(aiki task run {{data.target}} --next-session --lane $lane --async)
    wait_ids+=("$last_id")
  done

  # If nothing was started (all lanes already running or blocked), wait on existing
  [ ${#wait_ids[@]} -eq 0 ] && break

  # Wait for any session to finish
  aiki task wait "${wait_ids[@]}" --any
done
```

**How it works:**

1. Get ready lanes (may be empty if all lanes are blocked or running)
2. Start sessions for ready lanes
3. Wait for any running session to finish
4. Loop back - finished session may have unblocked new lanes
5. Exit when no ready lanes remain

## Failure handling

If a session fails, its lane cannot proceed. Dependent lanes
are also blocked. Independent lanes continue.

    aiki task lane {{data.target}} --all

If unrecoverable:

    aiki task stop {{id}} --reason "Failed: <reason>"

## Completion

When all lanes are complete:

    aiki task close {{id}} --summary "All lanes completed"
```

### Custom orchestrator examples

**Simple sequential (no lanes, needs-context-aware):**
```markdown
Loop until done:
    aiki task run {{data.target}} --next-session
```

**All-in-session (agent does everything, max context):**
```markdown
Work through all subtasks yourself in this session.
Close each subtask as you complete it.
```

**Lane-aware with concurrency limit of 2:**
```markdown
Same as default implement, but run at most 2 lanes at a time.
```

Orchestration strategy is template text, not CLI flags. Fully customizable.

---

## `aiki build` — Updated flow

```bash
aiki build ops/now/feature.md           # epic add → implement → task run
aiki build <epic-id>                     # implement → task run
aiki build ops/now/feature.md --async
aiki build ops/now/feature.md --restart
aiki build show ops/now/feature.md
aiki build ops/now/feature.md --template myorg/custom-implement
```

Under the hood:
```
aiki build <plan-path>
    │
    ├─ Validate plan, check draft status
    ├─ Cleanup stale builds
    ├─ find_or_create_epic(plan) → epic_id
    ├─ check_epic_blockers(epic_id)
    ├─ Create orchestrator task from aiki/implement template
    │   └─ data.target = epic_id
    └─ task_run(orchestrator_id)
        └─ orchestrator agent runs the implement template
```

No `--loop` or `--lanes` flags on build. The orchestration strategy is determined by the template.

---

## Use Cases

### 1. Build an epic (default — lane-aware orchestrator)
```bash
aiki build ops/now/feature.md
# Orchestrator derives lanes, runs up to 3 concurrently
# Each lane's sessions execute in order via --next-session --lane
```

### 2. Simple sequential execution (no lanes)
```bash
# Template loops:
aiki task run <parent> --next-session   # repeat until done
# needs-context chains automatically grouped into sessions
```

### 3. Fan-out with parallel lanes
```bash
aiki task lane <parent> --all
# xtuttn...:                                                ● ready
#   1. session: [explore plan]             needs-context
# nmpsp...:                                depends on xtuttn ○ waiting
#   1. session: [implement-frontend]
# kyxrt...:                                depends on xtuttn ○ waiting
#   1. session: [implement-backend]
# zxrko...:                                depends on nmpsp, kyxrt ○ waiting
#   1. session: [implement-tests]

# Orchestrator:
# t0: start xtuttn lane (--next-session --lane xtuttn)
# t1: xtuttn done → start nmpsp and kyxrt lanes
# t2: both done → start zxrko lane
# t3: done
```

### 4. Lane with mixed sessions
```bash
aiki task lane <parent> --all
# xtuttn...:
#   1. session: [explore plan]             needs-context
#   2. session: [implement]
#   3. session: [test verify]              needs-context

# Orchestrator drives the lane:
# --next-session --lane xtuttn → runs explore+plan session
# --next-session --lane xtuttn → runs implement session
# --next-session --lane xtuttn → runs test+verify session
# --next-session --lane xtuttn → nothing left, lane done
```

---

## Implementation Plan

*All phases depend on [task-lanes.md](task-lanes.md) being complete (provides `task lane`, `--lane`, `task wait --any`).*

*Each phase uses TDD: write failing tests for the desired behavior first, then implement until tests pass.*

### Phase 1: Create `aiki/implement` template (TDD)

**Tests first:**
1. Test that `aiki/implement` template exists and parses as `type: orchestrator`
2. Test that rendered template contains correct `task lane` and `--next-session --lane` commands for a given `data.target`
3. Test that the template resolves with standard template resolution (e.g. `aiki/implement` → `.aiki/templates/aiki/implement.md`)

**Then implement:**
4. Create `.aiki/templates/aiki/implement.md` with the orchestrator loop
5. Template is `type: orchestrator` for task graph semantics
6. Orchestrator loop: `task lane` → `--next-session --lane` → `task wait --any` → repeat

### Phase 2: Refactor `build.rs` (TDD)

**Tests first:**
1. Test that `build` creates a task from `aiki/implement` template (not `aiki/build`)
2. Test that `build` sets `data.target` to the epic ID
3. Test that `--template` flag overrides the default orchestrator template
4. Test that `--loop` and `--lanes` flags are rejected (no longer valid)
5. Test that build flow still validates plan, checks draft status, cleans up stale builds, and checks blockers

**Then implement:**
6. Replace `create_build_task()` with `create_implement_task()` using `aiki/implement` template
7. `build.rs` retains: plan validation, draft check, stale cleanup, epic find-or-create, blocker checks
8. Default template changes from `aiki/build` to `aiki/implement`
9. Keep `--template` flag for custom overrides
10. Remove `--loop` and `--lanes` flags from build (orchestration is template-driven)

### Phase 3: Extended integration tests

*Phases 1 and 2 cover unit and component-level tests via TDD. This phase adds end-to-end integration tests that span the full orchestrator lifecycle.*

1. Integration test: full orchestrator loop — `build` → orchestrator agent drives lanes → all tasks complete
2. Integration test: fan-out/fan-in — orchestrator starts parallel lanes, waits, then starts dependent lanes
3. Integration test: failure in one lane — independent lanes continue, dependent lanes stay blocked
4. Integration test: single-lane degenerate case — orchestrator falls back to sequential execution

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| `task run` role | Execution primitive only | No orchestration logic baked in. |
| Orchestration location | Template (agent instructions) | Fully customizable. Different templates = different strategies. |
| No `--loop` flag | Removed | Orchestrator template controls the loop. |
| No `--lanes N` flag | Removed | Orchestrator template controls concurrency. |
| Failure strategy | Orchestrator decides | Template implements failure policy. |
| Template name | `aiki/implement` | Describes intent not mechanism. |
| `build` refactor | Delegates to template → `task run` | Preserves customizability via `--template`. |
| Wait mechanism | `task wait --any` with explicit IDs | Uses primitive from task-lanes.md. Orchestrator tracks IDs explicitly. |

---

## Resolved Questions

1. **Where does orchestration live?** — In the template, not the runner. `task run` is a primitive. The template composes primitives into an execution strategy.

2. **How does the orchestrator drive a lane?** — Calls `--next-session --lane <lane-id>` repeatedly (lane ID = head task ID, prefix matching supported). Each call runs one session and returns. When no more sessions remain, the lane is done.

3. **How does the orchestrator wait?** — Uses `task wait --any` with explicit task IDs collected from `--async` launches. When any finishes, the orchestrator loops back to check for newly ready lanes.

## Follow-on: `build --review` / `build --fix`

After this plan lands, [loop-flags.md](loop-flags.md) Phase 2 needs rework to add `--review` and `--fix` flags to the refactored build command. Key integration points:

1. `build.rs` passes `data.options.review` / `data.options.fix` to the `aiki/implement` template
2. `aiki/implement` gets `spawns:` config to spawn review on orchestrator close (async path)
3. `build.rs` sync path runs `run_build_review()` directly after orchestrator completes

See [loop-flags.md](loop-flags.md) for full details.

---

## Open Questions

1. **Progress reporting** — Deferred. Orchestrator has all state needed; format TBD.

2. **Concurrency limits** — The default implement template caps at 3 concurrent lanes. Should this be configurable via `data.max_concurrent`? Or just a template concern?

3. **`task start --next`** — Convenience flag to start the next ready task in current scope without listing first. Deferred for future work.

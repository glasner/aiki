# End-to-End Workflow & Graph

**Date**: 2026-02-23
**Status**: Reference

---

## Workflow

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 1. PLAN                                                                 │
│    aiki plan ops/now/feature.md                                         │
│                                                                         │
│  Creates plan_task (type: plan) with subtasks:                          │
│    .1 Clarify user intent                                               │
│    .2 Draft initial plan                                                │
│    .3 Resolve open questions                                            │
│    .4 Validate completeness                                             │
│                                                                         │
│  Links:                                                                 │
│    plan_task ──adds-plan──→ file:ops/now/feature.md                  │
│    plan_task.N ──subtask-of──→ plan_task                                │
│                                                                         │
│  Output: plan document at ops/now/feature.md                            │
│                                                                         │
│  **NEEDS CHANGE:**                                                      │
│  - Replace `sourced-from` with `adds-plan` (semantic, plan-specific)    │
│  - Plan-specific link avoids reproducing all JJ file tracking in graph  │
│  - Note: edits/moves/deletes-plan deferred to ops/next/...links.md      │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 2. DECOMPOSE                                                            │
│    aiki decompose ops/now/feature.md                                    │
│                                                                         │
│  Creates:                                                               │
│    epic — container for implementation subtasks                         │
│    decompose_task — agent that reads plan and populates epic            │
│                                                                         │
│  Links:                                                                 │
│    epic ──implements-plan──→ file:ops/now/feature.md              (1:1)     │
│                                                                         │
│  Creates build_task that orchestrates epic execution.                   │
│  (Calls find_or_create_epic internally if no epic exists yet.)          │
│                                                                         │
│  Links:                                                                 │
│    build_task ──orchestrates──→ epic                         (1:1)     │
│                                                                         │
│  Execution: runs each epic subtask via aiki task run                    │
│                                                                         │
│  **NEEDS CHANGE:**                                                      │
│  - Build task must check if epic is blocked before execution            │
│  - Should exit early if epic has unresolved blockers (depends-on, etc.) │
│  - Links are correct, behavior needs implementation                     │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 4. REVIEW                                                               │
│    aiki review <task-id> --start                                        │
│                                                                         │
│  Creates review_task with subtasks:                                     │
│    .1 Explore scope                                                     │
│    .2 Review criteria                                                   │
│    .3 Review                                                            │
│                                                                         │
│  Links:                                                                 │
│    review_task ──validates──→ task:<reviewed-task>   (BLOCKS review)    │
│    review_task.N ──subtask-of──→ review_task                            │
│                                                                         │
│  Output: review with issue comments                                     │
│                                                                         │
│  **NEEDS CHANGE:**                                                      │
│  - Remove `sourced-from` link (redundant with validates)                │
│  - Remove `scoped-to` link (redundant with validates)                   │
│  - Use review_task.validates to find what task is being reviewed        │
│  - `validates` captures scope, relationship, and blocking semantics     │
│  - NOTE: This will have dependencies in code relying on scoped-to       │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 5. FIX                                                                  │
│    aiki fix <review-task-id>                                            │
│                                                                         │
│  If no issue comments → "Approved"                                      │
│  If issue comments → creates fix_task with subtasks per issue           │
│                                                                         │
│  Links:                                                                 │
│    fix_task ──remediates──→ review_task              (BLOCKS fix)      │
│    fix_task ──fixes──→ file:|task:<target>           (if task-scoped)   │
│    fix_task ──subtask-of──→ task:<original-task>     (if task-scoped)   │
│                                                                         │
│  Execution: agent creates & runs subtask per issue comment              │
│                                                                         │
│  **NEEDS CHANGE:**                                                      │
│  - Remove `sourced-from` link (redundant with remediates)               │
│  - Add `fixes` link to file: or task: entities (files or tasks)         │
│  - Keep `subtask-of` for hierarchy, `fixes` for semantic relationship   │
│  - Use fix_task.remediates to find what review triggered the fix        │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Before: Full Graph (single feature, end-to-end)

```
                         file:ops/now/feature.md
                        ╱         │          ╲
             sourced-from    implements-plan    sourced-from
                      ╱       (1:1)          ╲
               plan_task       epic      decompose_task
                 ▲  ▲          ▲  ▲
        subtask  │  │ sourced  │  │ depends-on
           -of   │  │ -from   │  │ (BLOCKS)
                 │  │          │  │
           plan.1│  │     epic.1  decompose_task
           plan.2│  │     epic.2
           plan.3│  │     epic.3
           plan.4│  │       ▲
                    │       │ subtask-of
                    │       │
                    │   build_task ──orchestrates (1:1)──→ epic
                    │
                    │
              file:feature.md


                task:epic.1
              ╱      │       ╲
    sourced  scoped  validates
      -from   -to   (BLOCKS)
              ╱      │       ╲
            review_task
              ▲  ▲  ▲
     subtask  │  │  │
        -of   │  │  │
              │  │  │
        rev.1 rev.2 rev.3


            review_task
           ╱           ╲
   remediates       sourced-from
   (BLOCKS)
           ╱           ╲
        fix_task ──subtask-of──→ task:epic.1
          ▲  ▲
 subtask  │  │
    -of   │  │
          │  │
       fix.1 fix.2
```

---


## After: Full Graph (with link cleanup applied)

```
                         file:ops/now/feature.md
                        ╱         │          ╲
             adds-plan     implements-plan    decomposes-plan
                         (1:1)
               plan_task       epic      decompose_task
                 ▲              ▲              │
        subtask  │              │              │ populates epic
           -of   │              │              │ with subtasks
                 │              │              ▼
           plan.1│         epic.1 ◄──────── epic
           plan.2│         epic.2            ▲
           plan.3│         epic.3            │
           plan.4│                           │ depends-on (BLOCKS)
                                             │
                                       decompose_task
                            ▲
                            │
                    build_task ──orchestrates (1:1)──→ epic
                                      (checks blockers, not subtask)


                task:epic.1
                    │
                validates
                (BLOCKS)
                    │
            review_task
              ▲  ▲  ▲
     subtask  │  │  │
        -of   │  │  │
              │  │  │
        rev.1 rev.2 rev.3


            review_task
                │
            remediates
            (BLOCKS)
                │
            fix_task ──fixes──→ file:|task:<target>
                │
            subtask-of
                │
          task:epic.1
              ▲  ▲
     subtask  │  │
        -of   │  │
              │  │
           fix.1 fix.2
```

**Key Changes:**
- Changed `plan_task ──sourced-from──→` to `──adds-plan──→` (semantic, plan-specific)
- Removed `epic ──sourced-from──→ file:` (redundant with implements-plan)
- Changed `decompose_task ──sourced-from──→` to `──decomposes-plan──→`
- Removed `review_task ──sourced-from──→` (redundant with validates)
- Removed `review_task ──scoped-to──→` (redundant with validates)
- Removed `fix_task ──sourced-from──→` (redundant with remediates)
- Added `fix_task ──fixes──→ file:|task:` (semantic relationship)
- Added blocker check requirement for build_task
- Plan links (`adds-plan`, `edits-plan`, `moves-plan`, `deletes-plan`) avoid duplicating JJ file tracking

---
## Before: Clean Inter-Entity View

```
file:ops/now/feature.md
     ▲            ▲
 sourced-from  implements-plan (1:1)
     │            │
 plan_task      epic ◄──── orchestrates (1:1) ──── build_task
                  ▲
            depends-on (BLOCKS)
                  │
            decompose_task


task:epic.1 ◄── validates (BLOCKS) ── review_task ◄── remediates (BLOCKS) ── fix_task
            ◄── scoped-to ─────────┘              ◄── sourced-from ─────────┘
                                                       │
                                                   subtask-of
                                                       │
                                                       ▼
                                                  task:epic.1
```

---

## After: Clean Inter-Entity View

```
file:ops/now/feature.md
         ▲
    implements-plan (1:1)
         │
       epic ◄──── orchestrates (1:1) ──── build_task
         ▲                                (checks blockers)
         │
    depends-on (BLOCKS)
         │
    decompose_task ──decomposes-plan──→ file:ops/now/feature.md


task:epic.1 ◄── validates (BLOCKS) ── review_task ◄── remediates (BLOCKS) ── fix_task
                                                                                  │
                                                                              fixes
                                                                                  │
                                                                                  ▼
                                                                         file:|task:<target>
```

---

## Before: All Link Types

| Link | Blocking? | Cardinality | Created By | Unblocks When |
|---|---|---|---|---|
| `depends-on` | **yes** | many:many | decompose (epic → decompose_task) | target Closed + **Done** |
| `validates` | **yes** | many:many | `aiki review` (review → reviewed task) | any terminal (Closed or Stopped) |
| `remediates` | **yes** | many:many | `aiki fix` (fix → review) | any terminal (Closed or Stopped) |
| `sourced-from` | no | many:many | `--source` flag, plan/fix/review creation | — |
| `subtask-of` | no | **1**:many | dot-notation IDs, `--subtask-of`, fix | — |
| `implements` | no | **1**:**1** | `aiki decompose` (epic → plan file) | auto-supersedes on conflict |
| `orchestrates` | no | **1**:**1** | `aiki build` (build → epic) | auto-supersedes on conflict |
| `scoped-to` | no | many:many | `aiki review` (materialized from data.scope) | — |
| `supersedes` | no | **1**:many | auto-emitted on single-link conflict | — |
| `spawned-by` | no | **1**:many | automatic process provenance | — |

---

## After: All Link Types

| Link | Blocking? | Cardinality | Created By | Unblocks When |
|---|---|---|---|---|
| `depends-on` | **yes** | many:many | decompose (epic → decompose_task) | target Closed + **Done** |
| `validates` | **yes** | many:many | `aiki review` (review → reviewed task) | any terminal (Closed or Stopped) |
| `remediates` | **yes** | many:many | `aiki fix` (fix → review) | any terminal (Closed or Stopped) |
| `subtask-of` | no | **1**:many | dot-notation IDs, `--subtask-of`, fix | — |
| `implements-plan` | no | **1**:**1** | `aiki decompose` (epic → plan file) | auto-supersedes on conflict |
| `decomposes-plan` | no | many:1 | `aiki decompose` (decompose_task → plan) | — |
| `orchestrates` | no | **1**:**1** | `aiki build` (build → epic) | auto-supersedes on conflict |
| `fixes` | no | many:many | `aiki fix` (fix_task → file/task) | — |
| `adds-plan` | no | many:many | Derived from JJ (task → plan file) | — |
| `supersedes` | no | **1**:many | auto-emitted on single-link conflict | — |
| `spawned-by` | no | **1**:many | automatic process provenance | — |

**Changes:**
- ❌ Removed `sourced-from` (replaced by semantic links)
- ❌ Removed `scoped-to` (redundant with `validates`)
- ✅ Added `adds-plan` (tracks plan creation)
- ✅ Added `decomposes-plan` (renamed from `sourced-from` on decompose_task)
- ✅ Added `fixes` (semantic relationship for fix tasks)
- ✅ Renamed `implements` → `implements-plan` (plan-specific)

## Link Semantics by Category

**Blocking** — controls ready queue:
- `depends-on` — strict: only `Done` unblocks (not `WontDo`, not `Stopped`)
- `validates` / `remediates` — lenient: any terminal state unblocks

**Provenance** — tracks origin:
- `sourced-from` — "why does this task exist?" (points to file, task, comment, prompt, issue)
- `spawned-by` — "what automated process created this?" (single parent)

**Hierarchy**:
- `subtask-of` — parent/child (single parent, many children)

**Plan/Spec**:
- `implements-plan` — 1:1, epic implements a plan file
- `orchestrates` — 1:1, build task manages an epic

**Lifecycle**:
- `scoped-to` — review scoped to task(s) or file(s)
- `supersedes` — auto-emitted when a single-link is replaced

---

## PlanGraph (Derived View)

`PlanGraph` (`cli/src/plans/graph.rs`) is not a separate set of links — it's a **reverse index** built from `implements-plan` edges in the TaskGraph, plus optional filesystem scanning.

### What It Does

```rust
PlanGraph::build(&task_graph)                      // Index implements edges
    .with_filesystem_plans(cwd, &["ops/now"])       // Scan filesystem for .md files
    .infer_statuses(&task_graph)                     // Derive plan status from epic state
```

### Reverse Index

```
file:ops/now/feature.md  →  [epic_id_1, epic_id_2, ...]
```

Built by scanning all `implements-plan` edges in the TaskGraph and inverting them. Provides O(1) lookups:

- `implementing_task_ids(plan_path)` — all tasks that implement a plan
- `implementing_tasks(plan_path, graph)` — same but returns Task refs
- `find_epic_for_plan(plan_path, graph)` — most recent epic (excludes decompose/orchestrator tasks)

### Plan Status (inferred from epic state)

| PlanStatus | Condition |
|---|---|
| `Draft` | No epic, or epic Closed as WontDo, or `draft: true` in frontmatter |
| `Planned` | Epic exists and is Open |
| `Implementing` | Epic is InProgress |
| `Implemented` | Epic is Closed with Done outcome |

### How It Relates to the Graph

```
                    file:ops/now/feature.md
                              │
                         implements-plan (1:1)
                              │
                            epic
                              │
                    PlanGraph reverse index
                              │
              ┌───────────────┼───────────────┐
              │               │               │
         find_epic     infer_status    implementing_tasks
              │               │               │
         most recent     Draft/Planned/    all tasks with
         non-decompose   Implementing/     implements edge
         non-orchestrator Implemented      to this file
```

### Task Type Filtering

`find_epic_for_plan()` excludes these task types (they implement the plan file but aren't the epic):

| Task Type | What It Is | Why Excluded |
|---|---|---|
| `decompose` | Agent that reads plan and creates epic subtasks | Ephemeral, runs once |
| `plan` | Legacy name for decompose | Backward compat |
| `orchestrator` | Build task that coordinates execution | It orchestrates, doesn't implement |

### Usage in Commands

| Command | How It Uses PlanGraph |
|---|---|
| `aiki decompose` | `find_epic_for_plan()` — find existing epic or create new |
| `aiki build` | `find_epic_for_plan()` — find epic to orchestrate |
| `aiki decompose show` | `find_epic_for_plan()` → show epic + subtasks |
| `aiki build show` | `find_epic_for_plan()` → show build status |

---

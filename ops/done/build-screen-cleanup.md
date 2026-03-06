# Build Screen Cleanup

**Date**: 2026-03-04
**Status**: Draft
**Purpose**: Replace MdBuilder markdown output in build/loop commands with `WorkflowView` → `render_workflow()` TUI, matching the review screen cleanup pattern.

**Related Documents**:
- [Review Screen Cleanup](review-screen-cleanup.md) — Same pattern, applied to review

---

## Executive Summary

`aiki build` outputs plain `MdBuilder` markdown at key lifecycle moments (loop started, loop completed, build completed, build+review completed). The `build show` subcommand already uses the TUI (`render_workflow()` + `buffer_to_ansi()`), but the inline lifecycle output does not. This plan replaces all MdBuilder output in build/loop with TUI renders, so every screen the user sees during a build uses the same `WorkflowView`.

---

## Scope

**In scope (Phase 1):** Screens 1–13 — the core build→review→fix lifecycle, from decompose through fix completion. These require no new view types, widgets, or data model changes.

**Deferred:** Screens 14–17 — review-fix gate, review-fix loopback, rereview regression check, and the full build+review+fix+rereview terminal state. These treat review-fix and rereview as top-level peer stages, which would require changes to the data model (the existing `FixChild::ReviewFix` in `builder.rs:551-557` models review-fix as a child of fix, not a peer). A follow-up plan will address these.

---

## Problem

### What already works (TUI)

- `build show` uses `build_workflow_view()` → `render_workflow()` → `buffer_to_ansi()` (build.rs:834-843)
- `build_workflow_view()` in builder.rs constructs stages (build with decompose/loop sub-stages, review, fix)
- StageList expands Active/Failed stages, collapses Done stages
- EpicTree collapses when review/fix is active, expands during build
- Lane DAG renders when loop sub-stage is Active

### What's broken (MdBuilder)

| Function | File | What it outputs |
|----------|------|----------------|
| `output_loop_started()` | loop_cmd.rs:179 | `"## Loop Started"` + IDs |
| `output_loop_completed()` | loop_cmd.rs:191 | `"## Loop Completed"` + IDs |
| `output_loop_async()` | loop_cmd.rs:203 | `"## Loop Started"` + IDs + "background" |
| `output_build_completed()` | build.rs:807 | `"## Build Completed"` + subtask list |
| `output_build_review_completed()` | build.rs:784 | `"## Build + Review Completed"` + IDs |

These should all render the TUI instead of markdown.

---

## Design: One View, Three Phases

Same principle as review cleanup. The `WorkflowView` is the single screen for the entire build→review→fix lifecycle. What changes between phases is **what's collapsed and what's expanded**:

| Phase | Epic tree | Build stage | Review stage | Fix stage |
|-------|-----------|-------------|-------------|-----------|
| **decompose** | hidden (no subtasks yet) | expanded (decompose starting/active) | pending | pending |
| **loop (building)** | expanded (subtasks visible) | expanded (decompose done, loop active) | pending | pending |
| **build done** | expanded (subtasks visible) | collapsed (done line) | pending | — |
| **review** | collapsed ("6 subtasks 2m28s") | collapsed (done line) | expanded | pending |
| **fix** | collapsed | collapsed | collapsed | expanded |
| **all done (approved)** | collapsed | collapsed | collapsed (approved) | skipped (`─`) |
| **all done (fixed)** | collapsed | collapsed | collapsed | collapsed |

---

## Lifecycle Screens

All screens use `render_workflow()` — same view, different data states.

### 1. Build Starting — Decompose Agent Spawning

First thing the user sees after `aiki build ops/now/feature.md`. The decompose agent is about to launch (0.5-2s transition).

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build                                        ← yellow all
    ⧗ decompose                                 ← yellow ⧗ (hourglass), yellow text
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- No subtasks yet — epic shows just the header line (no collapsed summary)
- Build stage is Active, decompose sub-stage is Starting (⧗)

### 2. Build In-Progress — Decomposing (Subtasks Appearing)

Decompose agent is running, breaking the plan into subtasks. Subtasks appear under loop as they're created.

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  0/2  0:08                       cc     ← yellow all, progress, elapsed, cyan cc
    ▸ decompose  0:08                            ← yellow, dim 0:08
    ○ loop  0/2                                  ← dim ○, dim text (pending), progress
       ○ Write handler                           ← dim ○, dim name
       ○ Add validation                          ← dim ○, dim name
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Decompose agent actively creating subtasks - they appear under loop as they're added
- Build stage shows progress (0/2) as subtasks are created
- Loop sub-stage shown as Pending with progress (0/2)
- Subtasks visible immediately as decompose creates them (not waiting for decompose to finish)

### 3. Build In-Progress — Decompose Done, Loop Starting

Decompose finished (all subtasks created), loop agent spawning. First subtask agent about to launch.

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  0/3  0:20                       cc     ← yellow all, progress, elapsed, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ⧗ loop  0/3                              ○  ← yellow ⧗ (hourglass), progress, DAG (pending)
       ○ Write handler                           ← dim ○, dim name
       ○ Add validation                          ← dim ○, dim name
       ○ Add tests                               ← dim ○, dim name
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Subtasks now visible under loop (just created by decompose)
- Decompose done (✓), loop Starting (⧗)
- DAG shows pending session (○) aligned with loop line

### 4. Build In-Progress — Loop Running (First Subtask)

Loop is executing subtasks. First subtask agent spawned.

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  0/3  0:22                       cc     ← yellow all, progress, elapsed, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ▸ loop  0/3  0:10                        ◉  ← yellow, progress, elapsed, DAG (active)
       ⧗ Write handler                           ← yellow ⧗, yellow name
       ○ Add validation                          ← dim ○, dim name
       ○ Add tests                               ← dim ○, dim name
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

### 5. Build In-Progress — Multiple Subtasks Running

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  1/3  0:57                       cc     ← yellow all, progress, elapsed, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ▸ loop  1/3  0:45                      ●━━◉  ← yellow, progress, elapsed, DAG
       ✓ Write handler                cc  45s    ← green ✓, cyan cc, dim 45s
       ▸ Add validation               cc 0:12    ← yellow ▸, cyan cc, dim 0:12
       ○ Add tests                               ← dim ○, dim name
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Progress shown on build stage line (1/3)
- Subtasks shown as children under loop sub-stage
- Lane DAG always shown on loop line (right-aligned), shows sequential session progress

### 6. Build In-Progress — With Lane DAG (Concurrent)

When loop has forked subtasks across multiple concurrent sessions:

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  1/4  1:02                       cc     ← yellow, progress, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ▸ loop  1/4  0:50                ●━━●━━◉     ← yellow, progress, elapsed, DAG (multi-lane)
       ✓ Write handler                cc  45s    ← green ✓, cyan cc       ○
       ▸ Add validation               cc 0:12    ← yellow ▸, cyan cc
       ▸ Implement retry logic        cd 0:08    ← yellow ▸, cyan cd (concurrent)
       ○ Add tests                               ← dim ○
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Progress shown on build stage line (1/4)
- Lane DAG aligned with loop line, shows multi-lane fork/merge topology
- Additional lane rows appear below loop line to show concurrent sessions

### 7. Build In-Progress — Last Subtask Finishing

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ▸ build  2/3  1:38                       cc     ← yellow all, progress, elapsed, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ▸ loop  2/3  1:26                  ●━━●━━◉  ← yellow, progress, elapsed, DAG
       ✓ Write handler                cc  45s    ← green ✓, cyan cc, dim 45s
       ✓ Add validation               cc  38s    ← green ✓, cyan cc, dim 38s
       ▸ Add tests                    cc 0:15    ← yellow ▸, cyan cc, dim 0:15
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

### 8. Build Completed (No Review)

Build finished, no `--review` flag. All subtasks done.

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✓ build  3/3 done  1:57                        ← green all, completion summary
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text

---
Run `aiki review luppzupt` to review.          ← dim text, next action
```

**Notes:**
- Build stage shows "3/3 done" completion summary on stage line
- Epic tree collapsed (no subtasks shown, summary on stage line instead)
- Review/fix still Pending — they weren't requested
- Next action shown: run review command with epic ID

### 9. Build Completed — With Partial Failures

When some subtasks failed during the loop:

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✗ build  2/3  1:57                       cc     ← red all, progress, elapsed, cyan cc
    ✓ decompose  12s                             ← green, dim 12s
    ✗ loop  2/3  1:45                            ← red, progress, elapsed
       ✓ Write handler                cc  45s    ← green ✓, cyan cc, dim 45s
       ✗ Add validation               cc  38s    ← red ✗, cyan cc, dim 38s
         Connection refused                      ← red, indented error
       ✓ Add tests                    cc  22s    ← green ✓, cyan cc, dim 22s
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Build stage Failed (✗), expanded to show sub-stages
- Failed subtask shows error line underneath
- Loop sub-stage also Failed with partial progress
- Subtasks shown under loop even when build fails

### 10. Build + Review Starting

Build done, review auto-starting (from `aiki build --review`).

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✓ build  3/3 done  1m57s                       ← green all, completion summary
 ▸ review                                       ← yellow all
    ⧗ explore                                   ← yellow ⧗ (hourglass), yellow text
    ○ criteria                                  ← dim
    ○ record-issues                             ← dim
 ○ fix                                          ← dim ○, dim text
```

**Notes:**
- Epic tree collapses when review becomes active (no subtasks shown)
- Build stage shows completion summary (3/3 done) on stage line
- Build stage collapsed to single Done line

### 11. Build + Review Completed — Approved (0 Issues)

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✓ build  3/3 done  1m57s                       ← green all, completion summary
 ✓ review  approved  0:42                       ← green all
 ─ fix                                          ← dim ─, dim text
```

**Notes:**
- Epic collapsed (no subtasks shown)
- Build stage shows completion summary (3/3 done) on stage line
- Fix shows `─` (skipped) — review found 0 issues, fix not needed
- This is the same terminal state as review-screen-cleanup screen #12

### 12. Build + Review Completed — Issues Found

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✓ build  3/3 done  1m57s                       ← green all, completion summary
 ✓ review  2 issues found  0:42                 ← green ✓, green text, yellow "2 issues found"
 ○ fix                                          ← dim ○, dim text

---
Run `aiki fix rlztrklp` to remediate.          ← dim text, next action
```

### 13. Build + Review + Fix In-Progress

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name

 ✓ build  3/3 done  1m57s                       ← green all, completion summary
 ✓ review  2 issues found  0:42                 ← green ✓, green text, yellow "2 issues found"
 ▸ fix  1/2  0:18                         cc     ← yellow all, progress, elapsed, cyan cc
    ✓ plan  2s                                   ← green ✓, dim 2s
    ✓ decompose  4s                              ← green ✓, dim 4s
    ▸ loop  1/2  0:12                            ← yellow ▸, progress, elapsed
       ✓ Fix: Missing null check  cur  12s      ← green ✓, magenta cur, dim 12s
       ▸ Fix: Error message format  cc 0:06     ← yellow ▸, cyan cc, dim 0:06
```

**Notes:**
- Fix stage has plan → decompose → loop (build has no plan step)
- Fix tasks appear as children under loop sub-stage (like build subtasks)
- Progress shown on fix stage line (1/2) and loop sub-stage (1/2)
- Build stage shows completion summary (3/3 done)
- Epic section empty (no Fix #1 header needed since progress shown on stage)

### 14–17. Review-Fix, Rereview, Repeated Fix Cycles — DEFERRED

> **Out of Scope for Phase 1.** Screens 14–17 (review-fix gate, review-fix looping back to fix, rereview regression check, and the full build+review+fix+rereview completion state) are deferred to a follow-up plan.
>
> **Rationale:** These screens treat review-fix and rereview as top-level peer stages alongside build/review/fix. This would require data model changes to the stage list (the existing `FixChild::ReviewFix` model in `builder.rs:551-557` nests review-fix as a child of fix, not as a peer stage). The core build→review→fix lifecycle (screens 1–13) can be shipped independently without these changes.
>
> **What stays unchanged:** The existing `FixChild::ReviewFix` model in `builder.rs:551-557` remains as-is. No changes to how review-fix is modeled in the data layer.
>
> **Follow-up:** A separate plan will address review-fix and rereview as top-level stages, including the data model changes and the screen mockups originally drafted here as screens 14–17.

### 18. Async Build — After `aiki build --async` or `aiki loop --async`

Async mode emits no TUI output. The command returns immediately with only the ID on stdout:

- `aiki build --async`: stdout = `<epic-id>\n`, stderr = nothing
- `aiki loop --async`: stdout = `<loop-id>\n`, stderr = nothing

The ID is plain text — no decoration, no brackets, no labels. This makes async output directly pipeable (e.g., `aiki build --async | xargs aiki build show`).

**Background process output:** The background process writes its TUI output to its own log file, not to stdout/stderr of the invoking command. The user can check status at any time with:

```bash
aiki build show ops/now/feature.md
```

Which already uses the TUI (whichever screen matches the current build state).

---

## Implementation Plan

### Phase 1: Replace build/loop MdBuilder output with TUI

Replace all `MdBuilder` output functions in build.rs and loop_cmd.rs with TUI renders.

**Changes in `commands/build.rs`:**

1. **`output_build_completed()`** — Re-read events, build graph, find epic, call `build_workflow_view()` → `render_workflow()` → `buffer_to_ansi()`. Same pattern as `output_build_show()`.

2. **`output_build_review_completed()`** — Same: load graph state, find epic, render TUI. The review and fix stages will show their final states in the WorkflowView.

**Changes in `commands/loop_cmd.rs`:**

3. **`output_loop_started()`** — Needs parent epic + graph to render TUI. Either pass these in or re-read events inside. The loop is a sub-stage of build, so the TUI will show build Active, loop Starting/Active.

4. **`output_loop_completed()`** — Same: render TUI showing build stage completing.

5. **`output_loop_async()`** — Replace with the async output contract: print `<loop-id>\n` to stdout, nothing to stderr, no TUI output. The background process writes its TUI output to its own log file (not stdout/stderr of the invoking command).

### Phase 2: Thread context through output functions

The current output functions take minimal args (IDs, plan paths). TUI rendering needs:
- Epic task
- Subtasks
- TaskGraph
- Plan path

Options:
- **A (preferred):** Re-read events inside each output function (simple, consistent, always up-to-date)
- **B:** Pass the graph through from callers (more args, but avoids re-read)

Option A is simpler and matches how `output_build_show()` already works — the caller doesn't need to know about TUI internals.

#### Event Timing

Screens 1 and 3 show decompose/loop in a "starting" state (⧗) at the moment TUI output fires. The current call sites have a timing gap: `output_loop_started()` fires *before* `task_run()`, which means the loop task exists in the event log (created on loop_cmd.rs:152) but has not yet received a `Started` event — it's still in `Open` status. If the TUI re-reads events (Option A above) to determine stage states, it would see the loop task as pending, not starting.

The same issue applies to decompose: screen 1 shows decompose as Starting (⧗), but the TUI output would need to fire at a point where the decompose task is recognizably in-progress.

**Strategy: Move output calls after the `Started` event (Option C).**

`task_run()` already emits a `TaskEvent::Started` before spawning the agent (runner.rs:156-167), which transitions the task from `Open` to `InProgress` immediately. The fix is to move the TUI output call to *after* `task_run()` has emitted `Started` but *before* the agent work completes. Concretely:

1. **Split `task_run()` into two phases**: a `task_start()` function that emits `Started` + resolves the agent (but does not spawn), and the existing `task_run()` that calls `task_start()` then spawns. Alternatively, add an `on_started` callback parameter.

2. **Wire output into the gap**: The lifecycle output functions (`output_loop_started`, and a new `output_decompose_starting` if needed) fire between `task_start()` and agent spawn. At this point, re-reading events shows the task as `InProgress` — consistent with the ⧗ icon in screens 1 and 3.

3. **Completed/failed output stays after `task_run()`**: `output_loop_completed()` and `output_build_completed()` already fire after `task_run()` returns, so no change is needed for those.

**Why not the alternatives:**

- **Pass a `RenderPhase` hint (Option A from review):** Lightweight but introduces synthetic state that diverges from the actual task graph. The TUI would show a state that doesn't exist in events, making `build show` and inline output inconsistent.
- **Render before `task_run()` with the task still Open (status quo):** The ⧗ icon would be a lie — the task graph says "pending" while the screen says "starting." If the user runs `build show` at the same instant, they'd see a different state.

**Impact on screen mockups:** Screens 1 and 3 remain unchanged. With this strategy, the decompose/loop task will have a `Started` event in the log when the TUI renders, so the ⧗ Starting state shown in the mockups accurately reflects the task graph state.

### Phase 3: Clean up

- Remove `MdBuilder` import from build.rs and loop_cmd.rs (if no other uses remain)
- Remove the old `output_build_completed()`, `output_build_review_completed()`, `output_loop_started()`, `output_loop_completed()`, `output_loop_async()` function signatures
- Update tests that assert on MdBuilder output format

---

## Key Insight

`build show` already works perfectly with the TUI. The fix is just making the inline lifecycle output call the same code path. The `WorkflowView` builder already handles all the state transitions — we just need to call it instead of constructing markdown strings.

No new view types, widgets, or data model changes needed for the core build/review/fix lifecycle (screens 1–13). Review-fix and rereview stages (screens 14–17) are deferred to a follow-up plan — those would require data model changes. For the in-scope screens, this is purely a wiring change: replace `MdBuilder::new("build").build(...)` calls with `build_workflow_view()` → `render_workflow()` → `buffer_to_ansi()`.

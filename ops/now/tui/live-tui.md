# Live TUI for Workflow Commands

**Date**: 2026-03-25
**Status**: Plan
**Depends on**: [workflow-steps.md](tui/workflow-steps.md) (must be implemented first)
**Builds on**: [worker-thread-tui.md](tui/worker-thread-tui.md), [screen-states.md](tui/screen-states.md)

---

## Problem

After the workflow refactor, commands run with `RunMode::Text` (eprintln)
or `RunMode::Quiet`. The ratatui live TUI (`RunMode::Tui`) is stubbed
out but not wired. This plan connects the workflow engine to the existing
TUI infrastructure (worker thread + JJ reader thread + inline viewport).

## Prerequisites

The workflow refactor (workflow-steps.md) must be complete:
- `workflow.rs` with `WorkflowStep`, `Workflow<S>`, `WorkflowContext`, `RunMode`
- `BuildStep`, `ReviewStep`, `FixStep` enums in command files
- `drive()`, `drive_text()`, `drive_quiet()` working
- All behavioral safety net tests passing

## What This Plan Adds

### 1. `RunMode::Tui` implementation

```rust
// workflow.rs — add to Workflow::run()

RunMode::Tui { model } => {
    tui::app::run_workflow(model, self)?;
    Ok(())
}
```

### 2. `run_workflow()` in tui/app.rs

New function that accepts a `Workflow<S>` directly instead of a closure:

```rust
pub fn run_workflow<S: WorkflowStep + 'static>(
    model: Model,
    workflow: Workflow<S>,
) -> Result<Effect> {
    // ... setup terminal, JJ reader thread (same as run_with_worker) ...

    let (worker_tx, worker_rx) = mpsc::channel();
    let worker_handle = thread::spawn(move || {
        let status = WorkerStatus::new(worker_tx);
        workflow.drive(&status)
    });

    let result = run_loop_with_worker(model, &jj_rx, &worker_rx, ...);

    let _ = worker_handle.join();
    // ... restore terminal ...
    result
}
```

`run_with_worker` stays for non-workflow callers (e.g. `aiki run`
which has its own one-off worker closure via `LoadingPhase`).

### 3. `WorkerStatus::noop()`

For non-TUI paths, steps call `status.update()` but it goes nowhere:

```rust
impl WorkerStatus {
    pub fn noop() -> Self {
        let (tx, _rx) = mpsc::channel();
        Self { tx }
    }
}
```

### 4. `screen_fn` on Workflow

Deferred Screen construction — the Screen is built after the first step
populates `ctx.task_id`:

```rust
pub struct Workflow<S: WorkflowStep> {
    pub steps: Vec<S>,
    pub ctx: WorkflowContext,
    pub screen_fn: fn(&WorkflowContext) -> Screen,
}
```

### 5. Mid-execution `status.*()` calls

Steps that spawn agents must call these DURING `run()`, not after:

- **`status.task(id)`** — BEFORE blocking on `task_run()`, so TUI shows
  heartbeats during execution (screen-states 3.11, 4.1, 8.1)
- **`status.agent(name)`** — once agent type is known, so TUI shows
  agent label (screen-states 3.10, 4.0, 8.0c)
- **`status.orchestrates(id)`** — Decompose must call this INSIDE `run()`
  after setting `ctx.task_id`. The `drive()` call happens before `run()`
  when `ctx.task_id` is still None. This enables the subtask table to
  appear during decompose (screen-state 2.3a).

### 6. `drive()` for TUI worker thread

The `drive()` method sends structural status messages through `WorkerStatus`:

```rust
fn drive(mut self, status: &WorkerStatus) -> anyhow::Result<()> {
    for step in &self.steps {
        if let Some(section) = step.section() {
            status.section(section);
        }
        status.start(step.name());
        if let Some(id) = step.orchestrates_id(&self.ctx) {
            status.orchestrates(id);
        }
        match step.run(&mut self.ctx, status) {
            Ok(result) => {
                if let Some(id) = &result.task_id {
                    status.task(id);
                }
                status.done(&result.message);
            }
            Err(e) => {
                status.failed(&e.to_string());
                return Err(e);
            }
        }
    }
    status.finish();
    Ok(())
}
```

For `BuildStep`, `drive_build()` overrides this with VecDeque-based
iteration for fix cycles.

## Screen-States Alignment

Step names and status messages match [screen-states.md](tui/screen-states.md):

| Step | Name | Screen-states |
|------|------|---------------|
| `BuildStep::Plan` | `"plan"` | 2.0-2.1 |
| `BuildStep::Decompose` | `"decompose"` | 2.2a-2.5 |
| `BuildStep::Loop` | `"loop"` | 2.6-2.10 |
| `BuildStep::Review` | `"review"` | 3.10-3.12 |
| `BuildStep::Fix` | `"fix"` | 3.13-3.14 |
| `BuildStep::RegressionReview` | `"review for regressions"` | 3.18 |
| `ReviewStep::Review` | `"review"` | 4.0-7.3 |
| `FixStep::Fix` | `"fix"` | 8.0a-8.2 |

**Alignment notes:**
- State 2.1: Plan step calls `status.agent("claude")`
- State 2.2a: First decompose update is "Finding epic..." (mockup shows
  "Reading task graph..." -- either works, mockup is aspirational)
- State 3.10: Review shows "Creating review..." briefly before "starting
  session..." -- mockup shows steady state
- State 3.12: Done message is `"approved"` when clean, `"Found N issues"`
  when issues exist

## Open Design Questions

### Two-child-line phases

Several mockups show phases with TWO persistent child lines:

1. **Review scope line (4.0-7.0):** A static scope description
   (`⎿ ops/now/...`) persists above the heartbeat/done line. Options:
   - New `WorkerStatus::Description(String)` message (non-replaceable child)
   - View derives scope from review task's data in the graph

2. **Review done after fix (3.17):** Shows `⎿ 3/3 issues resolved` AND
   `⎿ ✔ approved`. Options:
   - View derives resolution count from graph above done line
   - Done message includes both lines via `\n`

### Summary rendering

The summary block (`合 build completed — ...` with agent/token lines)
is rendered by the view function when `model.finished` is true, not by
a workflow step. The workflow signals completion via `status.finish()`.
The view queries the graph for session counts, token totals, and elapsed
times. The `Screen` variant determines "build completed" vs "fix completed".

### Issue list for regression reviews

`render_entries()` must render issue lists for phases named
`"review for regressions"` too, not just `"review"` -- screen-state 3.20
shows issues from a regression review.

### Preventing stdout bleeding

Steps never print to stdout/stderr. All output flows through
`WorkerStatus` (TUI mode) or the workflow runner's `eprintln` (Text mode).

The underlying functions (`run_decompose`, `run_loop`) have `show_tui: false`
passed by steps, which skips their `output_*` calls. `task_run` is called
with `TaskRunOptions::new().quiet()` to suppress output.

## What Changes

| File | Change |
|------|--------|
| `cli/src/workflow.rs` | Add `RunMode::Tui` match arm, `screen_fn` field, `drive()` method |
| `cli/src/tui/app.rs` | Add `run_workflow()`, add `WorkerStatus::noop()` |
| `cli/src/commands/build.rs` | Wire `RunMode::Tui` in command, add `status.*()` calls to step `run()` |
| `cli/src/commands/review.rs` | Wire `RunMode::Tui`, add `status.*()` calls |
| `cli/src/commands/fix.rs` | Wire `RunMode::Tui`, add `status.*()` calls |

## Implementation Steps

1. Add `WorkerStatus::noop()` to `tui/app.rs`
2. Add `run_workflow()` to `tui/app.rs`
3. Add `screen_fn` field to `Workflow<S>`
4. Implement `RunMode::Tui` in `Workflow::run()`
5. Add `drive()` method (TUI variant with WorkerStatus channel)
6. Add `status.task()`, `status.agent()`, `status.orchestrates()` calls
   to step `run()` methods in build/review/fix
7. Wire `RunMode::Tui` in each command's mode selection
8. Test: verify TUI renders phases correctly (manual + screen-state snapshots)
9. Address two-child-line phases (scope line, resolution count)
10. Address summary rendering

---

---

# One LiveScreen: Shared Alternate Screen Across Build Stages

**Date**: 2026-03-06
**Status**: Draft
**Purpose**: Keep a single `LiveScreen` alive across all stages of `aiki build` (decompose → loop → review) so the TUI transitions smoothly instead of exiting/re-entering alternate screen between stages.

---

## Problem

During `aiki build`, each stage independently calls `task_run()`, which creates and destroys its own `LiveScreen`. Between stages, the alternate screen exits, intermediate messages dump to the main terminal, then a new alternate screen enters:

```
[alternate screen: decompose stage TUI]
  ↓ LiveScreen drops
[main terminal] "Task run complete"
[main terminal] "Summary: Created 5 subtasks..."
[main terminal] "## Loop Started"
[main terminal] "Spawning Claude agent session for task..."
  ↓ LiveScreen created
[alternate screen: loop stage TUI]
```

The user sees fragmented output instead of a smooth continuous display.

---

## Solution

Create a `ScreenSession` that owns a single `LiveScreen` for the entire build pipeline. Stages reuse the shared screen — no exit/re-enter between stages.

```
[alternate screen created once]
  decompose agent runs → TUI updates live
  decompose completes → screen stays active, shows last frame
  loop agent spawns → TUI updates with new stage
  loop completes → screen stays active
  review agent spawns → TUI updates
  review completes
[alternate screen exits once]
[main terminal] final summary
```

---

## Architecture

### New: `ScreenSession` (`cli/src/tasks/runner.rs`)

Encapsulates shared `LiveScreen` + scoped SIGINT handler + stop flag for multi-stage pipelines.

```rust
pub struct ScreenSession {
    screen: LiveScreen,
    stop_flag: Arc<AtomicBool>,
    #[cfg(unix)]
    sig_id: signal_hook::SigId,
}

impl ScreenSession {
    /// Create session: LiveScreen + register SIGINT handler
    pub fn new() -> Result<Self>;

    /// Access the shared screen
    pub fn screen(&mut self) -> &mut LiveScreen;

    /// Shared stop flag for monitors
    pub fn stop_flag(&self) -> Arc<AtomicBool>;
}

impl Drop for ScreenSession {
    /// LiveScreen drops (restores terminal), unregisters SIGINT handler
    fn drop(&mut self);
}
```

**`stop_flag` lifecycle:** The `stop_flag` is a shared `Arc<AtomicBool>` that is set to `true` once when SIGINT fires and is **never reset** between stages. This is safe because the pipeline short-circuits on detach — when `monitor_on_screen()` in `StatusMonitor` observes `stop_flag == true`, it returns `UserDetached`, and the orchestrator (e.g., `run_build_plan()`) exits early without running subsequent stages. Since no subsequent stage ever runs after a detach, the stale `true` value is harmless.

> **Future consideration:** If a future feature needs to detach from one stage but continue running subsequent stages (e.g., skip the current stage's agent but proceed to the next), the `stop_flag` must be reset between stages — either via `stop_flag.store(false, Ordering::Relaxed)` at the start of each stage, or by adding a `ScreenSession::reset()` method that clears the flag. Until then, the current "set once, never reset" behavior is correct.

### New: `task_run_on_session()` (`cli/src/tasks/runner.rs`)

Like `task_run()` but uses a shared `ScreenSession`. Differences:

- No "Spawning..." message (already in alternate screen)
- No "Task run complete" message
- Returns `AgentSessionResult` (caller decides what to print)
- Calls `StatusMonitor::monitor_on_screen()` instead of `monitor_until_complete_with_child()`
- Does NOT call `restore_terminal()` — the caller's `ScreenSession` owns terminal cleanup. Panic safety is ensured via `catch_unwind` + `resume_unwind`, which propagates to `ScreenSession::Drop`

```rust
pub fn task_run_on_session(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
    session: &mut ScreenSession,
) -> Result<AgentSessionResult>
```

**Stdout contract:** `task_run_on_session()` never writes to stdout. When `--output id` is active, ID output is the caller's responsibility after `drop(session)`.

Extract shared logic between `task_run()` and `task_run_on_session()` into private helpers:
- `prepare_task_run()` — validates task, resolves agent, emits Started event, builds spawn options
- `map_exit_reason()` — converts `MonitorExitReason` to `AgentSessionResult`

### Modified: `StatusMonitor` (`cli/src/tasks/status_monitor.rs`)

- Add `new_with_stop_flag(task_id, stop_flag)` constructor for shared stop flags
- Add `monitor_on_screen(&mut self, cwd, child, screen)` — uses existing `LiveScreen`
- Factor shared event loop body into private `run_event_loop(cwd, child, screen)` to avoid duplication between `monitor_until_complete_with_child()` and `monitor_on_screen()`

### Modified: Pipeline functions get optional session parameter

Add `session: Option<&mut ScreenSession>` to pipeline functions. When `Some`, use `task_run_on_session()` and skip inter-stage output. When `None`, preserve existing behavior. All pipeline functions return `Result<PipelineResult<T>>` where `T` is the stage-specific payload type, so orchestrators can both detect `Detached` and extract stage outputs (IDs, review info) without ad-hoc side channels.

| Function | File | Returns | Callers to update |
|----------|------|---------|-------------------|
| `run_loop()` | `cli/src/commands/loop_cmd.rs` | `Result<PipelineResult<String>>` (loop task ID) | build.rs (×2), fix.rs, loop_cmd handler |
| `run_decompose()` | `cli/src/commands/decompose.rs` | `Result<PipelineResult<()>>` | epic.rs, fix.rs (×2), decompose cmd handler |
| `find_or_create_epic()` | `cli/src/commands/epic.rs` | `Result<PipelineResult<String>>` (epic ID) | build.rs (×1) |
| `create_epic()` (private) | `cli/src/commands/epic.rs` | `Result<PipelineResult<String>>` (epic ID) | epic.rs (×2) |
| `run_build_review()` (private) | `cli/src/commands/build.rs` | `Result<PipelineResult<BuildReviewInfo>>` | build.rs (×2) |

Standalone command handlers (decompose, loop) pass `None`. Orchestrators that already own a `ScreenSession` (build.rs, fix.rs) pass `session.as_mut()` — no behavioral change for these callers. `task_run()` signature stays unchanged (15+ callers unaffected).

**Stdout gating rule:** When `--output id` is active, `task_run_on_session()` and all pipeline functions listed above MUST NOT write to stdout while the session is active. All stdout output (task IDs, summaries, or any other content) is deferred to after `drop(session)`. This ensures the IDs-only contract is not broken by intermediate or stage-level output leaking to stdout during session mode.

**Data-flow through session mode:** `PipelineResult::Completed(payload)` values returned by each stage are captured in local variables while the session is active (e.g., `epic_id` from `find_or_create_epic()`, `loop_task_id` from `run_loop()`, `review_info` from `run_build_review()`). These payloads are used for output after `drop(session)`. However, post-session output also requires data not carried in payloads — specifically, the final epic ID (which may differ from the initial `epic_id` if the epic was recreated) and subtask references for the completion summary. These are obtained by re-materializing the task graph after `drop(session)`: calling `read_events()` → `materialize_graph()` → `PlanGraph::build()`, then `find_epic_for_plan()` and `get_subtasks()` to compute `final_epic_id` and `subtask_refs`. See the `run_build_plan()` example below for the complete pattern.

### Result handling for `task_run_on_session()` callers

`handle_session_result()` returns `Result<SessionOutcome>` (not `Result<()>`) so callers can distinguish terminal states:

```rust
pub enum SessionOutcome {
    Completed,
    Detached,
}
```

`SessionOutcome` is the low-level type returned by `handle_session_result()`, which operates on `AgentSessionResult` from a single agent session. Pipeline functions use a higher-level return type that carries stage-specific payloads:

```rust
pub enum PipelineResult<T> {
    Completed(T),
    Detached,
}
```

- `PipelineResult<T>` is the pipeline-level return type for orchestrator stage functions (`run_loop`, `find_or_create_epic`, `run_build_review`, etc.)
- `Completed(T)` carries the stage-specific payload (e.g., epic ID from decompose, loop task ID from loop, review info from review)
- `Detached` signals the user detached mid-stage — no payload, caller exits early

Callers of `task_run_on_session()` MUST handle `AgentSessionResult` using `handle_session_result(cwd, task_id, result, quiet)`:
- `Completed` — print summary (unless quiet), return `Ok(SessionOutcome::Completed)`
- `Stopped` — emit `TaskEvent::Stopped`, cascade-close orchestrator subtasks, return `Err`
- `Detached` — return `Ok(SessionOutcome::Detached)` (**no print** — caller decides)
- `Failed` — emit `TaskEvent::Stopped`, cascade-close, return `Err`

`task_run()` (the standalone entry point) calls `handle_session_result()` and maps `SessionOutcome::Detached` to a print + `Ok(())` internally, preserving its existing `Result<()>` API for the 15+ callers that don't need to distinguish outcomes.

---

## Integration: Build Orchestrators

### `run_build_plan()` sync path (`cli/src/commands/build.rs`)

```rust
let mut session = if std::io::stderr().is_terminal() {
    Some(ScreenSession::new()?)
} else {
    None
};

// Decompose (if needed)
let epic_id = match epic_id {
    Some(id) => id,
    None => match find_or_create_epic(cwd, plan_path, tmpl, session.as_mut())? {
        PipelineResult::Completed(id) => id,
        PipelineResult::Detached => {
            drop(session);
            eprintln!("Session detached during decompose stage.");
            return Ok(());
        }
    },
};

// Loop
let loop_task_id = match run_loop(cwd, &epic_id, loop_options, session.as_mut())? {
    PipelineResult::Completed(id) => id,
    PipelineResult::Detached => {
        drop(session);
        eprintln!("Session detached during loop stage.");
        return Ok(());
    }
};

// Review (if requested) — same session, no drop/re-create
let review_info = if review_after {
    match run_build_review(cwd, plan_path, &epic_id, fix_after,
        review_template, fix_template, session.as_mut())? {
        PipelineResult::Completed(info) => Some(info),
        PipelineResult::Detached => {
            drop(session);
            eprintln!("Session detached during review stage.");
            return Ok(());
        }
    }
} else {
    None
};

// Drop screen ONCE after all stages complete, then print to main terminal
drop(session);

// Re-read events and materialize graph to get final state
let events = read_events(cwd)?;
let graph = materialize_graph(&events);
let plan_graph = PlanGraph::build(&graph);

// Compute final epic ID (may differ from initial epic_id if recreated during build)
let final_epic = plan_graph.find_epic_for_plan(plan_path, &graph);
let final_epic_id = final_epic
    .map(|p| p.id.as_str())
    .unwrap_or(epic_id.as_str());

// Collect subtask references for the completion summary
let subtasks = final_epic
    .map(|p| get_subtasks(&graph, &p.id))
    .unwrap_or_default();
let subtask_refs: Vec<&Task> = subtasks.into_iter().collect();

if output_id {
    println!("{}", loop_task_id);
    println!("{}", final_epic_id);
    if let Some(ref info) = review_info {
        println!("{}", info.review_task_id);
    }
} else {
    output_build_completed(&loop_task_id, final_epic_id, &subtask_refs)?;
    if let Some(ref info) = review_info {
        output_build_review_completed(&info.review_task_id, plan_path, fix_after)?;
    }
}
```

### `run_build_review()` output deferral

When `session.is_some()`, `run_build_review` must NOT call `output_build_review_completed()` — the alternate screen is still active and stderr output would be invisible or corrupt the display. Instead, it returns `Ok(PipelineResult::Completed(info))` so the caller can extract the `BuildReviewInfo` and print after `drop(session)`. On detach, it returns `Ok(PipelineResult::Detached)`.

When `session.is_none()` (standalone `aiki review` or non-TTY), `run_build_review` calls `output_build_review_completed()` directly, preserving current behavior, and returns `Ok(PipelineResult::Completed(BuildReviewInfo { .. }))`.

### `run_build_epic()` — same single-session pattern (loop + optional review, one `drop(session)` at the end, same `output_id` branching, deferred review output, and early exit on `Detached` at each stage)

### `run_continue_async()` — no changes (background process, stderr null'd, no TTY)

---

## Behavior During Stage Transitions

Between stages (e.g., decompose agent completes → loop agent starting):

1. Previous monitor's `screen.run()` returns `ExitReason::TaskCompleted`
2. Screen stays in alternate mode showing the last rendered frame (static)
3. Orchestrator records `let setup_start = Instant::now()` immediately after `monitor_on_screen()` returns
4. Before each blocking sub-step (task creation, link writing, agent spawning), the orchestrator checks `setup_start.elapsed() > Duration::from_millis(500)`. If exceeded, it calls `session.screen().set_status("Preparing next stage…")` to provide visual feedback that work is in progress
5. Setup work happens (create loop task, write links, spawn agent) — ~<1s
6. New `StatusMonitor` takes over, calls `screen.run()` with new task's state. The first `screen.run()` call clears any active status overlay before rendering the new stage
7. First draw updates the display with new stage progress

**Timing ownership:** The orchestrator (`run_build_plan()` and `run_build_epic()`) owns the timing measurement using `std::time::Instant`. The `LiveScreen` owns the rendering of the status overlay via its `set_status()` method. The orchestrator decides *when* to show status; the `LiveScreen` decides *how* to render it.

**`LiveScreen::set_status(&mut self, msg: &str)`** — renders a single-line status overlay on the current static frame. The overlay is positioned at a fixed location (e.g., bottom of the frame) and does not interfere with the existing rendered content. Calling `set_status("")` or starting a new `screen.run()` clears the overlay.

This satisfies the smooth-transitions criterion (#2): the alternate screen is entered exactly once and exited exactly once, no writes reach the main terminal between enter and exit, and the `LiveScreen` instance is reused across stages without drop/recreate.

---

## What Stays the Same

- `task_run()` signature (15+ callers unaffected)
- `LiveScreen` core implementation (unchanged except for new `set_status()` method)
- Non-TTY / async paths (session = None, existing behavior)
- One-shot renders (`aiki task show`, `aiki build show`)
- Widget/view layer
- `BlitWidget` adapter
- Agent stderr capture

---

## Files Changed

| File | Change |
|------|--------|
| `cli/src/tasks/runner.rs` | Add `ScreenSession`, `SessionOutcome` enum, `PipelineResult<T>` enum, `task_run_on_session()`, change `handle_session_result()` return to `Result<SessionOutcome>`, extract helpers |
| `cli/src/tasks/status_monitor.rs` | Add `monitor_on_screen()`, `new_with_stop_flag()`, factor `run_event_loop()` |
| `cli/src/commands/build.rs` | Create `ScreenSession` in sync paths, pass through. Early exit on `Detached` at each stage (drop session, print detach message, skip remaining stages). `run_build_review` returns `PipelineResult<BuildReviewInfo>` and skips `output_build_review_completed()` when `session.is_some()`. `output_build_completed()` only called when final stage returns `Completed`. Add `output_id` branching after `drop(session)` |
| `cli/src/commands/loop_cmd.rs` | Add `session` param to `run_loop()`, conditional output |
| `cli/src/commands/decompose.rs` | Add `session` param to `run_decompose()` |
| `cli/src/commands/epic.rs` | Add `session` param to `find_or_create_epic()`, `create_epic()` |
| `cli/src/tui/live_screen.rs` | Add `set_status(&mut self, msg: &str)` method — renders a single-line status overlay on the current frame; cleared on next `run()` call |
| `cli/src/commands/fix.rs` | Handle `PipelineResult` returned by `run_decompose()` and `run_loop()` — early exit on `Detached` (same pattern as build.rs) |

---

## Acceptance Criteria

1. **Single alternate screen** — `aiki build plan.md` enters alternate screen once and exits once, regardless of how many stages run
2. **Smooth transitions** — between decompose → loop → review stages: (a) the alternate screen is entered exactly once and exited exactly once for the entire pipeline, (b) no writes occur to the main terminal between enter and exit, and (c) the `LiveScreen` instance is reused across stages without drop/recreate
3. **Terminal restoration** — all exit paths (normal, Ctrl+C, panic) restore the terminal correctly. `ScreenSession::Drop` handles cleanup. `task_run_on_session()` does NOT call `restore_terminal()` — panic propagation via `resume_unwind` ensures `ScreenSession::Drop` runs
4. **Non-TTY unaffected** — piped/CI builds work exactly as before (no screen)
5. **Existing callers unaffected** — `task_run()` API unchanged, all 15+ callers compile without modification
6. **Final summary visible** — after alternate screen exits, build completion summary prints to main terminal scrollback
7. **Result semantics preserved** — `handle_session_result()` returns `Result<SessionOutcome>` for internal agent-session result handling. Pipeline functions return `Result<PipelineResult<T>>` where `T` is the stage-specific payload (e.g., epic ID, loop task ID, `BuildReviewInfo`), allowing callers to both detect detach and extract stage outputs without side channels. Stop events, cascade-close, and error propagation are preserved via `Err` returns
8. **Intermediate-stage failure** — when a stage returns `AgentSessionResult::Failed` while in session mode, the terminal is correctly restored via `ScreenSession::Drop`, `TaskEvent::Stopped` is emitted, and cascade-close propagates to remaining orchestrator subtasks
9. **Detach propagation** — when any stage returns `PipelineResult::Detached`, the orchestrator drops the `ScreenSession` (restoring terminal), prints a detach message, and exits early without calling `output_build_completed()` or running subsequent stages. Stage payloads are extracted via `PipelineResult::Completed(payload)` pattern matching
10. **Review output deferred** — `run_build_review` does not call `output_build_review_completed()` while the shared alternate screen is active; review summary prints to main terminal only after `drop(session)`
11. **`--output id` preserves ID-only contract** — when `--output id` is set, no intermediate or stage-level output may be written to stdout while the session is active. All stdout output is deferred to after `drop(session)`, at which point only task IDs are printed (loop task ID, epic ID, and review task ID when applicable), with no human-readable summaries intermixed
12. **No false completion** — `output_build_completed()` is only called when the final pipeline stage returns `Completed`. If any stage returns `Detached`, `Stopped`, or `Failed`, the pipeline exits early and `output_build_completed()` is never called
13. **`stop_flag` lifecycle** — the `stop_flag` (`Arc<AtomicBool>`) is set to `true` once when SIGINT fires and is never reset between stages. This is safe because the pipeline short-circuits on detach: once `monitor_on_screen()` observes `stop_flag == true` and returns `UserDetached`, the orchestrator exits early without running subsequent stages, so the stale `true` value is never observed by a later stage
14. **Slow-transition feedback** — the orchestrator measures elapsed time during inter-stage setup using `Instant::now()`. If setup exceeds 500ms, the orchestrator calls `session.screen().set_status()` to display a transition indicator on the static frame. The `LiveScreen` owns the rendering via `set_status()`; the orchestrator owns the timing

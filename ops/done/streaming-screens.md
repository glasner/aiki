---

---

# Streaming Screens: ratatui Terminal for Live Task Output

**Date**: 2026-03-06
**Status**: Draft
**Purpose**: Replace cursor-up ANSI redraw with a proper ratatui `Terminal` to fix unreliable in-place rendering during `aiki task run`, `aiki review`, `aiki build`, etc.

---

## Executive Summary

The current rendering pipeline builds ratatui `Buffer`s, converts them to ANSI strings with `buffer_to_ansi()`, and manually manages cursor position with `MoveUp(n)` + `Clear(FromCursorDown)`. This is fundamentally fragile — any miscalculation of terminal line count, any interleaved stderr output, or any terminal resize causes the display to corrupt and never recover.

The fix: use ratatui's `Terminal` with a `CrosstermBackend` in **alternate screen mode** during live monitoring. This gives us diff-based rendering, automatic resize handling, and complete isolation from other terminal output. The existing widget layer (`EpicTree`, `StageList`, `LaneDag`, etc.) remains unchanged — only the rendering host changes.

---

## Problem: Why Cursor-Up Breaks

`StatusMonitor::render_task_tree()` (status_monitor.rs:179) does this every 500ms:

```rust
stderr.execute(MoveUp(self.last_line_count))?;
stderr.execute(Clear(ClearType::FromCursorDown))?;
// ... render new frame ...
```

This fails when:

1. **Line count miscalculation** — `count_terminal_lines()` must perfectly predict how many terminal rows the ANSI string occupies, accounting for wrapping, ANSI escape widths, and terminal-specific deferred-wrap behavior. Any error compounds: once off by 1 line, every subsequent frame overwrites the wrong region.

2. **Interleaved output** — `eprintln!("Spawning...")` prints before the monitor starts. Agent stderr may leak through. Any output between frames shifts the cursor position, but `last_line_count` doesn't know about it.

3. **Terminal resize** — If the user resizes their terminal mid-render, lines that wrapped at the old width now wrap differently. The stored `last_line_count` is wrong for the new geometry.

4. **Cascading corruption** — Unlike alternate screen (where each frame is a fresh canvas), cursor-up errors are permanent. Once a frame partially overwrites the wrong lines, all subsequent frames compound the damage.

These aren't fixable bugs — they're inherent to the cursor-up approach. The only reliable way to do in-place terminal rendering is to let a TUI framework manage the screen.

---

## Solution: Alternate Screen with ratatui Terminal

### During live monitoring (task running)

Enter alternate screen mode. Ratatui's `Terminal` takes full control:

```
┌──────────────────────────────────────────────┐
│ ops/now/ feature.md                          │
│                                              │
│ [luppzupt] Implement webhooks                │
│ ⎿ ✓   4 subtasks                             │
│                                              │
│ ✓ build  4/4                                 │
│ ▸ review  0:34                          cc   │
│    ▸ explore  0:34                           │
│    ○ criteria                                │
│    ○ record-issues                           │
│ ○ fix                                        │
│                                              │
│                                              │
│                                              │
│                                              │
│                           [Ctrl+C to detach] │
└──────────────────────────────────────────────┘
```

Benefits:
- **No cursor math** — ratatui handles all positioning
- **Diff rendering** — only changed cells are redrawn (no flicker)
- **Resize handling** — ratatui redraws on `Event::Resize`
- **Output isolation** — agent stderr can't corrupt the display
- **Full canvas** — can use the entire terminal, not a fixed 80-col buffer

### After task completes

Exit alternate screen. Print a final summary to the main terminal:

```
Task run complete
Summary: Review complete (0 issues). No defects found.
 ops/now/feature.md

 [luppzupt] Implement webhooks
 ⎿ ✓   4 subtasks

 ✓ build  4/4
 ✓ review  approved
 ─ fix
```

This stays in scrollback — the user can scroll up to see it. The alternate screen content is gone (by design — it was a live view, not a record).

### After user detaches (Ctrl+C)

Exit alternate screen. Print a short detach notice — the task is still running in the background:

```
Detached from task luppzupt — task is still running in background.
Run `aiki task show luppzupt` to check status.
```

No workflow snapshot or "Task run complete" message — the task hasn't completed. The user can re-attach or check status later.

---

## Architecture

### What stays the same

The entire widget and view layer is untouched:

| Layer | Files | Changes |
|-------|-------|---------|
| **Widgets** | `epic_tree.rs`, `stage_list.rs`, `stage_track.rs`, `path_line.rs`, `lane_dag.rs`, `breadcrumb.rs` | None |
| **Views** | `workflow.rs`, `issue_list.rs`, `epic_show.rs` | None |
| **Builder** | `builder.rs` | None |
| **Types** | `types.rs` | None |
| **Theme** | `theme.rs` | None |

These already implement ratatui's `Widget` trait. They render to `Buffer` via `render(area, &mut buf)`. That's exactly what `Terminal::draw()` expects.

### What changes

| Component | Current | New |
|-----------|---------|-----|
| **Rendering host** | `buffer_to_ansi()` → `stderr.write_all()` → `MoveUp(n)` | `Terminal::draw(\|f\| { ... })` in alternate screen |
| **Cursor management** | Manual `MoveUp` + `Clear` + `count_terminal_lines()` | Managed by ratatui |
| **Event loop** | `thread::sleep(500ms)` + `poll_and_display()` | crossterm `event::poll()` + `event::read()` |
| **Resize handling** | None (line count becomes stale) | `Event::Resize` → redraw |
| **Post-completion output** | Part of the same stderr stream | Separate: exit alt screen, then `buffer_to_ansi()` for final render |

### `buffer_to_ansi()` stays

It's still needed for:
- Post-completion summary printed to the main terminal
- `aiki build show`, `aiki task show` (one-shot renders, no live updating)
- Non-TTY output (piped, CI)
- Tests

---

## Design: `LiveScreen`

A reusable component that manages the alternate screen lifecycle:

```rust
pub struct LiveScreen {
    terminal: Terminal<CrosstermBackend<Stderr>>,
    poll_interval: Duration,
}

impl LiveScreen {
    /// Enter alternate screen, enable raw mode
    pub fn new() -> Result<Self>;

    /// Draw a single frame. Caller provides the widget/view to render.
    /// Uses ratatui's diff-based rendering internally.
    pub fn draw<F>(&mut self, render_fn: F) -> Result<()>
    where F: FnOnce(&mut Frame);

    /// Run the monitor loop until exit condition.
    /// Polls for events (resize, key) and calls the update function.
    pub fn run<F>(&mut self, update_fn: F) -> Result<ExitReason>
    where F: FnMut(&mut Self) -> Result<Option<ExitReason>>;
}

impl Drop for LiveScreen {
    /// Show cursor, exit alternate screen, disable raw mode (always runs, even on panic).
    /// All three operations are idempotent — safe to call even if already in restored state.
    fn drop(&mut self);
}
```

### Screen layout

The `LiveScreen` owns the full terminal canvas. The layout is:

```
┌─────────────────────────────────────────────────────────┐
│ ┌─────────────────────────────────────────────────────┐ │
│ │                   WorkflowView                      │ │ ← rendered by existing
│ │  (path, epic tree, stages, lane DAG)                │ │    render_workflow()
│ └─────────────────────────────────────────────────────┘ │
│                                                         │
│                                                         │ ← flexible spacing
│                                                         │
│                            [Ctrl+C to detach]           │ ← bottom-anchored footer
└─────────────────────────────────────────────────────────┘
```

The workflow view renders into the top portion. The footer is pinned to the bottom. Vertical space between them flexes with terminal height.

### Event loop

```rust
fn run_screen(
    screen: &mut LiveScreen,
    monitor: &mut TaskMonitor,
    child: &mut MonitoredChild,
) -> Result<ExitReason> {
    let poll_interval = std::env::var("AIKI_STATUS_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_millis(500));

    screen.run(|screen| {
        // 1. Check crossterm events (resize, Ctrl+C)
        //    In raw mode, Ctrl+C is delivered as a crossterm key event, NOT as
        //    SIGINT. A scoped stop_flag (Arc<AtomicBool>) exists for the windows
        //    outside raw mode (startup/teardown), but while the event loop is
        //    running in raw mode, this crossterm key event is the Ctrl+C path.
        //    See "Signal handling migration" section for the full two-mechanism
        //    approach.
        if event::poll(poll_interval)? {
            match event::read()? {
                Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. }) => {
                    return Ok(Some(ExitReason::UserDetached));
                }
                Event::Resize(_, _) => {} // ratatui redraws automatically
                _ => {}
            }
        }

        // 2. Check if agent process exited
        match child.try_wait() {
            Ok(Some(_exit_status)) => {
                // Agent exited — capture stderr, then do bounded reconciliation
                // to allow task-state propagation before classifying the exit.
                //
                // Why retry? The agent may have closed the task just before
                // exiting, but the state store (JJ commit / file flush) hasn't
                // propagated yet.  A single poll can see stale "in-progress"
                // state and misclassify the run as AgentExited instead of
                // TaskCompleted.  We retry up to RECONCILE_RETRIES times with
                // RECONCILE_DELAY_MS between polls to give the store time to
                // catch up.
                const RECONCILE_RETRIES: usize = 5;
                const RECONCILE_DELAY_MS: u64 = 200;

                let stderr = child.read_stderr();
                for _ in 0..RECONCILE_RETRIES {
                    let (view, is_terminal) = monitor.poll()?;
                    screen.draw(|f| { /* final render */ })?;
                    if is_terminal {
                        return Ok(Some(ExitReason::TaskCompleted));
                    }
                    std::thread::sleep(Duration::from_millis(RECONCILE_DELAY_MS));
                }
                // All retries exhausted — state never reached terminal; treat
                // as an unexpected agent exit.
                return Ok(Some(ExitReason::AgentExited { stderr }));
            }
            Ok(None) => {} // still running, continue
            Err(e) => {
                // Error checking process status — treat as exited.
                // Apply the same bounded reconciliation as the normal-exit
                // branch so we don't misclassify due to state-propagation lag.
                const RECONCILE_RETRIES: usize = 5;
                const RECONCILE_DELAY_MS: u64 = 200;

                let stderr = child.read_stderr();
                for _ in 0..RECONCILE_RETRIES {
                    let (_, is_terminal) = monitor.poll()?;
                    if is_terminal {
                        return Ok(Some(ExitReason::TaskCompleted));
                    }
                    std::thread::sleep(Duration::from_millis(RECONCILE_DELAY_MS));
                }
                // Retries exhausted — classify as unexpected exit.
                return Ok(Some(ExitReason::AgentExited { stderr }));
            }
        }

        // 3. Poll task state
        let (view, is_terminal) = monitor.poll()?;

        // 4. Draw
        screen.draw(|f| {
            let chunks = Layout::vertical([
                Constraint::Min(0),     // workflow view
                Constraint::Length(1),  // footer
            ]).split(f.area());

            let buf = render_workflow(&view, &theme);
            // render buf into chunks[0]

            let footer = Line::from(" [Ctrl+C to detach]").right_aligned();
            f.render_widget(footer, chunks[1]);
        })?;

        if is_terminal {
            Ok(Some(ExitReason::TaskCompleted))
        } else {
            Ok(None)
        }
    })
}
```

### Key detail: Widget rendering into Frame

Currently `render_workflow()` returns a `Buffer`. To use it inside `Terminal::draw()`, we have two options:

**Option A: Blit the buffer into the frame.** Render to a standalone `Buffer` (as today), then copy cells into the frame's buffer. Simple, preserves the existing view API.

**Option B: Refactor views to render directly into Frame.** Change `render_workflow()` to accept a `&mut Frame` and area. Cleaner long-term, but touches more code.

**Recommendation: Option A for Phase 1.** A `BlitWidget` adapter that wraps a `Buffer` and implements `Widget` by copying cells:

```rust
struct BlitWidget {
    buf: Buffer,
}

impl Widget for BlitWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let src = &self.buf;
        for y in 0..area.height.min(src.area.height) {
            for x in 0..area.width.min(src.area.width) {
                *buf.cell_mut((area.x + x, area.y + y)).unwrap()
                    = src.cell((x, y)).unwrap().clone();
            }
        }
    }
}
```

This lets us use `render_workflow()` unchanged inside `Terminal::draw()`.

---

## Integration Points

### `StatusMonitor` → `LiveScreen`

`StatusMonitor` currently does two things:
1. Polls task state (reads events, materializes graph, builds view)
2. Renders to terminal (cursor-up, ANSI output)

Split these:
- **`TaskMonitor`** — the polling/state logic (extracted from `StatusMonitor`)
- **`LiveScreen`** — the rendering host (new)

`StatusMonitor::monitor_until_complete_with_child()` becomes a thin wrapper that creates a `LiveScreen`, runs the event loop, and handles the child process lifecycle.

### `runner.rs` integration

```rust
// Current (runner.rs:193-199)
let result = if show_status {
    run_with_status_monitor(cwd, task_id, runtime, &spawn_options)?
} else {
    runtime.spawn_blocking(&spawn_options)?
};

// New: same interface, LiveScreen is internal to run_with_status_monitor
```

The runner doesn't need to know about `LiveScreen`. The `show_status` branch still calls `run_with_status_monitor()`, which internally creates a `LiveScreen` instead of doing cursor-up rendering.

**Signal handling migration:** During live monitoring, raw mode is active and Ctrl+C is delivered as a crossterm key event (not SIGINT), so the event loop's `ExitReason::UserDetached` is the primary Ctrl+C path. However, there are windows outside raw mode — startup before `LiveScreen::new()`, teardown after `LiveScreen` drops, and error paths that exit early — where SIGINT is still the delivery mechanism.

Use a **scoped signal handler** that is registered just before entering the monitor and unregistered after exiting. This confines custom SIGINT handling to the monitoring window only — non-monitor commands (`aiki task list`, `aiki task show`, one-shot renders, etc.) are completely unaffected and retain default SIGINT behavior (immediate process termination).

`run_with_status_monitor` registers a SIGINT handler using `signal_hook::flag::register(SIGINT, stop_flag.clone())`, which returns a `SigId`. The `stop_flag` is a per-run `Arc<AtomicBool>` that is checked in the brief windows outside raw mode (startup before `LiveScreen::new()`, teardown after `LiveScreen` drops, and error paths that exit early). After `LiveScreen` drops and monitoring is complete, `signal_hook::low_level::unregister(sig_id)` restores default SIGINT behavior. Use `signal_hook` (not `ctrlc`) because `signal_hook::flag::register` supports multiple registrations and returns a `SigId` for targeted unregistration, whereas `ctrlc::set_handler` is process-global, can only be called once, and provides no way to restore default behavior.

```rust
fn run_with_status_monitor(...) -> Result<...> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    // Register scoped SIGINT handler — only active during monitoring
    let sig_id = signal_hook::flag::register(signal_hook::consts::SIGINT, stop_flag.clone())?;

    // ... LiveScreen lifecycle, event loop, etc. ...

    // After LiveScreen drops, unregister to restore default SIGINT behavior
    signal_hook::low_level::unregister(sig_id);

    // Non-monitor code paths never see this handler
    result
}
```

The `stop_flag` is checked in `run_with_status_monitor` to detect whether a SIGINT arrived outside raw mode (e.g., during startup or teardown). During raw mode, Ctrl+C is handled by the crossterm event loop as `ExitReason::UserDetached`. This two-mechanism approach covers all windows without leaking signal handling into unrelated commands.

**Platform scope:** Signal handling is Unix-only (macOS, Linux). Windows is not a supported platform for interactive task monitoring.

### Post-completion output

**Output ownership:** `run_with_status_monitor()` returns structured data but does NOT print anything after the live screen exits. All post-completion printing is owned by `task_run()` in `runner.rs:204-210`, which is the canonical post-completion output point.

After `LiveScreen` drops (alternate screen exits), `run_with_status_monitor()` returns the `ExitReason` and `AgentSessionResult` to `task_run()`. The existing `task_run()` match block (`runner.rs:203-210`) then handles printing:

- **`ExitReason::TaskCompleted`** → falls through to existing `AgentSessionResult::Completed` arm, which prints "Task run complete" and summary
- **`ExitReason::UserDetached`** → `task_run()` should add a new arm (or early return) to print detach messaging instead:

```rust
// In task_run(), after run_with_status_monitor() returns:
if exit_reason == ExitReason::UserDetached {
    eprintln!("Detached from task {} — task is still running in background.", task_id_short);
    eprintln!("Run `aiki task show {}` to check status.", task_id_short);
    return Ok(());
}

// Existing post-completion output (runner.rs:203-210) handles TaskCompleted:
match &result {
    AgentSessionResult::Completed { summary } => {
        if !options.quiet {
            eprintln!("Task run complete");
            // ...
        }
    }
    // ...
}
```

**Note:** `run_with_status_monitor()` must NOT contain any `eprintln!` calls after `LiveScreen` drops. It returns the result; `task_run()` prints it.

### `review.rs` and `build.rs` — automatic coverage

Both `review.rs` and `build.rs` use `task_run()` → `run_with_status_monitor()`, so migrating `StatusMonitor` to `LiveScreen` in `runner.rs` automatically covers these callers. No separate integration is needed.

### Non-TTY fallback

When `stderr().is_terminal()` is false, skip `LiveScreen` entirely. Use the existing simple blocking spawn with no status display. This preserves CI/pipe compatibility.

---

## Dynamic Width

Currently all views are hardcoded to 80 columns (`const WIDTH: u16 = 80` in workflow.rs). With alternate screen, the full terminal width is available.

**Phase 1:** Keep 80-col rendering, center it in the terminal (or left-align with padding). The views already work at 80 cols and don't need changes.

**Phase 2 (polish):** Pass terminal width to `render_workflow()` so views can use the full width. This would require updating the width constant to be a parameter, but the widget implementations already use relative positioning within their allocated `Rect`.

---

## Implementation Plan

### Phase 1: `LiveScreen` core + `StatusMonitor` migration

**New file: `cli/src/tui/live_screen.rs`**

1. Implement `LiveScreen` struct with:
   - `new()` — sequentially enable raw mode, enter alternate screen, and create `Terminal<CrosstermBackend<Stderr>>`. Each step must roll back all previously-applied state changes if a subsequent step fails, because the struct is never fully constructed and `Drop` will not run. The `Drop` impl only handles cleanup for successfully-constructed instances.

     ```rust
     pub fn new() -> Result<Self> {
         enable_raw_mode()?;
         if let Err(e) = execute!(stderr(), EnterAlternateScreen, Hide) {
             let _ = disable_raw_mode();
             return Err(e.into());
         }
         match Terminal::new(CrosstermBackend::new(stderr())) {
             Ok(terminal) => Ok(Self { terminal }),
             Err(e) => {
                 let _ = execute!(stderr(), Show, LeaveAlternateScreen);
                 let _ = disable_raw_mode();
                 Err(e.into())
             }
         }
     }
     ```

   - `draw()` — wrapper around `Terminal::draw()`
   - `run()` — event loop with crossterm event polling
   - `Drop` impl — show cursor, leave alternate screen, disable raw mode (infallible, idempotent cleanup). Only runs for fully-constructed instances; partial-failure rollback is handled within `new()` itself.

2. Implement `BlitWidget` adapter for rendering existing `Buffer`s into frames.

3. Add `tui::live_screen` module to `tui/mod.rs`.

**Modified file: `cli/src/tasks/status_monitor.rs`**

4. Extract task-polling logic into `TaskMonitor` (or keep in `StatusMonitor` and just replace the rendering path).

5. Replace `render_task_tree()` internals: instead of cursor-up + ANSI write, call `LiveScreen::draw()` with the workflow view buffer via `BlitWidget`.

6. **Audit and remove all direct `eprintln!`/stderr writes from code paths active during alternate screen rendering.** In raw/alternate mode, any direct write to stderr bypasses ratatui and corrupts the screen.
   - **`status_monitor.rs:152`** — the `eprintln!` in the error handler must be removed. Errors during monitoring should be collected into a `Vec<String>` and displayed after the screen exits, or rendered as a status line within the TUI.
   - Audit all other direct stderr writes in the monitor/runner hot path (`status_monitor.rs`, `runner.rs`, and any helpers they call) for `eprintln!`, `writeln!(stderr(), ...)`, or similar.
   - Any diagnostic output needed during live monitoring must go through `LiveScreen::draw()`, not direct stderr writes.
   - **Enforceable guardrails** (prevent future regressions, not just one-time audit):
     - **CI grep-check:** Add a CI script (or extend an existing one) that scans all files in the tasks directory and `live_screen.rs` for direct stderr write patterns (`eprintln!`, `eprint!`, `writeln!(stderr`). The check fails the build if any matches are found outside of lines annotated with `// stderr-ok: <reason>` (for justified exceptions like pre-alternate-screen messages) or guarded by `#[cfg(test)]`. The scope covers the full `cli/src/tasks/` directory (not just `runner.rs` and `status_monitor.rs`) because helpers called from the runner/monitor hot path may also write to stderr, and any such write corrupts the TUI while the alternate screen is active. Example implementation: `grep -rn -E 'eprintln!|eprint!|writeln!\(stderr' cli/src/tasks/*.rs cli/src/tui/live_screen.rs | grep -v 'stderr-ok' | grep -v '#\[cfg(test)\]'` — fail if output is non-empty.
     - **Runtime debug_assert guard:** Add a thread-local `LIVE_SCREEN_ACTIVE: Cell<bool>` flag in `live_screen.rs`. Set it to `true` in `LiveScreen::new()` and back to `false` in `Drop`. Provide a `debug_assert_no_live_screen!()` macro that asserts `!LIVE_SCREEN_ACTIVE.get()`. Place this assertion at the top of any remaining raw stderr write helper functions. This catches violations in dev/test builds at the point of the illegal write.

7. Replace the `thread::sleep` loop in `monitor_until_complete_with_child()` with the crossterm event loop from `LiveScreen::run()`, integrating child process monitoring.

8. Delete `count_terminal_lines()` and the `last_line_count` field — no longer needed.

**Modified file: `cli/src/tasks/runner.rs`**

9. Update `run_with_status_monitor()` to return structured data (`ExitReason` + `AgentSessionResult`) after the live screen exits, but **do NOT print any post-completion output** from within this function. Post-completion printing is owned by `task_run()` (see `runner.rs:204-210`), which already handles "Task run complete" and summary output. `run_with_status_monitor()` must only return the result so the caller can decide what to print.

10. Move the `eprintln!("Spawning...")` message to emit *before* `LiveScreen::new()` — it MUST be printed before entering alternate screen mode. In raw/alternate mode, direct `eprintln!` writes corrupt the TUI frame, so this message cannot be rendered "inside" the live screen via `eprintln!`.

11. **Verify terminal-state restoration on all exit paths.** The `Drop` impl for `LiveScreen` must reliably restore the terminal (show cursor, leave alternate screen, disable raw mode) on every exit path:
    - **Normal exit** (task completes) — `LiveScreen` drops normally, terminal restored
    - **User detach** (Ctrl+C) — event loop returns `ExitReason::UserDetached`, `LiveScreen` drops, terminal restored
    - **Panic during rendering** — `Drop` runs during unwinding, terminal restored
    - **Error in event loop** — `?` propagates error, `LiveScreen` drops, terminal restored

    **Idempotency contract:** All three cleanup operations are idempotent — safe to call even if already in the restored state:
    - `disable_raw_mode()` — checks an internal `Option<Termios>` guard; returns `Ok(())` immediately if raw mode was never enabled or already disabled ([source](https://github.com/crossterm-rs/crossterm/blob/master/src/terminal/sys/unix.rs)).
    - `LeaveAlternateScreen` — emits `\x1b[?1049l`, a standard ANSI escape. Terminals treat this as a no-op when already on the main screen.
    - `Show` (cursor) — emits `\x1b[?25h`. Terminals treat this as a no-op when the cursor is already visible.

    This means `Drop` and the defense-in-depth wrapper can both call the full cleanup sequence without risk of double-cleanup errors or inconsistent terminal state.

    **Defense-in-depth:** Rely on `Drop` as the primary cleanup mechanism, but wrap the event loop in `std::panic::catch_unwind` as a safety net for panics that would otherwise unwind past the event loop:

    > **Note:** `catch_unwind` does not handle `abort`, `SIGKILL`, or `panic=abort` — these are unrecoverable by any in-process mechanism and are accepted limitations (see manual testing table).

    ```rust
    pub fn run_with_live_screen(...) -> Result<...> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut screen = LiveScreen::new()?;
            screen.run(|screen| { /* event loop */ })
        }));
        // Defensive cleanup — always runs, even if catch_unwind caught a panic.
        // Idempotent: safe even when Drop already cleaned up.
        let _ = execute!(stderr(), Show);  // restore cursor visibility
        let _ = execute!(stderr(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        match result {
            Ok(inner) => inner,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    ```

    The `Drop` impl handles normal/error paths; `catch_unwind` acts as a safety net for panics. No extra dependency needed.

**Note:** `review.rs` and `build.rs` do not need separate `LiveScreen` integration. Both use `task_run()` → `run_with_status_monitor()`, so migrating `StatusMonitor` to `LiveScreen` in this phase automatically covers all callers (reviews, builds, and direct task runs).

### Terminal-State Restoration: Manual Testing

Before merging, verify terminal restoration manually on each exit path:

| Exit Path | How to Test | Expected Result |
|-----------|-------------|-----------------|
| Normal exit | Run a task to completion | Terminal returns to normal mode, shell prompt works correctly |
| User detach | Press Ctrl+C during a running task | Terminal returns to normal, cursor visible, input echoed |
| Panic | Inject `panic!("test")` in the render loop, run a task | Terminal restored (Drop runs during unwind), panic message visible |
| `kill -9` | Run a task, `kill -9 <pid>` from another terminal | Terminal stuck in raw mode — recoverable with `reset` command. Document this as a known limitation (no process can recover from SIGKILL). |
| Error in poll | Simulate an error in `monitor.poll()` (e.g., return `Err`) | Error propagates, `LiveScreen` drops, terminal restored, error message printed |

**Automated tests (`#[cfg(test)]`):**

Terminal state changes (`enable_raw_mode`, `EnterAlternateScreen`, etc.) are crossterm operations that bypass the ratatui backend — `TestBackend` cannot observe them. Automated tests should therefore focus on:

1. **Rendering logic** — Use `TestBackend` to verify that frames render correctly (layout, content, styling).
2. **Cleanup call verification** — Extract the cleanup sequence into a testable function and use a `#[cfg(test)]` flag to verify it runs on all exit paths.

```rust
/// Standalone cleanup function — called by Drop and by the defense-in-depth wrapper.
/// Extracting this makes the cleanup sequence unit-testable.
pub(crate) fn restore_terminal() {
    #[cfg(test)]
    CLEANUP_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);

    let _ = execute!(std::io::stderr(), cursor::Show);
    let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

#[cfg(test)]
static CLEANUP_CALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: reset the cleanup flag before each test.
    fn reset_cleanup_flag() {
        CLEANUP_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Verify that dropping a LiveScreen triggers restore_terminal().
    /// We don't assert on actual terminal state (that's crossterm's job);
    /// we verify our cleanup logic is invoked.
    #[test]
    fn drop_calls_restore_terminal() {
        reset_cleanup_flag();
        {
            let _screen = LiveScreen::new_test(); // test-only constructor with TestBackend
        } // _screen drops here
        assert!(CLEANUP_CALLED.load(std::sync::atomic::Ordering::SeqCst),
            "restore_terminal() should be called on drop");
    }

    /// Verify that Drop (and thus restore_terminal) executes during unwinding.
    #[test]
    fn drop_runs_on_panic() {
        reset_cleanup_flag();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _screen = LiveScreen::new_test();
            panic!("simulated panic");
        }));
        assert!(result.is_err());
        assert!(CLEANUP_CALLED.load(std::sync::atomic::Ordering::SeqCst),
            "restore_terminal() should run even during panic unwind");
    }

    /// Verify rendering output using TestBackend (layout, content, etc.).
    #[test]
    fn renders_status_frame() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        // ... render a frame with test data, assert on buffer contents
    }
}
```

**Note:** Terminal state restoration itself (raw mode, alternate screen, cursor visibility) is best verified via the manual test matrix above. The automated tests verify that our cleanup *logic* is invoked on every exit path — the actual terminal operations are crossterm's responsibility and are well-tested upstream.

### Phase 2: Polish

12. Add terminal width passthrough (replace hardcoded 80-col width).

13. Add a status line to the footer showing elapsed time, agent type, etc.

14. Consider adding key bindings (e.g., `q` to detach, scroll through subtask details).

---

## Acceptance Criteria

All criteria must be met before the implementation is considered complete:

1. **Terminal lifecycle management** — `LiveScreen::new()` enters alternate screen, hides cursor, and enables raw mode; `Drop` reliably restores the terminal (shows cursor, leaves alternate screen, disables raw mode) on all exit paths: normal completion, Ctrl+C detach, panic during rendering, and error propagation. All cleanup operations are idempotent, so the `catch_unwind` defense-in-depth wrapper can safely repeat them even after `Drop` has already run.

2. **Constructor rollback on partial failure** — If `LiveScreen::new()` succeeds at enabling raw mode but fails at entering alternate screen (or vice versa), the constructor rolls back the completed step. No code path leaves the terminal in a partially-initialized state (e.g., raw mode enabled but alternate screen not entered).

3. **No direct stderr writes during alternate screen** — Zero `eprintln!`, `writeln!(stderr(), ...)`, or other direct stderr writes execute while `LiveScreen` is active. All diagnostic output during live monitoring goes through `LiveScreen::draw()` or is collected into a buffer and displayed after the screen exits. This invariant is enforced by both a CI grep-check (catches new violations at review time) and a runtime `debug_assert` guard (catches violations in dev/test builds). See implementation step 6 for details.

4. **Manual testing matrix passes** — All five exit paths pass manual verification:
   - Normal exit (task completes): terminal returns to normal mode, shell prompt works correctly
   - User detach (Ctrl+C): terminal restored, cursor visible, input echoed
   - Panic in render loop: terminal restored via `Drop` during unwind, panic message visible
   - Error in poll: error propagates, `LiveScreen` drops, terminal restored, error printed
   - `kill -9`: terminal stuck in raw mode — documented as known limitation recoverable with `reset`

5. **Existing widgets render correctly via BlitWidget** — `EpicTree`, `StageList`, and `LaneDag` produce identical visual output when rendered through the `BlitWidget` adapter inside `Terminal::draw()` as they do through the current `buffer_to_ansi()` path.

6. **Dead code removed** — `count_terminal_lines()` function and the `last_line_count` field are deleted from `status_monitor.rs`. No references to either remain in the codebase.

7. **`kill -9` documented as known limitation** — The `kill -9` (SIGKILL) exit path is explicitly documented as a known limitation: the terminal will be left in raw mode because no process can intercept SIGKILL, and the user can recover with the `reset` command.

8. **Stderr guardrails enforced** — A CI grep-check scans all files in `cli/src/tasks/` and `cli/src/tui/live_screen.rs` for direct stderr write patterns and fails the build on violations (with an escape hatch for annotated exceptions). A thread-local `LIVE_SCREEN_ACTIVE` flag and `debug_assert_no_live_screen!()` macro catch runtime violations in dev/test builds.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/tui/live_screen.rs` | **NEW** — `LiveScreen`, `BlitWidget`, alternate screen lifecycle |
| `cli/src/tui/mod.rs` | Add `pub mod live_screen;` |
| `cli/src/tasks/status_monitor.rs` | Replace cursor-up rendering with `LiveScreen::draw()`, delete `count_terminal_lines()` |
| `cli/src/tasks/runner.rs` | Update post-completion output flow (after alt screen exit) |
| `cli/src/commands/review.rs` | No changes needed — covered by `run_with_status_monitor()` migration in runner.rs |
| `cli/src/commands/build.rs` | No changes needed — covered by `run_with_status_monitor()` migration in runner.rs |
| `cli/Cargo.toml` | Possibly no change — `ratatui` and `crossterm` already dependencies |

---

## What This Doesn't Change

- **One-shot renders** (`aiki build show`, `aiki task show`, `aiki review issues`) still use `buffer_to_ansi()` → `eprintln!`. These are not live-updating and don't need alternate screen.
- **Async output** (`--async` flag) still prints IDs to stdout with no TUI. No live monitoring happens.
- **Non-TTY output** — still detected via `stderr().is_terminal()`, still skips all TUI rendering.
- **Widget implementations** — all existing widgets render unchanged.
- **View builders** — `build_workflow_view()`, `render_workflow()`, etc. unchanged.
- **Theme system** — unchanged.
- **Test infrastructure** — `render_png.rs` and buffer-based tests unchanged.

---

## Risk: Raw Mode + Stderr Corruption

In alternate screen / raw mode, all terminal I/O must go through ratatui. Any direct stderr write — whether from the spawned agent or from the CLI's own code — would corrupt the alternate screen.

### Agent stderr

**Mitigation:** The agent process already runs with redirected stderr (captured by `MonitoredChild`). Agent stderr is collected, not displayed live. This is the existing behavior and doesn't change.

If future agents need live stderr streaming, it could be rendered as a log pane within the alternate screen (similar to `lazygit` or `bottom`). But that's out of scope for this plan.

### Internal stderr (CLI code)

**Risk:** The CLI's own code paths — `status_monitor.rs`, `runner.rs`, and helpers called during live monitoring — may contain `eprintln!` calls that execute while the alternate screen is active. These bypass ratatui and write raw bytes directly to stderr, corrupting the TUI frame. For example, `status_monitor.rs:152` has an `eprintln!` in an error handler that would fire during monitoring.

**Mitigation:** Implementation step 6 (Phase 1) requires a full audit of all direct stderr writes in the monitor/runner hot path. Any `eprintln!`, `writeln!(stderr(), ...)`, or similar must be removed from code paths that execute while `LiveScreen` is active. Errors should be collected into a buffer (e.g., `Vec<String>`) and displayed after the screen exits, or rendered as part of the TUI via `LiveScreen::draw()`.

---

## Alternatives Considered

### 1. Inline viewport (`Viewport::Inline(height)`)

Ratatui supports an inline viewport that renders within the existing terminal flow without alternate screen.

**Rejected:** Still requires cursor management (ratatui handles it better than our manual approach, but it's fundamentally the same mechanism). Interleaved output can still corrupt it. Doesn't solve the root problem.

### 2. Fix `count_terminal_lines()`

Make the line counting more robust — handle edge cases, test more thoroughly.

**Rejected:** The approach is inherently fragile. Even a perfect implementation breaks on interleaved output or terminal resize. We've tried fixing it multiple times already.

### 3. Double-buffer with full clear

Instead of `MoveUp(n)`, use `Clear(ClearType::All)` before each frame.

**Rejected:** Causes visible flicker on every frame. Also doesn't handle interleaved output.

### 4. Use a PTY to capture agent output

Run the agent in a pseudo-terminal, capture its output, and render everything through a single controlled channel.

**Rejected:** Massive complexity increase for minimal benefit over alternate screen. Agent output is already captured — the problem is our own rendering, not the agent's output.

# Chatty Output: Iteration 2 — Live Feel

**Date**: 2026-03-20
**Status**: Proposed
**Builds on**: [chatty-iteration-1.md](chatty-iteration-1.md)

---

## Problems

Two things make the pipeline view feel unresponsive:

1. **Timers stall.** Elapsed counters freeze for seconds and then jump instead of ticking smoothly.
2. **Alternate screen.** The view takes over the entire terminal, hiding everything above it. When the build finishes, the view vanishes. Claude Code renders inline — output scrolls up naturally, previous commands stay visible, and the final state persists in scrollback.

```
 ▸ Decomposing plan
 claude · 0% · $0.00                                                       1m04
```

That `1m04` should tick to `1m05` a second later. Instead it stays stuck until something triggers an update.

---

## Root Cause

`read_events()` (`storage.rs:368`) spawns a `jj log` subprocess to read task events. The status monitor calls it **twice per tick**:

1. `poll()` (`status_monitor.rs:87`) — reads events, materializes graph, checks for changes
2. `build_view()` (`status_monitor.rs:112`) — reads events **again**, materializes graph **again**, builds the chat view

That's two `jj log` processes per 500ms tick (the poll interval from `live_screen.rs:25`). Meanwhile the agent is also writing to JJ constantly. Under this contention, each `jj log` can take 1-3 seconds, meaning the actual refresh rate is 2-6 seconds — not the 500ms target.

The elapsed time computation itself is correct — `elapsed_secs()` (`chat_builder.rs:116`) uses `chrono::Utc::now()` for in-progress tasks, so it would show the right value if the view were redrawn. The view just isn't being redrawn fast enough.

---

## Fix

### 1. One read per tick, not two

Merge `poll()` and `build_view()` into a single method that reads events once and returns both the change-detection result and the rendered buffer.

```rust
/// Poll for changes and build the view in a single read.
fn poll_and_build(&mut self, cwd: &Path) -> Result<(bool, bool, Option<Buffer>)> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    let root_task = match graph.tasks.get(&self.task_id) {
        Some(task) => task,
        None => return Ok((false, false, None)),
    };

    let changed = events.len() != self.last_event_count || !self.has_rendered;
    if changed {
        self.last_event_count = events.len();
        self.has_rendered = true;
    }

    let is_terminal = matches!(root_task.status, TaskStatus::Closed | TaskStatus::Stopped);
    let buf = self.build_view_from_graph(&graph, root_task, cwd);

    Ok((changed, is_terminal, buf.ok()))
}
```

This halves the JJ load per tick.

### 2. Cache the graph, always redraw

Even with one read per tick, JJ contention can still stall. Fix: cache the last successfully-read graph and rebuild the view from cache on every tick.

```rust
struct StatusMonitor {
    // ... existing fields ...
    last_graph: Option<(Vec<TaskEvent>, TaskGraph)>,
}
```

On each tick:
1. **Try** to read events (non-blocking or with a short timeout)
2. If new events arrive → update cache, rebuild view from new graph
3. If read fails or is slow → rebuild view from **cached** graph (timer still updates because `elapsed_secs()` uses `Utc::now()`)

The timer ticks every cycle regardless of whether JJ responded, because the elapsed is computed at render time, not read time.

### 3. Reduce poll interval to 1000ms

The current 500ms interval is pointlessly fast given JJ's latency. A 1-second interval matches the timer granularity (seconds) and halves the JJ load again. The view shows seconds, not milliseconds — updating faster than once per second is invisible.

```rust
const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
```

Combined effect: 1 JJ read per second (down from 4), cached fallback for contention. Timer ticks reliably.

### 4. Inline rendering instead of alternate screen

**Current behavior:** `LiveScreen` enters alternate screen mode (`EnterAlternateScreen`), takes over the terminal, and clears everything on drop. The pipeline view exists in a void — nothing above it, nothing after it. When the build finishes, the view vanishes and gets replaced by a markdown summary.

**Desired behavior:** Render inline like Claude Code. Output scrolls up naturally in the terminal. Previous commands stay visible above. The final pipeline state persists in scrollback after the build completes.

**How Claude Code does it:** Overwrite-in-place using cursor movement, without alternate screen:

1. Render the chat as ANSI text to stderr (already have `buffer_to_ansi()`)
2. Track how many lines the last render used (`last_height`)
3. On each redraw, move cursor up `last_height` lines (`\x1b[{N}A`), clear each line (`\x1b[2K`), and re-render
4. When done, stop updating — the output is already in scrollback

```rust
struct InlineRenderer {
    last_height: u16,
}

impl InlineRenderer {
    fn render(&mut self, buf: &Buffer) {
        let ansi = buffer_to_ansi(buf);
        let new_height = buf.area().height;

        // Move cursor up to overwrite previous render
        if self.last_height > 0 {
            eprint!("\x1b[{}A", self.last_height);
        }

        // Clear and re-render
        for line in ansi.lines() {
            eprint!("\x1b[2K{}\r\n", line);
        }

        self.last_height = new_height;
    }
}
```

**What this replaces:**
- `LiveScreen::new()` → no alternate screen entry
- `LiveScreen::run()` → simple loop with `crossterm::event::poll()` + inline render
- `LiveScreen::drop()` → no terminal restore needed (cursor is already in the right place)
- `draw_frame()` → `inline_renderer.render(buf)`

**What this fixes for free:**
- Issue #8 from iteration-1 (chat doesn't persist after build) — no alternate screen means the final state is already in scrollback. `output_build_completed()` becomes unnecessary.
- The `[Ctrl+C to detach]` footer can be the last line of the inline render rather than a separate ratatui widget.

**Edge cases:**
- **Terminal resize:** `buffer_to_ansi()` already works at any width. On resize, just recompute the buffer width from terminal size and re-render. If height changes, the cursor math might be off for one frame — acceptable.
- **Ctrl+C:** Still works via `crossterm::event::poll()` in the tick loop. No raw mode needed for basic key detection (but raw mode may be needed for immediate Ctrl+C without Enter — check if `ctrlc` crate handler is sufficient instead).
- **Scroll overflow:** If the pipeline chat is taller than the terminal, the cursor-up approach breaks. Fix: cap the render height at terminal height and let the content scroll naturally. Or: since the chat starts small and grows, it won't overflow until very long builds with many subtasks.
- **Loading screen transition:** `LoadingScreen` currently converts to `LiveScreen`. With inline rendering, the loading spinner can also be inline, and the transition is just "stop rendering spinner, start rendering chat."

**Migration:** Replace `LiveScreen` usage in `status_monitor.rs` and `commands/build.rs` with the inline renderer. `LiveScreen` can remain for any future use case that genuinely needs full-screen mode (e.g., the two-pane layout from `tui-two-pane-plan.md`).

### 5. Show agent spin-up state on subtasks

**What happens:** After decompose finishes, there's a dead period where subtasks are listed but nothing is visibly happening. The orchestrator is assigning lanes and spinning up agents, but the view shows a wall of `○` pending subtasks with no indication that work is starting:

```
 Decomposed into 8 subtasks                                                4m41
 ○ Wrap flat subtasks in LaneBlock when no orchestrator found
 ○ Suppress elapsed on active/pending lane subtasks
 ○ Remove dead if/else in footer and issue detail rendering
 ○ Drop iteration count when zero in summary line
 ○ Suppress check symbol on summary Done line
 ○ Investigate and fix missing agents line in summary
 ○ Replace output_build_completed with output_build_show
 ○ Verify progressive dimming works correctly
```

This can last 10-30 seconds depending on how many agents need to start. The view looks frozen.

**Desired behavior:** Use the heartbeat line from [task-heartbeat.md](task-heartbeat.md) as a unified status mechanism. The heartbeat line always shows what the agent is doing — and "starting..." is just the first heartbeat state before any real progress comments arrive. No separate `Starting` `MessageKind` needed.

**Mockup — agents starting up:**

Lane blocks appear as soon as tasks are assigned. The heartbeat line shows "starting..." until the agent connects. Subtasks use `◌` (dotted circle) to distinguish "assigned but not running" from `○` (unassigned).

```
 Decomposed into 8 subtasks                                                4m41

 ┃  ◌ Wrap flat subtasks in LaneBlock when no orchestrator found              ← surface bg, dim ◌
 ┃  ◌ Suppress elapsed on active/pending lane subtasks                        ← surface bg, dim ◌
 ┃  claude/opus-4.6                                                           ← surface bg, dim
 ┃  ⎿ starting...                                                            ← surface bg, yellow

 ┃  ◌ Remove dead if/else in footer and issue detail rendering                ← surface bg, dim ◌
 ┃  ◌ Drop iteration count when zero in summary line                          ← surface bg, dim ◌
 ┃  claude/opus-4.6                                                           ← surface bg, dim
 ┃  ⎿ starting...                                                            ← surface bg, yellow

 ○ Suppress check symbol on summary Done line                                ← dim (unassigned)
 ○ Investigate and fix missing agents line in summary
 ○ Replace output_build_completed with output_build_show
 ○ Verify progressive dimming works correctly
```

**Agent connects, first heartbeat arrives:**

```
 Decomposed into 8 subtasks                                                4m41

 ┃  ▸ Wrap flat subtasks in LaneBlock when no orchestrator found              ← surface bg, yellow ▸
 ┃  ○ Suppress elapsed on active/pending lane subtasks                        ← surface bg, dim ○
 ┃  claude/opus-4.6 · 4% · $0.02                                      12s   ← surface bg, dim footer
 ┃  ⎿ Reading the existing chat_builder implementation                        ← surface bg, dim italic

 ┃  ◌ Remove dead if/else in footer and issue detail rendering                ← surface bg, dim ◌
 ┃  ◌ Drop iteration count when zero in summary line                          ← surface bg, dim ◌
 ┃  claude/opus-4.6                                                           ← surface bg, dim
 ┃  ⎿ starting...                                                            ← surface bg, yellow

 ○ Suppress check symbol on summary Done line
 ○ Investigate and fix missing agents line in summary
 ○ Replace output_build_completed with output_build_show
 ○ Verify progressive dimming works correctly
```

**Key design points:**
- `◌` (dotted circle, U+25CC) = assigned to lane, agent starting. Visually distinct from `○` (unassigned) and `▸` (active).
- Lane blocks appear as soon as tasks are assigned, not when the agent starts working.
- The heartbeat line (`⎿`) is the unified status mechanism from [task-heartbeat.md](task-heartbeat.md). It shows "starting..." before the agent connects, then transitions to the agent's own progress comments.
- Footer shows `claude/opus-4.6` (agent/model, dim) while starting — the model is known at assignment time. Once the agent connects, it expands to the full `claude/opus-4.6 · 4% · $0.02   12s` format with context/cost/elapsed.
- The `⎿` prefix gives the heartbeat a "nested under" feel — it's the agent's voice, not the pipeline's.

**Block anatomy — starting vs. active:**

```
Starting:                                    Active:
   ◌ {subtask}                                  ▸ {subtask}
   ◌ {subtask}                                  ○ {subtask}
   {agent}/{model}                              {agent}/{model} · {ctx}% · ${cost}    {elapsed}
   ⎿ starting...                                ⎿ {latest progress comment}
```

**No new `MessageKind` variant needed.** Instead:
- `◌` is just a rendering choice when a lane subtask is `Pending` but inside a `LaneBlock` (assigned to a lane). The builder doesn't need a `Starting` kind — the widget renders `○` as `◌` when it's inside a block.
- The heartbeat line is an `Option<String>` on `AgentBlock` and `LaneBlock` (already proposed in task-heartbeat.md). The builder populates it with `"starting..."` when no comments exist yet.

**Builder logic:** A `LaneBlock` is in "starting" state when:
- It has an assigned agent (from lane data)
- No subtask in the lane is `InProgress` yet (all are `Open` or `Pending`)
- The footer shows agent/model but no context/cost/elapsed data yet

### 6. Immediate feedback on command start

**What happens:** After typing `aiki build ops/now/plan.md`, nothing appears for several seconds while the CLI reads the task graph, finds/creates the epic, and sets up tasks. A `LoadingScreen` exists (`loading_screen.rs`) but it enters alternate screen — so it takes over the terminal, hides scrollback, and then gets replaced again when the pipeline view starts. On fast machines the loading screen flashes invisibly; on slow machines the user stares at a blank alternate screen.

**Desired behavior:** The pipeline chat starts rendering immediately, even before the full graph is loaded. The first line is the plan filename. The second is the loading state, using the same heartbeat pattern:

```
 fix-rhai-int-conditionals.md

 ⧗ Loading task graph...                                                     ← yellow
```

Then as state becomes available, lines appear naturally:

```
 fix-rhai-int-conditionals.md

 Created plan                                                             18:52

 ⎿ Finding or creating epic...                                              ← yellow
```

Then decompose starts and we're in the normal pipeline view flow.

**How it works with inline rendering:** Since the inline renderer overwrites in place (section #4), the loading state is just the first few renders of the same pipeline view. No separate `LoadingScreen` needed — the chat builder returns a partial `Chat` with whatever state is available, and the inline renderer draws it. As more data arrives, the view grows.

**What this replaces:** `LoadingScreen` goes away entirely for the pipeline use case. The inline renderer handles the full lifecycle from first keystroke to final "Done" line.

---

## Scope

| File | Changes |
|---|---|
| `cli/src/tasks/status_monitor.rs` | Merge poll+build, add graph cache, rebuild from cache, use inline renderer |
| `cli/src/tui/live_screen.rs` | Change default poll interval to 1000ms |
| `cli/src/tui/inline_renderer.rs` | New: cursor-up overwrite renderer using `buffer_to_ansi()` |
| `cli/src/commands/build.rs` | Use inline renderer instead of `ScreenSession`/`LiveScreen`; remove `output_build_completed()` |
| `cli/src/tui/loading_screen.rs` | Remove for pipeline use case — replaced by inline renderer's early renders |
| `cli/src/tui/types.rs` | Add `heartbeat: Option<String>` to `AgentBlock` and `LaneBlock` (aligns with task-heartbeat.md) |
| `cli/src/tui/chat_builder.rs` | Populate heartbeat from latest comment or "starting..." when no comments; detect starting-state lanes |
| `cli/src/tui/views/pipeline_chat.rs` | Render `⎿` heartbeat line (dim italic, surface bg); render `◌` for pending-in-block subtasks |

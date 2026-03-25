# Step 3a: Replace Status Monitor with Elm Event Loop

**Date**: 2026-03-22
**Status**: Ready
**Priority**: P1
**Phase**: 3 — Status monitor
**Depends on**: 1a (inline renderer), 1b (Elm core), 2b (build flow — most complex consumer)

---

## Problem

`status_monitor.rs` (443 lines) has three issues:

1. **Double JJ reads per tick.** `poll()` reads events + materializes graph, then `build_view()` reads events + materializes graph again. That's 2 `jj log` subprocesses per 500ms tick. Under contention, each takes 1-3s → actual refresh rate is 2-6s.

2. **No caching.** If JJ is slow, the view freezes because it can't render without fresh data. But elapsed times are computed at render time (`Utc::now()`) — a cached graph would still show correct timers.

3. **Coupled to LiveScreen.** The monitor returns `MonitorExitReason` consumed by `LiveScreen::run()`. Ad-hoc state management with no clear separation between model and view.

See [chatty-iteration-2.md](../chatty-iteration-2.md) sections 1-3 for the full analysis.

---

## Fix: The Elm event loop from step 1b replaces StatusMonitor entirely

The `tui::run()` function from step 1b **is** the status monitor. It:
- Reads JJ events once per tick (single read, not double)
- Caches the graph in the Model (always renders, even when JJ is slow)
- Uses the inline renderer (not LiveScreen)
- Renders at 100ms ticks (smooth spinner), JJ reads at ~1s on background thread
- Routes all events through `update()` (clean state management)

### What `poll_next_msg` does (the JJ integration)

See step 1b for the canonical implementation. Key design:

- **Background JJ reader thread** sends `TaskGraph` updates via `mpsc::channel` at ~1s intervals
- **Main loop ticks at ~100ms** for smooth spinner animation (10 braille frames)
- `poll_next_msg()` does a non-blocking `rx.try_recv()` for graph updates, polls crossterm for 100ms, returns `Msg::Tick` if nothing happened
- `Model.graph` is `Arc<TaskGraph>` — the JJ-contention fallback (use cached) is a cheap refcount bump, not a clone

The view still re-renders on every tick because `view()` computes elapsed from `Utc::now()` at render time, not from the graph.

### Usage from commands

Each command creates a `Model` with the appropriate `Screen` and calls `tui::run()`:

```rust
// build.rs (Screen::Build has no `fix` field — rendering is graph-driven.
// The `--fix` flag affects what the command orchestrates, not the TUI.)
let model = Model {
    graph: initial_graph,
    screen: Screen::Build { epic_id, plan_path },
    window: WindowState::new(terminal_width()),
};
tui::run(model, &cwd)?;

// runner.rs (task run)
let model = Model {
    graph: initial_graph,
    screen: Screen::TaskRun { task_id },
    window: WindowState::new(terminal_width()),
};
tui::run(model, &cwd)?;

// review.rs
let model = Model {
    graph: initial_graph,
    screen: Screen::Review { review_id, target },
    window: WindowState::new(terminal_width()),
};
tui::run(model, &cwd)?;
```

### What this replaces

| Old | New |
|---------|-----|
| `status_monitor.rs` (443 lines) | `app.rs::run()` + `poll_next_msg()` + JJ reader thread (~100 lines in step 1b) |
| `StatusMonitor::poll()` + `build_view()` (double read) | Single `read_events()` per ~1s on background thread via `mpsc::channel` |
| `MonitorExitReason` | `Effect::Done` / `Effect::Detached` |
| `ScreenSession` (131 lines) | Not needed — `tui::run()` owns the renderer directly |
| 500ms poll interval | 100ms render tick (smooth spinner) + ~1s JJ read interval on background thread |

### Files changed (wire commands)

| File | Change |
|------|--------|
| `cli/src/tasks/runner.rs` | Remove `ScreenSession`, call `tui::run()` instead |
| `cli/src/commands/build.rs` | Create `Model` with `Screen::Build`, call `tui::run()` |
| `cli/src/commands/fix.rs` | Create `Model` with `Screen::Fix`, call `tui::run()` |
| `cli/src/commands/review.rs` | Create `Model` with `Screen::Review`, call `tui::run()` |
| `cli/src/commands/epic.rs` | `epic show` calls `epic_show::view()` + render once |

### Files deleted (cutover — see [step-4a-cleanup.md](step-4a-cleanup.md) for full list)

All old TUI files are deleted in this same step. No coexistence period. See meta plan "What We Delete" table (6046 lines total).

### Additional cleanup in this step

- Update `cli/src/tui/AGENTS.md` to document new architecture
- Update `cli/src/tui/mod.rs` exports
- Remove `image`, `ab_glyph` dev-dependencies (PNG rendering gone)

### Tests

- `poll_next_msg` with JJ failure → returns cached graph
- Piping regression: `aiki build <plan> -o id` with piped stdout produces IDs only
- All `insta` snapshots from Phase 2 still pass
- Integration: run loop with mock graph sequence → verify view output at each step

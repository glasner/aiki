# Step 1b: Elm Core + Line-Based Data Model

**Date**: 2026-03-22
**Status**: Ready
**Priority**: P1
**Phase**: 1 — Foundation
**Depends on**: Nothing

---

## Problem

The current TUI has no clear data flow. `StatusMonitor` reads events, builds a `Chat` model, and renders it — but the boundaries between model, update, and view are blurred. State changes happen ad-hoc across multiple files. The `Chat`/`Message`/`ChatChild` model is too abstract and breeds edge cases.

We need:
1. A clean architecture that separates state, state transitions, and rendering
2. A view model that's cheap to iterate on as the design evolves
3. A foundation that supports future user interactions (pause, retry, scroll)

---

## Fix: Elm architecture with line-based view output

One new file: `app.rs` (Elm core: Screen, Model, Msg, update, view, run loop, plus Line types).

### New file: `cli/src/tui/app.rs`

The Elm core. Owns the event loop and enforces the Model → update → view → render cycle.

```rust
use std::sync::Arc;
use crate::tasks::TaskGraph;

/// Which screen to render. Defines lifecycle/exit behavior.
/// View rendering is graph-driven — view functions adapt to whatever
/// tasks exist in the graph. The Screen enum determines WHEN to exit,
/// not WHAT to render (e.g. build::view() renders fix/iteration
/// sections when fix subtasks are present in the graph — no separate
/// BuildFix variant needed).
pub enum Screen {
    TaskRun { task_id: String },
    Build { epic_id: String, plan_path: String },
    Review { review_id: String, target: String },
    Fix { fix_parent_id: String, review_id: String },
    EpicShow { epic_id: String },
    ReviewShow { review_id: String },
}

/// Window state — local view concerns not derived from TaskGraph.
pub struct WindowState {
    pub width: u16,
    pub scroll_offset: u16,
    // Future: expanded sections, focused element, paused, etc.
}

impl WindowState {
    pub fn new(width: u16) -> Self {
        Self { width, scroll_offset: 0 }
    }
}

/// Everything the view needs to render.
pub struct Model {
    /// Domain state — materialized from JJ task events.
    /// Wrapped in Arc so the JJ-contention fallback path ("use cached")
    /// is a cheap refcount bump instead of cloning the full graph.
    pub graph: Arc<TaskGraph>,
    /// Which screen we're rendering.
    pub screen: Screen,
    /// Window state (scroll, terminal size, etc.).
    pub window: WindowState,
}

/// Events that can change the model.
pub enum Msg {
    /// New TaskGraph read from JJ (or cached if JJ was slow).
    GraphUpdated(TaskGraph),
    /// User pressed Ctrl+C.
    Detach,
    /// Terminal resized.
    Resize { width: u16, height: u16 },
    /// Render tick — no new data, but re-render for elapsed time updates + spinner.
    Tick,
    // Future:
    // Pause,
    // Resume,
    // Retry(String),       // task_id
    // Scroll(i16),         // delta
    // ToggleExpand(u16),   // group
}

/// Result of update — tells the event loop what to do.
pub enum Effect {
    /// Continue the loop.
    Continue,
    /// Stop the loop — build completed or task done.
    Done,
    /// Stop the loop — user detached.
    Detached,
}

/// Pure state transition. Returns updated model + effect.
pub fn update(mut model: Model, msg: Msg) -> (Model, Effect) {
    match msg {
        Msg::GraphUpdated(graph) => {
            let done = is_finished(&graph, &model.screen);
            model.graph = Arc::new(graph);
            let effect = if done { Effect::Done } else { Effect::Continue };
            (model, effect)
        }
        Msg::Detach => (model, Effect::Detached),
        Msg::Resize { width, .. } => {
            model.window.width = width;
            (model, Effect::Continue)
        }
        Msg::Tick => (model, Effect::Continue),
    }
}

/// Check if the monitored task/epic has finished (done or failed).
fn is_finished(graph: &TaskGraph, screen: &Screen) -> bool {
    match screen {
        Screen::TaskRun { task_id } => {
            graph.tasks.get(task_id)
                .map(|t| t.status.is_terminal())
                .unwrap_or(false)
        }
        Screen::Build { epic_id, .. } => {
            // Build is done when epic is closed (graph-driven — covers
            // both plain build and build-with-fix, since the epic only
            // closes after all fix iterations complete)
            graph.tasks.get(epic_id)
                .map(|t| t.status.is_terminal())
                .unwrap_or(false)
        }
        Screen::Review { review_id, .. } => {
            graph.tasks.get(review_id)
                .map(|t| t.status.is_terminal())
                .unwrap_or(false)
        }
        Screen::Fix { fix_parent_id, .. } => {
            graph.tasks.get(fix_parent_id)
                .map(|t| t.status.is_terminal())
                .unwrap_or(false)
        }
        // Static views — render once, always done
        Screen::EpicShow { .. } | Screen::ReviewShow { .. } => true,
    }
}

/// Pure view function — produces lines from model.
/// Each screen has its own view function (in screens/*.rs).
/// View rendering is graph-driven: view fns adapt to whatever tasks
/// exist in the graph (e.g. build::view renders fix sections when present).
pub fn view(model: &Model) -> Vec<Line> {
    match &model.screen {
        Screen::TaskRun { task_id } =>
            crate::tui::screens::task_run::view(&model.graph, task_id, &model.window),
        Screen::Build { epic_id, plan_path } =>
            crate::tui::screens::build::view(&model.graph, epic_id, plan_path, &model.window),
        Screen::Review { review_id, target } =>
            crate::tui::screens::review::view(&model.graph, review_id, target, &model.window),
        Screen::Fix { fix_parent_id, review_id } =>
            crate::tui::screens::fix::view(&model.graph, fix_parent_id, review_id, &model.window),
        Screen::EpicShow { epic_id } =>
            crate::tui::screens::epic_show::view(&model.graph, epic_id, &model.window),
        Screen::ReviewShow { review_id } =>
            crate::tui::screens::review_show::view(&model.graph, review_id, &model.window),
    }
}
```

### The event loop

See step 1a for full terminal setup.

**Key design: decoupled JJ reads.** The main loop ticks at ~100ms for smooth spinner animation (10 braille frames). JJ reads happen on a background thread via `mpsc::channel`, so JJ latency (500ms–3s) doesn't stutter the spinner. The view already handles cached graphs correctly — it just renders whatever's in `model.graph` and computes elapsed times from `Utc::now()`.

```rust
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Run the Elm loop until done or detached.
pub fn run(mut model: Model, cwd: &Path) -> Result<Effect> {
    // Terminal setup (see step 1a)
    install_panic_hook();
    let mut terminal = create_inline_terminal(model.window.width)?;
    crossterm::terminal::enable_raw_mode()?;

    // Spawn JJ reader thread — sends updated TaskGraphs via channel.
    // Reads every ~1s (matches second-granularity timers).
    let (tx, rx) = mpsc::channel::<TaskGraph>();
    let cwd_owned = cwd.to_owned();
    let _jj_thread = thread::spawn(move || {
        loop {
            if let Ok(graph) = read_events(&cwd_owned) {
                if tx.send(graph).is_err() {
                    break; // main loop exited
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    loop {
        // 1. View (pure)
        let lines = view(&model);

        // 2. Render via Viewport::Inline
        terminal.draw(|frame| {
            render_lines(&lines, frame.area(), frame.buffer_mut(), &theme);
        })?;

        // 3. Tick at ~100ms — smooth spinner animation.
        //    Drain any graph updates from the JJ reader thread.
        //    Check crossterm events (Ctrl+C, resize).
        let msg = poll_next_msg(&rx, &model)?;

        // 4. Update
        let (new_model, effect) = update(model, msg);
        model = new_model;

        match effect {
            Effect::Continue => continue,
            Effect::Done | Effect::Detached => {
                // Final render
                let lines = view(&model);
                terminal.draw(|frame| {
                    render_lines(&lines, frame.area(), frame.buffer_mut(), &theme);
                })?;
                crossterm::terminal::disable_raw_mode()?;
                if matches!(effect, Effect::Detached) {
                    println!("[detached]");
                }
                return Ok(effect);
            }
        }
    }
}

/// Poll at ~100ms tick rate. Non-blocking:
/// 1. Try rx.try_recv() for a new graph from the JJ reader thread
/// 2. Poll crossterm events (Ctrl+C, resize) with 100ms timeout
/// 3. If neither, return Tick (view re-renders with updated Utc::now() for elapsed times)
fn poll_next_msg(rx: &mpsc::Receiver<TaskGraph>, model: &Model) -> Result<Msg> {
    // Check for new graph (non-blocking)
    if let Ok(graph) = rx.try_recv() {
        return Ok(Msg::GraphUpdated(graph));
    }

    // Poll crossterm for 100ms
    if crossterm::event::poll(Duration::from_millis(100))? {
        match crossterm::event::read()? {
            Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. }) => {
                return Ok(Msg::Detach);
            }
            Event::Resize(w, h) => {
                return Ok(Msg::Resize { width: w, height: h });
            }
            _ => {}
        }
    }

    // Nothing happened — return Tick so view re-renders (elapsed times update)
    Ok(Msg::Tick)
}
```

### View output types (in `app.rs`)

The Line types live alongside Screen, Model, and Msg in `app.rs`. Intentionally simple — lines match 1:1 with rendered output.

```rust
/// A single rendered line.
pub struct Line {
    /// Indent level (0 = flush left, 1 = 4 spaces, etc.)
    pub indent: u8,
    /// The text content (indent applied at render time)
    pub text: String,
    /// Right-aligned metadata (elapsed time, etc.)
    pub meta: Option<String>,
    /// Visual style
    pub style: LineStyle,
    /// Phase group — lines with the same group dim together
    pub group: u16,
    /// Whether this line is dimmed (set by apply_dimming, not by builders)
    pub dimmed: bool,
}

#[derive(Clone, Copy)]
pub enum LineStyle {
    PhaseHeader,                       // 合 name (agent)
    Child,                             // ⎿ text (dim)
    ChildActive,                       // ⎿ text (yellow)
    ChildDone,                         // ⎿ ✓ text (green check, dim text)
    ChildError,                        // ⎿ ✗ text (red)
    Subtask { status: SubtaskStatus }, // icon + name + elapsed
    Separator,                         // ---
    SectionHeader,                     // Initial Build, Iteration 2
    Issue,                             // 1. text
    Dim,                               // plain dim text
    Blank,                             // empty line
}

#[derive(Clone, Copy)]
pub enum SubtaskStatus {
    Pending,    // ○
    Assigned,   // ◌
    Active,     // ▸
    Done,       // ✓
    Failed,     // ✗
}
```

### Key properties

**View is pure.** `view(model) → Vec<Line>`. No side effects. No reading from JJ. No mutations. Given the same model, always the same output.

**All state changes through update.** User input, timer ticks, graph refreshes — all are `Msg` values dispatched to `update()`. No ad-hoc state mutation in event handlers or view functions.

**Model = TaskGraph + window state.** One struct holds everything. The TaskGraph is the domain state. WindowState is local view concerns (terminal size, scroll, etc.). Both go through the same update cycle.

**Effect tells the loop what to do.** `update()` doesn't perform side effects — it returns an `Effect` that the loop acts on. This keeps update pure and testable.

### What this replaces

| Old | New |
|-----|-----|
| `types.rs` (122 lines) | Line types in `app.rs` |
| `status_monitor.rs` polling logic | `app.rs` event loop (~100 lines) + JJ reader thread |
| `Chat`, `Message`, `ChatChild` | `Vec<Line>` |
| `Stage` enum | `group: u16` on each line |
| `MessageKind` | `LineStyle` enum |
| Ad-hoc state management | `Model` + `update(Msg)` + `Effect` |

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/app.rs` | New (~200 lines: Screen, Model, Msg, update, run loop, Line types) |
| `cli/src/tui/mod.rs` | Add `pub mod tui;` |

### Tests

**State logic tests** (no terminal, no buffer — fastest layer):

```rust
#[test]
fn graph_updated_with_terminal_task_returns_done() {
    let model = make_model(Screen::TaskRun { task_id: "abc".into() });
    let graph = make_graph_with_closed_task("abc");
    let (_, effect) = update(model, Msg::GraphUpdated(graph));
    assert!(matches!(effect, Effect::Done));
}

#[test]
fn graph_updated_with_in_progress_task_returns_continue() {
    let model = make_model(Screen::TaskRun { task_id: "abc".into() });
    let graph = make_graph_with_active_task("abc");
    let (_, effect) = update(model, Msg::GraphUpdated(graph));
    assert!(matches!(effect, Effect::Continue));
}

#[test]
fn detach_returns_detached() {
    let model = make_model(Screen::TaskRun { task_id: "abc".into() });
    let (_, effect) = update(model, Msg::Detach);
    assert!(matches!(effect, Effect::Detached));
}

#[test]
fn resize_updates_window_width() {
    let model = make_model(Screen::TaskRun { task_id: "abc".into() });
    let (model, _) = update(model, Msg::Resize { width: 120, height: 40 });
    assert_eq!(model.window.width, 120);
}
```

**Key principle from ratatui testing best practices:** state logic tests should be the majority and fastest. Never mix state assertions with rendering assertions in the same test.

- `view()` is a pure function — snapshot tests per flow (covered in step 2a-2e, with 2e absorbed into 2b)

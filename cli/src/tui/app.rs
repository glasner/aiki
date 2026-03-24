use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::tasks::types::TaskStatus;
use crate::tasks::{materialize_graph, read_events, TaskGraph};
use crate::tui::render::{apply_dimming, lines_height, render_lines};
use crate::tui::theme;

/// Which screen to render. Defines lifecycle/exit behavior.
/// View rendering is graph-driven — view functions adapt to whatever
/// tasks exist in the graph. The Screen enum determines WHEN to exit,
/// not WHAT to render (e.g. build::view() renders fix/iteration
/// sections when fix subtasks are present in the graph — no separate
/// BuildFix variant needed).
pub enum Screen {
    TaskRun {
        task_id: String,
    },
    Build {
        epic_id: String,
        plan_path: String,
    },
    Review {
        review_id: String,
        target: String,
    },
    Fix {
        fix_parent_id: String,
        review_id: String,
    },
    EpicShow {
        epic_id: String,
    },
    ReviewShow {
        review_id: String,
    },
}

/// Window state — local view concerns not derived from TaskGraph.
pub struct WindowState {
    pub width: u16,
    pub scroll_offset: u16,
    /// Monotonically increasing tick counter, used for spinner animation.
    /// Incremented on every update cycle.
    pub tick: u64,
}

impl WindowState {
    pub fn new(width: u16) -> Self {
        Self {
            width,
            scroll_offset: 0,
            tick: 0,
        }
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
    model.window.tick += 1;
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
        Screen::TaskRun { task_id } => graph
            .tasks
            .get(task_id)
            .map(|t| matches!(t.status, TaskStatus::Closed | TaskStatus::Stopped))
            .unwrap_or(false),
        Screen::Build { epic_id, .. } => graph
            .tasks
            .get(epic_id)
            .map(|t| matches!(t.status, TaskStatus::Closed | TaskStatus::Stopped))
            .unwrap_or(false),
        Screen::Review { review_id, .. } => graph
            .tasks
            .get(review_id)
            .map(|t| matches!(t.status, TaskStatus::Closed | TaskStatus::Stopped))
            .unwrap_or(false),
        Screen::Fix { fix_parent_id, .. } => graph
            .tasks
            .get(fix_parent_id)
            .map(|t| matches!(t.status, TaskStatus::Closed | TaskStatus::Stopped))
            .unwrap_or(false),
        // Static views — render once, always done
        Screen::EpicShow { .. } | Screen::ReviewShow { .. } => true,
    }
}

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
    PhaseHeader { active: bool },
    /// Failed phase header — red+bold `合` icon and text.
    PhaseHeaderFailed,
    /// Subtask table header — `[short-id] title` with dim brackets+id, fg title.
    SubtaskHeader,
    Child,
    ChildActive,
    ChildDone,
    ChildError,
    ChildWarning,
    ChildBold,
    Subtask { status: SubtaskStatus },
    Separator,
    SectionHeader,
    Issue,
    Dim,
    Blank,
}

#[derive(Clone, Copy)]
pub enum SubtaskStatus {
    PendingUnassigned,  // ◌ — no lane has claimed this yet
    Pending,            // ○ — in active lane, not yet started
    Assigned,           // ⧗ — assigned to session
    Active,             // ▸
    Done,               // ✔
    Failed,             // ✘
}

/// Run the Elm loop until done or detached.
///
/// Creates an inline viewport terminal on stdout, renders via the
/// view → render → poll → update cycle at ~100ms tick rate.
pub fn run(model: Model, cwd: &Path) -> Result<Effect> {
    run_inner(model, cwd)
}

/// Temporary shim — callers pass a child_pid but it's currently ignored.
/// Will be replaced by worker thread approach where the work runs in-process.
pub fn run_with_child(model: Model, cwd: &Path, _child_pid: Option<u32>) -> Result<Effect> {
    run_inner(model, cwd)
}

fn run_inner(model: Model, cwd: &Path) -> Result<Effect> {
    crate::tui::panic_hook::install_panic_hook();

    // Shared stop flag — set by signal handlers (SIGTERM, SIGHUP)
    // and checked by the JJ reader thread and poll loop.
    let stop = Arc::new(AtomicBool::new(false));
    let _signal_guards = crate::tui::panic_hook::install_signal_handlers(Arc::clone(&stop));

    let theme = theme::Theme::from_mode(theme::detect_mode());

    // Create inline terminal on stdout — output persists in scrollback
    let backend = CrosstermBackend::new(std::io::stdout());
    let initial_height = 1u16;
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(initial_height),
        },
    )?;
    let mut current_height = initial_height;

    crossterm::terminal::enable_raw_mode()?;

    // Spawn JJ reader thread — sends updated TaskGraphs via channel.
    // Reads every ~1s (matches second-granularity timers).
    // Checks the stop flag each iteration for graceful shutdown.
    let (tx, rx) = mpsc::channel::<TaskGraph>();
    let cwd_owned = cwd.to_owned();
    let jj_stop = Arc::clone(&stop);
    let _jj_thread = thread::spawn(move || {
        loop {
            if jj_stop.load(Ordering::Relaxed) {
                break;
            }
            if let Ok(events) = read_events(&cwd_owned) {
                let graph = materialize_graph(&events);
                if tx.send(graph).is_err() {
                    break; // main loop exited, receiver dropped
                }
            }
            // Sleep in 100ms increments so we notice the stop flag promptly
            for _ in 0..10 {
                if jj_stop.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    });

    let result = run_loop(model, &rx, &stop, &theme, &mut terminal, &mut current_height);

    // Cleanup: signal the JJ thread to stop and restore terminal
    stop.store(true, Ordering::Relaxed);
    crossterm::terminal::disable_raw_mode()?;

    match result {
        Ok(Effect::Detached) => {
            println!("[detached]");
            Ok(Effect::Detached)
        }
        other => other,
    }
}

/// Inner loop — separated so cleanup always runs via the outer `run()`.
fn run_loop(
    mut model: Model,
    rx: &mpsc::Receiver<TaskGraph>,
    stop: &AtomicBool,
    theme: &theme::Theme,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    current_height: &mut u16,
) -> Result<Effect> {
    loop {
        // 1. View (pure)
        let mut lines = view(&model);
        apply_dimming(&mut lines);

        // 2. Grow viewport if content exceeds current height
        let new_height = lines_height(&lines).max(1);
        if new_height > *current_height {
            let growth = new_height - *current_height;
            terminal.insert_before(growth, |_| {})?;
            terminal.resize(ratatui::layout::Rect::new(
                0,
                0,
                model.window.width,
                new_height,
            ))?;
            *current_height = new_height;
        }

        // 3. Render via Viewport::Inline
        let tick = model.window.tick;
        terminal.draw(|frame| {
            let area = frame.area();
            render_lines(&lines, frame.buffer_mut(), area, theme, tick);
        })?;

        // 4. Poll for next event (also checks stop flag for signal-driven exit)
        let msg = poll_next_msg(rx, stop)?;

        // 5. Update
        let (new_model, effect) = update(model, msg);
        model = new_model;

        match effect {
            Effect::Continue => continue,
            Effect::Done | Effect::Detached => {
                // Final render — grow viewport to fit (same logic as main loop)
                let mut lines = view(&model);
                apply_dimming(&mut lines);
                let new_height = lines_height(&lines).max(1);
                if new_height > *current_height {
                    let growth = new_height - *current_height;
                    terminal.insert_before(growth, |_| {})?;
                    terminal.resize(ratatui::layout::Rect::new(
                        0,
                        0,
                        model.window.width,
                        new_height,
                    ))?;
                    *current_height = new_height;
                }
                let tick = model.window.tick;
                terminal.draw(|frame| {
                    let area = frame.area();
                    render_lines(&lines, frame.buffer_mut(), area, theme, tick);
                })?;
                return Ok(effect);
            }
        }
    }
}

/// Poll at ~100ms tick rate. Non-blocking:
/// 1. Check stop flag (set by signal handlers on SIGTERM/SIGHUP)
/// 2. Try rx.try_recv() for a new graph from the JJ reader thread
/// 3. Poll crossterm events (Ctrl+C, resize) with 100ms timeout
/// 4. If neither, return Tick (view re-renders with updated Utc::now() for elapsed times)
fn poll_next_msg(rx: &mpsc::Receiver<TaskGraph>, stop: &AtomicBool) -> Result<Msg> {
    // Signal-driven stop — SIGTERM or SIGHUP received
    if stop.load(Ordering::Relaxed) {
        return Ok(Msg::Detach);
    }

    // Check for new graph (non-blocking)
    if let Ok(graph) = rx.try_recv() {
        return Ok(Msg::GraphUpdated(graph));
    }

    // Poll crossterm for 100ms
    if event::poll(Duration::from_millis(100))? {
        match event::read()? {
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Msg::Detach);
            }
            Event::Resize(w, h) => {
                return Ok(Msg::Resize {
                    width: w,
                    height: h,
                });
            }
            _ => {}
        }
    }

    // Nothing happened — return Tick
    Ok(Msg::Tick)
}

/// Pure view function — produces lines from model.
pub fn view(model: &Model) -> Vec<Line> {
    match &model.screen {
        Screen::TaskRun { task_id } =>
            super::screens::task_run::view(&model.graph, task_id, &model.window),
        Screen::Build { epic_id, plan_path } =>
            super::screens::build::view(&model.graph, epic_id, plan_path, &model.window),
        Screen::Review { review_id, target } =>
            super::screens::review::view(&model.graph, review_id, target, &model.window),
        Screen::Fix { fix_parent_id, review_id } =>
            super::screens::fix::view(&model.graph, fix_parent_id, review_id, &model.window),
        Screen::EpicShow { epic_id } =>
            super::screens::epic_show::view(&model.graph, epic_id, &model.window),
        Screen::ReviewShow { review_id } =>
            super::screens::review_show::view(&model.graph, review_id, &model.window),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::EdgeStore;
    use crate::tasks::types::{FastHashMap, TaskOutcome, TaskPriority, TaskStatus};
    use crate::tasks::Task;
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: "test task".to_string(),
            slug: None,
            task_type: None,
            status,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: if status == TaskStatus::Closed {
                Some(TaskOutcome::Done)
            } else {
                None
            },
            summary: None,
            turn_started: None,
            closed_at: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn make_model(screen: Screen) -> Model {
        Model {
            graph: Arc::new(TaskGraph {
                tasks: FastHashMap::default(),
                edges: EdgeStore::default(),
                slug_index: FastHashMap::default(),
            }),
            screen,
            window: WindowState::new(80),
        }
    }

    fn make_graph_with_task(id: &str, status: TaskStatus) -> TaskGraph {
        let mut tasks = FastHashMap::default();
        tasks.insert(id.to_string(), make_task(id, status));
        TaskGraph {
            tasks,
            edges: EdgeStore::default(),
            slug_index: FastHashMap::default(),
        }
    }

    #[test]
    fn graph_updated_with_terminal_task_returns_done() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let graph = make_graph_with_task("abc", TaskStatus::Closed);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn graph_updated_with_in_progress_task_returns_continue() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let graph = make_graph_with_task("abc", TaskStatus::InProgress);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Continue));
    }

    #[test]
    fn detach_returns_detached() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let (_, effect) = update(model, Msg::Detach);
        assert!(matches!(effect, Effect::Detached));
    }

    #[test]
    fn resize_updates_window_width() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let (model, effect) = update(
            model,
            Msg::Resize {
                width: 120,
                height: 40,
            },
        );
        assert_eq!(model.window.width, 120);
        assert!(matches!(effect, Effect::Continue));
    }

    #[test]
    fn tick_returns_continue() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let (_, effect) = update(model, Msg::Tick);
        assert!(matches!(effect, Effect::Continue));
    }

    #[test]
    fn build_done_when_epic_closed() {
        let model = make_model(Screen::Build {
            epic_id: "epic1".into(),
            plan_path: "plan.md".into(),
        });
        let graph = make_graph_with_task("epic1", TaskStatus::Closed);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn build_continues_when_epic_in_progress() {
        let model = make_model(Screen::Build {
            epic_id: "epic1".into(),
            plan_path: "plan.md".into(),
        });
        let graph = make_graph_with_task("epic1", TaskStatus::InProgress);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Continue));
    }

    #[test]
    fn static_screen_always_done() {
        let model = make_model(Screen::EpicShow {
            epic_id: "epic1".into(),
        });
        let graph = make_graph_with_task("epic1", TaskStatus::InProgress);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn review_show_always_done() {
        let model = make_model(Screen::ReviewShow {
            review_id: "rev1".into(),
        });
        let graph = make_graph_with_task("rev1", TaskStatus::InProgress);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn stopped_task_is_terminal() {
        let model = make_model(Screen::TaskRun {
            task_id: "abc".into(),
        });
        let graph = make_graph_with_task("abc", TaskStatus::Stopped);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn fix_done_when_parent_closed() {
        let model = make_model(Screen::Fix {
            fix_parent_id: "fix1".into(),
            review_id: "rev1".into(),
        });
        let graph = make_graph_with_task("fix1", TaskStatus::Closed);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }

    #[test]
    fn review_done_when_review_closed() {
        let model = make_model(Screen::Review {
            review_id: "rev1".into(),
            target: "task1".into(),
        });
        let graph = make_graph_with_task("rev1", TaskStatus::Closed);
        let (_, effect) = update(model, Msg::GraphUpdated(graph));
        assert!(matches!(effect, Effect::Done));
    }
}

# Worker Thread TUI Architecture

**Date**: 2026-03-24
**Status**: Plan
**Replaces**: Background process spawn-then-monitor pattern

---

## Problem

The current TUI architecture spawns a detached background process (`--_continue-async`) and polls the JJ task graph for updates. This creates a blind spot: the TUI can't show process lifecycle states like "creating isolated workspace...", "starting session...", "resolving agent..." because these happen inside the background process before any graph events are written.

The screen-states spec (screen-states.md) defines states 1.0a-1.0d, 2.0-2.3b, 6.0, 8.0a-0c that all require visibility into process lifecycle. The graph-only approach can't render these.

## Solution

Replace the background process with a **worker thread** that runs inside the same process as the TUI. The worker thread sends status messages through a channel. The TUI event loop consumes messages from two sources:

1. **Worker channel** — status messages (phase lifecycle, pre-session statuses)
2. **JJ reader thread** — task graph updates (existing pattern)

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│ Main Thread (TUI event loop)                             │
│                                                          │
│  poll_next_msg() checks:                                 │
│    1. stop flag (signals)                                │
│    2. worker_rx.try_recv() → WorkerStatusMsg             │
│    3. jj_rx.try_recv() → GraphUpdated                    │
│    4. crossterm events (Ctrl+C, resize)                  │
│    5. Tick                                               │
│                                                          │
│  update(model, msg) → (model, effect)                    │
│  view(model) → lines                                     │
│  render(lines)                                           │
└──────────┬───────────────────────┬───────────────────────┘
           │                       │
    ┌──────┴──────┐         ┌──────┴──────┐
    │ Worker      │         │ JJ Reader   │
    │ Thread      │         │ Thread      │
    │             │         │             │
    │ Sends:      │         │ Sends:      │
    │ WorkerStat… │         │ TaskGraph   │
    │             │         │             │
    │ Runs:       │         │ Runs:       │
    │ pipeline    │         │ read_events │
    │ functions   │         │ every ~1s   │
    └─────────────┘         └─────────────┘
```

## Data Sources

The TUI screen is built from two independent data sources:

| Screen element | Worker thread | JJ graph poll |
|---|---|---|
| Phase name + agent | `PhaseStarted` | — |
| Pre-session status (1.0a-d) | `Update(text)` | — |
| Agent heartbeat (active) | — | heartbeat text + elapsed |
| Phase result (done) | `PhaseDone { result }` | task close confirmation |
| Section headers | `Section("Iteration 2")` | — |
| Subtask table | — | subtask names, statuses, elapsed |
| Lane blocks (inside loop) | — | lane structure, completion counts, heartbeats |
| "starting session..." in lane | — | task is Reserved, no heartbeat yet |
| Issue list | — | review task issues |
| Summary stats | `Done` signal | session/token counts |

**The worker is a sequential phase driver.** It sends coarse-grained
structural updates (which phases exist, when they start/finish). The
graph fills in fine-grained live data (heartbeats, subtask statuses,
lane progress) for active phases.

**Handoff point:** For each phase that spawns an agent, the worker sends
pre-session statuses ("Reading task graph...", "creating isolated
workspace...", "starting session..."). Once the agent connects and the
graph has a heartbeat, the view switches to showing the heartbeat instead
of the worker status. The worker is blocked on `task_run()` during
this time.

### Flow 1 walkthrough: `aiki task run <id>`

```
State 1.0a  Worker: PhaseStarted { name: "task" } + Update("Reading task graph...")
State 1.0b  Worker: Update("Resolving agent...")
State 1.0c  Worker: Update("creating isolated workspace...") + agent resolved
State 1.0d  Worker: Update("starting session...")
            ── handoff: worker blocked on task_run(), graph takes over ──
State 1.1   Graph: heartbeat text replaces worker status
State 1.2   Graph: task Closed (Done) → Worker: PhaseDone + Done
State 1.3   Graph: task Closed (Failed) → Worker: PhaseFailed
State 1.5+  Graph: subtask table (statuses, heartbeats, elapsed)
```

### Flow 2 walkthrough: `aiki build <plan>`

```
State 2.0   Worker: PhaseStarted { name: "plan" } + Update("Validating...")
State 2.1   Worker: PhaseDone { result: "ops/now/..." }
State 2.2a  Worker: Section("Initial Build") + PhaseStarted { name: "decompose" }
                    + Update("Reading task graph...")
State 2.2b  Worker: Update("Finding epic...")
State 2.3a  Worker: Update("creating isolated workspace...") + agent resolved
            Graph: epic/subtask table appears
State 2.3b  Worker: Update("starting session...")
            ── handoff: worker blocked on run_decompose() ──
State 2.4   Graph: decompose agent heartbeat
State 2.4b  Graph: subtasks arriving in table
State 2.5   Worker: PhaseDone { result: "5 subtasks created" }
State 2.6   Worker: PhaseStarted { name: "loop" }
            Graph: lane structure, per-lane "starting session..." (Reserved, no heartbeat)
            ── handoff: worker blocked on run_loop() ──
State 2.7+  Graph: per-lane heartbeats, subtask status changes
State 2.10  Worker: PhaseDone for loop + Done
            Graph: summary stats (sessions, tokens)
```

## Msg Enum

```rust
pub enum Msg {
    /// New TaskGraph read from JJ.
    GraphUpdated(TaskGraph),
    /// Worker thread status update.
    Worker(WorkerStatusMsg),
    /// Worker channel disconnected without sending Done.
    /// Detected via TryRecvError::Disconnected in poll_next_msg.
    /// Worker panicked or returned Err without calling status.failed().
    WorkerDisconnected,
    /// User pressed Ctrl+C.
    Detach,
    /// Terminal resized.
    Resize { width: u16, height: u16 },
    /// Render tick.
    Tick,
}

pub enum WorkerStatusMsg {
    /// New phase row. Appended to model's entry list.
    PhaseStarted { name: &'static str },
    /// Update current (last) phase's status line.
    /// Shown as the ⎿ child text until graph heartbeat takes over.
    Update(String),
    /// Agent type resolved (updates current phase's agent display).
    AgentResolved(String),
    /// Bind current phase to a task ID for heartbeat lookup. The view
    /// shows this task's heartbeat instead of worker_status once available.
    TaskBound(String),
    /// Bind current phase to a parent task whose children it orchestrates.
    /// View uses this to render subtask table and lane blocks.
    Orchestrates(String),
    /// Current phase completed. Plain text — view adds ✔ icon + styling.
    PhaseDone { result: String },
    /// Current phase failed. Plain text — view adds ✘ icon + styling.
    PhaseFailed { error: String },
    /// Section header (e.g., "Initial Build", "Iteration 2").
    /// Inserted as a non-phase entry in the display list.
    Section(String),
    /// Pipeline finished successfully.
    Done,
}
```

## Model

```rust
/// An entry in the ordered display list.
pub enum Entry {
    Phase(PhaseState),
    Section(String),
}

pub struct PhaseState {
    /// Phase name: "plan", "decompose", "loop", "review", "fix", "task"
    pub name: &'static str,
    /// Agent type, if known (set by AgentResolved)
    pub agent: Option<String>,
    /// Task ID for heartbeat lookup (set by TaskBound).
    /// None for phases without an agent (e.g., "plan").
    pub task_id: Option<String>,
    /// Parent task whose children this phase orchestrates (set by Orchestrates).
    /// View uses this for subtask table and lane block rendering.
    /// None for phases that don't manage subtasks (e.g., "plan", "review").
    pub orchestrates_id: Option<String>,
    /// Last status text from worker (pre-heartbeat)
    pub worker_status: Option<String>,
    /// Phase lifecycle
    pub state: PhaseLifecycle,
    /// When this phase started (for elapsed timer)
    pub started_at: Instant,
}

pub enum PhaseLifecycle {
    Active,
    Done { result: String },
    Failed { error: String },
}

pub struct Model {
    pub graph: Arc<TaskGraph>,
    pub window: WindowState,
    /// Ordered list of phases and sections, built up by worker messages.
    pub entries: Vec<Entry>,
    /// True after worker sends Done (or WorkerDisconnected handled).
    /// In run_with_worker: set by WorkerStatusMsg::Done.
    /// In worker-less run (--attach): set by graph-based is_finished check.
    pub finished: bool,
    /// True after Ctrl+C
    pub detached: bool,
}
```

**View rendering for each phase:**
1. If `Done` → render completed (合 icon, dim, `✔ {result}` in green)
2. If `Failed` → render failed (合 icon, red, `✘ {error}` in red)
3. If `Active` and `task_id` is set → look up heartbeat in graph for that task ID:
   - Heartbeat found → render heartbeat text + elapsed
   - No heartbeat yet → render `worker_status` (pre-session text)
4. If `Active` and `task_id` is None → render `worker_status` (no heartbeat possible)

**Graph-derived components** are driven by `phase.orchestrates_id`:

If a phase has `orchestrates_id`, the view looks up that task's children
in the graph and renders:
- **Subtask table** — shown when the phase (or any later phase with the
  same `orchestrates_id`) is active or done
- **Lane blocks** — shown inside the active loop phase as child lines of
  `⠹ loop`, derived from the orchestrated task's children

The subtask table is positioned **above the loop phase** (between
decompose-done and loop-active) since it needs to stay visible during
orchestration. Multiple phases can share the same `orchestrates_id`
(decompose and loop both orchestrate the epic).

**Issue list** is driven by `phase.task_id` on review phases — the view
looks up review issues for that task in the graph.

Lane blocks show per-lane heartbeat, completion counts, and agent type.
Lanes with Reserved tasks but no heartbeat show "starting session...".

## WorkerStatus Helper

Thin wrapper around the channel sender. Eliminates
`tx.send(WorkerStatusMsg::...)` boilerplate from worker closures.

```rust
pub struct WorkerStatus {
    tx: mpsc::Sender<WorkerStatusMsg>,
}

impl WorkerStatus {
    pub fn new(tx: mpsc::Sender<WorkerStatusMsg>) -> Self { Self { tx } }

    /// Start a new phase. Appends a phase row to the display.
    pub fn start(&self, name: &'static str)  { self.send(WorkerStatusMsg::PhaseStarted { name }); }

    /// Update current phase's ⎿ line (pre-heartbeat status).
    pub fn update(&self, text: &str)         { self.send(WorkerStatusMsg::Update(text.into())); }

    /// Set current phase's agent display (e.g., "claude").
    pub fn agent(&self, name: &str)          { self.send(WorkerStatusMsg::AgentResolved(name.into())); }

    /// Bind current phase to a task ID. Enables heartbeat lookup in graph.
    pub fn task(&self, id: &str)             { self.send(WorkerStatusMsg::TaskBound(id.into())); }

    /// Bind current phase to a parent task it orchestrates.
    /// View uses this to render subtask table and lane blocks from children.
    pub fn orchestrates(&self, id: &str)     { self.send(WorkerStatusMsg::Orchestrates(id.into())); }

    /// Mark current phase completed. View adds ✔ icon.
    pub fn done(&self, result: &str)         { self.send(WorkerStatusMsg::PhaseDone { result: result.into() }); }

    /// Mark current phase failed. View adds ✘ icon.
    pub fn failed(&self, error: &str)        { self.send(WorkerStatusMsg::PhaseFailed { error: error.into() }); }

    /// Insert a section header ("Initial Build", "Iteration 2").
    pub fn section(&self, title: &str)       { self.send(WorkerStatusMsg::Section(title.into())); }

    /// Signal pipeline completion.
    pub fn finish(&self)                     { self.send(WorkerStatusMsg::Done); }

    fn send(&self, msg: WorkerStatusMsg)     { let _ = self.tx.send(msg); }
}
```

## Worker Thread API

Each command creates a worker closure and passes it to `run_with_worker()`.
The closure receives a `WorkerStatus` helper (not a raw `Sender`).

```rust
// In build command:
let worker = move |status: WorkerStatus, cwd: PathBuf| -> Result<()> {
    // Phase 1: plan
    status.start("plan");
    status.update("Validating...");
    let plan = validate_plan(&cwd, &plan_path)?;
    let agent = resolve_agent_type(&cwd, &decompose_task)?;
    status.agent(agent.display_name());
    status.done(&plan_path);

    status.section("Initial Build");

    // Phase 2: decompose
    status.start("decompose");
    status.orchestrates(&epic_id);    // subtask table shows epic's children
    status.update("Reading task graph...");
    let epic = find_or_create_epic(&cwd, &plan)?;
    status.update("Finding epic...");
    status.update("Resolving agent...");
    let agent = resolve_agent_type(&cwd, &decompose_task)?;
    status.agent(agent.display_name());
    status.update("creating isolated workspace...");
    status.update("starting session...");
    status.task(&decompose_task_id);  // heartbeat lookup
    // Blocks here — graph heartbeat takes over in the TUI
    run_decompose(&cwd, &plan_path, &epic_id, options, false)?;
    let count = count_subtasks(&cwd, &epic_id)?;
    status.done(&format!("{} subtasks created", count));

    // Phase 3: loop
    status.start("loop");
    status.task(&loop_task_id);       // loop orchestrator task
    status.orchestrates(&epic_id);    // lane blocks from epic's children
    // Blocks here — graph provides lane data, per-lane agents resolved dynamically
    run_loop(&cwd, &epic_id, loop_options, false)?;
    status.done("All lanes complete");

    // Optional phase 4: review + fix cycle
    if review_after {
        status.start("review");
        status.update("Resolving agent...");
        let agent = resolve_agent_type(&cwd, &review_task)?;
        status.agent(agent.display_name());
        status.update("starting session...");
        status.task(&review_task_id);  // bind for heartbeat handoff
        let review_result = run_review(&cwd, &epic_id)?;
        // ... handle review result, fix cycles, sections ...
    }

    status.finish();
    Ok(())
};

tui::app::run_with_worker(model, &cwd, worker)?;
```

## TUI `run` Function Signature

```rust
/// Run the TUI with a worker thread that performs the actual pipeline work.
///
/// The closure receives a `WorkerStatus` helper for lifecycle updates and
/// runs in a separate thread. The TUI renders from both worker status
/// messages and JJ graph polls.
///
/// On Ctrl+C: the stop flag is set, the worker thread is expected to check
/// it periodically and exit. The main thread joins the worker before returning.
pub fn run_with_worker<F>(model: Model, cwd: &Path, worker: F) -> Result<Effect>
where
    F: FnOnce(WorkerStatus, PathBuf) -> Result<()> + Send + 'static,
{
    // ... setup terminal, spawn JJ reader thread ...

    // Spawn worker thread — construct WorkerStatus helper around channel
    let (tx, worker_rx) = mpsc::channel();
    let status = WorkerStatus::new(tx);
    let cwd_owned = cwd.to_owned();
    let worker_handle = thread::spawn(move || worker(status, cwd_owned));

    // Event loop polls worker_rx alongside jj_rx
    let result = run_loop(model, &jj_rx, &worker_rx, &stop, ...);

    // Cleanup: join worker thread
    let _ = worker_handle.join();

    // ... restore terminal ...
}
```

## View Functions

**Keep per-screen view files and existing screen dispatch.** The flows
diverge in rendering: build has iteration sections and lane blocks,
review has issue lists, fix has quality loop progress. Screen selection
works the same way it does today — no changes needed.

Per-screen files (`screens/build.rs`, `screens/review.rs`,
`screens/fix.rs`, `screens/task_run.rs`) are refactored to iterate
`model.entries` instead of deriving state from the graph alone. They
share **common rendering components** extracted into shared helpers:

- `render_phase_line(phase, graph)` — spinner/合 icon, name, agent, status/heartbeat
- `render_subtask_table(graph, orchestrates_id)` — bordered subtask list
- `render_lane_blocks(graph, orchestrates_id)` — per-lane heartbeats
- `render_issue_list(graph, task_id)` — numbered issue list
- `render_summary_line(...)` — final stats

Each screen composes these for its flow.

## What Changes Per Command

### `aiki build <plan>` (TTY sync path)
- **Before**: spawn `--_continue-async`, show `Screen::Build` TUI
- **After**: create worker closure that runs decompose → loop → review → fix, pass to `run_with_worker`

### `aiki review <target>` (TTY sync path)
- **Before**: spawn `--_continue-async`, show `Screen::Review` TUI
- **After**: create worker closure that runs `task_run` on review task, pass to `run_with_worker`

### `aiki fix <review-id>` (TTY sync path)
- **Before**: spawn `--_continue-async`, show `Screen::Fix` TUI
- **After**: create worker closure that runs quality loop, pass to `run_with_worker`

### `--async` paths
- **Unchanged**: still use `spawn_aiki_background`. These are intentionally fire-and-forget.

### Non-TTY sync paths
- **Unchanged**: still block with text output.

## Ctrl+C Handling

The worker thread needs to check the `stop` flag periodically. The cleanest approach:

1. Share the `Arc<AtomicBool>` stop flag with the worker
2. Pipeline functions (`task_run`, `run_decompose`, `run_loop`) already block until the agent finishes — but the agent process can be killed
3. On Ctrl+C: set stop flag, the JJ reader exits, the worker's current blocking call (`task_run`) detects the signal and returns, the worker thread exits
4. Main thread joins the worker thread, restores terminal

The `task_run` function already handles signals (the spawned agent process receives SIGTERM via process group). So Ctrl+C should propagate naturally.

## Worker Panic / Unexpected Exit

If the worker thread panics or returns `Err` without calling
`status.failed()`, the channel sender is dropped. `poll_next_msg`
detects this via `TryRecvError::Disconnected`:

```rust
match worker_rx.try_recv() {
    Ok(msg) => Some(Msg::Worker(msg)),
    Err(TryRecvError::Empty) => None,
    Err(TryRecvError::Disconnected) => {
        if !model.finished {
            Some(Msg::WorkerDisconnected)  // abnormal exit
        } else {
            None  // normal: worker sent Done then dropped
        }
    }
}
```

The `update()` handler for `WorkerDisconnected`:
- Marks the current (last active) phase as
  `Failed { error: "worker exited unexpectedly" }`
- Sets `model.finished = true` to avoid re-triggering

This ensures the TUI never gets stuck showing a spinner after the
worker is gone.

## Implementation Steps

1. Add `WorkerStatusMsg` enum, `WorkerStatus` helper, `Entry`, `PhaseState`, `PhaseLifecycle` types
2. Add `entries: Vec<Entry>` to `Model`
3. Add `run_with_worker` to `tui::app`
4. Update `poll_next_msg` to check `worker_rx` — emit `WorkerDisconnected`
   on `TryRecvError::Disconnected` when `!model.finished`
5. Implement `update()` handlers for each `WorkerStatusMsg` variant + `WorkerDisconnected`:
   - `PhaseStarted` → push new `Entry::Phase` to `entries`
   - `Update` → update last phase's `worker_status`
   - `AgentResolved` → update last phase's `agent`
   - `TaskBound` → set last phase's `task_id` (heartbeat lookup)
   - `Orchestrates` → set last phase's `orchestrates_id` (subtask table/lanes)
   - `PhaseDone` → set last phase to `Done { result }`
   - `PhaseFailed` → set last phase to `Failed { error }`
   - `Section` → push `Entry::Section` to `entries`
   - `Done` → set `model.finished = true`
   - `WorkerDisconnected` → mark last active phase as Failed, set `finished`
6. Replace per-screen view functions with single `render_entries()`:
   - Iterate `model.entries`, render phase lines + section headers
   - Insert graph-derived components using `phase.orchestrates_id`
   - Heartbeat handoff: active phase with `task_id` → graph lookup
7. Refactor `build.rs` TTY sync path to use worker closure
8. Refactor `review.rs` TTY sync path to use worker closure
9. Refactor `fix.rs` TTY sync path to use worker closure
10. Extract shared rendering helpers (`render_phase_line`, `render_subtask_table`,
    `render_lane_blocks`, `render_issue_list`, `render_summary_line`)
11. Refactor per-screen view files to use entries + shared helpers
12. Remove `run_with_child` shim
13. Remove `build_continue_args` helper (no longer needed for TTY path)
14. Add tests for worker status message handling in `update()`

## What We Keep

- `--_continue-async` infrastructure (used by `--async` flag)
- `spawn_aiki_background` (used by `--async` flag)
- Screen state tests (`tui_screen_states.rs`) — updated for entry-based rendering
- View function tests (`tui_visual_verify.rs`) — updated for entry-based rendering
- `latest_heartbeat()` function (used by phase rendering for graph lookup)
- `Screen` enum and per-screen view files (refactored, not removed)
- Viewport growth in final render

## What We Remove

- `run_with_child` (replaced by `run_with_worker`)
- `kill_process` (worker thread dies with the process)
- `build_continue_args` (only used by the now-removed TTY spawn path)
- The spawn-then-monitor pattern in build/review/fix TTY sync paths
- `"working..."` fallback in `latest_heartbeat()` (redundant — `worker_status`
  covers the pre-heartbeat gap; heartbeat-not-yet-available is handled by
  the view's phase rendering rules)

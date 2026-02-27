# Aiki TUI — Implementation Plan

**Design**: [v3.html](../research/aiki-tui/mockups/v3.html) (polished screens: task list, task show, build, review, fix)
**Design reference**: [HANDOFF.md](../research/aiki-tui/HANDOFF.md)
**Previous iterations**: [v1.html](../research/aiki-tui/mockups/v1.html) (sidebar + lanes), [v2.html](../research/aiki-tui/mockups/v2.html) (tree DAG + stage track)

## What we're building

Seven interconnected TUI screens that form the complete aiki experience:

1. **Task list** (`aiki task`) — landing screen, all tasks grouped by status
2. **Task show** (`aiki task show <id>`) — task detail with subtask tree, comments
3. **Build live** (`aiki build`) — watching a build in progress, event log
4. **Build show** (`aiki build show`) — retrospective: subtask table, timing, agents
5. **Review live** (`aiki review`) — watching review passes, issues surfacing inline
6. **Review show** (`aiki review show`) — retrospective: issues, file-review matrix
7. **Fix** (`aiki fix`) — fix tasks with issue←→task linking, iteration tracking

Each screen has **consistent chrome**: breadcrumb bar at top, content area, key hints at bottom.

```
 aiki > tasks > luppzupt > build

 Implement Stripe webhook event handling                          0:34
 ▸ build  3/6            ○ review            ○ fix

 ───────────────────────────────────────────────────────────────────────────

   ● ━━ ● ━┬━ ◉ Implement webhook route… ━━ ○               cur
              │
              ├━ ◉ Verify Stripe signatures…                   cc
              │
              ╰━ ○ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┬━ ○
                                                        ╰━ ○

 ───────────────────────────────────────────────────────────────────────────

 00:14  ✓ nmps       6s
 00:14  ▸ kyxr  cur  │  ▸ qwvp  cc  (parallel)
```

### Live vs Show screens

Each phase has two modes with different information needs:

| Mode | Command | Purpose | Density | Key content |
|------|---------|---------|---------|-------------|
| **Live** | `aiki build`, `aiki review`, `aiki fix` | Watch active work | Dense, kinetic | Tree DAG, event log, active nodes with names |
| **Show** | `aiki build show`, `aiki review show` | Examine completed work | Spacious, static | Full subtask table, file matrix, issue detail |

Live answers "what's happening now?" Show answers "what happened and what do I do next?"

### Screen navigation

```
task list ──[Enter]──> task show ──[Enter on phase]──> build/review/fix (live or show)
                                                          │
                                                          ├──[Enter on focused node]──> subtask drill-down
                                                          └──[Esc]──> back

build (live) ──[auto-advance]──> review (live) ──[auto-advance]──> fix (live)
task show: Enter on a completed phase opens its "show" retrospective view
[Esc] always goes back one level. [q] quits from anywhere.
[Enter] is the universal drill-in key — works on task list items, phase headers, and tree nodes.
```

## Scoping — what's in / what's out

**In scope (this plan)**:
- All 7 screens from v3.html
- Tier 1 only: full DAG tree for a single pipeline (1 task)
- Build / review / fix stage progression (live + show)
- Live refresh via fs-notify on JJ op_heads (fallback: timed polling) + crossterm event loop
- Tree DAG rendering with box-drawing connectors
- Breadcrumb navigation bar
- Subtask table for build show (name, agent, duration, status)
- Issue list for review show with structured severity/file data (from [expand-issues.md](expand-issues.md))
- File-review matrix for review show
- Issue←→fix linking in fix screen
- Fix iteration tracking
- Event log on live screens
- Key bindings: `q` quit, `Enter` drill/select, `Esc` back, `j/k` navigate, `Space` toggle log scroll
- Subtask drill-down with comment history

**Out of scope (later plans)**:
- Tier 2–4 density scaling (row view, group view, fleet dashboard)
- Sidebar / multi-plan view (v1 design — separate plan)
- `--ascii` fallback mode
- `--density` override flags
- Agent filtering (`--agent`)
- `--stuck` filter
- Action keybindings (`[b]` build, `[f]` fix, `[r]` retry, `[x]` close, `[c]` comment, `[d]` diff) — shown in v3 mockup key hints as aspirational but require command integration
- Interactive agent control (stop/retry/follow from TUI)
- Session attachment
- Plan editor integration
- Color theme customization
- Log scrollback / search

## Architecture

```
cli/src/
  main.rs                    ← add Status command variant
  commands/
    mod.rs                   ← add pub mod status
    status.rs                ← NEW: command entry point, arg parsing, screen routing
  tui/
    mod.rs                   ← NEW: pub mod app, screens, widgets, theme
    app.rs                   ← NEW: App state, event loop, screen transitions
    theme.rs                 ← NEW: color palette, symbols, separator helpers
    screens/
      mod.rs                 ← NEW: Screen trait, screen enum
      task_list.rs           ← NEW: screen 1 — task list grouped by status
      task_show.rs           ← NEW: screen 2 — task detail, comments, subtask tree
      build_live.rs          ← NEW: screen 3 — live build with tree DAG + event log
      build_show.rs          ← NEW: screen 3e — retrospective subtask table
      review_live.rs         ← NEW: screen 4 — live review with inline issues
      review_show.rs         ← NEW: screen 4d — issue detail, file-review matrix
      fix_live.rs            ← NEW: screen 5 — fix with issue linking
      drill_down.rs          ← NEW: screen 6 — subtask detail + comment history
    widgets/
      mod.rs                 ← NEW: pub mod tree, stage_track, log, breadcrumb, ...
      tree.rs                ← NEW: DAG tree renderer (custom widget)
      stage_track.rs         ← NEW: stage track header bar
      breadcrumb.rs          ← NEW: breadcrumb navigation bar
      log.rs                 ← NEW: event log widget
      subtask_table.rs       ← NEW: subtask list with name/agent/time/status
      issue_list.rs          ← NEW: issue list with severity/source/file
      file_matrix.rs         ← NEW: file × review pass matrix with ✓/✗
  tasks/
    status_monitor.rs        ← existing (keep for `aiki task run` inline display)
    lanes.rs                 ← existing (reuse derive_lanes, lane_status)
    graph.rs                 ← existing (reuse materialize_graph, TaskGraph)
```

### Data flow

```
refresh (live screens — triggered by fs-notify on .jj/repo/op_heads/ or fallback timer):
  read_events(cwd)
  → fingerprint check (event_count + latest_timestamp) — skip if unchanged
  → materialize_graph(events)
  → find build/epic task for target
  → identify active stage (build / review / fix)
  → derive_lanes(graph, stage_parent_id)
  → for each lane: lane_status(lane, graph)
  → build TreeLayout from lanes + task states
  → render active screen via ratatui

static render (show screens — single read, no refresh loop):
  read_events(cwd)
  → materialize_graph(events)
  → find completed stage
  → collect all subtasks with full metadata (name, agent, duration, status)
  → for review: collect issues, build file-review matrix
  → render via ratatui
```

### New dependencies

```toml
# cli/Cargo.toml
ratatui = "0.29"
notify = { version = "7", features = ["macos_kqueue"] }
```

crossterm is already a dependency (0.28). ratatui 0.29 uses crossterm 0.28, so compatible.
`notify` provides fs-notification-based change detection for live screens (see §5.2).

## Design system (from v3)

### Chrome

Every screen has three zones:

1. **Breadcrumb bar** (top): `aiki > tasks > luppzupt > build` — spatial orientation
2. **Content area** (middle): screen-specific content
3. **Key hints** (bottom): contextual keybindings for current screen

### Information weights

- **Primary**: white/bright (`#cccccc` / `#e8e8e8`) — task names, headers
- **Supporting**: mid-gray (`#777`) — descriptions, metadata values
- **Structural**: dim (`#3a3a44`) — separators, labels, inactive elements
- **Status colors**: reserved for state indicators only (one color per element)

### Separators

No box-drawing borders on content panels. Content separated by:
- Blank lines between sections
- Dim horizontal rules (`───────`) between major blocks
- Box-drawing chars used **only** for the DAG tree itself

### Color palette

```rust
pub const GREEN: Color = Color::Rgb(95, 204, 104);   // #5fcc68 — done, success, build
pub const CYAN: Color = Color::Rgb(91, 184, 201);    // #5bb8c9 — review, info, ready
pub const YELLOW: Color = Color::Rgb(212, 168, 64);   // #d4a840 — active, in-progress, fix
pub const RED: Color = Color::Rgb(224, 85, 85);       // #e05555 — failed, error, stuck
pub const MAGENTA: Color = Color::Rgb(196, 112, 176); // #c470b0 — cursor agent
pub const BLUE: Color = Color::Rgb(85, 136, 204);     // #5588cc — informational
pub const ORANGE: Color = Color::Rgb(204, 136, 68);   // #cc8844 — warnings, issues
pub const DIM: Color = Color::Rgb(58, 58, 68);        // #3a3a44 — borders, inactive
pub const FG: Color = Color::Rgb(119, 119, 119);      // #777777 — supporting text
pub const WHITE: Color = Color::Rgb(204, 204, 204);   // #cccccc — primary text
pub const HI: Color = Color::Rgb(232, 232, 232);      // #e8e8e8 — high-contrast headers
```

### Symbols

```rust
pub const SYM_DONE: &str = "●";      // filled circle — completed
pub const SYM_ACTIVE: &str = "◉";    // fisheye — in-progress (fallback: bold ●)
pub const SYM_PENDING: &str = "○";   // empty circle — pending
pub const SYM_FAILED: &str = "✗";    // ballot X — failed
pub const SYM_CHECK: &str = "✓";     // check mark — phase passed
pub const SYM_RUNNING: &str = "▸";   // right-pointing triangle — in progress
```

## Implementation phases

### Phase 1: Foundation — theme, chrome, static task list

Goal: `aiki status` renders the task list once and exits. Establishes the chrome pattern.

**1.1 — Scaffolding**

- Add `ratatui = "0.29"` to `cli/Cargo.toml`
- Create module structure: `tui/mod.rs`, `tui/screens/mod.rs`, `tui/widgets/mod.rs`
- Create `cli/src/commands/status.rs` — parse args, find target, route to screen
- Register `Status` variant in `main.rs` Commands enum

```rust
// main.rs
Commands::Status {
    target: Option<String>,  // plan path, task ID, or subcommand
    show: bool,              // --show flag for retrospective view
}
```

**1.2 — Theme + chrome widgets**

`tui/theme.rs`:
- Color constants (palette above)
- Symbol constants
- Helper functions: `separator_line(width)`, `dim_text()`, `status_color(state)`

`tui/widgets/breadcrumb.rs`:
- Renders: `aiki > tasks > luppzupt > build`
- Takes a `Vec<BreadcrumbSegment>` with text + style
- Dim separators, last segment bold/white

**1.3 — Task list screen (v3 scene 1a/1b)**

`tui/screens/task_list.rs`:
- Query all tasks, group by status (In Progress, Ready, Done)
- One line per task with inline indicators
- Pipeline tasks: `B:✓ R:✗2 F:1/3` compact phase progress
- Completed pipeline tasks: include compact file-review provenance, e.g. `6 files, 3 reviewed`
  or `8 files ✓` (all reviewed). Surfacing this on the task list gives immediate signal about
  review coverage without drilling in. Data comes from the file-review matrix (§4.5).
  **Note**: this requires pipeline state (Phase 2) and file-review data (Phase 4). In Phase 1,
  render task list without provenance; add it as a Phase 4 enhancement.
- Ad-hoc tasks: `3/5 subtasks`
- Right-aligned agent abbreviation + time
- Empty state (scene 1b)

This screen establishes the rendering pattern: breadcrumb → content → key hints.

### Phase 2: Task show + pipeline state model

Goal: `aiki status <task-id>` shows task detail with subtask tree.

**2.1 — Pipeline state model**

```rust
enum PipelineStage { Build, Review, Fix }

enum StageStatus { Pending, Active, Complete, Failed, Skipped }

struct StageState {
    stage: PipelineStage,
    status: StageStatus,
    parent_task_id: Option<String>,
    done_count: usize,
    total_count: usize,
    issue_count: Option<usize>,    // review only
    elapsed: Option<Duration>,
}

struct PipelineState {
    task_name: String,
    task_id: String,
    build: StageState,
    review: StageState,
    fix: StageState,
    iteration: usize,
    total_elapsed: Duration,
    agents: Vec<String>,
}
```

Derivation:
1. Find the epic task for the target
2. Build stage: epic's direct subtasks (implementation tasks), use `derive_lanes`
3. Review stage: tasks linked via `validates` to the epic, or `data.target` pointing to build
4. Fix stage: tasks linked via `remediates` to review, or `source: task:<review-id>`

Handle edge cases:
- Build hasn't started review → review/fix Pending
- Build failed → review/fix blocked (show as Pending, not Failed)
- Review found 0 issues → fix Skipped (show `—`)
- Review found N issues → fix has N tasks

**Action item**: Before implementing, audit `aiki build --review --fix` to confirm how stages are linked in the task graph.

**2.2 — Stage track widget**

`tui/widgets/stage_track.rs`:
- Compact form (task list): `B:✓ R:✗2 F:1/3`
- Full form (task show / live screens): `✓ build  6/6     ▸ review  1/3      ○ fix`
- Active stage: yellow bold with `▸`
- Complete stage: green with `✓`
- Pending stage: dim with `○`
- Complete with issues: green `✓` + orange issue count

**2.3 — Task show screen (v3 scenes 2a/2b/2c)**

`tui/screens/task_show.rs`:
- Without subtasks (2a): name, status, metadata, description, comments timeline
- With subtasks and pipeline (2b): stage track + tree DAG per stage + comments
- All phases visible (2c): build/review trees dimmed green, fix active yellow

Tree rendering reuses the tree widget (Phase 3) — task show calls it per stage.

### Phase 3: Tree DAG widget + build live

Goal: `aiki status <task-id> --live` (or auto when build is active) shows live build progress.

**3.1 — Tree layout algorithm**

Core: convert `LaneDecomposition` + task states into renderable rows.

```rust
struct TreeNode {
    task_id: String,
    state: NodeState,      // Done | Active | Pending | Failed | Stuck
    name: Option<String>,  // only Active/Failed nodes (truncated)
    agent: Option<String>, // only Active nodes
    has_children: bool,    // true if this node has its own subtasks (recursive)
}

enum TreeRow {
    /// Main line: ● ━━ ● ━┬━ ◉ Name… ━━ ○
    MainLine { nodes: Vec<(TreeNode, Connector)> },
    /// Branch line:           ├━ ◉ Name…
    Branch { indent: usize, connector: BranchConnector, node: TreeNode },
    /// Continuation line:     │
    Continuation { indent: usize },
}
```

Layout algorithm:
1. Topological sort all tasks in the stage
2. Walk left-to-right; each task becomes a node
3. Fan-out: `━┬━` — first dependent on main line, rest on branch lines below
4. Fan-in: `━╯` — branches converge at dependency node
5. Measure indentation by horizontal position
6. Same approach as `git log --graph` but horizontal

**3.2 — Tree renderer widget**

`tui/widgets/tree.rs` — custom `StatefulWidget`:
- Takes a `TreeLayout`
- Renders each `TreeRow` as a `Line` of `Span`s
- Node colors: green done, yellow bold active, dim pending, red failed
- `stuck` label (red) on nodes blocked by a failed dependency
- Box-drawing chars as dim spans
- Agent abbreviation right-aligned on active branches

**3.3 — Event log widget**

`tui/widgets/log.rs`:
- `List` widget with task event entries
- Parallel events on one line: `▸ kyxr cur  │  ▸ qwvp cc  (parallel)`
- Timestamps left-aligned, agent abbreviations color-coded
- Oldest entries dim (opacity ~0.45), newest bright (~0.9)
- Auto-scroll, `[Space]` to pause/resume scroll (avoids conflict with vim-style `l`)

**3.4 — Build live screen (v3 scenes 3a-3d)**

`tui/screens/build_live.rs`:
- Breadcrumb: `aiki > tasks > <id> > build`
- Stage track header (full form)
- Tree DAG for build stage
- Event log at bottom
- Handle: fan-out (3a), fan-in (3b), failure with stuck (3c), complete with auto-advance (3d)

### Phase 4: Build show + review screens

Goal: retrospective build view and live/show review.

**4.1 — Subtask table widget**

`tui/widgets/subtask_table.rs`:
- Table with columns: `# SUBTASK AGENT TIME STATUS`
- Full subtask names (not truncated — retrospective mode has room)
- Failed rows: red status + error reason on next line, indented
- Stuck rows: dim name, `—` for agent/time, red `stuck` status
- Footer: parallelism stats, agent breakdown

**4.2 — Build show screen (v3 scenes 3e/3f)**

`tui/screens/build_show.rs`:
- Breadcrumb: `aiki > tasks > <id> > build (completed)`
- Summary header: `✓ build  6/6  2m28s  2 agents`
- Tree DAG (all green, static)
- Subtask table below
- Footer: `Parallelism: peak 2 concurrent │ Agents: cc 4 tasks  cur 4 tasks`
- Failure variant (3f): red `✗` in tree, error in table, `[r] retry failed` key hint

**4.3 — Issue list widget**

`tui/widgets/issue_list.rs`:

Issues are task comments with `data.issue = "true"` on the review task. Read via
`review::get_issue_comments(task)` (already public in `cli/src/commands/review.rs`).

- Numbered issues with structured fields from [expand-issues.md](expand-issues.md):
  - **Severity**: `data.severity` — `high` (red), `medium` (orange), `low` (dim). Default: medium
  - **Location**: `data.path`, `data.start_line`, `data.end_line` (or `data.locations` for multi-file).
    Use `parse_locations()` helper to normalize both formats into `Vec<Location>`
  - **Source**: derived from the review subtask that created the comment (the agent/pass name)
- Comment author (which agent/review pass found it)
- Issue status tracking when linked to fix tasks: `fixing…`, `fixed`
- Full description text (multi-line — the `comment.text` body)
- Issues sorted by severity: high → medium → low (matches expand-issues.md display order)

Backward compatibility: issues created before expand-issues.md have no severity/location
fields. The widget treats missing severity as `medium` and omits location display.

**4.4 — Review live screen (v3 scenes 4a-4c)**

`tui/screens/review_live.rs`:
- Breadcrumb: `aiki > tasks > <id> > review`
- Completed build tree (dimmed green) above
- Review tasks as simple list (not DAG — reviews are typically 1-3 parallel passes)
- Issues section appears below as review tasks find them
- Event log at bottom

**4.5 — File-review matrix widget**

`tui/widgets/file_matrix.rs`:
- Table: file path, diff stats (`+142 −0`), per-review-pass `✓`/`✗`
- Files that triggered issues get `✗` in the relevant review column
- Compact but informative provenance chain

**4.6 — Review show screen (v3 scenes 4d/4e)**

`tui/screens/review_show.rs`:
- Breadcrumb: `aiki > tasks > <id> > review (completed)`
- Three sections: Review Passes, Issues, Files Modified
- Review passes: name, agent, duration, outcome (pass / N issues)
- Issues: full detail — severity color (high=red, medium=orange, low=dim), source pass, description, file location(s)
- File matrix: every build file × which reviews checked it
- Clean pass variant (4e): no issues section, `No issues found.`
- `[f] fix` key hint to trigger fix from this screen

### Phase 5: Fix screen + live event loop

Goal: Fix screen with issue linking, plus make all live screens actually live.

**5.1 — Fix live screen (v3 scenes 5a-5d)**

`tui/screens/fix_live.rs`:
- Breadcrumb: `aiki > tasks > <id> > fix`
- Fix tasks with issue back-references: `← issue 1: Missing rate limit`
- Tree DAG for fix tasks (typically simpler than build)
- Issue status tracker: `fixing…` → `fixed` per issue
- Iteration counter: `iter 1`, `iter 2`
- Re-review gate: verify task at end of fix tree
- Iteration 2 scene (5d): `← re-review found 1 remaining issue`
- **Victory screen (5c)**: when all fixes complete, show summary with total iteration count
  (`Completed in 2 iterations`) — key provenance data showing how many review→fix cycles
  the feature went through. Also show total elapsed time across all iterations and
  per-iteration breakdown (e.g., `iter 1: 3 issues, 4m12s │ iter 2: 1 issue, 1m45s`)

**5.2 — Change detection via fs-notify**

Task events are JJ commits on the `aiki/tasks` branch, read via `jj log`. They are NOT
filesystem files, so we can't watch an "events directory" directly. However, JJ stores
operation heads as files in `.jj/repo/op_heads/heads/` — a new file appears for every
JJ operation (commit, bookmark move, etc). Watching this directory tells us "something
changed in JJ" without knowing what.

```toml
# cli/Cargo.toml — new dependency
notify = { version = "7", features = ["macos_kqueue"] }
```

Setup:
```rust
use notify::{recommended_watcher, RecursiveMode, Watcher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Resolve the op_heads directory — follows .jj/repo pointer if it's a file (worktrees)
fn resolve_op_heads(cwd: &Path) -> Option<PathBuf> {
    let jj_repo = cwd.join(".jj/repo");
    let repo_dir = if jj_repo.is_file() {
        // Worktree: .jj/repo contains a path to the real repo dir
        std::fs::read_to_string(&jj_repo).ok()?.trim().into()
    } else {
        jj_repo
    };
    let heads = repo_dir.join("op_heads/heads");
    heads.is_dir().then_some(heads)
}

fn setup_watcher(cwd: &Path) -> Option<(impl Watcher, Arc<AtomicBool>)> {
    let op_heads = resolve_op_heads(cwd)?;
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();

    let mut watcher = recommended_watcher(move |_res| {
        flag_clone.store(true, Ordering::Release);
    }).ok()?;

    watcher.watch(&op_heads, RecursiveMode::NonRecursive).ok()?;
    Some((watcher, flag))
}
```

The `AtomicBool` approach is simpler than channel-based multiplexing — the crossterm
event loop already uses `poll(timeout)`, and we just check the flag after each iteration.

**5.3 — Event loop with hybrid refresh**

```rust
fn run_app(terminal: &mut Terminal, state: &mut AppState) -> Result<()> {
    // Try fs-notify; fall back to timed polling if unavailable
    let watcher = setup_watcher(&state.cwd);
    let notify_flag = watcher.as_ref().map(|(_, f)| f.clone());

    // Timed polling fallback: check every 500ms when notify unavailable
    let mut last_poll = Instant::now();
    let poll_interval = Duration::from_millis(500);

    loop {
        terminal.draw(|f| render_active_screen(f, state))?;

        // Poll crossterm for key events (100ms timeout for responsiveness)
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                match state.handle_key(key) {
                    Action::Quit => return Ok(()),
                    Action::Navigate(screen) => state.push_screen(screen),
                    Action::Back => state.pop_screen(),
                    Action::None => {}
                }
            }
        }

        if !state.is_live() {
            continue;
        }

        // Determine whether to refresh
        let should_refresh = if let Some(ref flag) = notify_flag {
            // fs-notify path: only refresh when JJ op_heads changed
            flag.swap(false, Ordering::AcqRel)
        } else {
            // Fallback: timed polling
            let now = Instant::now();
            if now.duration_since(last_poll) >= poll_interval {
                last_poll = now;
                true
            } else {
                false
            }
        };

        if should_refresh {
            if state.refresh()? {
                state.check_auto_advance()?;
            }
        }
    }
}
```

Key properties:
- **Idle efficiency**: when no agents are running, `notify_flag` stays false and we never
  call `jj log`. CPU usage is near-zero between key events.
- **Batched updates**: if an agent makes 5 commits in 100ms, we see one flag=true and
  call `refresh()` once. Natural debounce from the poll interval.
- **Graceful fallback**: if `notify` fails (permissions, unsupported fs, CI environment),
  falls back to 500ms timed polling — same as the existing `StatusMonitor`.

**5.4 — Fingerprinted refresh with scoped re-materialization**

`refresh()` avoids redundant work at two levels:

```rust
struct TuiState {
    cwd: PathBuf,
    target: String,               // epic/task ID we're watching
    graph: TaskGraph,             // cached full graph
    pipeline: PipelineState,      // derived pipeline for target
    fingerprint: (usize, u64),    // (event_count, latest_timestamp)
    // ... screen stack, etc
}

impl TuiState {
    /// Returns true if state actually changed
    fn refresh(&mut self) -> Result<bool> {
        // 1. Read events (runs `jj log` — the expensive I/O call)
        let events = read_events(&self.cwd)?;

        // 2. Fingerprint: skip re-materialization if nothing changed
        //    (handles the case where op_heads changed for unrelated reasons,
        //     e.g. a working-copy commit, not a task event)
        let fp = (
            events.len(),
            events.last().map(|e| e.timestamp()).unwrap_or(0),
        );
        if fp == self.fingerprint {
            return Ok(false);
        }
        self.fingerprint = fp;

        // 3. Re-materialize the full graph
        //    (In practice this is fast — materialize_graph is O(events) string parsing,
        //     not I/O. The expensive part was step 1.)
        self.graph = materialize_graph(&events);

        // 4. Re-derive only the pipeline for our target
        self.pipeline = find_pipeline_stages(&self.graph, &self.target);

        Ok(true)
    }
}
```

The fingerprint uses `(event_count, latest_timestamp)` — if both match, the task
events are identical and we skip re-materialization. This catches the common case where
the op_heads notification fired for an unrelated JJ operation (e.g., working copy snapshot).

**Future optimization** (not in v1): incremental graph updates. Instead of re-materializing
from scratch, process only new events (events after the last known timestamp) and patch
the existing graph. This would make `refresh()` O(new_events) instead of O(all_events).
Only worth doing if profiling shows `materialize_graph` is a bottleneck at scale.

**5.5 — Auto-advance between phases**

When build completes, auto-advance to review screen after 0.5s pause (showing "Advancing to review… ▸"). When review completes with issues, advance to fix. When review is clean, show done summary. User can `[Enter]` to skip the pause or `[Esc]` to go back.

### Phase 6: Navigation + drill-down

Goal: Navigate between screens, drill into subtask detail.

**6.1 — Screen stack navigation**

```rust
struct AppState {
    screen_stack: Vec<ScreenState>,  // breadcrumb = stack of screens
    // ...
}

impl AppState {
    fn push_screen(&mut self, screen: ScreenState) { ... }
    fn pop_screen(&mut self) { ... }  // [Esc]
    fn current_screen(&self) -> &ScreenState { ... }
}
```

The breadcrumb bar renders from the screen stack. `[Esc]` pops. `[Enter]` pushes.

**6.2 — Focus cursor on tree**

Track which node/branch is focused within tree views. `j`/`k` move between branches. Focused branch highlighted with brighter color or `>` indicator.

**6.3 — Subtask drill-down (v3 scenes 6a/6b)**

`tui/screens/drill_down.rs`:
- Breadcrumb: `aiki > tasks > <id> > build > kyxr`
- Full task name (not truncated), status, agent, duration
- Task description (if any)
- Comment history — agents leave progress comments via `aiki task comment`, so this is where
  you see what the agent has been doing (files edited, decisions made, progress updates)
- Close summary (if completed)
- `[Esc]` back to tree, `[j/k]` scroll comments

**Recursive subtasks:** A subtask can have its own subtasks (the task model is recursive).
When a drilled-into subtask has children, the drill-down renders its own subtask tree —
same tree widget, just one level deeper. The breadcrumb grows:
`aiki > tasks > <id> > build > kyxr > nmps`. `[Enter]` on a child subtask drills further.
`[Esc]` goes back one level. The screen stack handles arbitrary depth.

This means drill_down.rs is really a generic "task detail" screen that can recurse:
- No children → show comment history + file diff
- Has children → show subtask tree (reuse tree widget) + comments below
- Has children with pipeline stages → show stage track + trees (same as task_show)

The implementation reuses the same tree/stage_track widgets — drill-down is just task_show
at a different depth in the hierarchy.

Note: there is no live agent output stream. Agents work in separate sessions and communicate
via task comments and close summaries. The drill-down shows these comments chronologically.

### Phase 7: Polish + edge cases

**7.1 — Terminal resize handling**

Tree can get wide with many nodes. Handle narrow terminals:
- Truncate task names more aggressively
- Compress horizontal spacing in tree
- Minimum viable width: 60 columns

**7.2 — Edge cases**

- No build started yet → show task in "ready" state, `[b] build` prominent
- Build active, no review yet → review/fix stages Pending
- Build failed midway → show failure in tree, stuck dependents
- Review clean → skip fix, show done summary immediately
- Multiple iterations → track iter count, show which issues persist
- Empty fix (0 issues) → never shown, review→done
- Single subtask (no tree needed) → render as single node, no connectors

**7.3 — Accessibility**

- All colors have sufficient contrast on dark background
- No information conveyed by color alone (symbols always accompany colors)
- `◉` (fisheye) fallback: if rendering fails, use bold colored `●`
- Future: `--ascii` mode (Tier 1 chars only)

## Mapping v3 scenes to implementation

| Scene | Screen | What it shows | Phase |
|-------|--------|--------------|-------|
| 1a | task_list | Mixed tasks grouped by status | 1 |
| 1b | task_list | Empty state | 1 |
| 2a | task_show | No subtasks, in-progress | 2 |
| 2b | task_show | With subtasks, pipeline, mid-build | 2 |
| 2c | task_show | All three phases visible | 2 |
| 3a | build_live | Early fan-out, two agents | 3 |
| 3b | build_live | Fan-in convergence | 3 |
| 3c | build_live | Branch failure, stuck dependents | 3 |
| 3d | build_live | Complete, auto-advance to review | 3 |
| 3e | build_show | Retrospective, subtask table | 4 |
| 3f | build_show | Retrospective with failure | 4 |
| 4a | review_live | In progress, issues found | 4 |
| 4b | review_live | Complete, clean pass | 4 |
| 4c | review_live | Complete with issues, advance to fix | 4 |
| 4d | review_show | Retrospective, issues + file matrix | 4 |
| 4e | review_show | Retrospective, clean pass | 4 |
| 5a | fix_live | Two parallel fixes | 5 |
| 5b | fix_live | One done, one in progress | 5 |
| 5c | fix_live | All done, victory screen | 5 |
| 5d | fix_live | Iteration 2 | 5 |
| 6a | drill_down | Subtask detail — leaf (comment history) | 6 |
| 6b | drill_down | Subtask detail — has own subtask tree (recursive) | 6 |

## Finding the pipeline stages

The trickiest implementation question: how to identify the three stages from the task graph.

**Build stage**: The epic's direct subtasks (created by `aiki epic add` / template decomposition). These are the implementation tasks.

**Review stage**: Tasks linked via `validates` to the build/epic task. Created by `aiki review`. The review task is the parent; its subtasks are the review passes.

**Fix stage**: Tasks linked via `remediates` to the review task, or created by `aiki fix`. Each fix task addresses one review issue.

**Detection algorithm**:
```rust
fn find_pipeline_stages(graph: &TaskGraph, epic_id: &str) -> PipelineState {
    // Build: direct subtasks of the epic (excluding digest)
    let build_subtasks = get_subtasks(graph, epic_id)
        .into_iter()
        .filter(|t| t.name != DIGEST_SUBTASK_NAME)
        .collect();

    // Review: find tasks that validate the epic or build task
    let review_tasks: Vec<_> = graph.edges
        .sources(epic_id, "validates")
        .filter_map(|id| graph.tasks.get(id))
        .collect();

    // Fix: find tasks that remediate the review tasks
    let fix_tasks: Vec<_> = review_tasks.iter()
        .flat_map(|rt| graph.edges.sources(&rt.id, "remediates"))
        .filter_map(|id| graph.tasks.get(id))
        .collect();

    // ... build StageState for each
}
```

**Alternative approach**: May need to follow `data.epic`, `data.target`, `data.scope.id`, or `source: task:<id>` fields rather than link kinds. Audit `aiki build --review --fix` before implementing.

**Action item**: Before implementing, audit exactly how `aiki build --review --fix` creates review and fix tasks and what links/data fields connect them.

## File manifest (new files)

```
cli/src/tui/mod.rs                     ~15 lines    module declarations
cli/src/tui/theme.rs                    ~60 lines    colors, symbols, helpers
cli/src/tui/app.rs                     ~250 lines    App state, event loop, screen stack
cli/src/tui/screens/mod.rs              ~30 lines    Screen trait, enum
cli/src/tui/screens/task_list.rs       ~120 lines    screen 1
cli/src/tui/screens/task_show.rs       ~180 lines    screen 2
cli/src/tui/screens/build_live.rs      ~150 lines    screen 3
cli/src/tui/screens/build_show.rs      ~130 lines    screen 3e/3f
cli/src/tui/screens/review_live.rs     ~140 lines    screen 4a
cli/src/tui/screens/review_show.rs     ~160 lines    screen 4d/4e
cli/src/tui/screens/fix_live.rs        ~150 lines    screen 5
cli/src/tui/screens/drill_down.rs      ~100 lines    screen 6
cli/src/tui/widgets/mod.rs              ~15 lines    module declarations
cli/src/tui/widgets/tree.rs            ~300 lines    DAG tree renderer (biggest piece)
cli/src/tui/widgets/stage_track.rs      ~70 lines    stage track header bar
cli/src/tui/widgets/breadcrumb.rs       ~40 lines    breadcrumb nav bar
cli/src/tui/widgets/log.rs              ~80 lines    event log widget
cli/src/tui/widgets/subtask_table.rs    ~90 lines    subtask table for build show
cli/src/tui/widgets/issue_list.rs      ~100 lines    issue list for review/fix
cli/src/tui/widgets/file_matrix.rs      ~80 lines    file × review matrix
cli/src/commands/status.rs              ~80 lines    arg parsing, target resolution
```

Estimated total: ~2,340 lines of new code.

## Subtask breakdown

1. **Scaffold + deps** — Cargo.toml, module structure, commands/status.rs, main.rs registration
2. **Theme + chrome** — theme.rs, breadcrumb.rs, separator helpers
3. **Task list screen** — task_list.rs, query tasks, group/sort, compact phase indicators
4. **Pipeline state model** — PipelineState/StageState types, derivation logic, audit link wiring
5. **Stage track widget** — stage_track.rs, compact + full forms
6. **Task show screen** — task_show.rs, metadata, comments, description, subtask tree
7. **Tree layout algorithm** — topo sort, fan-out/fan-in, horizontal DAG layout
8. **Tree renderer widget** — tree.rs, box-drawing, colored spans, agent labels
9. **Event log widget** — log.rs, parallel events, timestamps, opacity gradient
10. **Build live screen** — build_live.rs, tree + log + stage track, failure states
11. **Subtask table widget** — subtask_table.rs, full names, timing, error reasons
12. **Build show screen** — build_show.rs, tree + table + summary footer
13. **Issue list widget** — issue_list.rs, severity colors, source, file refs, status tracking
14. **Review live screen** — review_live.rs, dimmed build tree, review passes, inline issues
15. **File-review matrix widget** — file_matrix.rs, file × pass grid
16. **Review show screen** — review_show.rs, passes + issues + file matrix
17. **Fix live screen** — fix_live.rs, issue←→task linking, iteration tracking, re-review gate
18. **Event loop + change detection** — app.rs, notify watcher on `.jj/repo/op_heads/`, fingerprinted refresh, key handling, screen stack navigation
19. **Auto-advance** — build→review→fix transitions, pause animation
20. **Drill-down screen** — drill_down.rs, full task detail, comment history (from `aiki task comment`)
21. **Polish** — terminal resize, edge cases, accessibility, `◉` fallback

## Resolved questions

### Review issue storage

**Issues are task comments with `data.issue = "true"`.** They live on the review task itself.

- `aiki review issue add <review-id> <text>` → calls `comment_on_task()` with `data: { "issue": "true" }`
- `aiki review issue list <review-id>` → filters comments via `get_issue_comments(task)` which checks `comment.data["issue"] == "true"`
- `aiki fix <review-id>` reads the same issue comments to create fix subtasks
- Review tasks also track `data.issue_count` for quick counts without scanning comments

**For the TUI**, the issue list widget reads from `get_issue_comments()` in `cli/src/commands/review.rs` (already public). Each issue comment has:
- `comment.id` — unique ID
- `comment.text` — the issue description (free-form text from the reviewing agent)
- `comment.data["issue"]` — `"true"` marker
- `comment.data["severity"]` — `"high"` | `"medium"` | `"low"` (default: medium if absent)
- `comment.data["path"]` — file path (optional, single-file issues)
- `comment.data["start_line"]` — start line number (optional)
- `comment.data["end_line"]` — end line number (optional)
- `comment.data["locations"]` — comma-separated `path:line-end` (optional, multi-file issues)
- `comment.author` — which agent found it

These structured fields come from [expand-issues.md](expand-issues.md), which adds `--severity`
and `--file` flags to `aiki review issue add`. The `parse_locations()` helper normalizes
both storage formats (decomposed fields vs packed `locations` string) into `Vec<Location>`.

**"Source" in the mockup** refers to which review pass found the issue. This is NOT a data
field — it's derived from the review subtask that created the comment. The TUI can determine
this by matching the comment's authoring context to the review subtask tree.

**Backward compatibility**: Issues created before expand-issues.md will have no severity or
location fields. The TUI treats missing severity as `medium` and omits location display.

## Open questions

1. **How does `aiki build --fix` connect review/fix tasks?** Need to audit orchestrator template and build.rs spawn logic. Determines pipeline state derivation.
2. **What if there's no epic yet?** Show plan in "ready to build" state with empty stages.
3. **Terminal too narrow for tree?** Need horizontal compression or truncation for narrow terminals.
4. **Multiple builds for same plan?** Show most recent or let user pick.

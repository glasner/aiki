# Command Startup: Blank Screen Problem

## Problem

When running `aiki build` in sync/TTY mode, there is a long pause with a completely blank screen before anything appears. The user sees nothing — no spinner, no status, no indication that work is happening.

## Root Cause

The sync path in `build.rs` enters the alternate screen (`ScreenSession::new()`) at line 369 **before** doing any of the expensive work that follows. The alternate screen is blank, and the first frame isn't drawn until the status monitor's event loop starts polling much later.

### Timeline of what happens

```
1. Validation & metadata parsing           (fast, ~ms)
2. cleanup_stale_builds() → jj log         (slow, subprocess)
3. read_events() → jj log                  (slow, subprocess)
4. materialize_graph + PlanGraph::build     (fast, in-memory)
5. find_epic_for_plan                       (fast, lookup)
6. ScreenSession::new()                     ← BLANK SCREEN STARTS HERE
7. find_or_create_epic():
   a. read_events() → jj log               (slow, subprocess)
   b. write_link_event() ×3 → jj new       (slow, 3 subprocesses)
   c. create_from_template():
      - find_templates_dir                  (fs walk)
      - load_template                       (file read + parse)
      - read_events() → jj log             (slow, subprocess)
      - get_working_copy_change_id → jj log (slow, subprocess)
      - write_event() → jj new             (slow, subprocess)
   d. write_link_event() ×2 → jj new       (slow, 2 subprocesses)
8. prepare_task_run():
   a. read_events() → jj log               (slow, subprocess)
   b. write_event(Started) → jj new        (slow, subprocess)
9. spawn_monitored() → spawns claude        (process start)
10. First monitor.poll() → read_events()    (slow, subprocess)
11. First build_view()                      (fast)
12. FIRST FRAME DRAWN                       ← BLANK SCREEN ENDS
```

Between steps 6 and 12, there are roughly **10-12 jj subprocess invocations** happening invisibly on a blank alternate screen. Each `jj log` or `jj new` call takes 100-500ms+, so the blank period can easily be 3-10 seconds.

### Pre-screen work (steps 2-5) also contributes

Steps 2-5 happen *before* the alternate screen but still contribute to perceived startup latency. The terminal shows nothing during this time either — just the shell prompt hanging.

## Scope

This affects `aiki build` in sync/TTY mode. The async path (`--async`) doesn't use a screen session.

Other commands that use `ScreenSession` may have similar issues (e.g., `aiki task run` in TTY mode), but `aiki build` is the most visible because it does the most pre-work (epic lookup, decompose, template creation) before the monitor starts.

---

## `aiki review` — Similar Issue

The `aiki review` command delegates to the shared `task_run()` function in `runner.rs`, which has blank screen issues.

### Timeline for `aiki review` (sync/TTY mode)

```
1. create_review_task_from_template():
   a. read_events() → jj log                (slow, subprocess)
   b. generate_task_id()                     (fast)
   c. load template & render                 (file I/O + parsing)
   d. write_event(AddTask) → jj new         (slow, subprocess)
   e. write_link_event() (if scope=task)    (slow, subprocess)

2. task_run() calls prepare_task_run():
   a. read_events() → jj log                (slow, subprocess)
   b. materialize_graph()                    (fast, in-memory)
   c. find_task()                            (fast, lookup)
   d. resolve_agent_type()                   (fast, lookup)
   e. write_event(Started) → jj new         (slow, subprocess)

3. run_with_status_monitor():
   a. runtime.spawn_monitored()              (process start)
   b. StatusMonitor::new()                   (fast)
   c. LiveScreen::new()                      ← BLANK SCREEN STARTS HERE
   d. First monitor_until_complete iteration:
      - read_events() → jj log              (slow, subprocess)
      - build_view()                         (fast)
   e. FIRST FRAME DRAWN                      ← BLANK SCREEN ENDS
```

**Key difference from build:** The blank screen period is shorter because `run_with_status_monitor` calls `LiveScreen::new()` (via `monitor_until_complete_with_child` at status_monitor.rs:335) after spawning the agent. However, **steps 1-2 happen with no terminal feedback** — the user sees a frozen shell prompt during 4-6 jj subprocess calls.

### Pre-screen work impact

Steps 1-2 happen before entering the LiveScreen, contributing ~500ms-2s of silent latency where the terminal shows nothing.

---

## `aiki fix` — Severe Blank Screen Issue

The `aiki fix` command has the **most severe blank screen problem** because it:
1. Creates a `ScreenSession` early (line 223 in `fix.rs`)
2. Performs extensive work on a blank alternate screen before any monitoring begins

### Timeline for `aiki fix` (sync/TTY mode)

```
1. read_events_with_ids() → jj log          (slow, subprocess)
2. materialize_graph_with_ids()              (fast, in-memory)
3. find_task() (review task lookup)          (fast, lookup)
4. ReviewScope::from_data()                  (fast, parsing)
5. resolve_fix_template_name()               (fast, lookup)
6. determine_followup_assignee()             (fast, logic)

7. ScreenSession::new()                      ← BLANK SCREEN STARTS HERE

8. run_quality_loop() → first iteration:
   a. read_events_with_ids() → jj log       (slow, subprocess)
   b. materialize_graph_with_ids()           (fast, in-memory)
   c. find_task()                            (fast, lookup)
   d. has_actionable_issues()                (fast, data check)
   
   e. create_plan_fix_task():
      - read_events_with_ids() → jj log     (slow, subprocess)
      - generate_task_id()                   (fast)
      - get_working_copy_change_id() → jj log (slow, subprocess)
      - load template & render               (file I/O + parsing)
      - write_event(AddTask) → jj new       (slow, subprocess)
      - write_link_event() ×2 → jj new      (slow, 2 subprocesses)
   
   f. run_task_with_session() → task_run_on_session():
      - prepare_task_run():
        * read_events() → jj log            (slow, subprocess)
        * write_event(Started) → jj new    (slow, subprocess)
      - runtime.spawn_monitored()            (process start)
      - monitor_on_screen() first iteration:
        * read_events() → jj log            (slow, subprocess)
        * build_view()                       (fast)
   
   g. FIRST FRAME DRAWN                      ← BLANK SCREEN ENDS
```

**Between steps 7 and 8g, there are roughly 8-10 jj subprocess invocations** happening invisibly on a blank alternate screen. Each call takes 100-500ms+, so the blank period can easily be **3-8 seconds**.

### Pre-screen work (steps 1-6) also contributes

Steps 1-6 happen *before* the alternate screen but still contribute to perceived startup latency (1-2 additional jj calls).

### The quality loop problem

If the fix cycle triggers multiple iterations (fix → review → fix again), each iteration repeats step 8, though the screen is already active so subsequent iterations show progress.

---

## Summary

| Command | Blank Screen Severity | Root Cause |
|---------|----------------------|------------|
| `aiki build` | **High** | `ScreenSession::new()` at line 369 before epic creation & task prep (~10-12 jj calls) |
| `aiki fix` | **Very High** | `ScreenSession::new()` at line 223 before quality loop setup (~8-10 jj calls per iteration) |
| `aiki review` | **Low** | `LiveScreen` entered after agent spawn, but 4-6 jj calls happen silently before screen |

**Common pattern:** All three commands perform expensive jj operations either:
- On a blank alternate screen (build, fix)
- During silent pre-screen setup (review)

**Impact:** Users see 2-10 seconds of blank/frozen terminal with no indication that work is happening.

---

## Proposed Solution: LoadingScreen Abstraction

The core problem is that expensive setup work happens either:
1. **Before** any visual feedback (frozen shell prompt)
2. **After** entering alternate screen but before rendering (blank screen)

### Solution: Early LoadingScreen

Introduce a **LoadingScreen** that enters the alternate screen immediately and shows a single status line that updates as each setup step completes. The screen shows only the *current* step — no progressive checklist, just one `⧗ <label>...` line that changes text as work progresses.

```
┌─────────────────────────────────────────────────────────┐
│                    Current Flow                          │
└─────────────────────────────────────────────────────────┘

Shell Prompt (visible)
    ↓
Expensive Setup Work          ← USER SEES: frozen prompt
    - read_events() → jj log
    - materialize_graph()
    - create tasks/epics
    - write_event() → jj new
    ↓
ScreenSession::new() → LiveScreen::new()
    ↓
Blank Alternate Screen        ← USER SEES: blank screen
    ↓
More Expensive Work
    - read_events() → jj log
    - template loading
    - write_event() → jj new
    - spawn_monitored()
    ↓
First monitor.poll()
    ↓
First Frame Rendered          ← USER SEES: content (finally!)


┌─────────────────────────────────────────────────────────┐
│                    Proposed Flow                         │
└─────────────────────────────────────────────────────────┘

Shell Prompt (visible)
    ↓
LoadingScreen::new()           ← Immediate alt-screen + first frame!
    ↓
 ⧗ Loading task graph...      ← Single status line, updates in-place
    ↓
Expensive Setup Work (with step updates)
    - read_events() → jj log          → set_step("Finding or creating epic...")
    - find_or_create_epic()            → set_context(filepath, task_id, desc)
    - write_event() → jj new          → set_step("Spawning agent session...")
    ↓
loading.into_live_screen()     ← Hands off alternate screen ownership
    ↓
ScreenSession wraps LiveScreen / StatusMonitor uses LiveScreen
    ↓
First Frame with Live Status   ← Smooth transition, no flicker!
```

### Key Abstraction: LoadingScreen

```rust
/// A lightweight loading screen shown during expensive command setup.
///
/// Enters the alternate screen immediately on construction and renders
/// a single status line (`⧗ <step>...`). As setup progresses, callers
/// update the step label and optionally add context lines (filepath,
/// task ID + description) that accumulate above the step line.
///
/// Rendering uses ratatui's `terminal.draw()` for consistency with the
/// rest of the TUI. Each `set_step()` / `set_context()` call triggers
/// an immediate synchronous redraw — no event loop needed.
pub struct LoadingScreen {
    /// Ratatui terminal (owns the alternate screen via CrosstermBackend<Stderr>)
    terminal: Terminal<CrosstermBackend<Stderr>>,

    /// Optional filepath shown at the top
    filepath: Option<String>,

    /// Optional task context: (change_id, description)
    task_context: Option<(String, String)>,

    /// Current step label (shown as "⧗ <step>...")
    step: String,
}

impl LoadingScreen {
    /// Enter alternate screen, enable raw mode, render initial frame.
    /// Returns a no-op stub if stderr is not a terminal.
    pub fn new(initial_step: &str) -> Result<Self>;

    /// Update the current step label and redraw.
    pub fn set_step(&mut self, label: &str);

    /// Set the filepath context line (rendered above the step).
    pub fn set_filepath(&mut self, path: &str);

    /// Set the task context line: "[change_id] description".
    pub fn set_task_context(&mut self, change_id: &str, description: &str);

    /// Show an error frame ("✗ <step>\n   <message>"), then leave
    /// the alternate screen and reprint the error to stderr so it
    /// persists in terminal scrollback.
    pub fn fail(self, message: &str);

    /// Consume self and return the underlying LiveScreen, preserving
    /// the alternate screen session. The caller wraps it in either
    /// a ScreenSession (for build/fix) or passes it to StatusMonitor
    /// (for review/task-run).
    pub fn into_live_screen(self) -> LiveScreen;
}
```

**Why `into_live_screen()` instead of `transition_to_main()`:**

The existing codebase has two consumers of the alternate screen:

1. **`ScreenSession`** (build.rs, fix.rs) — wraps a `LiveScreen` + SIGINT handler. Used when multiple tasks share one screen session.
2. **`StatusMonitor::monitor_until_complete_with_child()`** (status_monitor.rs:335) — currently creates its own `LiveScreen::new()`. Needs to accept an existing `LiveScreen` instead.

`into_live_screen()` returns a bare `LiveScreen` that either consumer can adopt:

```rust
// Build/fix path: wrap in ScreenSession
let screen = loading.into_live_screen();
let session = ScreenSession::from_live_screen(screen)?;  // new constructor

// Review/task-run path: pass to StatusMonitor
let screen = loading.into_live_screen();
monitor.monitor_until_complete_with_child_on_screen(cwd, child, screen)?;  // new method
```

This requires two small additions:
- `ScreenSession::from_live_screen(screen: LiveScreen) -> Result<Self>` — wraps existing screen + installs SIGINT handler (no new `LiveScreen::new()` call).
- `StatusMonitor::monitor_until_complete_with_child_on_screen()` — like the existing method but accepts a `LiveScreen` parameter instead of creating one internally.

### Integration Points

#### 1. `aiki build` (`run_build_plan()` in build.rs)

```rust
pub fn run_build_plan(...) -> Result<()> {
    // IMMEDIATE FEEDBACK
    let mut loading = LoadingScreen::new("Loading task graph...")?;
    loading.set_filepath(&plan_path.display().to_string());

    // Steps 2-5: pre-screen work (now visible)
    let events = read_events(cwd)?;                          // jj log
    let graph = materialize_graph(&events)?;
    let existing_epic = find_epic_for_plan(&graph, plan_id);

    loading.set_step("Finding or creating epic...");
    let epic = find_or_create_epic(cwd, ...)?;               // jj log + jj new ×N
    loading.set_task_context(&epic.change_id, &epic.description);

    loading.set_step("Preparing task...");
    let prepared = prepare_task_run(cwd, &task_id, &options)?; // jj log + jj new

    loading.set_step("Spawning agent session...");

    // Transition: hand off to ScreenSession
    let screen = loading.into_live_screen();
    let mut session = ScreenSession::from_live_screen(screen)?;

    // Main screen renders first frame immediately
    run_loop(cwd, ..., &mut session)?;
    drop(session);
    // ... final output ...
}
```

#### 2. `aiki fix` (`run_fix()` in fix.rs)

```rust
pub fn run_fix(...) -> Result<()> {
    let mut loading = LoadingScreen::new("Loading task graph...")?;

    let events = read_events_with_ids(cwd)?;                 // jj log
    let graph = materialize_graph_with_ids(&events)?;
    let review_task = find_task(&graph, task_id)?;

    loading.set_step("Checking for actionable issues...");
    let scope = ReviewScope::from_data(&review_task)?;
    let template = resolve_fix_template_name(&review_task)?;
    let assignee = determine_followup_assignee(...)?;

    // Show context once we know what we're fixing
    loading.set_filepath(&scope.filepath);

    loading.set_step("Creating fix plan...");

    // Transition: hand off to ScreenSession (shared across quality loop)
    let screen = loading.into_live_screen();
    let mut session = ScreenSession::from_live_screen(screen)?;

    run_quality_loop(cwd, ..., &mut session)?;
    drop(session);
}
```

#### 3. `aiki review` / `aiki task run` (via `task_run()` in runner.rs)

```rust
pub fn task_run(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    let show_status = std::io::stderr().is_terminal() && !options.quiet;

    if show_status {
        let mut loading = LoadingScreen::new("Preparing task...")?;

        let prepared = prepare_task_run(cwd, task_id, &options)?; // jj log + jj new
        loading.set_task_context(&prepared.change_id, &prepared.description);

        loading.set_step("Spawning agent session...");
        let mut monitored = prepared.runtime.spawn_monitored(&prepared.spawn_options)?;

        // Transition: hand off to StatusMonitor
        let screen = loading.into_live_screen();
        let mut monitor = StatusMonitor::new(task_id);
        let result = monitor.monitor_until_complete_with_child_on_screen(
            cwd, &mut monitored, screen
        )?;

        handle_session_result(cwd, task_id, result, options.quiet)?;
    } else {
        // Non-TTY path (no LoadingScreen)
        let prepared = prepare_task_run(cwd, task_id, &options)?;
        let result = prepared.runtime.spawn_blocking(&prepared.spawn_options)?;
        handle_session_result(cwd, task_id, result, options.quiet)?;
    }
}
```

### Benefits

1. **Immediate feedback**: User sees something within milliseconds
2. **Progress visibility**: User knows what's happening during slow operations
3. **Smooth transition**: Loading screen hands off to main screen seamlessly
4. **Consistent UX**: All commands use the same loading pattern
5. **No blank screens**: Alternate screen is never empty

### Implementation Notes

- **Rendering**: Uses `terminal.draw()` (ratatui) for each step update — synchronous, immediate, consistent with the rest of the TUI. No event loop needed; each `set_step()` call redraws the full frame.
- **Transition**: `into_live_screen()` extracts the `Terminal<CrosstermBackend<Stderr>>` and wraps it in a `LiveScreen`. No alternate screen exit/re-enter — single continuous session.
- **Non-TTY**: When stderr is not a terminal, `LoadingScreen::new()` returns a no-op variant. All methods are stubs.
- **Ctrl+C during loading**: The loading phase is short (1-5s) and no agent is running yet. Default SIGINT behavior (process exit) is acceptable. No custom handler needed until `ScreenSession` / `StatusMonitor` takes over.
- **Terminal resize during loading**: Not handled. The loading screen is simple enough (a few lines of text) that stale layout from a resize is harmless for a 1-5s window.
- **Error handling**: `LoadingScreen::fail()` renders the error frame (`✗ <step>` + indented message), pauses briefly (~500ms) so the user sees it, then leaves the alternate screen and reprints the error to stderr. This ensures the error persists in terminal scrollback (alternate screen content vanishes on exit).

---

## Loading Screen Mockups

Each screen shows only the current loading step with `⧗`. No progressive checkmarks — screens transition directly to the next step.

### `aiki build` — Frame Sequence

**Frame 1** (~0ms, immediate entry into alternate screen)

```
[80 cols]
 ops/now/design.md 

 ⧗ Loading task graph...
```

**Frame 2** (~200ms, graph loaded, finding epic)

```
[80 cols]
 ops/now/design.md 

 ⧗ Finding or creating epic...
```

**Frame 3** (~1.5s, epic found/created, shows epic details)

```
[80 cols]
 ops/now/design.md                                     
                                                         
 [luppzupt] Implement webhook event handling     
 
 ⧗ Spawning agent session...
```

**Frame 4** (~3s, transition to main screen)

Buffer clears and the `ScreenSession` renders its first frame immediately on the same alternate screen. No flicker.

```
[80 cols]
 ops/now/webhooks.md                                     
                                                         
 [luppzupt] Implement webhook event handling                                        
                                                         
 ▸ build                                                 
    ⧗ decompose                                          
 ○ review                                                
 ○ fix
```

This is the standard `epic_show` view: PathLine + EpicTree + StageTrack. The first subtask shows `⧗` (starting) because the agent is spawning.

---

### `aiki fix` — Frame Sequence

**Frame 1** (~0ms)

```
[80 cols]
 ⧗ Loading task graph...
```

**Frame 2** (~200ms)

```
[80 cols]
 ⧗ Finding review task...
```

**Frame 3** (~400ms)

```
[80 cols]
 ⧗ Checking for actionable issues...
```

**Frame 4** (~800ms, shows filepath and epic after creation)

```
[80 cols]
 ops/now/webhooks.md

 [xqrmnpst] Fix: Code review issues

 ⧗ Creating plan...
```

**Frame 5** (~2.5s, transition to main screen)

Buffer clears; `ScreenSession` renders first frame. The fix task's first subtask shows `⧗` (starting).

```
[80 cols]
 ops/now/webhooks.md

 [xqrmnpst] Fix: Code review issues
 ⎿ ⧗ Fix null pointer in auth handler

 ⧗ build  0/1              │  ○ review  │  ○ fix
```

Subsequent quality loop iterations (fix → re-review → fix again) render directly on this session — no return to loading screen.

---


### `aiki review` — Frame Sequence

`aiki review` must first **create** the review task, then run it.

**Frame 1** (~0ms)

```
[80 cols]
 ⧗ Loading task graph...
```

**Frame 2** (~200ms)

```
[80 cols]
 ⧗ Creating review task...
```

**Frame 3** (~800ms, after task created and scope determined)

```
[80 cols]
 ops/now/webhooks.md

 [oorznprs] Review: webhooks implementation

 ⧗ Spawning agent session...
```

**Frame 4** (~1.5s, transition to main screen)

### `aiki task run` — Frame Sequence

`aiki task run` is given an **existing** task and runs it.

**Frame 1** (~0ms)

```
[80 cols]
 ⧗ Loading task...
```

**Frame 2** (~200ms, after task loaded)

```
[80 cols]
 [xqrmnpst] Fix null pointer in auth handler

 ⧗ Spawning agent session...
```

**Frame 3** (~1s, transition to main screen)

```
[80 cols]
 [xqrmnpst] Fix null pointer in auth handler              claude-code

 ⧗ build  0/0                      │  ○ review  │  ○ fix
```

---

If a step fails, the screen shows failure and exits to the shell.

```
[80 cols]
 ops/now/design.md

 ✗ Finding or creating epic
   no plan file found at ops/now/webhooks.md
```

Error text indents 3 chars (aligned under step label, past symbol + space). After rendering this frame, the screen pauses briefly (~500ms), then exits the alternate screen and reprints the error to stderr so it persists in terminal scrollback.

---

### Layout Rules

```
[80 cols]
 <filepath>
 
 [<task-id>] <task description>     ← shown after epic/task is found/created
 
 <sym> <step label>
```

- 1-char left padding on all lines (matching existing TUI margin)
- Filepath: `text_style()` — primary text color
- Task ID (in brackets): `dim_style()` — dim brackets, `hi_style()` for description
- Step labels: `text_style()` — primary text color
- Symbol colors: `⧗` yellow, `✗` red
- No borders, no boxes, no right-aligned metadata
- `[80 cols]` default width (no width-dependent layout)

### Transition Behavior

Loading screen owns the alternate screen buffer (via a ratatui `Terminal<CrosstermBackend<Stderr>>`). On transition:

1. Final loading frame renders (showing filepath/task context + "Spawning agent session...")
2. Caller invokes `loading.into_live_screen()` — extracts the `Terminal` and wraps it in a `LiveScreen`
3. Caller passes the `LiveScreen` to either:
   - `ScreenSession::from_live_screen()` (build, fix) — wraps it + installs SIGINT handler
   - `StatusMonitor::monitor_until_complete_with_child_on_screen()` (review, task-run) — runs the event loop on it
4. Main screen clears the buffer and renders first frame immediately

No flicker — single alternate screen session throughout. Raw mode and hidden cursor persist across the transition.

### Non-TTY Behavior

When stderr is not a terminal (piped output, CI, `--quiet`): `LoadingScreen::new()` returns a no-op variant. All methods are stubs, no alternate screen entered. `into_live_screen()` still returns a valid `LiveScreen` (creating one fresh) so callers don't need conditional logic.

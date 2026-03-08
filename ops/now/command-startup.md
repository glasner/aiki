# Command Startup: Blank Screen Problem

## Problem

When running `aiki build` in sync/TTY mode, there is a long pause with a completely blank screen before anything appears. The user sees nothing — no spinner, no status, no indication that work is happening.

## Root Cause

The sync path in `build.rs` enters the alternate screen (`ScreenSession::new()`) at line 368 **before** doing any of the expensive work that follows. The alternate screen is blank, and the first frame isn't drawn until the status monitor's event loop starts polling much later.

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
   c. LiveScreen::enter()                    ← BLANK SCREEN STARTS HERE
   d. First monitor_until_complete iteration:
      - read_events() → jj log              (slow, subprocess)
      - build_view()                         (fast)
   e. FIRST FRAME DRAWN                      ← BLANK SCREEN ENDS
```

**Key difference from build:** The blank screen period is shorter because `run_with_status_monitor` calls `LiveScreen::enter()` (via `monitor_until_complete_with_child`) after spawning the agent. However, **steps 1-2 happen with no terminal feedback** — the user sees a frozen shell prompt during 4-6 jj subprocess calls.

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
| `aiki build` | **High** | `ScreenSession::new()` at line 368 before epic creation & task prep (~10-12 jj calls) |
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

Introduce a **LoadingScreen** that appears immediately and shows progress during expensive setup.

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
ScreenSession::new() / LiveScreen::enter()
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
LoadingScreen::enter()         ← Immediate visual feedback!
    ↓
┌──────────────────────────────────────────────┐
│  🔄 Preparing...                              │
│  ▸ Loading task graph                        │ ← Shows progress
│                                              │
└──────────────────────────────────────────────┘
    ↓
Expensive Setup Work (with progress updates)
    - read_events() → jj log          → Update: "✓ Loaded task graph"
    - materialize_graph()              → Update: "▸ Finding tasks"
    - create tasks/epics               → Update: "▸ Creating epic"
    - write_event() → jj new          → Update: "✓ Created epic"
    ↓
LoadingScreen::transition_to(MainScreen)
    ↓
Main Screen (ScreenSession/LiveScreen)
    ↓
Continue Work (still visible to user)
    - read_events() → jj log          → Shows in main screen
    - spawn_monitored()                → Shows "Spawning agent..."
    ↓
First Frame with Live Status          ← Smooth transition!
```

### Key Abstraction: LoadingScreen

```rust
/// A lightweight loading screen shown during expensive command setup
pub struct LoadingScreen {
    /// Alternate screen handle (entered immediately)
    screen: AlternateScreen,
    
    /// Current loading message
    status: String,
    
    /// Progress items (with completion state)
    items: Vec<LoadingItem>,
}

struct LoadingItem {
    label: String,
    state: LoadingState,
}

enum LoadingState {
    Pending,        // ▸ Item
    InProgress,     // 🔄 Item
    Complete,       // ✓ Item
}

impl LoadingScreen {
    /// Enter alternate screen and show initial loading state
    pub fn enter(title: &str) -> Result<Self>;
    
    /// Add a progress item
    pub fn add_item(&mut self, label: &str) -> ItemHandle;
    
    /// Update an item's state
    pub fn update_item(&mut self, handle: ItemHandle, state: LoadingState);
    
    /// Update the main status message
    pub fn set_status(&mut self, status: &str);
    
    /// Transition to main screen (hands off alternate screen ownership)
    pub fn transition_to_main(self) -> MainScreen;
    
    /// Exit and return to shell (if not transitioning)
    pub fn exit(self);
}
```

### Integration Points

#### 1. `aiki build`

```rust
// build.rs line ~150
pub fn run_build_sync(...) -> Result<()> {
    // IMMEDIATE FEEDBACK: Enter LoadingScreen
    let mut loading = LoadingScreen::enter("Building Plan")?;
    
    // Show what we're doing
    let graph_item = loading.add_item("Loading task graph");
    let events = read_events(cwd)?;
    loading.update_item(graph_item, LoadingState::Complete);
    
    let epic_item = loading.add_item("Finding or creating epic");
    let epic_id = find_or_create_epic(cwd, ...)?;
    loading.update_item(epic_item, LoadingState::Complete);
    
    let prep_item = loading.add_item("Preparing task");
    let prepared = prepare_task_run(cwd, &plan_id, &options)?;
    loading.update_item(prep_item, LoadingState::Complete);
    
    loading.set_status("Starting agent...");
    let spawn_item = loading.add_item("Spawning agent session");
    
    // Transition to main monitoring screen
    let mut main_screen = loading.transition_to_main();
    loading.update_item(spawn_item, LoadingState::Complete);
    
    // Continue with monitoring (already visible)
    monitor_until_complete(&mut main_screen, ...)?;
}
```

#### 2. `aiki fix`

```rust
// fix.rs line ~150
pub fn run_fix(...) -> Result<()> {
    let mut loading = LoadingScreen::enter("Fix Quality Loop")?;
    
    let graph_item = loading.add_item("Loading task graph");
    let events = read_events_with_ids(cwd)?;
    loading.update_item(graph_item, LoadingState::Complete);
    
    let review_item = loading.add_item("Finding review task");
    let review_task = find_task(&tasks, task_id)?;
    loading.update_item(review_item, LoadingState::Complete);
    
    // Transition to ScreenSession (shared across quality loop iterations)
    let mut session = loading.transition_to_session()?;
    
    // Quality loop runs on the visible session
    run_quality_loop(cwd, ..., &mut session)?;
}
```

#### 3. `aiki review` (via `task_run`)

```rust
// runner.rs line ~383
pub fn task_run(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    let show_status = std::io::stderr().is_terminal() && !options.quiet;
    
    if show_status {
        let mut loading = LoadingScreen::enter("Starting Review")?;
        
        let prep_item = loading.add_item("Preparing task");
        let prepared = prepare_task_run(cwd, task_id, &options)?;
        loading.update_item(prep_item, LoadingState::Complete);
        
        loading.set_status("Spawning agent...");
        let spawn_item = loading.add_item("Starting agent session");
        let mut monitored = prepared.runtime.spawn_monitored(&prepared.spawn_options)?;
        loading.update_item(spawn_item, LoadingState::Complete);
        
        // Transition to live monitoring
        let mut main_screen = loading.transition_to_main();
        
        let result = monitor_until_complete(&mut main_screen, &mut monitored)?;
        handle_session_result(cwd, task_id, result, options.quiet)?;
    } else {
        // Non-TTY path (no LoadingScreen)
        ...
    }
}
```

#### 4. `aiki task run` (same as review)

`aiki task run` delegates to `task_run()`, so it automatically gets the LoadingScreen behavior when in TTY mode.

### Benefits

1. **Immediate feedback**: User sees something within milliseconds
2. **Progress visibility**: User knows what's happening during slow operations
3. **Smooth transition**: Loading scene hands off to main screen seamlessly
4. **Consistent UX**: All commands use the same loading pattern
5. **No blank screens**: Alternate screen is never empty

### Implementation Notes

- **LoadingScreen** should be lightweight (simple text rendering, no fancy TUI)
- Progress updates should be cheap (single line redraws, not full screen rebuilds)
- Transition should reuse the alternate screen handle (no flicker)
- For commands without screens (non-TTY), LoadingScreen is a no-op

---

## Loading Screen Mockups

Steps appear progressively — each step is added when its work begins (not pre-rendered as a checklist). Only the current step shows `⧗`; completed steps show `✓`.

### `aiki build` — Frame Sequence

**Frame 1** (~0ms, immediate entry into alternate screen)

```
[80 cols]
 Building plan...                                ← hi+bold
                                                 ← blank row
 ⧗ Loading task graph                            ← yellow ⧗, text label
```

**Frame 2** (~200ms, graph loaded)

```
[80 cols]
 Building plan...                                ← hi+bold
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ⧗ Finding or creating epic                      ← yellow ⧗, text label
```

**Frame 3** (~1.5s, epic created)

```
[80 cols]
 Building plan...                                ← hi+bold
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✓ Finding or creating epic                      ← green ✓, text label
 ⧗ Preparing task                                ← yellow ⧗, text label
```

**Frame 4** (~2.5s, task prepared)

```
[80 cols]
 Starting agent...                               ← hi+bold (status changes)
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✓ Finding or creating epic                      ← green ✓, text label
 ✓ Preparing task                                ← green ✓, text label
 ⧗ Spawning agent session                        ← yellow ⧗, text label
```

**Frame 5** (~3s) — transitions to `ScreenSession`. Loading screen hands off the alternate screen buffer; main screen renders its first frame immediately. No blank gap.

---

### `aiki fix` — Frame Sequence

**Frame 1** (~0ms)

```
[80 cols]
 Preparing fix...                                ← hi+bold
                                                 ← blank row
 ⧗ Loading task graph                            ← yellow ⧗, text label
```

**Frame 2** (~200ms)

```
[80 cols]
 Preparing fix...                                ← hi+bold
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ⧗ Finding review task                           ← yellow ⧗, text label
```

**Frame 3** (~400ms)

```
[80 cols]
 Preparing fix...                                ← hi+bold
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✓ Finding review task                           ← green ✓, text label
 ⧗ Checking for actionable issues                ← yellow ⧗, text label
```

**Frame 4** (~600ms)

```
[80 cols]
 Creating fix task...                            ← hi+bold (status changes)
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✓ Finding review task                           ← green ✓, text label
 ✓ Checking for actionable issues                ← green ✓, text label
 ⧗ Creating fix task from template               ← yellow ⧗, text label
```

**Frame 5** (~2s)

```
[80 cols]
 Starting agent...                               ← hi+bold (status changes)
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✓ Finding review task                           ← green ✓, text label
 ✓ Checking for actionable issues                ← green ✓, text label
 ✓ Creating fix task from template               ← green ✓, text label
 ⧗ Spawning agent session                        ← yellow ⧗, text label
```

Transitions to `ScreenSession` for the quality loop. Subsequent loop iterations render on the already-visible session.

---

### `aiki review` / `aiki task run` — Frame Sequence

**Frame 1** (~0ms)

```
[80 cols]
 Preparing task...                               ← hi+bold
                                                 ← blank row
 ⧗ Preparing task run                            ← yellow ⧗, text label
```

**Frame 2** (~500ms)

```
[80 cols]
 Starting agent...                               ← hi+bold (status changes)
                                                 ← blank row
 ✓ Preparing task run                            ← green ✓, text label
 ⧗ Spawning agent session                        ← yellow ⧗, text label
```

**Frame 3** (~800ms) — transitions to `LiveScreen`. Shortest sequence since review does the least pre-work.

---

### Error State

If a step fails, the screen shows failure and exits to the shell.

```
[80 cols]
 Build failed                                    ← hi+bold
                                                 ← blank row
 ✓ Loading task graph                            ← green ✓, text label
 ✗ Finding or creating epic                      ← red ✗, text label
   no plan file found at ops/now/webhooks.md     ← red, 3-char indent
```

Error text indents 3 chars (aligned under step label, past symbol + space). Screen exits alternate screen after rendering this frame.

---

### Layout Rules

```
[80 cols]
 <status message>                                ← hi+bold
                                                 ← blank row
 <sym> <step label>                              ← symbol + space + label
 <sym> <step label>
 <sym> <step label>
```

- 1-char left padding on all lines (matching existing TUI margin)
- Status message: `hi_style()` — bold, high-contrast
- Step labels: `text_style()` — primary text color
- Symbol colors: `⧗` yellow, `✓` green, `✗` red
- No borders, no boxes, no right-aligned metadata
- `[80 cols]` default width (no width-dependent layout)

### Transition Behavior

Loading screen owns the alternate screen buffer. On transition:

1. Final frame renders (all steps `✓`)
2. Buffer clears
3. Ownership passes to `ScreenSession` / `LiveScreen`
4. Main screen renders first frame immediately

No flicker — single alternate screen session, no exit/re-enter.

### Non-TTY Behavior

When stderr is not a terminal (piped output, CI, `--quiet`): `LoadingScreen` is a no-op — all methods are stubs, no alternate screen entered.


# Epic Show: Styled Terminal Output

## Context

The `aiki build show <plan>` command currently outputs a markdown table (subtask ID, status, name). This is functional for agents but poor for humans — no color, no visual hierarchy, no status indicators. The v3 mockup (ops/research/aiki-tui/mockups/v3.html) envisions a full interactive TUI with tree DAG, event log, and key hints. That's too ambitious for the first screen.

This plan implements a **non-interactive styled print** for epic output — the same approach as `git status` or `cargo build`. Read the data, render one frame of styled text to stdout, exit. No event loop, no key handling, no scrolling. This gives us a polished human-readable view while deferring interactivity to a later pass.

## Scope: What we're building

Two views, both non-interactive:

1. **`aiki build show <plan>`** — View of an epic (completed or in-progress). Replaces the current markdown table output.
2. **`aiki build` (during execution)** — Progress line printed after each subtask completes. Replaces the current silent execution.

The tree DAG from v3 is **out of scope** for this pass. The subtask list is flat (numbered), matching v3 Scene 3e but without the tree visualization above it.

## Mockups

Using the convention from ux-foundation.md: left side is literal terminal output, right side after `←` is style annotation.

### Layout structure

Every epic show follows this order:

1. **Path line** — plan file path (dim dir, fg filename)
2. **Epic tree** — `[short_id] epic name` with subtasks as `⎿` children
3. **Stage track** — build → review → fix pipeline status (bottom)

The stage track at the bottom acts as a status bar / summary — pipeline progress and stats in one line. The tree is the hero.

### Path line

```
 ops/now/webhooks.md                    ← dim "ops/now/", fg "webhooks.md"
```

### Stage track (at bottom)

```
 ✓ build  6/6  2m28s  │  ▸ review  │  ○ fix       ← green "✓ build", yellow "▸ review", dim "○ fix"
 ▸ build  3/6  0:34   │  ○ review  │  ○ fix       ← yellow "▸ build", dim others
```

### Epic show — build completed successfully

```
[80×14]
 ops/now/webhooks.md                                       ← dim "ops/now/", fg "webhooks.md"

 [luppzupt] Implement Stripe webhook event handling        ← dim "[luppzupt]", hi+bold name
  ⎿ [xtuttnyw] ✓ Explore webhook requirements        cc    8s   ← dim "⎿", dim id, green ✓, fg name, cyan "cc", dim time
  ⎿ [nmpskvxr] ✓ Create implementation plan           cc    6s
  ⎿ [kyxrwqvp] ✓ Implement webhook route handler      cur  48s   ← magenta "cur"
  ⎿ [qwvptrvp] ✓ Verify Stripe webhook signatures     cc   22s
  ⎿ [zxrkqstt] ✓ Add idempotency key tracking         cur  34s
  ⎿ [nwqprmxv] ✓ Write integration tests              cc   18s

 ✓ build  6/6  2m28s  │  ○ review  │  ○ fix               ← stage track at bottom
```

### Epic show — build with failure

```
[80×18]
 ops/now/realtime.md                                       ← dim "ops/now/", fg "realtime.md"

 [xtuttnyv] Add real-time notifications to dashboard       ← dim id, hi+bold name
  ⎿ [nmpskvxr] ✓ Explore notification requirements   cc   22s
  ⎿ [kyxrwqvp] ✓ Create implementation plan          cc    8s
  ⎿ [qwvptrvp] ✓ Set up WebSocket server             cur  38s
  ⎿ [trvpzxrk] ✓ Implement event broadcast           cur 1m02
  ⎿ [nwqprmxv] ✓ Run schema migrations               cc   22s
  ⎿ [rmxvkyxr] ✓ Build preference settings UI        cc   44s
  ⎿ [wqvpnmps] ✓ Write integration tests             cur  28s
  ⎿ [zxrkqstt] ✗ Add retry logic for deliveries      cc   36s   ← red ✗
                  Redis connection refused                       ← red, indented error reason
  ⎿ [trvpwqvp] ○ Wire up notification toggle                    ← red ○, "stuck" — blocked by zxrk
  ⎿ [kyxrnwqp] ○ Add notification sound prefs                   ← red ○, "stuck"

 ✗ build  8/10  2m48s  1 failed  │  ○ review  │  ○ fix    ← stage track at bottom
```

### Epic show — build in progress

```
[80×12]
 ops/now/webhooks.md                                       ← dim "ops/now/", fg "webhooks.md"

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
  ⎿ [xtuttnyw] ✓ Explore webhook requirements        cc    8s   ← green ✓
  ⎿ [nmpskvxr] ✓ Create implementation plan           cc    6s
  ⎿ [kyxrwqvp] ▸ Implement webhook route handler      cur        ← yellow ▸, no time yet
  ⎿ [qwvptrvp] ▸ Verify Stripe signatures             cc
  ⎿ [zxrkqstt] ○ Add idempotency key tracking                   ← dim ○, no agent/time
  ⎿ [nwqprmxv] ○ Write integration tests

 ▸ build  3/6  0:34  │  ○ review  │  ○ fix                ← stage track at bottom
```

### Epic show — mid-fix (all phases visible)

```
[80×10]
 ops/now/webhooks.md                                       ← dim "ops/now/", fg "webhooks.md"

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
  ⎿ [trvpzxrk] ▸ Add rate limit to broadcast          cur        ← yellow ▸
  ⎿ [nwqprmxv] ○ Fix missing null check in handler                ← dim ○
  ⎿ [rmxvkyxr] ○ Update error message format

 ✓ build  6/6  │  ✓ review  2 issues  │  ▸ fix 1/3        ← stage track at bottom
```

### Live progress (during execution)

There's already a `StatusMonitor` (`cli/src/tasks/status_monitor.rs`) that polls task state every 500ms and redraws a tree in-place using `SavePosition`/`RestorePosition`. It renders plain-text `├─` / `└─` trees with basic symbols. This plan replaces its rendering with themed widgets — same poll loop, same cursor management, just better output.

The live view shows the same layout as the static view, redrawn in place:

```
 ops/now/webhooks.md                                       ← dim "ops/now/", fg "webhooks.md"

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
  ⎿ [xtuttnyw] ✓ Explore webhook requirements        cc    8s   ← green ✓, completed
  ⎿ [nmpskvxr] ✓ Create implementation plan           cc    6s
  ⎿ [kyxrwqvp] ▸ Implement webhook route handler      cur        ← yellow ▸, in flight
  ⎿ [qwvptrvp] ▸ Verify Stripe signatures             cc
  ⎿ [zxrkqstt] ○ Add idempotency key tracking                   ← dim ○, pending
  ⎿ [nwqprmxv] ○ Write integration tests

 ▸ build  3/6  0:34  │  ○ review  │  ○ fix                ← stage track, elapsed updates live

 [Ctrl+C to detach]                                        ← dim hint
```

The StatusMonitor already handles: polling events, detecting new state, `SavePosition`/`Clear(FromCursorDown)`/`RestorePosition` for in-place redraws, Ctrl+C detach, agent exit detection. We only replace `render_task_tree()` and `format_task_line()` with a call to our widgets → `buffer_to_ansi()`.

## Architecture

### Ratatui widgets → Buffer → output

Build everything as ratatui `Widget`s from the start. For now, render them to a `Buffer` and convert to ANSI for printing. When interactivity comes, the same widgets render inside `Terminal::draw()` — zero widget code rewritten.

**Rendering pipeline:**

```
Widgets (PathLine, EpicTree, StageTrack)
    ↓ render into
Buffer (ratatui::buffer::Buffer)
    ↓ convert via
buffer_to_ansi() → String        ← non-interactive (now)
buffer_to_png()  → PNG           ← snapshot tests (already exists)
Terminal::draw() → screen        ← interactive (later, same widgets)
```

Pipe handling uses the existing `output_utils` convention: styled output goes to stderr (only when stderr is a TTY via `emit()`), machine-readable IDs go to stdout (only when stdout is piped).

### New files

| File | Lines | Purpose |
|------|-------|---------|
| `cli/src/tui/buffer_ansi.rs` | ~60 | `buffer_to_ansi(buf: &Buffer) → String` — converts a ratatui Buffer to ANSI-escaped string |
| `cli/src/tui/views/mod.rs` | ~3 | Module root for view renderers |
| `cli/src/tui/views/epic_show.rs` | ~250 | Composes widgets and renders epic show into a Buffer |
| `cli/src/tui/widgets/path_line.rs` | ~30 | Widget: dimmed directory + fg filename |
| `cli/src/tui/widgets/epic_tree.rs` | ~120 | Widget: `[id] name` + `⎿ [id] subtask` children |
| `cli/src/tui/widgets/stage_track.rs` | ~60 | Widget: `✓ build │ ▸ review │ ○ fix` pipeline |

### Modified files

| File | Change |
|------|--------|
| `cli/src/tui/mod.rs` | Add `pub mod buffer_ansi;` and `pub mod views;` |
| `cli/src/tui/widgets/mod.rs` | Add new widget modules |
| `cli/src/commands/build.rs` | Replace `output_build_show()`, `output_build_started()`, `output_build_completed()` with TUI renderer calls |
| `cli/src/tasks/status_monitor.rs` | Replace `render_task_tree()` / `format_task_line()` with widget-based rendering via `buffer_to_ansi()` |

### `buffer_ansi.rs` — Buffer to ANSI renderer (~60 lines)

Walks every cell in the buffer, emitting ANSI escapes for fg color and bold. Resets between cells when style changes. Emits newlines at row boundaries. This parallels the existing `buffer_to_png()` in `render_png.rs` — same Buffer, different output format.

```rust
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

pub fn buffer_to_ansi(buf: &Buffer) -> String {
    let area = buf.area();
    let mut out = String::new();
    for row in area.y..area.y + area.height {
        let mut last_style = None;
        for col in area.x..area.x + area.width {
            let cell = &buf[(col, row)];
            let style = (cell.fg, cell.modifier);
            if Some(style) != last_style {
                out.push_str("\x1b[0m");
                if let Color::Rgb(r, g, b) = cell.fg {
                    out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
                }
                if cell.modifier.contains(Modifier::BOLD) {
                    out.push_str("\x1b[1m");
                }
                last_style = Some(style);
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m");
        // Trim trailing spaces for cleaner output
        let trimmed = out.trim_end_matches(' ');
        out.truncate(trimmed.len());
        if row < area.y + area.height - 1 {
            out.push('\n');
        }
    }
    out
}
```

### Widgets

All widgets implement ratatui's `Widget` trait and accept a `&Theme` reference, consistent with the existing `Breadcrumb` widget from the foundation.

**`PathLine`** (~30 lines): Renders `ops/now/webhooks.md` — splits on last `/`, dims the directory prefix, fg for the filename.

**`EpicTree`** (~120 lines): Renders the epic headline + subtask tree. Takes `&Task` (epic) and `&[&Task]` (subtasks). Each subtask line: `⎿ [short_id] symbol name  agent  time`. Failed subtasks get an indented error line below.

**`StageTrack`** (~60 lines): Renders `✓ build 6/6 │ ○ review │ ○ fix`. Takes phase states (done/active/pending + counts). Each phase colored by its state.

### `views/epic_show.rs` — Epic show view (~100 lines)

Composes the three widgets into a single Buffer:

```rust
pub fn render_epic_show(epic: &Task, subtasks: &[&Task], plan_path: &str, theme: &Theme) -> Buffer {
    // Calculate required height: 1 (path) + 1 (blank) + 1 (epic) + subtasks.len() + errors + 1 (blank) + 1 (stage)
    let height = /* ... */;
    let area = Rect::new(0, 0, 80, height);
    let mut buf = Buffer::empty(area);

    // Layout: split vertically
    PathLine::new(plan_path, theme).render(path_area, &mut buf);
    EpicTree::new(epic, subtasks, theme).render(tree_area, &mut buf);
    StageTrack::new(phases, theme).render(stage_area, &mut buf);

    buf
}
```

**Data model:** The headline shows the **epic** (parent task), not the build task. The epic owns the plan file path and the subtasks. Build tasks are orchestrators — an implementation detail. The `[luppzupt]` prefix is the epic's short ID (first 8 chars), giving users a handle for `aiki task show luppzupt`.

**One screen for all phases:** The same view handles build, review, and fix. The stage track shows which phase is active. The subtask list shows subtasks for the *current active phase*. No separate screens.

### Shared conventions

**Status rendering rules:**
| Status | Symbol | Color |
|--------|--------|-------|
| Closed (done) | ✓ | green |
| In progress | ▸ | yellow |
| Pending/open | ○ | dim |
| Failed | ✗ | red |
| Stopped | ✗ | red, + "stopped" label |
| Stuck (blocked by failure) | ○ | red, + "stuck" label |

**Agent color:** `cc` → cyan, `cur` → magenta, others → fg.

**Time formatting:** Seconds for < 60s (`8s`), minutes+seconds for < 60m (`1m02`), hours for longer. Omitted when no duration available.

### Integration with `commands/build.rs`

The existing `output_build_show()` function (line 765) currently builds markdown. Replace its body:

```rust
fn output_build_show(epic: &Task, subtasks: &[&Task], _build_tasks: &[&Task]) -> Result<()> {
    let plan_path = epic.data.get("plan").map(|s| s.as_str()).unwrap_or("unknown");
    // Styled output on stderr (skipped if stderr not a TTY), epic ID on stdout (if piped)
    output_utils::emit(&epic.id, || {
        let theme = Theme::from_mode(detect_mode());
        let buf = tui::views::epic_show::render_epic_show(epic, subtasks, plan_path, &theme);
        tui::buffer_ansi::buffer_to_ansi(&buf)
    });
    Ok(())
}
```

### Integration with `status_monitor.rs` (live progress)

The existing `StatusMonitor::render_task_tree()` method (line 180) builds plain-text lines with `├─`/`└─` connectors and writes them via `writeln!`. Replace its body to use the same widgets:

```rust
fn render_task_tree(&mut self, graph: &TaskGraph, root_task: &Task) -> Result<()> {
    let plan_path = root_task.data.get("plan").map(|s| s.as_str()).unwrap_or("");
    let subtasks = self.get_sorted_subtasks(graph, &root_task.id);
    let subtask_refs: Vec<&Task> = subtasks.iter().collect();
    let theme = Theme::from_mode(detect_mode());
    let buf = tui::views::epic_show::render_epic_show(root_task, &subtask_refs, plan_path, &theme);
    let ansi = tui::buffer_ansi::buffer_to_ansi(&buf);

    let mut stderr = stderr();
    if self.has_rendered {
        stderr.execute(RestorePosition)?;
        stderr.execute(Clear(ClearType::FromCursorDown))?;
    }
    stderr.execute(SavePosition)?;
    writeln!(stderr, "{}", ansi)?;
    writeln!(stderr, "\n [Ctrl+C to detach]")?;
    stderr.flush()?;
    Ok(())
}
```

The Ctrl+C handling and exit detection are unchanged. Only the rendering output changes.

Note: the StatusMonitor currently renders the *build task* tree (the orchestrator and its children), but we want to show the *epic* tree. The monitor will need to resolve the epic from the build task's `data.epic`/`data.target` field — it already does this partially (lines 242-275 in the current code) but as a secondary section. We'll make the epic the primary view.

### Update mechanism: polling → jj event watching

The current `StatusMonitor` polls `read_events()` every 500ms, which runs `jj log` twice a second. This is expensive. The rendering layer should be agnostic about how updates arrive — it just takes a `TaskGraph` snapshot and renders. This plan focuses on the rendering side. Replacing the poll loop with a jj operation watcher (e.g. filesystem notifications on `.jj/repo/op_store/`, or `jj op log --watch` if/when jj adds it) is a separate concern that can be swapped in without touching widget code.

For now, we keep the existing poll loop but note it as a known limitation to address.

### Test strategy

**Snapshot tests** in `cli/tests/tui_snapshot_tests.rs`:
1. `snapshot_epic_show_build_complete` — render completed build, verify text content, save dark+light PNGs
2. `snapshot_epic_show_build_failure` — render build with failure + stuck, verify red indicators, save PNGs
3. `snapshot_epic_show_build_in_progress` — render mid-build, verify yellow indicators, save PNGs
4. `snapshot_epic_show_mid_fix` — render with build done, review done, fix active, verify stage track

Tests construct mock `Task` structs directly (no event system needed). Call `render_epic_show()` to get a `Buffer`, then:
- Extract text via `buffer_to_text()` helper and assert content
- Render PNGs via existing `buffer_to_png()` for visual verification
- Both dark and light theme PNGs per test

**Unit tests** in each widget file:
- `PathLine`: verify directory dimmed, filename fg
- `EpicTree`: verify ID prefix, subtask connectors, status symbols
- `StageTrack`: verify phase rendering for each state combination

## Steps

1. Create `cli/src/tui/buffer_ansi.rs` — Buffer to ANSI converter
2. Create `cli/src/tui/widgets/path_line.rs` — path line widget
3. Create `cli/src/tui/widgets/epic_tree.rs` — epic tree widget
4. Create `cli/src/tui/widgets/stage_track.rs` — stage track widget
5. Create `cli/src/tui/views/mod.rs` and `views/epic_show.rs` — view composer
6. Update `cli/src/tui/mod.rs` and `widgets/mod.rs` — add new modules
7. Update `cli/src/commands/build.rs` — wire `output_build_show()`, `output_build_started()`, `output_build_completed()` to TUI renderer
8. Update `cli/src/tasks/status_monitor.rs` — replace `render_task_tree()` with widget-based rendering
9. Add snapshot tests to `cli/tests/tui_snapshot_tests.rs`
10. Verify: `cargo check`, `cargo test`, visual PNG review, manual `aiki build show` test

## Key decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Widgets + Buffer, not Line strings | Render ratatui widgets to Buffer | Same widgets work for both non-interactive (buffer_to_ansi) and interactive (Terminal::draw). Zero rewrite when adding interactivity. |
| buffer_to_ansi for non-interactive | Convert Buffer → ANSI string | Parallels existing buffer_to_png. Avoids alternate screen. Works with emit() pipe convention. |
| No tree DAG | `⎿` flat children | Tree DAG is the most complex widget. `⎿` connectors give parent/child hierarchy with minimal code. DAG comes later. |
| Three separate widgets | PathLine, EpicTree, StageTrack | Each is independently testable and reusable. EpicTree can be used in task show later. StageTrack in any pipeline view. |
| Live progress via StatusMonitor | Replace rendering, keep poll loop | StatusMonitor already handles polling, cursor management, Ctrl+C. We just swap in themed widgets for the output. |
| Path line, not breadcrumb | `ops/now/webhooks.md` with dimmed dir | Just a styled file path. Phase is *state*, shown in the stage track. |
| One screen for all phases | Epic show, not build/review/fix screens | Same layout works for all three phases — just update the stage track and subtask list. |

## What comes next

After this is working:
1. **Tree DAG widget** — Renders task dependency graph with fan-out/fan-in (replaces flat `⎿` list)
2. **Interactive epic screen** — Full TUI with `Terminal::draw()`, event loop, key handling — reuses the same widgets
3. **Task list / task show screens** — Apply the same widget pattern to other commands

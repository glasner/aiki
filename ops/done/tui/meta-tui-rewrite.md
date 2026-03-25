# TUI Rewrite: Implementation Order

**Date**: 2026-03-21
**Status**: Plan
**Priority**: P1
**Screen states**: [screen-states.md](screen-states.md)

---

## The Problem

The current TUI (6206 lines across 17 files) has persistent data-mapping edge cases. The root cause is architectural: a generic `Chat` → `Message` → `ChatChild` data model tries to represent every pipeline flow through one abstraction, and the 1240-line `chat_builder.rs` mapping layer handles every combination. Every new feature or flow creates new edge cases in the builder.

Additionally:
- **Alternate screen** hides scrollback and loses output when the build finishes (see [chatty-iteration-2.md](../chatty-iteration-2.md))
- **Double JJ reads** per tick cause 2-6s actual refresh vs 500ms target, making timers freeze
- **No agent spin-up visibility** — wall of `○` pending subtasks with no indication that work is starting

---

## Architecture

### Full System Diagram

```
                            ┌─────────────────────────────┐
                            │        JJ Store             │
                            │  (aiki/tasks bookmark)      │
                            └─────────────┬───────────────┘
                                          │ jj log (1x per tick)
                                          │
                    ┌─────────────────────↓──────────────────────┐
                    │               poll_next_msg()               │
                    │                                             │
                    │  ┌─────────────┐   ┌──────────────────┐    │
                    │  │ read_events │──→│materialize_graph()│    │
                    │  └─────────────┘   └────────┬─────────┘    │
                    │                             │              │
                    │  ┌─────────────┐            │              │
                    │  │ crossterm   │  ┌─────────↓──────────┐   │
                    │  │  events     │  │  Msg::GraphUpdated  │   │
                    │  │ (Ctrl+C,   │  │  Msg::Detach        │   │
                    │  │  resize)   │  │  Msg::Resize        │   │
                    │  └──────┬─────┘  └─────────┬───────────┘   │
                    │         └──────────────┬────┘              │
                    └────────────────────────┼───────────────────┘
                                             │
                    ┌─ app.rs ───────────────↓───────────────────┐
                    │                                             │
                    │   ┌──────────────────────────────────────┐  │
                    │   │  update(model, msg) → (model, effect) │  │
                    │   │  ▲ PURE FUNCTION — no side effects    │  │
                    │   │  ▲ LEARNED: old code had ad-hoc state │  │
                    │   │    mutations across multiple files     │  │
                    │   └──────────────────┬───────────────────┘  │
                    │                      │                      │
                    │   ┌──────────────────↓───────────────────┐  │
                    │   │              Model                    │  │
                    │   │  ┌──────────────────────────────┐    │  │
                    │   │  │ graph: TaskGraph              │    │  │
                    │   │  │ screen: Screen (TaskRun/..)    │    │  │
                    │   │  │ window: WindowState            │    │  │
                    │   │  │   (width, scroll, etc.)       │    │  │
                    │   │  └──────────────────────────────┘    │  │
                    │   │  ▲ LEARNED: single source of truth   │  │
                    │   │    old code split state across        │  │
                    │   │    StatusMonitor + LiveScreen +       │  │
                    │   │    ChatBuilder                        │  │
                    │   └──────────────────┬───────────────────┘  │
                    │                      │                      │
                    └──────────────────────┼──────────────────────┘
                                           │
                    ┌─ view (pure fn) ─────↓──────────────────────┐
                    │                                              │
                    │   match model.screen {                       │
                    │       TaskRun    → task_run::view()          │
                    │       Build     → build::view()              │
                    │       Review    → review::view()             │
                    │       Fix       → fix::view()                │
                    │       EpicShow  → epic_show::view()          │
                    │       ReviewShow→ review_show::view()        │
                    │   }                                          │
                    │                                              │
                    │   View rendering is GRAPH-DRIVEN: view fns   │
                    │   adapt to whatever tasks exist in the graph. │
                    │   e.g. build::view() renders fix/iteration     │
                    │   sections when fix subtasks are present —    │
                    │   no separate BuildFix variant needed.        │
                    │                                              │
                    │   The Screen enum defines LIFECYCLE/EXIT      │
                    │   behavior (when to stop polling), not what   │
                    │   to render.                                  │
                    │                                              │
                    │   Each view fn calls components:             │
                    │   ┌────────────────────────────────────┐     │
                    │   │ components::phase()    → Vec<Line> │     │
                    │   │ components::subtask_table()        │     │
                    │   │ components::loop_block()           │     │
                    │   │ components::issues()               │     │
                    │   │ components::section_header()       │     │
                    │   └────────────────────────────────────┘     │
                    │   ▲ LEARNED: old code had one giant          │
                    │     chat_builder (1240 lines) mapping        │
                    │     every flow through a generic model.      │
                    │     Now each flow is its own small fn        │
                    │     calling shared helpers.                   │
                    │                                              │
                    │   Output: Vec<Line>                           │
                    │   ▲ LEARNED: old Chat/Message/ChatChild      │
                    │     tree was too abstract — edge cases in    │
                    │     the mapping layer. Lines are 1:1 with    │
                    │     rendered output. No mapping needed.       │
                    │                                              │
                    └──────────────────────┬───────────────────────┘
                                           │
                    ┌─ render ─────────────↓───────────────────────┐
                    │                                              │
                    │   apply_dimming(lines)                       │
                    │       │                                      │
                    │       ↓                                      │
                    │   render_lines(lines, buf, theme)             │
                    │       │  iterates lines, applies LineStyle   │
                    │       │  to ratatui Buffer cells             │
                    │       ↓                                      │
                    │   terminal.draw(buf)                         │
                    │       │                                      │
                    │       ↓                                      │
                    │   Viewport::Inline (ratatui built-in)        │
                    │       │  cursor-up overwrite, double-buffer  │
                    │       │  diff — only changed cells written   │
                    │       ↓                                      │
                    │   stdout                                     │
                    │   ▲ LEARNED: old code used alternate screen  │
                    │     (output vanished when build finished)    │
                    │     and stderr (unbuffered, flickering).     │
                    │     Now inline on stdout — output persists   │
                    │     in scrollback like Claude Code.           │
                    │                                              │
                    └──────────────────────────────────────────────┘
```

### Lessons Learned (annotated in diagram)

| Problem in old code | Fix in new architecture |
|-----|-----|
| **Ad-hoc state mutations** across StatusMonitor, LiveScreen, ChatBuilder — no clear ownership | **Single Model** struct, all changes through `update(model, msg)`. Pure function, testable. |
| **State split across files** — monitor tracked event count, screen tracked terminal state, builder tracked stage | **One struct** holds everything: graph + screen + window state |
| **Generic Chat/Message/ChatChild model** — 1240-line mapping layer with edge cases at every combination | **Line-based output** — `Vec<Line>` is 1:1 with rendered output. No intermediate tree. No mapping. |
| **One giant chat_builder** handled every screen (build, review, fix) through switches and special cases | **Per-screen view functions** — `build::view()`, `review::view()`, etc. Each is small and self-contained, calling shared components. View rendering is graph-driven — view fns adapt to whatever tasks exist, so `build::view()` handles fix/iteration sections when they appear in the graph without needing a separate `BuildFix` variant. |
| **Double JJ reads per tick** — `poll()` read events, then `build_view()` read them again | **Single read per tick** on background thread via `mpsc::channel`. Graph stored in Model as `Arc<TaskGraph>`. |
| **500ms poll on 60fps loop** — JJ subprocess took 1-3s, so actual refresh was 2-6s | **100ms render tick** for smooth spinner animation (10 braille frames). JJ reads on background thread at ~1s interval — JJ latency doesn't block the render loop. |
| **Alternate screen** — output vanished when build finished, no scrollback | **Viewport::Inline** — output stays in scrollback, like Claude Code |
| **stderr rendering** — unbuffered, caused flickering | **stdout rendering** — buffered, gated by `stdout().is_terminal()` |
| **No panic recovery** — terminal left in raw mode on crash | **Panic hook** — restores terminal before printing panic |
| **Interleaved state + rendering tests** — hard to isolate bugs | **Testing pyramid** — state tests (pure `update()`), buffer tests (pure `render()`), snapshot tests (`insta`) |

### Data Flow Summary

```
JJ ──1x read──→ TaskGraph ──→ Model ──→ view() ──→ Vec<Line> ──→ Buffer ──→ stdout
                              (screen)

                                ↑                      │
                           update(msg)            components
                                ↑              (shared consistency)
                          user input /
                          timer tick
```

Every arrow is a pure transformation except the JJ read (I/O) and stdout write (I/O). Everything in between is testable without a terminal.

---

## Key Decisions

### stdout vs stderr

TUI rendering moves from stderr to stdout. Rationale:
- stderr is unbuffered → causes flickering and performance issues (ratatui FAQ)
- TUI and text output are never concurrent — TUI runs during the build, text results come after
- Gate: `stdout().is_terminal()` — if piped, skip TUI, print only text results
- `--output id` still writes bare IDs to stdout after TUI finishes (no conflict)

**Pre-refactor test coverage required** — snapshot piping behavior and `is_terminal()` gates before changing anything. See step 1a.

---

## Design Principles

1. **Elm architecture.** `Model → update(Msg) → Model → view → Lines → render`. View is pure. All state changes through update. User interactions are Msgs.
2. **Line-based view output.** `Vec<Line>` — flat list of styled lines, 1:1 with rendered output. No intermediate tree or typed phase hierarchy. Cheap to iterate on.
3. **Typed components.** `components::phase()`, `components::subtask_table()`, `components::loop_block()` enforce visual consistency. View functions call these to produce lines. Unusual patterns can produce raw `Line` values directly.
4. **Inline rendering.** Cursor-up overwrite like Claude Code. No alternate screen. Output persists in scrollback.
5. **Cached graph, always redraw.** Cache the last successfully-read TaskGraph in the model. Rebuild the view from the model on every tick so timers always update, even when JJ is slow.
6. **Graph-driven view rendering.** View functions adapt to whatever tasks exist in the graph. The `Screen` enum defines lifecycle/exit behavior (when to stop polling), not what to render. Flags like `--fix` affect what the command orchestrates (spawning tasks), and the TUI renders whatever appears. This avoids a combinatorial explosion of Screen variants for flag combinations.

---

## Recommended Order

### Phase 0: Pre-refactor safety net

Before touching any TUI code, snapshot the current behavior:

1. **Snapshot existing `chat_builder` output** — for each flow, run through current builder, render to Buffer, snapshot with `insta`.
2. **Snapshot `epic_show` and `issue_list`** — these views get rewritten; capture current output first.
3. **Test piping behavior** — verify `aiki build <plan> -o id` with stdout piped produces bare IDs and no TUI.
4. **Test `is_terminal()` gate** — verify TUI is skipped when stdout is not a terminal.

This creates the regression safety net before any code changes.

### Phase 1: Foundation (build new code alongside old)

All new code lives in new files. Old code is untouched. Both coexist — no breakage.

#### 1a. Inline rendering + ratatui upgrade
**Plan:** [step-1a-inline-renderer.md](step-1a-inline-renderer.md)
**Effort:** Cargo.toml change + ~20 lines terminal setup in `app.rs`
**Why first:** Upgrades ratatui 0.29 → 0.30 and uses built-in `Viewport::Inline` instead of a custom renderer. Also switches from stderr to stdout and enables `scrolling-regions` to fix flickering. No custom `InlineRenderer` needed — ratatui handles cursor-up overwrite natively.
**Also adds:** `ratatui-macros` (ergonomic `line![]`/`span![]` macros, in ratatui mono-repo). `ansi-to-tui` is deferred until a view function actually consumes ANSI subprocess output.
**Tests:** Pre-refactor piping snapshots from Phase 0.
**Depends on:** Nothing.

#### 1b. Line-based data model + Elm core
**Plan:** [step-1b-data-model.md](step-1b-data-model.md)
**Effort:** ~200 lines new (`app.rs`), replaces `types.rs` (122 lines)
**Why:** Defines `Model`, `Msg`, `update()`, `Line`, `LineStyle`. This is the Elm core — the foundation everything else plugs into.
**Tests:** Unit tests for `update()` — pure function, no terminal needed.
**Depends on:** Nothing (can parallel with 1a).

#### 1c. Shared components
**Plan:** [step-1c-shared-components.md](step-1c-shared-components.md)
**Effort:** ~200 lines new, replaces `pipeline_chat.rs` (1110 lines) and most widgets
**Why:** Implements the reusable components that view functions call: `components::phase()`, `components::subtask_table()`, `components::loop_block()`, `components::issues()`. Plus the line renderer that converts `Vec<Line>` → ratatui `Buffer`.
**Tests:** Unit tests for each component (line count, styles, content). Buffer-level render tests (cell assertions, color checks, bounds safety). Dimming tests.
**Depends on:** 1b (needs Line types).

### Phase 2: View functions (all parallel, each ships with tests)

**Plan:** [step-2-new-screens.md](step-2-new-screens.md)

All flow view functions are pure `fn(&TaskGraph, ...) → Vec<Line>`. They can be built, tested, and reviewed independently — they don't touch the event loop or terminal. Each ships with its own `insta` snapshots.

#### View functions to implement (~760 lines total)

| View function | Section | Effort | Key components used |
|---------------|---------|--------|-------------------|
| `task_run::view()` | 2.1 | ~80 lines | `components::phase`, `components::subtask_table` |
| `build::view()` | 2.2 | ~300 lines | All components, lane derivation, graph-driven fix/iteration rendering |
| `review::view()` | 2.3 | ~100 lines | `components::phase`, `components::issues` |
| `fix::view()` | 2.4 | ~120 lines | Composes build + review helpers |
| `epic_show::view()` | 2.5 | ~30 lines | `components::phase`, `components::subtask_table` |
| `review_show::view()` | 2.5 | ~30 lines | `components::phase`, `components::issues` |
| `helpers.rs` | — | ~150 lines | Shared graph query + rendering helpers |

**Tests per view function:** Each gets `insta` snapshots for its key states (loading, active, done, failed). Width-variant snapshots (40, 80, 120 cols) for `build::view()` which is the most layout-sensitive.

**Depends on:** 1b, 1c.

### Phase 3: Switch over (one step, all commands at once)

#### 3a. Wire commands to Elm loop + delete old code
**Plan:** [step-3a-status-monitor.md](step-3a-status-monitor.md)
**Effort:** ~50 lines wiring + deletion of old code
**Why:** This is the cutover. Each command (build, review, fix, task run) switches from `ScreenSession` + `StatusMonitor` to `Model` + `tui::run()`. Old TUI code is deleted in the same step — no messy coexistence period.

**What happens:**
1. Wire each command to create `Model` with appropriate `Screen`, call `tui::run()`
2. Delete all old TUI files (6046 lines — see "What We Delete" table)
3. Update `cli/src/tui/AGENTS.md` to document new architecture
4. Update `mod.rs` exports
5. Remove `image`, `ab_glyph` dev-dependencies (PNG rendering gone)
6. Verify all `insta` snapshots pass
7. Run pre-refactor piping tests to confirm no regression

**Depends on:** Phase 1 (foundation) + Phase 2 (all view functions).

---

## Testing Strategy

Tests ship **with each step**, not as a cleanup task at the end.

| Step | Tests included |
|------|---------------|
| Phase 0 | Pre-refactor snapshots (safety net) |
| 1b | `update()` unit tests |
| 1c | Emit helper unit tests, buffer-level render tests, dimming tests |
| Phase 2 | `insta` snapshots per view function + width-variant snapshots |
| 3a | Piping regression tests, full integration snapshots |

**Test pyramid:** ~10 state tests (fastest) + ~10 buffer tests + ~31 snapshot tests + 3 width-variant tests = ~54 tests total.

**Source of truth:** `insta` snapshots are the authoritative format for "what the TUI looks like." `screen-states.md` is the design intent document — it may drift as we iterate, but the snapshots always reflect reality.

See [step-4b-tests.md](step-4b-tests.md) for the full coverage mapping (old tests → new equivalents).

---

## Ratatui Best Practices Applied

From [ratatui-ecosystem.md](../research/tui/ratatui-ecosystem.md):

| Practice | How we apply it |
|----------|----------------|
| Use `Viewport::Inline` for inline rendering | Built-in cursor-up overwrite — no custom renderer |
| Enable `scrolling-regions` feature | Eliminates flickering on rapid updates |
| Use stdout, not stderr | Switch from current stderr to stdout |
| Install panic hooks | `install_panic_hook()` restores terminal before printing |
| `impl Widget for &YourWidget` | Line renderer takes `&[Line]` |
| Don't fight immediate mode | `view()` re-runs every tick, pure function |
| Unit test against `Buffer` directly | Components + `render_lines()` tested against Buffer |
| Use `insta` snapshot testing | Snapshot each screen state from mockups |
| Test state transitions separately from rendering | `update()` is pure, tested independently |
| Upgrade to ratatui 0.30 | Gets `init()`/`restore()` convenience methods |
| Use `ratatui-macros` for `Line`/`Span` construction | `line![]`, `span![]` macros reduce boilerplate in view functions and components |
| Add `ansi-to-tui` only when needed | Deferred until a view function consumes ANSI subprocess output |

---

## What We Keep

| File | Why | Notes |
|------|-----|-------|
| `theme.rs` (144 lines) | Colors and symbols | Review API surface — may need minor updates for `LineStyle` integration |

---

## What We Delete

| File | Lines | Replaced by |
|------|-------|-------------|
| `chat_builder.rs` | 1240 | Screen-specific view functions |
| `views/pipeline_chat.rs` | 1110 | `components.rs` + `render.rs` line renderer |
| `live_screen.rs` | 400 | `Viewport::Inline` (built into ratatui 0.30) |
| `loading_screen.rs` | 278 | First render of pipeline (loading state) |
| `buffer_ansi.rs` | 125 | Not needed — ratatui's `Viewport::Inline` handles output |
| `types.rs` | 122 | `app.rs` (Screen, Model, Msg, Line types) |
| `status_monitor.rs` | 443 | `app.rs` Elm event loop |
| `widgets/stage_track.rs` | 490 | Not needed |
| `widgets/epic_tree.rs` | 538 | `components::subtask_table()` |
| `widgets/breadcrumb.rs` | 188 | Not needed |
| `widgets/path_line.rs` | 200 | `components::phase()` with `ChildStyle::Done` |
| `views/epic_show.rs` | 413 | View function using components |
| `views/issue_list.rs` | 379 | View function using components |
| `render_png.rs` | 120 | `insta` text snapshots replace PNG rendering |
| `AGENTS.md` | — | Rewritten to document new architecture |
| **Total deleted** | **6046** | |

---

## Dependency Graph

```
Phase 0 (pre-refactor snapshots)
    │
    ↓
1a (ratatui upgrade) ──┐
                       ├──→ Phase 2 (all view functions, parallel)
1b (elm + model) ──┐   │
                   ├───┘
1c (components + render)─┘
                       3a (switch over: wire commands + delete old code)
```

**Key improvement:** Phase 2 flows are all independent pure functions — they can be built in parallel or as a single step. Phase 3 is now one step (wire + delete) instead of separate wire/cleanup/test phases. No messy coexistence period.

---

## Estimated Total

~1020 lines of new code replacing ~6046 lines of old code. Net reduction of ~5026 lines.

| New file | Lines |
|----------|-------|
| `app.rs` | ~200 |
| `components.rs` | ~120 |
| `render.rs` | ~80 |
| `screens/task_run.rs` | ~80 |
| `screens/build.rs` | ~300 |
| `screens/review.rs` | ~100 |
| `screens/fix.rs` | ~120 |
| `screens/epic_show.rs` | ~30 |
| `screens/review_show.rs` | ~30 |
| **Total new** | **~1020** |

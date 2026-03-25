# Step 1a: Inline Rendering with Viewport::Inline

**Date**: 2026-03-22
**Status**: Ready
**Priority**: P1
**Phase**: 1 — Foundation
**Depends on**: Nothing
**Research**: [ratatui-ecosystem.md](../../research/tui/ratatui-ecosystem.md)

---

## Problem

`LiveScreen` enters alternate screen mode (`EnterAlternateScreen`), taking over the terminal. When the build finishes, the view vanishes — output doesn't persist in scrollback. This is the opposite of what users expect (Claude Code renders inline, output stays visible).

The current `LiveScreen` is 400 lines managing raw mode, alternate screen, cursor hiding, resize handling, and a thread-local guard to prevent stderr writes during rendering.

---

## Fix: Use ratatui's `Viewport::Inline` instead of custom renderer

Ratatui 0.29+ has built-in inline viewport support via `Viewport::Inline(height)`. This does exactly what we need — cursor-up overwrite without alternate screen — and it's battle-tested. No need to build a custom `InlineRenderer`.

### How `Viewport::Inline` works

```rust
use ratatui::{Terminal, TerminalOptions, Viewport};
use ratatui::crossterm::terminal;

// Create terminal with inline viewport
let backend = CrosstermBackend::new(std::io::stdout());
let mut terminal = Terminal::with_options(
    backend,
    TerminalOptions {
        viewport: Viewport::Inline(height),
    },
)?;
```

- **First draw:** output appears at current cursor position, scrolling terminal down
- **Subsequent draws:** ratatui moves cursor up and overwrites previous output (built-in double-buffering diffs only changed cells)
- **Growth:** call `terminal.insert_before(n, |buf| ...)` to push content into scrollback, or resize the viewport
- **Done:** drop the terminal — output persists in scrollback. No cleanup needed
- **`scrolling-regions` feature:** fixes flickering with high-throughput updates (research doc #28)

### Key changes from original plan

1. **No custom `InlineRenderer`** — ratatui handles cursor-up, diffing, and overwrite natively via `Viewport::Inline`
2. **Use stdout, not stderr** — research doc explicitly recommends stdout (stderr is unbuffered, causes flickering). Current code uses stderr.
3. **Upgrade to ratatui 0.30** — gets `ratatui::init()`/`ratatui::restore()` convenience methods and the workspace split. Since we're rewriting the TUI, good time to upgrade.
4. **Enable `scrolling-regions` feature** — eliminates flickering on rapid updates
5. **Install panic hook** — use `color-eyre` or custom hook that calls `ratatui::restore()` before printing the panic. Without this, a panic in raw mode leaves the terminal unusable.

### Terminal setup in `app.rs`

The Elm event loop from step 1b owns the terminal:

```rust
use ratatui::{Terminal, TerminalOptions, Viewport};

pub fn run(mut model: Model, cwd: &Path) -> Result<Effect> {
    // Install panic hook (restores terminal on panic)
    install_panic_hook();

    // Create inline terminal on stdout
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(initial_height),
        },
    )?;

    // Enable raw mode for immediate key detection
    crossterm::terminal::enable_raw_mode()?;

    loop {
        // View (pure)
        let lines = view(&model);

        // Render using ratatui's built-in inline viewport
        terminal.draw(|frame| {
            render_lines(&lines, frame.area(), frame.buffer_mut(), &theme);
        })?;

        // Poll for next event
        let msg = poll_next_msg(cwd, &model)?;
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

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Restore terminal first so panic output is readable
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
        original(info);
    }));
}

// Additionally, wrap the render path with catch_unwind to provide
// a graceful fallback when view() or render_lines() panics:
//
//   match std::panic::catch_unwind(AssertUnwindSafe(|| {
//       let lines = view(&model);
//       terminal.draw(|frame| {
//           render_lines(&lines, frame.area(), frame.buffer_mut(), &theme);
//       })
//   })) {
//       Ok(result) => result?,
//       Err(_) => {
//           // Restore terminal and dump last known state as plain text
//           crossterm::terminal::disable_raw_mode()?;
//           eprintln!("TUI render panic — last known state:");
//           for line in &last_good_lines {
//               eprintln!("  {}", line.text);
//           }
//           return Err(anyhow!("TUI render panic"));
//       }
//   }
//
// This is especially important during the transition period when
// edge cases in the new view functions are being shaken out.
// The user keeps context about what was happening instead of losing it.
```

### Viewport height management

The inline viewport has a fixed height. As the output grows (new phases appear), we need to grow it. **Important:** `terminal.resize()` alone is insufficient with `Viewport::Inline` — ratatui may overwrite lines that should have been preserved in scrollback.

Since our viewport height only ever grows (phases accumulate, never shrink), we use a monotonically increasing height approach:

```rust
// Before each draw, check if height needs to grow
let new_height = lines_height(&lines);
if new_height > current_height {
    // Use insert_before() to push completed content into scrollback,
    // then resize the viewport for the new height.
    // This prevents ratatui from overwriting preserved lines.
    let growth = new_height - current_height;
    terminal.insert_before(growth, |_| {})?;
    terminal.resize(Rect::new(0, 0, width, new_height))?;
    current_height = new_height;
}
```

**Spike early:** Test this with a real terminal before assuming the cursor math works. The ratatui docs on `Viewport::Inline` growth have specific guidance on `insert_before()` semantics. If `insert_before()` + `resize()` causes visual glitches, the fallback is to start with a generous initial height (e.g., 30 lines) and only grow beyond that.

### What this replaces

| Old | New |
|-----|-----|
| `LiveScreen` (400 lines) | `Viewport::Inline` (built into ratatui) |
| `LoadingScreen` (278 lines) | First render of the pipeline (loading state) |
| `buffer_to_ansi()` custom conversion | ratatui's built-in double-buffered rendering |
| Custom cursor-up ANSI sequences | ratatui handles it |
| stderr rendering | stdout rendering |
| `ScreenSession` (131 lines) | Terminal owned by `tui::run()` |

### Cargo.toml changes

```toml
# Upgrade ratatui and enable scrolling-regions
ratatui = { version = "0.30", features = ["scrolling-regions"] }
crossterm = "0.28"  # keep — compatible with ratatui 0.30

# Ecosystem crates
ratatui-macros = "0.7"   # line![], span![], text![] macros — in the ratatui mono-repo (1.6M recent DL)
# ansi-to-tui = "8"      # Deferred — add when a view function actually needs ANSI→Text conversion
```

**ratatui-macros** — ergonomic macros for constructing `Line`, `Span`, `Text`, `Row`, and `Constraint` values. Since our architecture outputs `Vec<Line>`, this directly reduces boilerplate in every view function and component. Lives in the ratatui mono-repo, zero adoption risk.

**ansi-to-tui** — deferred. No Phase 2 view function currently consumes ANSI-escaped subprocess output. Add this dependency when a specific view function needs it (likely when rendering raw agent logs or build stderr). Phantom dependencies create confusion.

### Files changed

| File | Change |
|------|--------|
| `cli/Cargo.toml` | Upgrade ratatui 0.29 → 0.30, add `scrolling-regions` feature, add `ratatui-macros` |
| `cli/src/tui/app.rs` | Terminal setup with `Viewport::Inline` (part of step 1b) |

Note: Old files (`live_screen.rs`, `loading_screen.rs`, etc.) are deleted in Phase 3 cutover, not here. New code is additive — old and new coexist until switchover.

### Pre-refactor test coverage

See Phase 0 in [meta-tui-rewrite.md](meta-tui-rewrite.md). Snapshot current behavior before any code changes.

### Post-refactor tests

- `render_lines()` is tested against `Buffer` directly (research doc #23) — no terminal needed
- For integration tests, use `TestBackend` with `insta` snapshot testing (#25)
- Panic hook test: verify terminal is restored after panic

### Ratatui best practices applied

From [ratatui-ecosystem.md](../../research/tui/ratatui-ecosystem.md):

| # | Practice | How we apply it |
|---|----------|----------------|
| 1 | Use `ratatui::init()`/`restore()` or `run()` | We use `Terminal::with_options` for `Viewport::Inline`, with proper init/restore |
| 2 | Install panic hooks | `install_panic_hook()` restores terminal |
| 3 | Use stdout, not stderr | `CrosstermBackend::new(std::io::stdout())` |
| 9 | Implement `Widget` for `&YourWidget` | Line renderer takes `&[Line]` |
| 12 | Don't fight immediate mode — render everything every frame | Our `view()` is a pure function, re-runs every tick |
| 23 | Unit test against `Buffer` directly | Components + `render_lines()` tested against Buffer |
| 25 | Use `insta` snapshot testing | Snapshot each screen state from the mockups |
| 26 | Test state transitions separately from rendering | `update()` is pure, tested independently |
| 28 | Enable `scrolling-regions` for `Viewport::Inline` | Feature enabled in Cargo.toml |
| — | Add `ansi-to-tui` only when needed | Deferred until a view function consumes ANSI subprocess output |

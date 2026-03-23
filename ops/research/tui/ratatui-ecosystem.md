# Ratatui Ecosystem Deep Dive: Developers, Best Practices & Patterns

## Key People in the Ecosystem

### Core Maintainers

**Orhun Parmaksız** (`@orhun`) — Project lead and most visible face of Ratatui. Originally from Turkey, Orhun led the fork from tui-rs in early 2023 when the original project went unmaintained. Beyond Ratatui, he created **git-cliff** (changelog generator, 10K+ GitHub stars), **binsider** (binary analyzer), **kmon** (kernel module monitor), and **Ratzilla** (Ratatui → WebAssembly for browser rendering). He's an Arch Linux package maintainer for Rust tools and gives conference talks internationally (FOSDEM, RustLab, Rustikon, Tokyo Rust Meetup). His blog at `blog.orhun.dev` is a top resource for Ratatui internals and open source funding models.

**Josh McKinney** (`@joshka`) — Co-lead maintainer and the most prolific code contributor. Joshka drives the library's API design, widget system evolution, and modular architecture (the 0.30.0 workspace split was largely his work). He maintains several key ecosystem crates:
- **tui-framework-experiment** — An experimental widget framework building on Ratatui with buttons, stack containers, toggle switches, and event handling abstractions. This is the closest thing to a "component library" in the ecosystem.
- **tui-prompts** — Interactive prompt widgets (text input, selection, etc.)
- **tui-markdown** — Proof-of-concept Markdown → `ratatui::Text` renderer with an accompanying `mdr` CLI viewer.

He also authored a notable PR updating deprecation notes specifically to help AI coding assistants suggest correct replacements — a sign of how seriously the project thinks about the AI-assisted development workflow.

**Dheepak Krishnamurthy** (`@kdheepak`) — Maintainer and creator of **taskwarrior-tui** (1.7K stars), a full TUI for the Taskwarrior task manager. Also created **lazygit.nvim** (1.9K stars) for Neovim integration, and built TUI libraries in Julia (`TerminalUserInterfaces.jl`). His cross-language TUI experience brings valuable perspective to Ratatui's design.

**Other maintainers**: `@j-g00da`, `@mindoodoo`, `@sayanarijit` (creator of **xplr**, a hackable terminal file explorer). The original tui-rs creator **Florian Dehau** (`@fdehau`) is listed as a past maintainer and has expressed he's happy to see the community carrying the project forward.

### Notable Ecosystem Developers

**benjajaja** — Creator of **ratatui-image**, the go-to crate for rendering images in the terminal. Supports Sixel, Kitty, and iTerm2 graphics protocols with automatic detection, plus a "halfblocks" Unicode fallback. Demonstrates best practices for both stateless and stateful widget patterns.

**jfernandez** — Key contributor to Netflix's **bpftop**, arguably the highest-profile production Ratatui app. Netflix's use case validated Ratatui for real-time systems monitoring.

**tachyonfx author** — Created the shader-like effects library for Ratatui animations and transitions.

---

## Production Users

Ratatui has 19K+ GitHub stars, 20M+ downloads, and 2,800+ dependent crates. Notable production adopters:
- **Netflix** — bpftop (eBPF program monitoring)
- **AWS** — amazon-q-developer-cli
- **OpenAI** — internal tooling
- **Vercel** — developer tools
- **Radicle/Radworks** — decentralized Git collaboration (they donated funds to the project)

---

## Architecture Patterns (Expanded)

### The Elm Architecture (TEA) — Recommended Starting Point

The TEA pattern maps cleanly to Ratatui:
- **Model** — A struct holding all application state
- **Message** — An enum representing every possible event/action
- **Update** — Takes `(model, message)` → mutates or produces new model
- **View** — Pure function: `(model, frame)` → renders widgets

Key discipline: the view function should be a pure projection of state. For a given model, it always produces the same visual output. This makes testing straightforward — render to a `TestBackend`, assert on the buffer.

### Component Architecture — For Larger Apps

Each component owns its state, event handlers, and rendering logic via traits. Good when you have independent panels (e.g., a file browser panel + preview panel + status bar). The tradeoff: inter-component communication requires explicit message passing or shared state, which adds complexity.

### Flux — For Complex Event Flows

Central dispatcher routes all actions to stores. Best when you have many event sources (user input, network responses, timers) that all need to flow through consistent state management.

### Practical Recommendation

Start with TEA. When you find yourself passing too many fields between functions or your single update function exceeds ~200 lines, consider breaking into components. The Flux pattern is rarely needed outside of apps with complex async data flows.

---

## Best Practices (Comprehensive)

### Project Setup

1. **Use `ratatui::init()` / `ratatui::restore()` or `ratatui::run()`** — Don't manually manage raw mode and alternate screen unless you have specific needs. The 0.30.0 `run()` convenience method handles the full lifecycle.

2. **Install panic hooks immediately** — Use `color-eyre` or a custom hook that restores terminal state before printing the panic. Without this, a panic in raw mode leaves the terminal unusable.

3. **Use stdout, not stderr** — Stderr is unbuffered in most cases and causes noticeable performance degradation. The project FAQ explicitly recommends stdout.

4. **Pin your Crossterm version** — Multiple Crossterm major versions in the dependency graph produce confusing compiler errors like `no method named 'is_key_press' found`. Ensure your `Cargo.lock` has exactly one Crossterm major version.

5. **Enable the `layout-cache` feature** — In 0.30.0+, disabling default features also disables the layout cache, which hurts performance. If you're doing `default-features = false`, add `features = ["layout-cache"]` explicitly.

### State Management

6. **Use enums for modes, not booleans** — If your app has Normal/Editing/Searching modes, model them as `enum AppMode { Normal, Editing, Searching }`. Boolean flags (`is_editing`, `is_searching`) create impossible states and are harder to reason about.

7. **Keep state flat when possible** — Deeply nested state makes the update function harder to write. Prefer a flat `App` struct with clear fields over deeply nested sub-structs.

8. **Don't store layout rectangles in state (usually)** — Layout `Rect` values are computed at render time and change with terminal resizing. Store the *data* your widgets need, not the *areas* they render in. Exception: when you need the previous frame's area for the next frame's calculations.

### Widget Design

9. **Implement `Widget` for `&YourWidget`, not just `YourWidget`** — Since 0.26.0, all built-in widgets implement Widget for references. This allows storing widgets between frames and rendering them multiple times. For custom widgets, prefer `impl Widget for &MyWidget`.

10. **Use `StatefulWidget` for interactive lists/tables** — Selection state (scroll position, highlighted row) should go through `render_stateful_widget` with a mutable state reference, not tracked externally.

11. **Compose widgets by nesting** — Widgets can render other widgets inside their `render` implementation. A `Line` widget can be used inside other widgets. This is the primary composition mechanism.

12. **Don't fight immediate mode** — Every frame, render everything. Don't try to cache rendered output or do partial updates. Ratatui's double-buffering (current vs. previous buffer, diffing only changes) handles performance. The diff-based approach means you're not actually rewriting the entire terminal every frame.

### Layout

13. **Use constraints, not fixed sizes** — `Constraint::Percentage`, `Min`, `Max`, `Ratio`, and `Fill` make your TUI responsive. Hardcoded widths break in different terminal sizes.

14. **Nest layouts for complex UIs** — A horizontal split containing vertical splits is the standard pattern. Layouts return an indexed list of `Rect`s; you can split those recursively.

15. **Guard against out-of-bounds rendering** — When rendering to a sub-area, clamp it: `let safe_area = area.intersection(buf.area());`. This prevents panics when widgets try to render outside the buffer.

16. **Watch for large terminal issues** — There's a known issue where widgets can misposition in very large terminals. Test your app at both small (80x24) and large sizes.

### Performance

17. **Avoid reconstructing large data structures every frame** — The Table widget had a known performance issue: converting 15K items to `Vec` on every frame caused 1-2 second lag. Construct data once, reference it during rendering.

18. **Profile before optimizing** — Use `cargo-flamegraph` to find actual hotspots. The Ratatui maintainers explicitly state: "We take performance into account as long as that is aligned with an actual problem in a real application."

19. **Don't over-render** — If nothing changed, you can skip the draw call entirely. Wrap your `terminal.draw()` in a check for whether state has been modified.

### Async & Threading

20. **Start synchronous, go async only when needed** — The blocking event loop (draw → poll event → update → repeat) works for most apps. Only add tokio when you need concurrent I/O.

21. **When async, use a channel-based architecture** — Spawn async tasks that send results back through `tokio::sync::mpsc`. The main loop reads from both the event channel and the task result channel. The `simple-async` template demonstrates this pattern.

22. **Offload heavy work to background threads** — Image encoding (ratatui-image), network requests, and file I/O should happen off the render thread. Pass results back via channels.

### Testing

23. **Unit test widgets against `Buffer` directly** — The official docs note: "it is preferable to write unit tests for widgets directly against the buffer rather than using TestBackend." Create a `Buffer`, render your widget into it, assert on cell contents.

24. **Use `TestBackend` for integration tests** — When testing the full terminal UI (layout + multiple widgets + state), use `TestBackend::new(width, height)`, call `terminal.draw(...)`, then `backend.assert_buffer_lines(...)`.

25. **Use snapshot testing with `insta`** — The official recipe recommends rendering to a `TestBackend`, converting to string, and using `insta::assert_snapshot!`. Use consistent terminal sizes (e.g., 80x20) for reproducible results. Update snapshots with `cargo insta review`.

26. **Test state transitions separately from rendering** — Your update function should be testable without any terminal. Feed it messages, assert on the resulting model state. Test rendering separately.

27. **For PTY-level integration tests**, consider `ratatui-testlib` — It runs your app in a real pseudo-terminal and provides assertions on screen state. Useful for testing terminal-specific behavior that `TestBackend` can't cover.

### Common Pitfalls

28. **Flickering with `Viewport::Inline`** — High-throughput `terminal.insert_before()` calls cause flickering. Enable the `scrolling-regions` feature (introduced in 0.29.0) which uses terminal scrolling regions to eliminate this.

29. **`Buffer::get(x, y)` is deprecated** — Use `Buffer[(x, y)]` instead. AI coding assistants frequently suggest the old API due to training data. The maintainers specifically updated deprecation notes to guide tools toward the correct replacement.

30. **Signed coordinate limitations** — Ratatui uses unsigned `Rect` coordinates. If you need scrolling or rendering outside visible bounds, you'll need a virtual canvas / viewport pattern. This is a known pain point and a frequently discussed design issue.

31. **Word-wrapping + scrolling is tricky** — A word-wrapped `Paragraph` scrolled to line N doesn't know what's on line N until it wraps. The recommended approach: keep two scroll values (the un-wrapped line position and the wrapped offset), or render to an intermediate oversized buffer and display a window into it.

---

## Essential Ecosystem Crates

| Crate | Purpose |
|-------|---------|
| `ratatui-macros` | Reduces layout boilerplate with declarative macros |
| `tui-input` | Headless text input handling |
| `tui-scrollview` | Scrollable viewport widget |
| `tui-popup` | Popup/modal dialog widget |
| `tachyonfx` | Shader-like visual effects and animations |
| `ratatui-image` | Terminal image rendering (Sixel/Kitty/iTerm2) |
| `tui-markdown` | Markdown → ratatui Text conversion |
| `terminput` | Backend-agnostic input event abstraction |
| `ratatui-macros` | Layout and style shorthand macros |
| `rat-salsa` | Event queue with tasks, timers, focus handling |
| `rat-widget` | Comprehensive data-input widgets (text, date, number, checkbox, slider, calendar, file dialog, menubar) |
| `edtui` | Vim-inspired editor widget |
| `ratzilla` | Deploy Ratatui apps to the web via WebAssembly |
| `tui-framework-experiment` | Experimental higher-level widget framework by joshka |
| `envision` | TEA-based framework with virtual terminal support for testing and AI agents |

---

## Project Health & Community

- **19.1K+ GitHub stars**, **20.4M+ downloads**
- Active Discord, Matrix, and Forum communities
- Funded by Radworks (Radicle), with transparent fund management
- Alpha releases every Saturday for early testing
- Uses conventional commits, cargo-semver-checks in CI, and git-cliff for changelogs
- Vision document (Issue #1321) explicitly states: a library, not a framework — flexibility over convention
- The project draws inspiration from Textual (Python) and Bubbletea (Go) for aesthetics while maintaining Rust's performance-first philosophy

---

## Resources

- **Official site**: https://ratatui.rs
- **API docs**: https://docs.rs/ratatui/latest/ratatui/
- **GitHub**: https://github.com/ratatui/ratatui
- **Templates**: `cargo generate ratatui/templates`
- **Awesome list**: https://github.com/ratatui/awesome-ratatui
- **Orhun's blog**: https://blog.orhun.dev
- **Forum**: https://forum.ratatui.rs
- **FOSDEM 2024 intro talk**: search "FOSDEM 2024 Ratatui"

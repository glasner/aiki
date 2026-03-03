# TUI Foundation: Theme, Visual Testing Rig, First Widget

## Context

The full TUI plan (ops/now/ux.md) describes ~2,300 lines across 7 screens, 20+ files. A previous implementation attempt had to be reverted because it was too ambitious. This plan establishes the **minimum foundation** that all future TUI work builds on: dependencies, theme with light/dark mode support, a PNG test renderer so agents can see what widgets look like, and one widget to prove it all works.

Total new code: ~550 lines across 7 new files + edits to 4 existing files.

## Mockup workflow

No off-the-shelf tool exists for full-color terminal TUI mockups that both humans and agents can work with precisely. Web design tools (Pencil.dev, Figma) think in pixels/CSS, not character grids. Terminal recording tools (VHS, asciinema) capture existing output but can't sketch new designs. There's a [Penpot feature request](https://community.penpot.app/t/title-terminal-tui-mockup-support-character-grid-an/10234) for exactly this — monospace grid + per-cell ANSI color — but it's not built yet.

### Approach: ASCII mockups + render_png feedback loop

**Phase 1 — Layout (ASCII + annotations):**

Use annotated ASCII art to define structure, content, and styling. This is the format agents parse with zero ambiguity. Convention:

```
[80×24]                                           ← optional dimensions
 aiki > tasks > luppzupt > build                  ← dim " > ", last=hi+bold
─────────────────────────────────────────          ← dim separator
 ● Build project              3m ago              ← green ●, text title, dim time
 ◉ Run tests                 running              ← yellow ◉, yellow "running"
 ✗ Deploy                     failed              ← red ✗, red "failed"
```

Rules:
- Left side is the literal terminal output (monospace, character-accurate)
- Right side after `←` is a style annotation referencing Theme field names (`dim`, `text`, `hi`, `green`, etc.)
- Annotations are not rendered — they're instructions for the implementer
- Include `[width×height]` when layout depends on terminal size
- For column-aligned layouts, mark column ranges: `[col 0-39: left panel]`

**Phase 2 — Visual refinement (render_png loop):**

Once the foundation is built, the `render_png` test renderer enables a tight iteration cycle:

1. Human describes what they want (ASCII mockup, prose, or corrections to a previous render)
2. Agent writes/modifies widget code
3. Test renders a PNG via `buffer_to_png()`
4. Agent reads the PNG to self-check for obvious errors
5. Human reviews the PNG and provides feedback
6. Repeat until correct

This loop has zero format-translation because the mockup *is* the implementation — the PNG comes directly from the same ratatui Buffer that the real TUI renders. What you see in the test PNG is exactly what you'll see in the terminal.

Both dark and light PNGs are rendered on each iteration so contrast issues are caught immediately.

### Why this works

| Alternative | Problem |
|-------------|---------|
| Pencil.dev / Figma | Thinks in pixels/CSS, not character cells. Agent must translate layouts — fidelity loss. |
| ANSI escape sequences in files | Precise but unreadable by humans without a renderer. |
| Pure prose descriptions | Ambiguous about exact spacing, alignment, column positions. |
| HTML with colored `<span>`s | Agent-readable, human-viewable, but adds a format that doesn't map 1:1 to ratatui concepts. |
| ASCII + annotations + render_png | ASCII is human-writable, agent-parseable, and the render loop provides visual verification with no translation layer. |

## Steps

### 1. Add dependencies to `cli/Cargo.toml`

Add to `[dependencies]`:
```toml
ratatui = "0.29"
terminal-colorsaurus = "0.4"
```

Add to `[dev-dependencies]`:
```toml
image = "0.25"
ab_glyph = "0.2"
```

ratatui 0.29 uses crossterm 0.28 (already present). `terminal-colorsaurus` auto-detects terminal background color via OSC 11 — used by bat and delta, actively maintained, minimal overhead (~5-15ms, skipped entirely when env var override is set). image + ab_glyph are dev-only (test builds only).

### 2. Bundle JetBrains Mono font

- Download `JetBrainsMono-Regular.ttf` from JetBrains/JetBrainsMono GitHub releases
- Place at `cli/assets/JetBrainsMono-Regular.ttf` (~200KB)
- Add `cli/assets/FONT_LICENSE` with OFL 1.1 text
- The font is compiled into test binaries via `include_bytes!` — never enters the release binary

### 3. Update `.gitignore` and create `.gitattributes`

`.gitignore` — add:
```
# TUI test snapshot PNGs (generated artifacts)
cli/tests/snapshots/*.png
```

`.gitattributes` — new file:
```
*.ttf binary
*.png binary
```

### 4. Create module structure

**`cli/src/tui/mod.rs`** (~8 lines):
- `pub mod theme;`
- `pub mod widgets;`
- `#[cfg(test)] pub mod render_png;`

**`cli/src/tui/widgets/mod.rs`** (~3 lines):
- `pub mod breadcrumb;`

**`cli/src/lib.rs`** — add `pub mod tui;`

**`cli/src/main.rs`** — add `mod tui;`

### 5. Implement `cli/src/tui/theme.rs` (~150 lines)

**Theme detection** — `detect_mode() → ThemeMode`:
1. Check `AIKI_THEME` env var — `"light"` or `"dark"` (case-insensitive). If set, skip detection entirely (zero overhead).
2. Call `terminal_colorsaurus::theme_mode(QueryOptions::default())` — auto-detects via OSC 11.
3. Fallback to `Dark` if detection fails (most terminal users use dark themes).

```rust
pub enum ThemeMode { Dark, Light }

pub fn detect_mode() -> ThemeMode {
    if let Ok(v) = std::env::var("AIKI_THEME") {
        return match v.to_lowercase().as_str() {
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        };
    }
    match terminal_colorsaurus::theme_mode(Default::default()) {
        Ok(terminal_colorsaurus::ThemeMode::Light) => ThemeMode::Light,
        _ => ThemeMode::Dark,
    }
}
```

**Theme struct** — holds all colors for a given mode:
```rust
pub struct Theme {
    // Accent colors (same in both modes — chosen for contrast on both backgrounds)
    pub green: Color,    // done, success
    pub cyan: Color,     // review, info
    pub yellow: Color,   // active, in-progress
    pub red: Color,      // failed, error
    pub magenta: Color,  // cursor agent
    pub blue: Color,     // informational
    pub orange: Color,   // warnings

    // Structural colors (differ between modes)
    pub dim: Color,      // borders, inactive
    pub fg: Color,       // supporting text
    pub text: Color,     // primary text (was WHITE)
    pub hi: Color,       // high-contrast headers
    pub bg: Color,       // background for filled regions
}
```

**Two constructors** — `Theme::dark()` and `Theme::light()`:

| Color | Dark | Light |
|-------|------|-------|
| green | `#5fcc68` | `#3a9e44` |
| cyan | `#5bb8c9` | `#2a8a9e` |
| yellow | `#d4a840` | `#a07820` |
| red | `#e05555` | `#c43030` |
| magenta | `#c470b0` | `#a04890` |
| blue | `#5588cc` | `#3366aa` |
| orange | `#cc8844` | `#aa6622` |
| dim | `#3a3a44` | `#c8c8d0` |
| fg | `#777777` | `#666666` |
| text | `#cccccc` | `#2a2a2a` |
| hi | `#e8e8e8` | `#111111` |
| bg | `#1e1e24` | `#f5f5f0` |

Design rationale: accent colors are darkened ~20% in light mode to maintain contrast against a light background. Structural colors are inverted. The dark palette is unchanged from ux.md.

**Convenience method** — `Theme::from_mode(mode: ThemeMode) → Theme`.

**Symbol constants** (unchanged — these are mode-independent):
- `SYM_DONE` (●), `SYM_ACTIVE` (◉), `SYM_PENDING` (○), `SYM_FAILED` (✗), `SYM_CHECK` (✓), `SYM_RUNNING` (▸)

**Helper methods on Theme** (replace the free functions — styles depend on theme colors):
- `theme.separator_line(width) → Line` — dim horizontal rule
- `theme.dim_style()`, `theme.fg_style()`, `theme.text_style()`, `theme.hi_style()` — common styles
- `theme.status_color(TaskStatus) → Color` — maps task status to theme accent color
- `status_symbol(TaskStatus) → &str` — free function, mode-independent

### 6. Implement `cli/src/tui/widgets/breadcrumb.rs` (~80 lines)

The simplest widget from the design — good first test subject.

- `BreadcrumbSegment` struct: `text: String`, `style: Option<Style>`
- `Breadcrumb` widget: holds `segments` and a `&Theme` reference
- Implements ratatui's `Widget` trait
- Renders segments separated by dim ` > ` (using `theme.dim_style()`)
- Last segment defaults to `theme.hi_style()` (bold + high-contrast)
- Handles narrow terminals gracefully (stops rendering when out of space)

### 7. Implement `cli/src/tui/render_png.rs` (~130 lines)

Buffer-to-PNG renderer (cfg(test) only):

- `buffer_to_png(buf: &Buffer, path: &Path, theme: &Theme) → Result<()>`
- Loads JetBrains Mono via `include_bytes!("../../assets/JetBrainsMono-Regular.ttf")`
- 10px wide × 20px tall cell grid
- Uses `theme.bg` as the default background for cells with `Color::Reset` — this means dark-theme PNGs render on `#1e1e24` and light-theme PNGs render on `#f5f5f0`
- Renders each buffer cell: fill bg color, rasterize glyph with fg color
- Handles `Modifier::DIM` (halve RGB) and `Modifier::BOLD` (brighten RGB)
- Creates parent directories automatically

### 8. Create `cli/src/commands/status.rs` stub (~12 lines)

Minimal stub that prints "TUI not implemented yet" — establishes the entry point. Add `Status` variant to `Commands` enum in main.rs with a `target: Option<String>` arg. Register in `commands/mod.rs`.

### 9. Write snapshot tests `cli/tests/tui_snapshot_tests.rs` (~170 lines)

Test helpers:
- `render_widget(widget, width, height) → Buffer`
- `buffer_to_text(buf) → Vec<String>` — extract plain text per row
- `save_png(buf, name, theme)` — write PNG to `cli/tests/snapshots/`

Tests (run for both dark and light themes):
1. **`snapshot_breadcrumb_basic`** — full breadcrumb "aiki > tasks > luppzupt > build", assert text, save PNGs (`breadcrumb_basic_dark.png`, `breadcrumb_basic_light.png`)
2. **`snapshot_breadcrumb_empty`** — empty segments, assert blank output
3. **`snapshot_breadcrumb_narrow`** — 20-char width, assert truncation without panic
4. **`snapshot_theme_sampler_dark`** — render all colors + symbols + styles on dark background, save PNG
5. **`snapshot_theme_sampler_light`** — same grid on light background, save PNG — visual proof that accents have adequate contrast on both backgrounds
6. **`test_detect_mode_env_override`** — set `AIKI_THEME=light`, assert `detect_mode()` returns `Light`; set `AIKI_THEME=dark`, assert `Dark`; unset, assert fallback works

## Key decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Theme detection | `terminal-colorsaurus` + `AIKI_THEME` env var | Same approach as bat/delta. `terminal-colorsaurus` is the best OSC 11 crate (fast heuristic pre-check, proper luminance calc, panic-safe). Env var provides zero-overhead override when detection is unreliable (tmux, SSH, CI). |
| Dark as default fallback | When detection fails, assume dark | ~80% of terminal users use dark themes. Safer default. |
| Theme struct vs constants | `Theme` struct with `dark()`/`light()` constructors | Constants can't vary by mode. Struct lets widgets accept `&Theme` and render correctly in either mode. |
| Accent color strategy | Same hues, darkened ~20% for light mode | Maintains color semantics (green=done, red=failed) across modes. Darkening accents on light bg preserves contrast without redesigning the palette. |
| `WHITE` renamed to `text` | Field name `text` instead of `white` | "White" is misleading in light mode where primary text is near-black. `text` communicates intent. |
| render_png gating | `#[cfg(test)]` | Simpler than feature flag; image/ab_glyph are dev-deps anyway |
| PNG output location | `cli/tests/snapshots/*.png` gitignored | Generated artifacts, not source of truth |
| Snapshot framework | Manual `assert_eq!` | Only 6 tests; insta adds unnecessary complexity at this stage |
| Module structure | Only files with real code | No empty placeholders for screens/, app.rs, etc. |
| Mockup workflow | ASCII + annotations → render_png loop | No off-the-shelf tool handles terminal character grids with color. ASCII is human-writable and agent-parseable; render_png closes the visual feedback loop with zero format translation. |

### tmux caveat

tmux intercepts OSC 11 and returns a cached background color from when the session started. If a user switches their terminal theme mid-session, detection will return the stale value until the terminal is resized. This is a known limitation across all OSC 11 tools (bat, delta, helix). The `AIKI_THEME` env var is the escape hatch — users can add `export AIKI_THEME=light` to their shell profile to bypass detection entirely.

## Verification

1. `cargo check` — all code compiles
2. `cargo check --tests` — test code (including render_png) compiles
3. `cargo test --test tui_snapshot_tests` — all 6 tests pass
4. Agent reads `cli/tests/snapshots/breadcrumb_basic_dark.png` — visually confirms breadcrumb renders on dark bg
5. Agent reads `cli/tests/snapshots/breadcrumb_basic_light.png` — visually confirms breadcrumb renders on light bg
6. Agent reads `cli/tests/snapshots/theme_sampler_dark.png` — visually confirms all colors/symbols on dark bg
7. Agent reads `cli/tests/snapshots/theme_sampler_light.png` — visually confirms all colors/symbols on light bg, checks accent contrast
8. `cargo build && ./target/debug/aiki status` — prints placeholder message
9. `cargo build && ./target/debug/aiki --help` — shows Status command in help
10. `AIKI_THEME=light cargo test --test tui_snapshot_tests` — tests pass with light theme override

## Files changed

| File | Change |
|------|--------|
| `cli/Cargo.toml` | Add ratatui, terminal-colorsaurus, image, ab_glyph |
| `cli/src/lib.rs` | Add `pub mod tui;` |
| `cli/src/main.rs` | Add `mod tui;`, Status command variant + match arm |
| `cli/src/commands/mod.rs` | Add `pub mod status;` |
| `.gitignore` | Add snapshot PNG pattern |
| `.gitattributes` | **New** — mark ttf/png as binary |
| `cli/assets/JetBrainsMono-Regular.ttf` | **New** — bundled font |
| `cli/assets/FONT_LICENSE` | **New** — OFL 1.1 license |
| `cli/src/tui/mod.rs` | **New** — module root |
| `cli/src/tui/theme.rs` | **New** — theme system with dark/light modes + auto-detection |
| `cli/src/tui/render_png.rs` | **New** — buffer-to-PNG renderer (theme-aware) |
| `cli/src/tui/widgets/mod.rs` | **New** — widget module root |
| `cli/src/tui/widgets/breadcrumb.rs` | **New** — first widget (theme-aware) |
| `cli/src/commands/status.rs` | **New** — command stub |
| `cli/tests/tui_snapshot_tests.rs` | **New** — visual snapshot tests (both themes) |

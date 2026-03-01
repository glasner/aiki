# Visual TUI Testing with Actual Screenshots

Status: plan
Related: ops/now/ux-with-testing.md

---

## Context

We have buffer-based snapshot testing for the TUI: widgets render to a `ratatui::Buffer`, then `render_to_text()` extracts plain text + color annotations. This catches regressions but has a fundamental limitation — no one can actually *see* what the TUI looks like. The agent reviewing TUI changes is comparing character grids and color annotation strings, not looking at the actual visual output.

This plan adds the ability to render ratatui buffers as PNG images, so the agent (or a human) can literally look at a screenshot of the TUI output during development and review.

## Goals

1. Agent can run a test and then **look at the resulting PNG** to verify the TUI looks correct
2. Works in the existing test harness — no external tools or display server required
3. Produces images that match what a real terminal would show (dark background, colored text, proper monospace font)
4. Images are useful for PR review (attach to PR descriptions, commit messages)

## Non-Goals

- Recording animated GIFs or video of interactive sessions (future work, see VHS below)
- Replacing buffer-based snapshot tests (those remain for fast regression checking)
- Pixel-perfect terminal emulation (close enough to be useful, not a terminal emulator)

---

## Approach: Buffer → PNG Renderer

Since we already have the full ratatui `Buffer` with per-cell data (character, fg color, bg color, modifiers like bold/dim), we can render directly to an image without any external tools.

### How It Works

```
ratatui::Buffer → iterate cells → draw glyphs with colors → write PNG
```

Each buffer cell has:
- `symbol: &str` — the character (including Unicode like `▸`, `✓`, `○`)
- `fg: Color` — foreground color (our RGB theme colors)
- `bg: Color` — background color (usually reset/default → dark bg)
- `modifier: Modifier` — bold, dim, italic, underlined, etc.

We render each cell as a colored glyph on a dark background at the correct grid position.

### Dependencies

| Crate | Purpose | Size |
|-------|---------|------|
| `image` | PNG encoding, pixel buffer | Well-known, no system deps |
| `ab_glyph` | Font rasterization (TrueType/OpenType) | Pure Rust, no system deps |

Both are pure Rust — no C dependencies, no system libraries, works everywhere including CI.

### Font

Bundle a monospace font as a static byte array in the test binary. Options:

| Font | License | Coverage | Notes |
|------|---------|----------|-------|
| **JetBrains Mono** | OFL 1.1 | Excellent Unicode + box-drawing | Best option — great glyph coverage, designed for terminals |
| Fira Code | OFL 1.1 | Good Unicode | Popular alternative |
| DejaVu Sans Mono | Bitstream Vera | Broad Unicode | Safe fallback |

Recommendation: **JetBrains Mono** — it has the best coverage for our Unicode symbols (`▸`, `✓`, `○`, `●`, `◉`, box-drawing chars, `━`, `┬`, `│`) and is designed for exactly this use case.

The font file (~200KB) gets compiled into the test binary via `include_bytes!`. Only test code references it, so it doesn't affect the release binary size.

### Implementation

Add a new module `cli/src/tui/render_png.rs`:

```rust
use ab_glyph::{FontRef, PxScale};
use image::{Rgba, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

const FONT_BYTES: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");
const CELL_WIDTH: u32 = 10;   // pixels per character column
const CELL_HEIGHT: u32 = 20;  // pixels per character row
const BG_COLOR: Rgba<u8> = Rgba([30, 30, 36, 255]); // dark terminal background

pub fn buffer_to_png(buf: &Buffer, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let font = FontRef::try_from_slice(FONT_BYTES)?;
    let scale = PxScale::from(CELL_HEIGHT as f32 * 0.8);

    let width = buf.area.width as u32 * CELL_WIDTH;
    let height = buf.area.height as u32 * CELL_HEIGHT;
    let mut img = RgbaImage::from_pixel(width, height, BG_COLOR);

    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let fg = color_to_rgba(cell.fg, Rgba([180, 180, 180, 255])); // default fg
            let bg = color_to_rgba(cell.bg, BG_COLOR);
            let dim = cell.modifier.contains(Modifier::DIM);

            // Fill cell background
            fill_rect(&mut img, x as u32 * CELL_WIDTH, y as u32 * CELL_HEIGHT,
                       CELL_WIDTH, CELL_HEIGHT, bg);

            // Draw glyph
            let fg = if dim { dim_color(fg) } else { fg };
            draw_glyph(&mut img, &font, scale, cell.symbol(),
                       x as u32 * CELL_WIDTH, y as u32 * CELL_HEIGHT, fg);
        }
    }

    img.save(path)?;
    Ok(())
}

fn color_to_rgba(c: Color, default: Rgba<u8>) -> Rgba<u8> {
    match c {
        Color::Rgb(r, g, b) => Rgba([r, g, b, 255]),
        Color::Reset => default,
        _ => default,
    }
}
```

### Test Integration

Add `render_to_png()` alongside existing `render_to_text()` in the snapshot test file:

```rust
fn render_to_png(widget: impl Widget, width: u16, height: u16, name: &str) {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    let path = PathBuf::from("tests/snapshots").join(format!("{name}.png"));
    buffer_to_png(&buf, &path).expect("Failed to render PNG");
}
```

Each snapshot test then produces both:
- `task_list_mixed.plain.txt` — text for fast `diff`-based regression
- `task_list_mixed.annotated.txt` — color annotations for automated checking
- `task_list_mixed.png` — **actual visual screenshot** for human/agent review

---

## File Changes

| File | What Changes |
|------|-------------|
| `cli/Cargo.toml` | Add `image` and `ab_glyph` as dev-dependencies |
| `cli/assets/JetBrainsMono-Regular.ttf` | Bundled font file (~200KB) |
| `cli/src/tui/render_png.rs` | New module: `buffer_to_png()` function |
| `cli/src/tui/mod.rs` | Expose `render_png` module (cfg test or feature-gated) |
| `cli/tests/tui_snapshot_tests.rs` | Add `render_to_png()` helper, call from each test |
| `.gitattributes` | Mark `*.ttf` as binary, mark `tests/snapshots/*.png` as binary |

---

## Phases

### Phase 1: Core renderer

1. Add `image` + `ab_glyph` as dev-dependencies
2. Download and bundle JetBrains Mono font
3. Implement `buffer_to_png()` — basic glyph rendering with fg/bg colors
4. Handle modifiers: bold (brighter), dim (darker), underline (draw line)
5. Test with the existing `snapshot_task_list_mixed` test case
6. Visually verify the output PNG matches expectations

### Phase 2: Wire into all snapshot tests

1. Add `render_to_png()` calls to all 4 existing tests
2. As new tests from ux-with-testing.md are added, they automatically get PNG output
3. Add PNGs to `.gitignore` or check them in (TBD — checking in enables PR review diffs)

### Phase 3: Agent workflow integration

1. After making TUI changes, agent runs `cargo test --test tui_snapshot_tests`
2. Agent reads the PNG files using the `Read` tool (which supports images)
3. Agent can visually verify the TUI looks correct before committing
4. If something looks wrong, agent can iterate without needing a human to check

---

## Future: Interactive Terminal Control (VHS)

Once static screenshots work, the next level is **interactive terminal testing** — actually running the TUI, sending keystrokes, and capturing the result. This is a separate effort but worth noting the path:

### VHS (Charmbracelet)

[VHS](https://github.com/charmbracelet/vhs) records terminal sessions from a script:

```tape
Output demo.gif
Set Width 80
Set Height 24
Set FontFamily "JetBrains Mono"
Set Theme "Ayu Dark"

Type "aiki task"
Enter
Sleep 1s
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Screenshot demo_after_nav.png
Enter
Sleep 1s
Screenshot task_detail.png
```

This would let the agent:
1. Write a `.tape` script describing an interaction
2. Run `vhs script.tape`
3. Read the output PNGs to verify interactive behavior (navigation, screen transitions)

**Dependencies:** VHS requires Go + a headless browser (for font rendering). Heavier than the buffer-to-PNG approach but captures things it can't — real input handling, actual terminal rendering, timing.

### tmux-based approach (lighter weight)

For environments where VHS is too heavy:

```bash
# Start headless tmux session
tmux new-session -d -s test -x 80 -y 24

# Run the TUI
tmux send-keys -t test 'aiki task' Enter
sleep 1

# Send keystrokes
tmux send-keys -t test j
tmux send-keys -t test j
tmux send-keys -t test Enter

# Capture ANSI output
tmux capture-pane -t test -e -p > /tmp/capture.ansi

# Convert ANSI to image (using our buffer_to_png or an external tool)
```

This could reuse the buffer-to-PNG renderer by parsing the ANSI escape sequences back into a ratatui-like buffer, then rendering.

---

## Decision Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Font | JetBrains Mono | Best Unicode coverage for terminal symbols, OFL licensed |
| Image format | PNG | Lossless, universally supported, small for terminal screenshots |
| Rendering approach | Rust-native (ab_glyph + image) | No system deps, works in CI, integrates with existing test harness |
| Font bundling | `include_bytes!` in test binary | Zero runtime deps, font ships with tests |
| Dev-dependency only | Yes | No impact on release binary size |
| Start with static, add interactive later | Yes | Static screenshots solve 80% of the problem with 20% of the effort |

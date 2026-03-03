# TUI System

Ratatui-based terminal UI for aiki. Renders styled output using character-cell buffers with 24-bit color, then converts to ANSI escape sequences for terminal display.

## Architecture

```
types.rs          Data models (WorkflowView, EpicView, StageView, etc.)
theme.rs          Color palette (dark/light) + symbols (✓ ▸ ○ ✗) + auto-detection
buffer_ansi.rs    Buffer → ANSI string converter (production output path)
render_png.rs     Buffer → PNG renderer (test-only, behind #[cfg(test)])
builder.rs        Converts Task objects → TUI data models

views/            Composers — assemble widgets into full screens, return Buffer
  workflow.rs     PathLine + EpicTree + StageList + LaneDag
  epic_show.rs    PathLine + EpicTree + StageTrack

widgets/          Leaf renderers — implement ratatui Widget trait
  path_line.rs    File path with dim directory, normal filename
  epic_tree.rs    Epic header + subtask tree with ⎿ connectors
  stage_list.rs   Vertical stage list with sub-stages and children
  stage_track.rs  Horizontal build/review/fix phase bar
  lane_dag.rs     Parallel session DAG (●━┬━◉)
```

## How it works

1. **Build data model** — `builder.rs` converts `Task` objects into typed view structs (`WorkflowView`, etc.)
2. **Render to Buffer** — A view function composes widgets into a `ratatui::Buffer` (fixed 80-column width)
3. **Emit to terminal** — `buffer_to_ansi()` walks the buffer and emits 24-bit ANSI escapes

The Buffer is the single source of truth — both terminal output and test PNGs render from the same Buffer.

## Theme

Two modes: dark and light. Auto-detected via `terminal-colorsaurus` (OSC 11), overridable with `AIKI_THEME=light|dark`.

**Semantic colors:** green (done), cyan (info), yellow (active), red (failed), magenta (cursor agent), dim (borders), fg (supporting text), text (primary), hi (headers, bold).

**Symbols (mode-independent):** `✓` check, `▸` running, `○` pending, `✗` failed.

Widgets accept `&Theme` and use `theme.dim_style()`, `theme.text_style()`, etc. Never hard-code colors.

## Testing views

Every view and widget has unit tests using buffer text assertions. The pattern:

```rust
// 1. Create theme and data
let theme = Theme::dark();

// 2. Render to buffer
let buf = render_my_view(&data, &theme);

// 3. Extract text for assertions
let text = buf_text(&buf);  // all rows joined
let line = buf_line(&buf, 0);  // single row, trimmed

// 4. Assert content
assert!(text.contains("expected text"));
assert!(line.contains("✓ build"));

// 5. Assert styling (check specific cell colors)
let cell = buf.cell((x, y)).unwrap();
assert_eq!(cell.style().fg, Some(theme.green));
```

### PNG snapshots

Integration tests in `cli/tests/tui_snapshot_tests.rs` render both dark and light PNGs for visual inspection:

```rust
save_png(&buf, "my_view_dark", &theme);  // → cli/tests/snapshots/my_view_dark.png
```

PNGs are gitignored — they're generated artifacts, not source of truth. The font is JetBrains Mono, bundled at `cli/assets/JetBrainsMono-Regular.ttf` (test-only via `include_bytes!`).

Run snapshots: `cargo test --test tui_snapshot_tests`

Then read the PNG to visually verify: the mockup IS the implementation — what the PNG shows is exactly what the terminal renders.

## Mockup format

When designing new views, use annotated ASCII art. Left side is literal terminal output (character-accurate). Right side after `←` is a style annotation referencing Theme field names.

```
[80 cols]
 [luppzupt] Implement webhooks              ← dim brackets, hi+bold name
                                             ← blank row
 ⎿ ✓ Explore webhook requirements    cc  8s ← dim ⎿, green ✓, text name, cyan cc, dim 8s
 ⎿ ▸ Implement route handler        cur     ← yellow ▸, magenta cur
 ⎿ ○ Write integration tests                ← dim ○, dim name

 ▸ build  3/6  0:34                          ← yellow ▸, yellow text
    ✓ decompose  0:12                        ← green ✓, 4-char indent
    ▸ implement  3/6  0:34                   ← yellow ▸, 4-char indent
 ○ review                                    ← dim ○, dim text
```

Rules:
- Monospace, character-accurate positioning
- 1 char left padding on all content lines
- `⎿` for tree connectors (dim)
- Right-aligned metadata (agent badge, elapsed) flush to column 80
- Annotations are for the implementer, never rendered
- Include `[width]` when layout depends on terminal size

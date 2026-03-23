# Step 1c: Shared Components + Line Renderer

**Date**: 2026-03-22
**Status**: Ready
**Priority**: P1
**Phase**: 1 — Foundation
**Depends on**: 1b (needs Line/LineStyle types)

---

## Problem

`pipeline_chat.rs` (1110 lines) renders the generic `Chat` model with a large `render_message_line` / `render_subtask_line` / `render_block_line` switch. These rendering functions are entangled with the generic data model.

With the line-based model from step 1b, we need two things:
1. **Components** — functions that produce `Vec<Line>` for common visual patterns (phase headers, subtask tables, lane blocks, etc.)
2. **Line renderer** — a function that takes `Vec<Line>` and renders them into a ratatui `Buffer`

---

## Fix: Components + line renderer

### New file: `cli/src/tui/components.rs`

Reusable view components. Each returns `Vec<Line>`:

```rust
/// Phase header + child lines.
///
///   ⠹ name (agent)          ← spinner when active, 合 when done
///   ⎿ child text                                              elapsed
///
pub fn phase(group: u16, name: &str, agent: Option<&str>, children: Vec<ChildLine>) -> Vec<Line>;

pub struct ChildLine {
    pub text: String,
    pub meta: Option<String>,
    pub style: ChildStyle,
}

pub enum ChildStyle {
    Active,   // yellow (heartbeat, loading step)
    Done,     // ✓ prefix, dim
    Error,    // ✗ prefix, red
    Normal,   // dim (target line, counts)
}

/// Subtask table between --- separators.
pub fn subtask_table(
    group: u16,
    short_id: &str,
    title: &str,
    subtasks: &[SubtaskData],
    loading: bool,
) -> Vec<Line>;

pub struct SubtaskData {
    pub name: String,
    pub status: SubtaskStatus,
    pub elapsed: Option<String>,
}

/// Lane blocks under a loop phase.
pub fn loop_block(group: u16, lanes: &[LaneData]) -> Vec<Line>;

pub struct LaneData {
    pub number: usize,
    pub agent: String,
    pub completed: usize,
    pub total: usize,
    pub failed: usize,
    pub heartbeat: Option<String>,
    pub elapsed: Option<String>,
    pub shutdown: bool,
}

/// Numbered issue list.
pub fn issues(group: u16, issues: &[String]) -> Vec<Line>;

/// Section header (no 合 prefix): "Initial Build", "Iteration 2"
pub fn section_header(text: &str) -> Vec<Line>;

/// Blank line.
pub fn blank() -> Vec<Line>;
```

### Line renderer: `cli/src/tui/render.rs`

Takes `&[Line]` and renders into a ratatui `Buffer`:

```rust
/// Render lines into a ratatui buffer.
pub fn render_lines(lines: &[Line], buf: &mut Buffer, area: Rect, theme: &Theme);

/// Calculate the height needed.
pub fn lines_height(lines: &[Line]) -> u16;
```

The renderer iterates lines and applies styles based on `LineStyle`:

| LineStyle | Rendering |
|-----------|-----------|
| `PhaseHeader` | spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` yellow (active) / `合` dim (done), space, name bold fg, space, `(agent)` dim |
| `Child` | indent, `⎿` dim, space, text dim |
| `ChildActive` | indent, `⎿` dim, space, text yellow |
| `ChildDone` | indent, `⎿` dim, space, `✓` green, space, text dim |
| `ChildError` | indent, `⎿` dim, space, `✗` red, space, text red |
| `Subtask { status }` | indent, status icon (colored), space, name, right-aligned elapsed dim |
| `Separator` | `---` dim |
| `SectionHeader` | text bold fg |
| `Issue` | indent, `N.` fg, space, text fg |
| `Dim` | indent, text dim |
| `Blank` | empty line |

For all styles, `meta` (if present) is right-aligned and dim.

### Progressive dimming

Applied before rendering:

```rust
pub fn apply_dimming(lines: &mut [Line]) {
    let active_group = lines.iter()
        .filter(|l| matches!(l.style, LineStyle::ChildActive))
        .map(|l| l.group)
        .max();

    if let Some(active) = active_group {
        for line in lines.iter_mut() {
            if line.group < active {
                line.dimmed = true;
            }
        }
    }
}
```

**Exception:** Issue lines (`LineStyle::Issue`) are never dimmed — the user needs to read them.

**Invariant:** Groups are monotonically increasing, and the "active group" is always the highest-numbered one with an active child. This means only one phase can be "active" at a time — earlier phases are dimmed, later phases haven't started. This invariant holds for all current flows (build, review, fix) because pipeline phases are strictly sequential. If we ever need two concurrent active phases (e.g., review running while a new iteration starts), this model breaks and we'd need an explicit `PhaseState { Active, Completed }` rather than relying on scan order. Document this assumption in the code with a comment on `apply_dimming()`.

### What this replaces

| Old | New |
|-----|-----|
| `pipeline_chat.rs` (1110 lines) | `render.rs` (~80 lines) + `components.rs` (~120 lines) |
| `render_message_line()` | `components::phase()` |
| `render_subtask_line()` | `components::subtask_table()` |
| `render_block_line()` / `render_block_footer()` | `components::loop_block()` |
| `render_error_line()` | `ChildStyle::Error` in `components::phase()` |
| `chat_height()` | `lines_height()` — just `lines.len()` |
| `widgets/stage_track.rs` (490 lines) | Not needed |
| `widgets/epic_tree.rs` (538 lines) | `components::subtask_table()` |
| `widgets/breadcrumb.rs` (188 lines) | Not needed |
| `widgets/path_line.rs` (200 lines) | `ChildStyle::Done` in `components::phase()` |

### Defensive rendering

Guard against out-of-bounds when rendering to sub-areas (ratatui best practice):

```rust
pub fn render_lines(lines: &[Line], buf: &mut Buffer, area: Rect, theme: &Theme) {
    let area = area.intersection(*buf.area());
    if area.is_empty() {
        return;
    }
    // ... render lines
}
```

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/components.rs` | New (~120 lines) |
| `cli/src/tui/render.rs` | New (~80 lines) |
| `cli/src/tui/mod.rs` | Add `pub mod components;` and `pub mod render;` |

### Tests

**Component tests** (pure function → lines, no terminal):

```rust
#[test]
fn phase_produces_header_and_children() {
    let lines = phase(0, "plan", Some("claude"), vec![
        ChildLine { text: "✓ path.md".into(), style: ChildStyle::Done, meta: None },
    ]);
    assert_eq!(lines.len(), 2);
    assert!(matches!(lines[0].style, LineStyle::PhaseHeader));
    assert!(matches!(lines[1].style, LineStyle::ChildDone));
}

#[test]
fn subtask_table_with_loading_shows_placeholder() {
    let lines = subtask_table(0, "lkji3d", "Epic: Test", &[], true);
    assert!(lines.iter().any(|l| l.text.contains("...")));
}

#[test]
fn loop_block_produces_two_child_lines_per_lane() {
    let lanes = vec![LaneData { number: 1, agent: "claude".into(), completed: 1, total: 3, .. }];
    let lines = loop_block(0, &lanes);
    // Phase header + lane header + count line + heartbeat line
    assert!(lines.len() >= 4);
}
```

**Buffer-level render tests** (render lines into Buffer, assert on cells):

```rust
#[test]
fn render_phase_header_has_correct_symbol() {
    let lines = phase(0, "plan", Some("claude"), vec![]);
    let area = Rect::new(0, 0, 80, 5);
    let mut buf = Buffer::empty(area);
    render_lines(&lines, &mut buf, area, &Theme::dark());
    // First cell should be 合
    assert_eq!(buf[(0, 0)].symbol(), "合");
}

#[test]
fn render_respects_area_bounds() {
    let lines: Vec<Line> = vec![/* 20 lines */];
    let area = Rect::new(0, 0, 80, 5); // Only 5 rows
    let mut buf = Buffer::empty(area);
    // Should not panic
    render_lines(&lines, &mut buf, area, &Theme::dark());
}
```

**Dimming tests** (pure function):

```rust
#[test]
fn apply_dimming_dims_groups_before_active() {
    let mut lines = vec![
        Line { group: 0, style: LineStyle::Child, dimmed: false, .. },
        Line { group: 1, style: LineStyle::ChildActive, dimmed: false, .. },
    ];
    apply_dimming(&mut lines);
    assert!(lines[0].dimmed);
    assert!(!lines[1].dimmed);
}

#[test]
fn apply_dimming_preserves_issue_lines() {
    let mut lines = vec![
        Line { group: 0, style: LineStyle::Issue, dimmed: false, .. },
        Line { group: 1, style: LineStyle::ChildActive, dimmed: false, .. },
    ];
    apply_dimming(&mut lines);
    assert!(!lines[0].dimmed); // Issues never dim
}
```

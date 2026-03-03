//! Vertical stage list widget.
//!
//! Renders workflow stages as a vertical list with one line per stage,
//! indented sub-stages and children when expanded.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::tui::theme::{Theme, SYM_CHECK, SYM_FAILED, SYM_PENDING, SYM_RUNNING, SYM_STARTING};
use crate::tui::types::{FixChild, StageChild, StageState, StageView, SubStageView, SubtaskStatus};

/// Vertical stage list widget.
///
/// Renders each stage on its own line with symbol, name, progress, and elapsed.
/// Active and failed group stages expand to show sub-stages; active stages with
/// children expand to show child lines.
pub struct StageList<'a> {
    stages: &'a [StageView],
    theme: &'a Theme,
}

impl<'a> StageList<'a> {
    pub fn new(stages: &'a [StageView], theme: &'a Theme) -> Self {
        Self { stages, theme }
    }

    /// Compute the total height this widget will consume.
    pub fn height(&self) -> u16 {
        self.stages.iter().map(|s| stage_height(s)).sum()
    }
}

/// Whether a stage should show its sub-stages/children expanded.
fn is_expanded(stage: &StageView) -> bool {
    match stage.state {
        StageState::Starting | StageState::Active | StageState::Failed => {
            !stage.sub_stages.is_empty() || !stage.children.is_empty()
        }
        _ => false,
    }
}

/// How many lines a stage will occupy.
fn stage_height(stage: &StageView) -> u16 {
    if !is_expanded(stage) {
        return 1;
    }
    1 + stage.sub_stages.len() as u16 + stage.children.len() as u16
}

/// Symbol and color for a stage state.
fn state_symbol_color(state: StageState, theme: &Theme) -> (&'static str, Style) {
    match state {
        StageState::Pending => (SYM_PENDING, Style::default().fg(theme.dim)),
        StageState::Starting => (SYM_STARTING, Style::default().fg(theme.yellow)),
        StageState::Active => (SYM_RUNNING, Style::default().fg(theme.yellow)),
        StageState::Done => (SYM_CHECK, Style::default().fg(theme.green)),
        StageState::Failed => (SYM_FAILED, Style::default().fg(theme.red)),
    }
}

/// Symbol and color for a subtask status.
fn subtask_symbol_color(status: SubtaskStatus, theme: &Theme) -> (&'static str, Style) {
    match status {
        SubtaskStatus::Pending => (SYM_PENDING, Style::default().fg(theme.dim)),
        SubtaskStatus::Starting => (SYM_STARTING, Style::default().fg(theme.yellow)),
        SubtaskStatus::Active => (SYM_RUNNING, Style::default().fg(theme.yellow)),
        SubtaskStatus::Done => (SYM_CHECK, Style::default().fg(theme.green)),
        SubtaskStatus::Failed => (SYM_FAILED, Style::default().fg(theme.red)),
    }
}

/// Style for agent badge text.
fn agent_style(agent: &str, theme: &Theme) -> Style {
    match agent {
        "claude" => Style::default().fg(theme.cyan),
        "cursor" => Style::default().fg(theme.magenta),
        _ => Style::default().fg(theme.fg),
    }
}

/// Render a top-level stage line: ` symbol  name  [progress]  [elapsed]  [extra]`
fn render_stage_line(
    stage: &StageView,
    theme: &Theme,
    area_x: u16,
    y: u16,
    max_x: u16,
    buf: &mut Buffer,
) {
    let (sym, style) = state_symbol_color(stage.state, theme);
    let mut x = area_x.saturating_add(1); // 1 char left padding

    // Symbol
    if x >= max_x { return; }
    buf.set_string(x, y, sym, style);
    x = x.saturating_add(sym.chars().count() as u16).saturating_add(1); // sym + space

    // Name
    if x >= max_x { return; }
    buf.set_string(x, y, &stage.name, style);
    x = x.saturating_add(stage.name.chars().count() as u16);

    // Progress
    if let Some(ref prog) = stage.progress {
        x = x.saturating_add(2); // two spaces
        if x < max_x {
            buf.set_string(x, y, prog, style);
            x = x.saturating_add(prog.chars().count() as u16);
        }
    }

    // Elapsed
    if let Some(ref elapsed) = stage.elapsed {
        x = x.saturating_add(2);
        if x < max_x {
            buf.set_string(x, y, elapsed, style);
        }
    }
}

/// Render an indented sub-stage line: `    symbol  name  [progress]  [elapsed]`
fn render_sub_stage_line(
    sub: &SubStageView,
    theme: &Theme,
    area_x: u16,
    y: u16,
    max_x: u16,
    buf: &mut Buffer,
) {
    let (sym, style) = state_symbol_color(sub.state, theme);
    let mut x = area_x.saturating_add(4); // 4 char indent

    if x >= max_x { return; }
    buf.set_string(x, y, sym, style);
    x = x.saturating_add(sym.chars().count() as u16).saturating_add(1);

    if x >= max_x { return; }
    buf.set_string(x, y, &sub.name, style);
    x = x.saturating_add(sub.name.chars().count() as u16);

    if let Some(ref prog) = sub.progress {
        x = x.saturating_add(2);
        if x < max_x {
            buf.set_string(x, y, prog, style);
            x = x.saturating_add(prog.chars().count() as u16);
        }
    }

    if let Some(ref elapsed) = sub.elapsed {
        x = x.saturating_add(2);
        if x < max_x {
            buf.set_string(x, y, elapsed, style);
        }
    }
}

/// Render an indented child line (subtask or review-fix).
/// Agent badge and elapsed are right-aligned.
fn render_child_line(
    child: &StageChild,
    theme: &Theme,
    area_x: u16,
    y: u16,
    area_width: u16,
    buf: &mut Buffer,
) {
    let max_x = area_x.saturating_add(area_width);
    let mut x = area_x.saturating_add(4); // 4 char indent

    match child {
        StageChild::Subtask(sub) => {
            let (sym, style) = subtask_symbol_color(sub.status, theme);

            if x >= max_x { return; }
            buf.set_string(x, y, sym, style);
            x = x.saturating_add(sym.chars().count() as u16).saturating_add(1);

            if x >= max_x { return; }
            buf.set_string(x, y, &sub.name, style);

            // Right-aligned: agent + elapsed
            let right_edge = max_x;
            let mut rx = right_edge;

            if let Some(ref elapsed) = sub.elapsed {
                let ew = elapsed.chars().count() as u16;
                rx = rx.saturating_sub(ew).saturating_sub(1);
                let dim = theme.dim_style();
                buf.set_string(rx, y, elapsed, dim);
            }

            if let Some(ref agent) = sub.agent {
                let aw = agent.chars().count() as u16;
                rx = rx.saturating_sub(aw).saturating_sub(1);
                buf.set_string(rx, y, agent, agent_style(agent, theme));
            }
        }
        StageChild::Fix(fix_child) => {
            render_fix_child(fix_child, theme, x, y, max_x, buf);
        }
    }
}

/// Render a FixChild (either a subtask or a review-fix gate).
fn render_fix_child(
    fix: &FixChild,
    theme: &Theme,
    x: u16,
    y: u16,
    max_x: u16,
    buf: &mut Buffer,
) {
    match fix {
        FixChild::Subtask(sub) => {
            let (sym, style) = subtask_symbol_color(sub.status, theme);
            let mut cx = x;

            if cx >= max_x { return; }
            buf.set_string(cx, y, sym, style);
            cx = cx.saturating_add(sym.chars().count() as u16).saturating_add(1);

            if cx >= max_x { return; }
            buf.set_string(cx, y, &sub.name, style);

            // Right-aligned: agent + elapsed
            let mut rx = max_x;

            if let Some(ref elapsed) = sub.elapsed {
                let ew = elapsed.chars().count() as u16;
                rx = rx.saturating_sub(ew).saturating_sub(1);
                let dim = theme.dim_style();
                buf.set_string(rx, y, elapsed, dim);
            }

            if let Some(ref agent) = sub.agent {
                let aw = agent.chars().count() as u16;
                rx = rx.saturating_sub(aw).saturating_sub(1);
                buf.set_string(rx, y, agent, agent_style(agent, theme));
            }
        }
        FixChild::ReviewFix { number, state, result, agent, elapsed } => {
            let (sym, style) = state_symbol_color(*state, theme);
            let mut cx = x;

            if cx >= max_x { return; }
            buf.set_string(cx, y, sym, style);
            cx = cx.saturating_add(sym.chars().count() as u16).saturating_add(1);

            // Name with optional numbering: "review fix" or "review fix #1"
            let name = match number {
                Some(n) => format!("review fix #{}", n),
                None => "review fix".to_string(),
            };
            if cx >= max_x { return; }
            buf.set_string(cx, y, &name, style);
            cx = cx.saturating_add(name.chars().count() as u16);

            // Result text (e.g., "1 issue", "approved")
            if let Some(ref result_text) = result {
                cx = cx.saturating_add(2); // two spaces
                if cx < max_x {
                    buf.set_string(cx, y, result_text, style);
                }
            }

            // Right-aligned: agent + elapsed
            let mut rx = max_x;

            if let Some(ref elapsed) = elapsed {
                let ew = elapsed.chars().count() as u16;
                rx = rx.saturating_sub(ew).saturating_sub(1);
                let dim = theme.dim_style();
                buf.set_string(rx, y, elapsed, dim);
            }

            if let Some(ref agent) = agent {
                let aw = agent.chars().count() as u16;
                rx = rx.saturating_sub(aw).saturating_sub(1);
                buf.set_string(rx, y, agent, agent_style(agent, theme));
            }
        }
    }
}

impl Widget for StageList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);
        let mut y = area.y;

        for stage in self.stages {
            if y >= max_y {
                break;
            }

            // Main stage line
            render_stage_line(stage, self.theme, area.x, y, max_x, buf);
            y += 1;

            // Expanded sub-stages and children
            if is_expanded(stage) {
                for sub in &stage.sub_stages {
                    if y >= max_y { break; }
                    render_sub_stage_line(sub, self.theme, area.x, y, max_x, buf);
                    y += 1;
                }

                for child in &stage.children {
                    if y >= max_y { break; }
                    render_child_line(child, self.theme, area.x, y, area.width, buf);
                    y += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::{SubtaskLine, SubtaskStatus};

    fn test_theme() -> Theme {
        Theme::dark()
    }

    fn buf_lines(buf: &Buffer, height: u16, width: u16) -> Vec<String> {
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        buf.cell((x, y))
                            .map(|c| {
                                let s = c.symbol();
                                if s.is_empty() { " ".to_string() } else { s.to_string() }
                            })
                            .unwrap_or_default()
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn render_stages(stages: &[StageView], width: u16) -> (Buffer, Vec<String>) {
        let theme = test_theme();
        let widget = StageList::new(stages, &theme);
        let h = widget.height().max(1);
        let area = Rect::new(0, 0, width, h);
        let mut buf = Buffer::empty(area);
        StageList::new(stages, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, h, width);
        (buf, lines)
    }

    #[test]
    fn pending_stage() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Pending,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let (_, lines) = render_stages(&stages, 40);
        assert!(lines[0].contains("○ review"), "got: {}", lines[0]);
    }

    #[test]
    fn active_stage_no_substages() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Active,
            progress: None,
            elapsed: Some("0:14".into()),
            sub_stages: vec![],
            children: vec![],
        }];
        let (_, lines) = render_stages(&stages, 40);
        assert!(lines[0].contains("▸ review"), "got: {}", lines[0]);
        assert!(lines[0].contains("0:14"), "got: {}", lines[0]);
    }

    #[test]
    fn active_group_stage_expanded() {
        let stages = vec![StageView {
            name: "build".into(),
            state: StageState::Active,
            progress: None,
            elapsed: None,
            sub_stages: vec![
                SubStageView {
                    name: "decompose".into(),
                    state: StageState::Done,
                    progress: None,
                    elapsed: Some("0:12".into()),
                },
                SubStageView {
                    name: "implement".into(),
                    state: StageState::Active,
                    progress: Some("3/6".into()),
                    elapsed: Some("0:34".into()),
                },
            ],
            children: vec![],
        }];
        let (_, lines) = render_stages(&stages, 40);
        assert!(lines[0].contains("▸ build"), "line0: {}", lines[0]);
        assert!(lines[1].contains("✓ decompose"), "line1: {}", lines[1]);
        assert!(lines[1].contains("0:12"), "line1: {}", lines[1]);
        assert!(lines[2].contains("▸ implement"), "line2: {}", lines[2]);
        assert!(lines[2].contains("3/6"), "line2: {}", lines[2]);
        assert!(lines[2].contains("0:34"), "line2: {}", lines[2]);
    }

    #[test]
    fn done_group_stage_collapsed() {
        let stages = vec![StageView {
            name: "build".into(),
            state: StageState::Done,
            progress: Some("6/6".into()),
            elapsed: Some("2m40s".into()),
            sub_stages: vec![
                SubStageView {
                    name: "decompose".into(),
                    state: StageState::Done,
                    progress: None,
                    elapsed: Some("0:12".into()),
                },
                SubStageView {
                    name: "implement".into(),
                    state: StageState::Done,
                    progress: Some("6/6".into()),
                    elapsed: Some("2m28s".into()),
                },
            ],
            children: vec![],
        }];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        assert_eq!(widget.height(), 1, "done group stage should be collapsed");
        let (_, lines) = render_stages(&stages, 60);
        assert!(lines[0].contains("✓ build"), "line0: {}", lines[0]);
        assert!(lines[0].contains("6/6"), "line0: {}", lines[0]);
        assert!(lines[0].contains("2m40s"), "line0: {}", lines[0]);
    }

    #[test]
    fn done_stage_with_result() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Done,
            progress: Some("2 issues".into()),
            elapsed: Some("0:42".into()),
            sub_stages: vec![],
            children: vec![],
        }];
        let (_, lines) = render_stages(&stages, 60);
        assert!(lines[0].contains("✓ review"), "line0: {}", lines[0]);
        assert!(lines[0].contains("2 issues"), "line0: {}", lines[0]);
        assert!(lines[0].contains("0:42"), "line0: {}", lines[0]);
    }

    #[test]
    fn failed_stage_expanded() {
        let stages = vec![StageView {
            name: "build".into(),
            state: StageState::Failed,
            progress: None,
            elapsed: None,
            sub_stages: vec![
                SubStageView {
                    name: "decompose".into(),
                    state: StageState::Done,
                    progress: None,
                    elapsed: Some("0:12".into()),
                },
                SubStageView {
                    name: "implement".into(),
                    state: StageState::Failed,
                    progress: Some("8/10".into()),
                    elapsed: Some("2m48s".into()),
                },
            ],
            children: vec![],
        }];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        assert_eq!(widget.height(), 3, "failed group stage should be expanded");
        let (_, lines) = render_stages(&stages, 60);
        assert!(lines[0].contains("✗ build"), "line0: {}", lines[0]);
        assert!(lines[1].contains("✓ decompose"), "line1: {}", lines[1]);
        assert!(lines[2].contains("✗ implement"), "line2: {}", lines[2]);
        assert!(lines[2].contains("8/10"), "line2: {}", lines[2]);
        assert!(lines[2].contains("2m48s"), "line2: {}", lines[2]);
    }

    #[test]
    fn fix_stage_with_children() {
        let stages = vec![StageView {
            name: "fix".into(),
            state: StageState::Active,
            progress: Some("1/2".into()),
            elapsed: Some("0:18".into()),
            sub_stages: vec![],
            children: vec![
                StageChild::Subtask(SubtaskLine {
                    name: "Fix: Missing null check (auth.rs)".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("cursor".into()),
                    elapsed: Some("12s".into()),
                    error: None,
                }),
                StageChild::Subtask(SubtaskLine {
                    name: "Fix: Error message format".into(),
                    status: SubtaskStatus::Active,
                    agent: Some("claude".into()),
                    elapsed: None,
                    error: None,
                }),
            ],
        }];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        assert_eq!(widget.height(), 3);
        let (_, lines) = render_stages(&stages, 80);
        assert!(lines[0].contains("▸ fix"), "line0: {}", lines[0]);
        assert!(lines[0].contains("1/2"), "line0: {}", lines[0]);
        assert!(lines[1].contains("✓ Fix: Missing null check (auth.rs)"), "line1: {}", lines[1]);
        assert!(lines[1].contains("cursor"), "line1 agent: {}", lines[1]);
        assert!(lines[1].contains("12s"), "line1 elapsed: {}", lines[1]);
        assert!(lines[2].contains("▸ Fix: Error message format"), "line2: {}", lines[2]);
        assert!(lines[2].contains("claude"), "line2 agent: {}", lines[2]);
    }

    #[test]
    fn fix_with_review_fix_gate() {
        let stages = vec![StageView {
            name: "fix".into(),
            state: StageState::Active,
            progress: Some("2/2".into()),
            elapsed: Some("0:38".into()),
            sub_stages: vec![],
            children: vec![
                StageChild::Subtask(SubtaskLine {
                    name: "Fix: Missing null check".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("cursor".into()),
                    elapsed: Some("12s".into()),
                    error: None,
                }),
                StageChild::Subtask(SubtaskLine {
                    name: "Fix: Error message format".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("claude".into()),
                    elapsed: Some("8s".into()),
                    error: None,
                }),
                StageChild::Fix(FixChild::ReviewFix {
                    number: None,
                    state: StageState::Active,
                    result: None,
                    agent: Some("claude".into()),
                    elapsed: None,
                }),
            ],
        }];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        assert_eq!(widget.height(), 4);
        let (_, lines) = render_stages(&stages, 80);
        assert!(lines[0].contains("▸ fix"), "line0: {}", lines[0]);
        assert!(lines[1].contains("✓ Fix: Missing null check"), "line1: {}", lines[1]);
        assert!(lines[2].contains("✓ Fix: Error message format"), "line2: {}", lines[2]);
        assert!(lines[3].contains("▸ review fix"), "line3: {}", lines[3]);
        assert!(lines[3].contains("claude"), "line3 agent: {}", lines[3]);
    }

    #[test]
    fn colors_pending_dim() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Pending,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        // Symbol at x=1 should be dim
        let cell = buf.cell((1, 0)).unwrap();
        assert_eq!(cell.symbol(), SYM_PENDING);
        assert_eq!(cell.style().fg, Some(theme.dim));
    }

    #[test]
    fn colors_active_yellow() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Active,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        let cell = buf.cell((1, 0)).unwrap();
        assert_eq!(cell.symbol(), SYM_RUNNING);
        assert_eq!(cell.style().fg, Some(theme.yellow));
    }

    #[test]
    fn colors_done_green() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Done,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        let cell = buf.cell((1, 0)).unwrap();
        assert_eq!(cell.symbol(), SYM_CHECK);
        assert_eq!(cell.style().fg, Some(theme.green));
    }

    #[test]
    fn colors_failed_red() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Failed,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        let cell = buf.cell((1, 0)).unwrap();
        assert_eq!(cell.symbol(), SYM_FAILED);
        assert_eq!(cell.style().fg, Some(theme.red));
    }

    #[test]
    fn agent_badge_colors() {
        let stages = vec![StageView {
            name: "fix".into(),
            state: StageState::Active,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![
                StageChild::Subtask(SubtaskLine {
                    name: "Task A".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("claude".into()),
                    elapsed: None,
                    error: None,
                }),
                StageChild::Subtask(SubtaskLine {
                    name: "Task B".into(),
                    status: SubtaskStatus::Active,
                    agent: Some("cursor".into()),
                    elapsed: None,
                    error: None,
                }),
            ],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        // Find "claude" on line 1, should be cyan
        let claude_x = (0..60u16).find(|&x| {
            buf.cell((x, 1)).map(|c| c.symbol()) == Some("c")
                && buf.cell((x + 1, 1)).map(|c| c.symbol()) == Some("l")
                && buf.cell((x + 2, 1)).map(|c| c.symbol()) == Some("a")
                && buf.cell((x + 3, 1)).map(|c| c.symbol()) == Some("u")
                && buf.cell((x + 4, 1)).map(|c| c.symbol()) == Some("d")
                && buf.cell((x + 5, 1)).map(|c| c.symbol()) == Some("e")
        });
        if let Some(x) = claude_x {
            assert_eq!(buf.cell((x, 1)).unwrap().style().fg, Some(theme.cyan));
        }

        // Find "cursor" on line 2, should be magenta
        let cursor_x = (0..60u16).find(|&x| {
            buf.cell((x, 2)).map(|c| c.symbol()) == Some("c")
                && buf.cell((x + 1, 2)).map(|c| c.symbol()) == Some("u")
                && buf.cell((x + 2, 2)).map(|c| c.symbol()) == Some("r")
                && buf.cell((x + 3, 2)).map(|c| c.symbol()) == Some("s")
                && buf.cell((x + 4, 2)).map(|c| c.symbol()) == Some("o")
                && buf.cell((x + 5, 2)).map(|c| c.symbol()) == Some("r")
        });
        if let Some(x) = cursor_x {
            assert_eq!(buf.cell((x, 2)).unwrap().style().fg, Some(theme.magenta));
        }
    }

    #[test]
    fn zero_area_no_panic() {
        let stages = vec![StageView {
            name: "build".into(),
            state: StageState::Active,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn multiple_stages_pipeline() {
        let stages = vec![
            StageView {
                name: "build".into(),
                state: StageState::Done,
                progress: Some("6/6".into()),
                elapsed: Some("2m40s".into()),
                sub_stages: vec![],
                children: vec![],
            },
            StageView {
                name: "review".into(),
                state: StageState::Active,
                progress: None,
                elapsed: Some("0:14".into()),
                sub_stages: vec![],
                children: vec![],
            },
            StageView {
                name: "fix".into(),
                state: StageState::Pending,
                progress: None,
                elapsed: None,
                sub_stages: vec![],
                children: vec![],
            },
        ];
        let (_, lines) = render_stages(&stages, 60);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("✓ build"), "line0: {}", lines[0]);
        assert!(lines[0].contains("6/6"), "line0: {}", lines[0]);
        assert!(lines[1].contains("▸ review"), "line1: {}", lines[1]);
        assert!(lines[1].contains("0:14"), "line1: {}", lines[1]);
        assert!(lines[2].contains("○ fix"), "line2: {}", lines[2]);
    }

    #[test]
    fn height_calculation() {
        let stages = vec![
            StageView {
                name: "build".into(),
                state: StageState::Active,
                progress: None,
                elapsed: None,
                sub_stages: vec![
                    SubStageView { name: "decompose".into(), state: StageState::Done, progress: None, elapsed: None },
                    SubStageView { name: "implement".into(), state: StageState::Active, progress: None, elapsed: None },
                ],
                children: vec![],
            },
            StageView {
                name: "review".into(),
                state: StageState::Pending,
                progress: None,
                elapsed: None,
                sub_stages: vec![],
                children: vec![],
            },
        ];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        // build: 1 (header) + 2 (sub-stages) = 3, review: 1 = total 4
        assert_eq!(widget.height(), 4);
    }

    #[test]
    fn starting_subtask_shows_hourglass() {
        let stages = vec![StageView {
            name: "fix".into(),
            state: StageState::Active,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![
                StageChild::Subtask(SubtaskLine {
                    name: "Fix null check".into(),
                    status: SubtaskStatus::Starting,
                    agent: None,
                    elapsed: None,
                    error: None,
                }),
            ],
        }];
        let (_, lines) = render_stages(&stages, 80);
        assert!(lines[1].contains("⧗ Fix null check"), "line1: {}", lines[1]);
    }

    #[test]
    fn starting_stage_shows_hourglass() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Starting,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let (_, lines) = render_stages(&stages, 40);
        assert!(lines[0].contains("⧗ review"), "got: {}", lines[0]);
    }

    #[test]
    fn starting_stage_expanded() {
        let stages = vec![StageView {
            name: "build".into(),
            state: StageState::Starting,
            progress: None,
            elapsed: None,
            sub_stages: vec![
                SubStageView {
                    name: "decompose".into(),
                    state: StageState::Starting,
                    progress: None,
                    elapsed: None,
                },
            ],
            children: vec![],
        }];
        let theme = test_theme();
        let widget = StageList::new(&stages, &theme);
        assert_eq!(widget.height(), 2, "starting stage should be expanded");
        let (_, lines) = render_stages(&stages, 60);
        assert!(lines[0].contains("⧗ build"), "line0: {}", lines[0]);
        assert!(lines[1].contains("⧗ decompose"), "line1: {}", lines[1]);
    }

    #[test]
    fn colors_starting_yellow() {
        let stages = vec![StageView {
            name: "review".into(),
            state: StageState::Starting,
            progress: None,
            elapsed: None,
            sub_stages: vec![],
            children: vec![],
        }];
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StageList::new(&stages, &theme).render(area, &mut buf);

        let cell = buf.cell((1, 0)).unwrap();
        assert_eq!(cell.symbol(), SYM_STARTING);
        assert_eq!(cell.style().fg, Some(theme.yellow));
    }
}

//! Pipeline chat widget — renders a Chat struct to a ratatui Buffer.
//!
//! Single widget that replaces the old workflow/epic_tree/stage_list view stack
//! with a narrative pipeline view. Applies progressive dimming based on stage.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::tui::theme::{Theme, SYM_CHECK, SYM_FAILED, SYM_PENDING, SYM_RUNNING};
use crate::tui::types::{Chat, ChatChild, Message, MessageKind, Stage};
use crate::tui::widgets::path_line::PathLine;

/// Widget that renders a pipeline chat narrative.
pub struct PipelineChat<'a> {
    chat: &'a Chat,
    theme: &'a Theme,
    repo_name: &'a str,
    plan_path: &'a str,
}

impl<'a> PipelineChat<'a> {
    pub fn new(
        chat: &'a Chat,
        theme: &'a Theme,
        repo_name: &'a str,
        plan_path: &'a str,
    ) -> Self {
        Self {
            chat,
            theme,
            repo_name,
            plan_path,
        }
    }
}

impl Widget for PipelineChat<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Find the active stage (last stage with Active or Attention)
        let active_stage = self
            .chat
            .messages
            .iter()
            .filter(|m| matches!(m.kind, MessageKind::Active | MessageKind::Attention))
            .map(|m| m.stage)
            .last();

        let mut y = area.y;
        let width = area.width;

        // Row 0: PathLine header
        if y < area.y + area.height {
            let path_area = Rect::new(area.x, y, width, 1);
            PathLine::new(self.repo_name, self.plan_path, self.theme).render(path_area, buf);
            y += 1;
        }

        // Blank line after header
        y += 1;

        let mut prev_stage: Option<Stage> = None;

        for msg in &self.chat.messages {
            if y >= area.y + area.height {
                break;
            }

            // Blank line between stage transitions
            if let Some(ps) = prev_stage {
                if ps != msg.stage {
                    y += 1;
                }
            }

            let stage_dimmed = active_stage
                .map(|active| msg.stage < active)
                .unwrap_or(false);

            // Render the message text line (if non-empty)
            if !msg.text.is_empty() && y < area.y + area.height {
                render_message_line(
                    buf,
                    area.x,
                    y,
                    width,
                    msg,
                    stage_dimmed,
                    self.theme,
                );
                y += 1;
            }

            // Render children
            for child in &msg.children {
                if y >= area.y + area.height {
                    break;
                }
                match child {
                    ChatChild::Subtask {
                        name,
                        status,
                        elapsed,
                        agent,
                        error,
                    } => {
                        render_subtask_line(
                            buf,
                            area.x,
                            y,
                            width,
                            name,
                            *status,
                            elapsed.as_deref(),
                            agent.as_deref(),
                            stage_dimmed,
                            self.theme,
                        );
                        y += 1;

                        // Error detail on next line
                        if let Some(err) = error {
                            if y < area.y + area.height {
                                render_error_line(
                                    buf,
                                    area.x,
                                    y,
                                    width,
                                    err,
                                    stage_dimmed,
                                    self.theme,
                                );
                                y += 1;
                            }
                        }
                    }
                    ChatChild::AgentBlock { task_name, footer } => {
                        // Blank line before block
                        y += 1;
                        if y >= area.y + area.height {
                            break;
                        }

                        let bg = if stage_dimmed {
                            Color::Reset
                        } else {
                            self.theme.surface
                        };

                        // Task line: ▸ {task_name}
                        render_block_line(
                            buf,
                            area.x,
                            y,
                            width,
                            &format!("{} {}", SYM_RUNNING, task_name),
                            if stage_dimmed {
                                self.theme.dim
                            } else {
                                self.theme.yellow
                            },
                            bg,
                        );
                        y += 1;

                        // Footer line
                        if y < area.y + area.height {
                            let footer_text = format_footer(
                                &footer.agent,
                                &footer.model,
                                footer.context_pct,
                                footer.cost,
                            );
                            render_block_footer(
                                buf,
                                area.x,
                                y,
                                width,
                                &footer_text,
                                footer.elapsed.as_deref(),
                                if stage_dimmed {
                                    self.theme.dim
                                } else {
                                    self.theme.dim
                                },
                                bg,
                            );
                            y += 1;
                        }

                        // Blank line after block
                        y += 1;
                    }
                    ChatChild::LaneBlock { subtasks, footer } => {
                        // Blank line before block
                        y += 1;
                        if y >= area.y + area.height {
                            break;
                        }

                        let bg = if stage_dimmed {
                            Color::Reset
                        } else {
                            self.theme.surface
                        };

                        // Subtask lines inside the lane block
                        for ls in subtasks {
                            if y >= area.y + area.height {
                                break;
                            }
                            let (sym, fg) = kind_sym_color(ls.status, stage_dimmed, self.theme);
                            let text = format!("{} {}", sym, ls.name);
                            render_block_line_with_meta(
                                buf,
                                area.x,
                                y,
                                width,
                                &text,
                                fg,
                                bg,
                                ls.elapsed.as_deref(),
                            );
                            y += 1;

                            // Error detail
                            if let Some(err) = &ls.error {
                                if y < area.y + area.height {
                                    render_block_line(
                                        buf,
                                        area.x,
                                        y,
                                        width,
                                        &format!("  {}", err),
                                        if stage_dimmed {
                                            self.theme.dim
                                        } else {
                                            self.theme.red
                                        },
                                        bg,
                                    );
                                    y += 1;
                                }
                            }
                        }

                        // Footer line
                        if y < area.y + area.height {
                            let footer_text = format_footer(
                                &footer.agent,
                                &footer.model,
                                footer.context_pct,
                                footer.cost,
                            );
                            render_block_footer(
                                buf,
                                area.x,
                                y,
                                width,
                                &footer_text,
                                footer.elapsed.as_deref(),
                                if stage_dimmed {
                                    self.theme.dim
                                } else {
                                    self.theme.dim
                                },
                                bg,
                            );
                            y += 1;
                        }

                        // Blank line after block
                        y += 1;
                    }
                    ChatChild::Issue {
                        number,
                        title,
                        location,
                        description,
                    } => {
                        // Issue number + title
                        let issue_fg = if stage_dimmed {
                            self.theme.dim
                        } else {
                            self.theme.yellow
                        };
                        let prefix = format!(" {}. ", number);
                        let x_start = area.x;
                        buf.set_string(x_start, y, &prefix, Style::default().fg(issue_fg));
                        let title_x = x_start + prefix.len() as u16;
                        buf.set_string(title_x, y, title, Style::default().fg(issue_fg));
                        y += 1;

                        // Location + description on next line(s)
                        if let Some(loc) = location {
                            if y < area.y + area.height {
                                let detail_fg = if stage_dimmed {
                                    self.theme.dim
                                } else {
                                    self.theme.dim
                                };
                                buf.set_string(
                                    area.x + 4,
                                    y,
                                    loc,
                                    Style::default().fg(detail_fg),
                                );
                                if let Some(desc) = description {
                                    let loc_end = area.x + 4 + loc.len() as u16;
                                    buf.set_string(
                                        loc_end,
                                        y,
                                        &format!(" — {}", desc),
                                        Style::default().fg(detail_fg),
                                    );
                                }
                                y += 1;
                            }
                        } else if let Some(desc) = description {
                            if y < area.y + area.height {
                                let detail_fg = if stage_dimmed {
                                    self.theme.dim
                                } else {
                                    self.theme.dim
                                };
                                buf.set_string(
                                    area.x + 4,
                                    y,
                                    desc,
                                    Style::default().fg(detail_fg),
                                );
                                y += 1;
                            }
                        }
                    }
                }
            }

            prev_stage = Some(msg.stage);
        }
    }
}

/// Calculate the total height needed to render the chat.
pub fn chat_height(chat: &Chat) -> u16 {
    let mut height: u16 = 2; // header + blank line
    let mut prev_stage: Option<Stage> = None;

    for msg in &chat.messages {
        // Stage transition blank line
        if let Some(ps) = prev_stage {
            if ps != msg.stage {
                height += 1;
            }
        }

        // Message text line
        if !msg.text.is_empty() {
            height += 1;
        }

        // Children
        for child in &msg.children {
            match child {
                ChatChild::Subtask { error, .. } => {
                    height += 1;
                    if error.is_some() {
                        height += 1;
                    }
                }
                ChatChild::AgentBlock { .. } => {
                    height += 4; // blank + task + footer + blank
                }
                ChatChild::LaneBlock { subtasks, .. } => {
                    height += 1; // blank before
                    height += subtasks.len() as u16;
                    // Error lines inside lane
                    for ls in subtasks {
                        if ls.error.is_some() {
                            height += 1;
                        }
                    }
                    height += 1; // footer
                    height += 1; // blank after
                }
                ChatChild::Issue {
                    location,
                    description,
                    ..
                } => {
                    height += 1; // title
                    if location.is_some() || description.is_some() {
                        height += 1;
                    }
                }
            }
        }

        prev_stage = Some(msg.stage);
    }

    height
}

/// Render a pipeline chat into a buffer (convenience function).
pub fn render_pipeline_chat(
    chat: &Chat,
    theme: &Theme,
    repo_name: &str,
    plan_path: &str,
) -> Buffer {
    let height = chat_height(chat);
    let width = 80; // default width
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    PipelineChat::new(chat, theme, repo_name, plan_path).render(area, &mut buf);

    buf
}

// ── Rendering helpers ───────────────────────────────────────────────

/// Get symbol and color for a MessageKind.
fn kind_sym_color(kind: MessageKind, dimmed: bool, theme: &Theme) -> (&'static str, Color) {
    if dimmed {
        let sym = match kind {
            MessageKind::Done => SYM_CHECK,
            MessageKind::Active => SYM_RUNNING,
            MessageKind::Pending => SYM_PENDING,
            MessageKind::Error => SYM_FAILED,
            _ => "",
        };
        return (sym, theme.dim);
    }

    match kind {
        MessageKind::Done => (SYM_CHECK, theme.green),
        MessageKind::Active => (SYM_RUNNING, theme.yellow),
        MessageKind::Pending => (SYM_PENDING, theme.dim),
        MessageKind::Attention => ("", theme.yellow),
        MessageKind::Error => (SYM_FAILED, theme.red),
        MessageKind::Meta => ("", theme.dim),
    }
}

/// Render a message text line with optional right-aligned meta.
fn render_message_line(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    msg: &Message,
    dimmed: bool,
    theme: &Theme,
) {
    let (sym, fg) = kind_sym_color(msg.kind, dimmed, theme);

    // Meta messages are always dim
    let fg = if msg.kind == MessageKind::Meta {
        theme.dim
    } else {
        fg
    };

    let style = Style::default().fg(fg);

    // Left padding + symbol + text
    let mut col = x + 1; // 1 char left padding
    if !sym.is_empty() {
        buf.set_string(col, y, sym, style);
        col += 2; // symbol + space
    }
    buf.set_string(col, y, &msg.text, style);

    // Right-aligned meta
    if let Some(ref meta) = msg.meta {
        let meta_style = Style::default().fg(theme.dim);
        let meta_len = meta.len() as u16;
        let meta_x = (x + width).saturating_sub(meta_len + 1);
        if meta_x > col {
            buf.set_string(meta_x, y, meta, meta_style);
        }
    }
}

/// Render a subtask line (outside of a block — no background).
fn render_subtask_line(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    name: &str,
    status: MessageKind,
    elapsed: Option<&str>,
    agent: Option<&str>,
    dimmed: bool,
    theme: &Theme,
) {
    let (sym, fg) = kind_sym_color(status, dimmed, theme);
    let style = Style::default().fg(fg);

    let mut col = x + 1; // 1 char left padding
    buf.set_string(col, y, sym, style);
    col += 2; // symbol + space
    buf.set_string(col, y, name, style);

    // Right-aligned: elapsed + agent
    let mut right_parts = Vec::new();
    if let Some(e) = elapsed {
        right_parts.push(e.to_string());
    }
    if let Some(a) = agent {
        right_parts.push(a.to_string());
    }
    if !right_parts.is_empty() {
        let right_text = right_parts.join("  ");
        let right_style = Style::default().fg(theme.dim);
        let right_len = right_text.len() as u16;
        let right_x = (x + width).saturating_sub(right_len + 1);
        if right_x > col + name.len() as u16 {
            buf.set_string(right_x, y, &right_text, right_style);
        }
    }
}

/// Render an error detail line (indented, red).
fn render_error_line(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    _width: u16,
    error: &str,
    dimmed: bool,
    theme: &Theme,
) {
    let fg = if dimmed { theme.dim } else { theme.red };
    buf.set_string(x + 4, y, error, Style::default().fg(fg));
}

/// Render a line inside a block (with background color).
fn render_block_line(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, fg: Color, bg: Color) {
    // Fill full width with background
    let style = Style::default().fg(fg).bg(bg);
    for col in x..x + width {
        buf.set_string(col, y, " ", Style::default().bg(bg));
    }
    // Left padding + text
    buf.set_string(x + 1, y, text, style);
}

/// Render a line inside a block with right-aligned metadata.
fn render_block_line_with_meta(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    text: &str,
    fg: Color,
    bg: Color,
    meta: Option<&str>,
) {
    // Fill full width with background
    let style = Style::default().fg(fg).bg(bg);
    for col in x..x + width {
        buf.set_string(col, y, " ", Style::default().bg(bg));
    }
    // Left padding + text
    buf.set_string(x + 1, y, text, style);

    // Right-aligned meta
    if let Some(m) = meta {
        let meta_len = m.len() as u16;
        let meta_x = (x + width).saturating_sub(meta_len + 1);
        let meta_style = Style::default().fg(fg).bg(bg);
        buf.set_string(meta_x, y, m, meta_style);
    }
}

/// Render a block footer line.
fn render_block_footer(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    left_text: &str,
    elapsed: Option<&str>,
    fg: Color,
    bg: Color,
) {
    // Fill full width with background
    for col in x..x + width {
        buf.set_string(col, y, " ", Style::default().bg(bg));
    }

    let style = Style::default().fg(fg).bg(bg);
    buf.set_string(x + 1, y, left_text, style);

    // Right-aligned elapsed
    if let Some(e) = elapsed {
        let e_len = e.len() as u16;
        let e_x = (x + width).saturating_sub(e_len + 1);
        buf.set_string(e_x, y, e, style);
    }
}

/// Format the block footer text (without elapsed, which is right-aligned).
fn format_footer(agent: &str, model: &str, context_pct: u8, cost: f64) -> String {
    let mut parts = Vec::new();

    if !model.is_empty() {
        parts.push(format!("{}/{}", agent, model));
    } else {
        parts.push(agent.to_string());
    }

    parts.push(format!("{}%", context_pct));
    parts.push(format!("${:.2}", cost));

    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::{BlockFooter, LaneSubtask};

    fn test_theme() -> Theme {
        Theme::dark()
    }

    #[test]
    fn empty_chat_renders_header_only() {
        let theme = test_theme();
        let chat = Chat {
            messages: Vec::new(),
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");
        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("test.md"));
    }

    #[test]
    fn done_message_uses_green() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![Message {
                stage: Stage::Build,
                kind: MessageKind::Done,
                text: "Built 6/6 subtasks".to_string(),
                meta: Some("1m54".to_string()),
                children: Vec::new(),
            }],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        // Find the row containing "Built"
        let rows = buf.area.height;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Built") {
                // Check that the symbol cell has green foreground
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(
                    sym_cell.style().fg,
                    Some(theme.green),
                    "Done symbol should be green"
                );
                return;
            }
        }
        panic!("Could not find 'Built' in rendered output");
    }

    #[test]
    fn lane_block_has_surface_bg() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![Message {
                stage: Stage::Build,
                kind: MessageKind::Active,
                text: String::new(),
                meta: None,
                children: vec![ChatChild::LaneBlock {
                    subtasks: vec![
                        LaneSubtask {
                            name: "Explore requirements".to_string(),
                            status: MessageKind::Done,
                            elapsed: Some("8s".to_string()),
                            error: None,
                        },
                        LaneSubtask {
                            name: "Verify signatures".to_string(),
                            status: MessageKind::Active,
                            elapsed: None,
                            error: None,
                        },
                    ],
                    footer: BlockFooter {
                        agent: "claude".to_string(),
                        model: "opus-4.6".to_string(),
                        context_pct: 42,
                        cost: 0.35,
                        elapsed: Some("32s".to_string()),
                    },
                }],
            }],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        // Find rows with lane content and verify surface background
        let rows = buf.area.height;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Explore") || line.contains("Verify") || line.contains("claude") {
                let cell = buf.cell((0, row)).unwrap();
                assert_eq!(
                    cell.style().bg,
                    Some(theme.surface),
                    "Lane block line '{}' should have surface bg",
                    line.trim()
                );
            }
        }
    }

    #[test]
    fn agent_block_has_surface_bg() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![Message {
                stage: Stage::Review,
                kind: MessageKind::Active,
                text: String::new(),
                meta: None,
                children: vec![ChatChild::AgentBlock {
                    task_name: "Reviewing changes".to_string(),
                    footer: BlockFooter {
                        agent: "claude".to_string(),
                        model: "opus-4.6".to_string(),
                        context_pct: 18,
                        cost: 0.12,
                        elapsed: Some("18s".to_string()),
                    },
                }],
            }],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        let rows = buf.area.height;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Reviewing") {
                let cell = buf.cell((0, row)).unwrap();
                assert_eq!(
                    cell.style().bg,
                    Some(theme.surface),
                    "Agent block should have surface bg"
                );
                return;
            }
        }
        panic!("Could not find 'Reviewing' in rendered output");
    }

    #[test]
    fn progressive_dimming_dims_old_stages() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![
                Message {
                    stage: Stage::Plan,
                    kind: MessageKind::Meta,
                    text: "Created plan".to_string(),
                    meta: Some("14:32".to_string()),
                    children: Vec::new(),
                },
                Message {
                    stage: Stage::Build,
                    kind: MessageKind::Done,
                    text: "Built 6/6 subtasks".to_string(),
                    meta: Some("1m54".to_string()),
                    children: Vec::new(),
                },
                Message {
                    stage: Stage::Review,
                    kind: MessageKind::Active,
                    text: String::new(),
                    meta: None,
                    children: vec![ChatChild::AgentBlock {
                        task_name: "Reviewing changes".to_string(),
                        footer: BlockFooter {
                            agent: "claude".to_string(),
                            model: "opus-4.6".to_string(),
                            context_pct: 18,
                            cost: 0.12,
                            elapsed: Some("18s".to_string()),
                        },
                    }],
                },
            ],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        // "Built 6/6" is in Build stage, active stage is Review.
        // Build < Review, so it should be dimmed.
        let rows = buf.area.height;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Built") {
                // The text should be dim (check the ✓ symbol)
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(
                    sym_cell.style().fg,
                    Some(theme.dim),
                    "Stage-dimmed Done text should use dim color"
                );
                return;
            }
        }
        panic!("Could not find 'Built' in rendered output");
    }

    #[test]
    fn pending_subtask_outside_block_no_surface_bg() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![Message {
                stage: Stage::Build,
                kind: MessageKind::Active,
                text: String::new(),
                meta: None,
                children: vec![
                    ChatChild::LaneBlock {
                        subtasks: vec![LaneSubtask {
                            name: "Active task".to_string(),
                            status: MessageKind::Active,
                            elapsed: None,
                            error: None,
                        }],
                        footer: BlockFooter {
                            agent: "claude".to_string(),
                            model: "opus-4.6".to_string(),
                            context_pct: 10,
                            cost: 0.05,
                            elapsed: Some("5s".to_string()),
                        },
                    },
                    ChatChild::Subtask {
                        name: "Pending outside".to_string(),
                        status: MessageKind::Pending,
                        elapsed: None,
                        agent: None,
                        error: None,
                    },
                ],
            }],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        let rows = buf.area.height;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Pending outside") {
                let cell = buf.cell((0, row)).unwrap();
                // Should NOT have surface background
                assert_ne!(
                    cell.style().bg,
                    Some(theme.surface),
                    "Unassigned subtask should not have surface bg"
                );
                return;
            }
        }
        panic!("Could not find 'Pending outside' in rendered output");
    }

    #[test]
    fn footer_format_correct() {
        let result = format_footer("claude", "opus-4.6", 42, 0.35);
        assert_eq!(result, "claude/opus-4.6 · 42% · $0.35");
    }

    #[test]
    fn footer_format_no_model() {
        let result = format_footer("claude", "", 10, 0.05);
        assert_eq!(result, "claude · 10% · $0.05");
    }

    #[test]
    fn chat_completed_pipeline() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![
                Message {
                    stage: Stage::Plan,
                    kind: MessageKind::Meta,
                    text: "Created plan".to_string(),
                    meta: Some("14:32".to_string()),
                    children: Vec::new(),
                },
                Message {
                    stage: Stage::Build,
                    kind: MessageKind::Done,
                    text: "Built 6/6 subtasks".to_string(),
                    meta: Some("1m54".to_string()),
                    children: Vec::new(),
                },
                Message {
                    stage: Stage::Review,
                    kind: MessageKind::Done,
                    text: "Review passed — approved".to_string(),
                    meta: Some("42s".to_string()),
                    children: Vec::new(),
                },
                Message {
                    stage: Stage::Summary,
                    kind: MessageKind::Done,
                    text: "Done in 1 iteration, 2m36 total".to_string(),
                    meta: None,
                    children: Vec::new(),
                },
            ],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        // All Done messages should be green (no active stage → nothing dimmed)
        let rows = buf.area.height;
        let mut found_built = false;
        let mut found_review = false;
        let mut found_done = false;
        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if line.contains("Built") {
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(sym_cell.style().fg, Some(theme.green), "Built should be green");
                found_built = true;
            }
            if line.contains("Review passed") {
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(sym_cell.style().fg, Some(theme.green), "Review should be green");
                found_review = true;
            }
            if line.contains("Done in") {
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(sym_cell.style().fg, Some(theme.green), "Summary should be green");
                found_done = true;
            }
        }
        assert!(found_built, "Should find 'Built' line");
        assert!(found_review, "Should find 'Review passed' line");
        assert!(found_done, "Should find 'Done in' line");
    }

    #[test]
    fn chat_build_failed() {
        let theme = test_theme();
        let chat = Chat {
            messages: vec![
                Message {
                    stage: Stage::Build,
                    kind: MessageKind::Error,
                    text: "Build failed: 1 subtask errored".to_string(),
                    meta: Some("48s".to_string()),
                    children: vec![
                        ChatChild::Subtask {
                            name: "Explore requirements".to_string(),
                            status: MessageKind::Done,
                            elapsed: Some("8s".to_string()),
                            agent: Some("claude".to_string()),
                            error: None,
                        },
                        ChatChild::Subtask {
                            name: "Implement handler".to_string(),
                            status: MessageKind::Error,
                            elapsed: Some("48s".to_string()),
                            agent: Some("cursor".to_string()),
                            error: Some("Connection refused to test database".to_string()),
                        },
                        ChatChild::Subtask {
                            name: "Write tests".to_string(),
                            status: MessageKind::Pending,
                            elapsed: None,
                            agent: None,
                            error: None,
                        },
                    ],
                },
            ],
        };
        let buf = render_pipeline_chat(&chat, &theme, "aiki", "ops/now/test.md");

        let rows = buf.area.height;
        let mut found_failed_msg = false;
        let mut found_error_subtask = false;
        let mut found_error_detail = false;

        for row in 0..rows {
            let line: String = (0..buf.area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();

            if line.contains("Build failed") {
                // Should have ✗ symbol in red
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(sym_cell.style().fg, Some(theme.red), "Failed message should be red");
                found_failed_msg = true;
            }
            if line.contains("Implement handler") {
                // Failed subtask should have ✗ in red
                let sym_cell = buf.cell((1, row)).unwrap();
                assert_eq!(sym_cell.style().fg, Some(theme.red), "Failed subtask should be red");
                found_error_subtask = true;
            }
            if line.contains("Connection refused") {
                // Error detail should be red
                let err_cell = buf.cell((4, row)).unwrap();
                assert_eq!(err_cell.style().fg, Some(theme.red), "Error detail should be red");
                found_error_detail = true;
            }
        }

        assert!(found_failed_msg, "Should find 'Build failed' line");
        assert!(found_error_subtask, "Should find failed subtask");
        assert!(found_error_detail, "Should find error detail line");
    }
}

//! Epic tree widget.
//!
//! Renders an epic task headline and its subtask tree with status symbols,
//! agent badges, elapsed time, and error lines for failed tasks.
//! Supports collapsed mode showing a single summary line.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::tui::theme::{Theme, SYM_CHECK, SYM_FAILED, SYM_PENDING, SYM_RUNNING, SYM_STARTING};
use crate::tui::types::{EpicView, SubtaskStatus};

/// Renders an epic task and its subtask tree.
pub struct EpicTree<'a> {
    epic: &'a EpicView,
    theme: &'a Theme,
}

impl<'a> EpicTree<'a> {
    pub fn new(epic: &'a EpicView, theme: &'a Theme) -> Self {
        Self { epic, theme }
    }
}

/// Returns the status symbol for a subtask status.
fn status_symbol(status: SubtaskStatus) -> &'static str {
    match status {
        SubtaskStatus::Done => SYM_CHECK,
        SubtaskStatus::Starting => SYM_STARTING,
        SubtaskStatus::Active => SYM_RUNNING,
        SubtaskStatus::Pending => SYM_PENDING,
        SubtaskStatus::Failed => SYM_FAILED,
    }
}

/// Returns the style for a subtask status symbol.
fn status_style(status: SubtaskStatus, theme: &Theme) -> Style {
    match status {
        SubtaskStatus::Done => Style::default().fg(theme.green),
        SubtaskStatus::Starting => Style::default().fg(theme.yellow),
        SubtaskStatus::Active => Style::default().fg(theme.yellow),
        SubtaskStatus::Pending => theme.dim_style(),
        SubtaskStatus::Failed => Style::default().fg(theme.red),
    }
}

/// Returns the agent display label and style.
fn agent_display<'a>(agent: Option<&str>, theme: &'a Theme) -> Option<(&'static str, Style)> {
    match agent {
        Some(a) if a.contains("claude") || a == "cc" => {
            Some(("claude", Style::default().fg(theme.cyan)))
        }
        Some(a) if a.contains("cursor") || a == "cur" => {
            Some(("cursor", Style::default().fg(theme.magenta)))
        }
        Some(a) if a.contains("codex") => Some(("codex", Style::default())),
        Some(a) if a.contains("gemini") => Some(("gemini", Style::default())),
        _ => None,
    }
}

impl Widget for EpicTree<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let dim = self.theme.dim_style();
        let hi_bold = self.theme.hi_style();
        let text_style = self.theme.text_style();

        let mut y = area.y;
        let max_y = area.y.saturating_add(area.height);

        // ── Epic headline ────────────────────────────────────────
        // " [shortid] Name"
        {
            let mut x = area.x.saturating_add(1); // 1 char left padding
            let id_str = format!("[{}]", self.epic.short_id);
            buf.set_string(x, y, &id_str, dim);
            x = x.saturating_add(id_str.len() as u16);
            x = x.saturating_add(1); // space
            buf.set_string(x, y, &self.epic.name, hi_bold);
        }
        y += 1;

        // ── Collapsed mode ───────────────────────────────────────
        if self.epic.collapsed {
            if y >= max_y {
                return;
            }
            if let Some(ref summary) = self.epic.collapsed_summary {
                let mut x = area.x.saturating_add(1); // 1 char left padding
                // "⎿" connector (dim)
                buf.set_string(x, y, "⎿", dim);
                x = x.saturating_add(2); // ⎿ + space

                // Green check
                let green_style = Style::default().fg(self.theme.green);
                buf.set_string(x, y, SYM_CHECK, green_style);
                x = x.saturating_add(SYM_CHECK.len() as u16);
                x = x.saturating_add(1); // space

                // Summary in dim text
                buf.set_string(x, y, summary, dim);
            }
            return;
        }

        // ── Subtask rows ─────────────────────────────────────────
        for subtask in &self.epic.subtasks {
            if y >= max_y {
                break;
            }

            let mut x = area.x.saturating_add(1); // 1 char left padding

            // "⎿" connector (dim)
            buf.set_string(x, y, "⎿", dim);
            x = x.saturating_add(2); // ⎿ + space

            // Status symbol (colored)
            let sym = status_symbol(subtask.status);
            let sym_style = status_style(subtask.status, self.theme);
            buf.set_string(x, y, sym, sym_style);
            x = x.saturating_add(sym.len() as u16);
            x = x.saturating_add(1); // space

            // Task name
            let name_style = match subtask.status {
                SubtaskStatus::Pending => dim,
                _ => text_style,
            };
            buf.set_string(x, y, &subtask.name, name_style);

            // Right-aligned: agent + time
            let agent_info = agent_display(subtask.agent.as_deref(), self.theme);
            let time_str = subtask.elapsed.as_deref();

            let right_width = area.x.saturating_add(area.width);
            let mut right_x = right_width;

            // Time (rightmost)
            if let Some(t) = time_str {
                let tw = t.len() as u16;
                right_x = right_x.saturating_sub(tw).saturating_sub(1);
                buf.set_string(right_x, y, t, dim);
            }

            // Agent
            if let Some((label, style)) = agent_info {
                let aw = label.len() as u16;
                right_x = right_x.saturating_sub(aw).saturating_sub(1);
                buf.set_string(right_x, y, label, style);
            }

            y += 1;

            // Error line for failed subtasks
            if let Some(ref err) = subtask.error {
                if y >= max_y {
                    break;
                }
                // Indent to align under the task name (past ⎿ + space + sym + space)
                let indent = area.x.saturating_add(1 + 2 + 2);
                let err_style = Style::default().fg(self.theme.red);
                let avail = right_width.saturating_sub(indent) as usize;
                let err_display = if err.len() > avail {
                    &err[..avail]
                } else {
                    err.as_str()
                };
                buf.set_string(indent, y, err_display, err_style);
                y += 1;
            }
        }
    }
}

#[cfg(test)]
impl<'a> EpicTree<'a> {
    fn required_height(&self) -> u16 {
        if self.epic.collapsed {
            // Epic headline + collapsed summary line
            return if self.epic.collapsed_summary.is_some() {
                2
            } else {
                1
            };
        }
        let error_lines: usize = self
            .epic
            .subtasks
            .iter()
            .filter(|t| t.error.is_some())
            .count();
        1 + self.epic.subtasks.len() as u16 + error_lines as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::SubtaskLine;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    fn make_subtask(name: &str, status: SubtaskStatus) -> SubtaskLine {
        SubtaskLine {
            name: name.to_string(),
            status,
            agent: None,
            elapsed: None,
            error: None,
        }
    }

    fn buf_line(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                buf.cell((x, y))
                    .map(|c| c.symbol().to_string())
                    .unwrap_or_default()
            })
            .collect::<String>()
    }

    #[test]
    fn all_completed_subtasks() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic task".to_string(),
            subtasks: vec![
                make_subtask("Subtask one", SubtaskStatus::Done),
                make_subtask("Subtask two", SubtaskStatus::Done),
            ],
            collapsed: false,
            collapsed_summary: None,
        };

        let widget = EpicTree::new(&epic, &theme);
        let area = Rect::new(0, 0, 60, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let line0 = buf_line(&buf, 0, 60);
        assert!(line0.contains("[abcdefgh]"));
        assert!(line0.contains("Epic task"));

        let line1 = buf_line(&buf, 1, 60);
        assert!(line1.contains("⎿"));
        assert!(line1.contains(SYM_CHECK));
        assert!(line1.contains("Subtask one"));
        // No task ID on subtask lines
        assert!(!line1.contains("[aaaabbbb]"));

        let line2 = buf_line(&buf, 2, 60);
        assert!(line2.contains(SYM_CHECK));
        assert!(line2.contains("Subtask two"));
    }

    #[test]
    fn mixed_statuses() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic".to_string(),
            subtasks: vec![
                make_subtask("Done task", SubtaskStatus::Done),
                make_subtask("Running task", SubtaskStatus::Active),
                make_subtask("Pending task", SubtaskStatus::Pending),
            ],
            collapsed: false,
            collapsed_summary: None,
        };

        let widget = EpicTree::new(&epic, &theme);
        let area = Rect::new(0, 0, 60, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let line1 = buf_line(&buf, 1, 60);
        assert!(line1.contains(SYM_CHECK), "Done task should have check symbol");

        let line2 = buf_line(&buf, 2, 60);
        assert!(line2.contains(SYM_RUNNING), "Running task should have running symbol");

        let line3 = buf_line(&buf, 3, 60);
        assert!(line3.contains(SYM_PENDING), "Pending task should have pending symbol");

        // Verify colors
        // Done task check symbol should be green
        let check_x = (0..60).find(|&x| {
            buf.cell((x, 1)).map(|c| c.symbol()) == Some(SYM_CHECK)
        });
        if let Some(x) = check_x {
            let style = buf.cell((x, 1)).unwrap().style();
            assert_eq!(style.fg, Some(theme.green));
        }

        // Running symbol should be yellow
        let run_x = (0..60).find(|&x| {
            buf.cell((x, 2)).map(|c| c.symbol()) == Some(SYM_RUNNING)
        });
        if let Some(x) = run_x {
            let style = buf.cell((x, 2)).unwrap().style();
            assert_eq!(style.fg, Some(theme.yellow));
        }

        // Pending symbol should be dim
        let pend_x = (0..60).find(|&x| {
            buf.cell((x, 3)).map(|c| c.symbol()) == Some(SYM_PENDING)
        });
        if let Some(x) = pend_x {
            let style = buf.cell((x, 3)).unwrap().style();
            assert_eq!(style.fg, Some(theme.dim));
        }
    }

    #[test]
    fn failed_subtask_shows_error_line() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic".to_string(),
            subtasks: vec![SubtaskLine {
                name: "Add retry logic".to_string(),
                status: SubtaskStatus::Failed,
                agent: None,
                elapsed: None,
                error: Some("Redis connection refused".to_string()),
            }],
            collapsed: false,
            collapsed_summary: None,
        };

        let widget = EpicTree::new(&epic, &theme);
        assert_eq!(widget.required_height(), 3); // epic + subtask + error line

        let area = Rect::new(0, 0, 80, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let line1 = buf_line(&buf, 1, 80);
        assert!(line1.contains(SYM_FAILED));
        assert!(line1.contains("Add retry logic"));

        let line2 = buf_line(&buf, 2, 80);
        assert!(line2.contains("Redis connection refused"));

        // Error line should be red
        let err_x = (0..80u16).find(|&x| {
            buf.cell((x, 2))
                .map(|c| c.symbol().starts_with('R'))
                .unwrap_or(false)
        });
        if let Some(x) = err_x {
            let style = buf.cell((x, 2)).unwrap().style();
            assert_eq!(style.fg, Some(theme.red));
        }
    }

    #[test]
    fn empty_subtask_list() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Solo epic".to_string(),
            subtasks: vec![],
            collapsed: false,
            collapsed_summary: None,
        };

        let widget = EpicTree::new(&epic, &theme);
        assert_eq!(widget.required_height(), 1);

        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let line0 = buf_line(&buf, 0, 40);
        assert!(line0.contains("[abcdefgh]"));
        assert!(line0.contains("Solo epic"));

        // No subtask lines
        let line1 = buf_line(&buf, 1, 40);
        assert!(!line1.contains("⎿"), "Should have no subtask connectors");
    }

    #[test]
    fn short_id_rendering() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "xyzwabcd".to_string(),
            name: "Test ID".to_string(),
            subtasks: vec![],
            collapsed: false,
            collapsed_summary: None,
        };

        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        EpicTree::new(&epic, &theme).render(area, &mut buf);

        let line = buf_line(&buf, 0, 40);
        assert!(line.contains("[xyzwabcd]"), "Should show short_id: {}", line);
    }

    #[test]
    fn collapsed_mode_renders_summary() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "luppzupt".to_string(),
            name: "Implement Stripe webhook event handling".to_string(),
            subtasks: vec![
                make_subtask("Subtask one", SubtaskStatus::Done),
                make_subtask("Subtask two", SubtaskStatus::Done),
            ],
            collapsed: true,
            collapsed_summary: Some("6 subtasks  2m28s".to_string()),
        };

        let widget = EpicTree::new(&epic, &theme);
        assert_eq!(widget.required_height(), 2); // epic headline + collapsed summary

        let area = Rect::new(0, 0, 60, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Epic headline
        let line0 = buf_line(&buf, 0, 60);
        assert!(line0.contains("[luppzupt]"));
        assert!(line0.contains("Implement Stripe webhook event handling"));

        // Collapsed summary line
        let line1 = buf_line(&buf, 1, 60);
        assert!(line1.contains("⎿"), "Should have connector");
        assert!(line1.contains(SYM_CHECK), "Should have green check");
        assert!(line1.contains("6 subtasks  2m28s"), "Should have summary text");

        // Check that the check symbol is green
        let check_x = (0..60).find(|&x| {
            buf.cell((x, 1)).map(|c| c.symbol()) == Some(SYM_CHECK)
        });
        if let Some(x) = check_x {
            let style = buf.cell((x, 1)).unwrap().style();
            assert_eq!(style.fg, Some(theme.green));
        }

        // Subtask lines should NOT be rendered (collapsed)
        let line2 = buf_line(&buf, 2, 60);
        assert!(!line2.contains("Subtask one"), "Subtasks should be hidden when collapsed");
    }

    #[test]
    fn collapsed_mode_without_summary() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic".to_string(),
            subtasks: vec![],
            collapsed: true,
            collapsed_summary: None,
        };

        let widget = EpicTree::new(&epic, &theme);
        assert_eq!(widget.required_height(), 1); // just the headline, no summary

        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let line0 = buf_line(&buf, 0, 40);
        assert!(line0.contains("[abcdefgh]"));

        let line1 = buf_line(&buf, 1, 40);
        assert!(!line1.contains("⎿"), "No connector without summary");
    }

    #[test]
    fn subtask_with_agent_and_elapsed() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic".to_string(),
            subtasks: vec![SubtaskLine {
                name: "Explore webhook requirements".to_string(),
                status: SubtaskStatus::Done,
                agent: Some("claude".to_string()),
                elapsed: Some("8s".to_string()),
                error: None,
            }],
            collapsed: false,
            collapsed_summary: None,
        };

        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);
        EpicTree::new(&epic, &theme).render(area, &mut buf);

        let line1 = buf_line(&buf, 1, 60);
        assert!(line1.contains(SYM_CHECK));
        assert!(line1.contains("Explore webhook requirements"));
        assert!(line1.contains("claude"), "Should show agent badge");
        assert!(line1.contains("8s"), "Should show elapsed time");
    }

    #[test]
    fn no_task_ids_on_subtask_lines() {
        let theme = test_theme();
        let epic = EpicView {
            short_id: "abcdefgh".to_string(),
            name: "Epic".to_string(),
            subtasks: vec![
                make_subtask("Subtask one", SubtaskStatus::Done),
            ],
            collapsed: false,
            collapsed_summary: None,
        };

        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);
        EpicTree::new(&epic, &theme).render(area, &mut buf);

        let line0 = buf_line(&buf, 0, 60);
        assert!(line0.contains("[abcdefgh]"), "Epic header should have ID");

        let line1 = buf_line(&buf, 1, 60);
        assert!(!line1.contains("["), "Subtask line should not have [id]");
    }
}

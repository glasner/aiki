//! Review issue list view composer.
//!
//! Renders a fixed-width 80-column layout:
//! - Row 0: `[short_id] Review Name  3 issues`
//! - Row 1: blank separator
//! - Rows 2+: issue rows with tree connector, severity badge/text, and location suffix.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::tui::theme::Theme;

const WIDTH: u16 = 80;
const GAP: usize = 2;
const LEFT_PAD: u16 = 1;
const TREE_PREFIX: &str = "⎿ ";
const TREE_PREFIX_WIDTH: u16 = 2;

#[derive(Debug)]
pub struct IssueListItem {
    pub severity: String,
    pub text: String,
    pub location: String,
}

/// Render the issue list into a fixed-width buffer.
pub fn render_issue_list(
    review_id: &str,
    review_name: &str,
    issues: &[IssueListItem],
    theme: &Theme,
) -> Buffer {
    let issue_rows = issue_rows(issues);
    let height = if issues.is_empty() {
        1
    } else {
        2 + issue_rows
    };

    let mut buf = Buffer::empty(Rect::new(0, 0, WIDTH, height));

    render_header(&mut buf, review_id, review_name, issues.len(), theme);
    if issues.is_empty() {
        return buf;
    }

    // Row 1: blank separator, start issues at row 2.
    let mut y = 2;
    for issue in issues {
        y = render_issue_row(&mut buf, issue, y, theme);
    }

    buf
}

fn render_header(buf: &mut Buffer, review_id: &str, review_name: &str, count: usize, theme: &Theme) {
    let short_id = short_id(review_id, 8);
    let header_id = format!("[{short_id}]");
    let count_text = if count == 0 {
        "No issues".to_string()
    } else {
        format!("{count} issue{}", if count == 1 { "" } else { "s" })
    };
    let count_style = if count == 0 {
        Style::default().fg(theme.green)
    } else {
        Style::default().fg(theme.yellow)
    };

    let mut x = LEFT_PAD;
    buf.set_string(x, 0, &header_id, theme.dim_style());
    x = x.saturating_add(header_id.len() as u16);
    x = x.saturating_add(1); // space
    buf.set_string(x, 0, review_name, theme.hi_style());
    x = x.saturating_add(review_name.len() as u16);
    x = x.saturating_add(2); // explicit two-space gap
    buf.set_string(x, 0, &count_text, count_style);
}

fn render_issue_row(
    buf: &mut Buffer,
    issue: &IssueListItem,
    start_y: u16,
    theme: &Theme,
) -> u16 {
    let y = start_y;
    let issue_style = severity_style(&issue.severity, theme);
    let dim = theme.dim_style();
    let badge = format!("[{}]", issue.severity);
    let location = issue.location.trim().to_string();

    let connector_x = LEFT_PAD;
    buf.set_string(connector_x, y, TREE_PREFIX.trim_end(), dim);

    let mut x = connector_x.saturating_add(TREE_PREFIX_WIDTH);
    buf.set_string(x, y, &badge, issue_style);
    x = x.saturating_add(badge.len() as u16).saturating_add(1); // space after badge

    let text_start = x;
    let text_width = if location.is_empty() {
        (WIDTH.saturating_sub(text_start)) as usize
    } else {
        let width = WIDTH
            .saturating_sub(text_start)
            .saturating_sub(GAP as u16)
            .saturating_sub(location.len() as u16);
        width.max(1) as usize
    };

    let wrapped = wrap_words(&issue.text, text_width);
    if wrapped.is_empty() {
        return y.saturating_add(1);
    }

    for (idx, line) in wrapped.iter().enumerate() {
        let row = y.saturating_add(idx as u16);

        buf.set_string(text_start, row, line, issue_style);

        if idx == 0 && !location.is_empty() {
            let loc = truncate_location(&location);
            let loc_x = x_for_location(&loc);
            buf.set_string(loc_x, row, &loc, dim);
        }
    }

    y.saturating_add(wrapped.len() as u16)
}

fn x_for_location(location: &str) -> u16 {
    let loc_len = location.len() as u16;
    let end = WIDTH;
    end.saturating_sub(loc_len)
}

fn truncate_location(location: &str) -> String {
    let max = WIDTH as usize;
    if location.len() <= max {
        return location.to_string();
    }

    let start = location.len().saturating_sub(max);
    location[start..].to_string()
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if word.len() <= width {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }

            let chars: Vec<char> = word.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let next = (i + width).min(chars.len());
                lines.push(chars[i..next].iter().collect());
                i = next;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    } else if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn severity_style(severity: &str, theme: &Theme) -> Style {
    let color = match severity {
        "high" => theme.red,
        "low" => theme.dim,
        _ => theme.yellow,
    };
    Style::default().fg(color)
}

fn issue_rows(issues: &[IssueListItem]) -> u16 {
    let mut rows = 0u16;

    for issue in issues {
        let badge = format!("[{}]", issue.severity);
        let text_start = LEFT_PAD.saturating_add(TREE_PREFIX_WIDTH).saturating_add(badge.len() as u16 + 1);
        let location_width = issue.location.trim().len() as u16;

        let width = if issue.location.trim().is_empty() {
            WIDTH.saturating_sub(text_start)
        } else {
            WIDTH
                .saturating_sub(text_start)
                .saturating_sub(GAP as u16)
                .saturating_sub(location_width)
                .max(1)
        } as usize;

        rows = rows.saturating_add(wrap_words(&issue.text, width).len() as u16);
    }

    rows
}

fn short_id(id: &str, max_len: usize) -> &str {
    if id.len() <= max_len {
        id
    } else {
        &id[..max_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::dark()
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
    fn short_issue_rows() {
        let theme = test_theme();
        let issues = vec![
            IssueListItem {
                severity: "high".to_string(),
                text: "Missing null check".to_string(),
                location: "src/auth.rs:42".to_string(),
            },
            IssueListItem {
                severity: "medium".to_string(),
                text: "Consider using const".to_string(),
                location: "src/utils.rs:10-15".to_string(),
            },
            IssueListItem {
                severity: "low".to_string(),
                text: "Trailing whitespace".to_string(),
                location: String::new(),
            },
        ];

        let buf = render_issue_list("rrmqxnps", "Review fix-auth-subtasks", &issues, &theme);
        assert_eq!(buf.area().height, 5);

        let header = buf_line(&buf, 0, WIDTH);
        assert!(header.contains("[rrmqxnps]"));
        assert!(header.contains("Review fix-auth-subtasks"));
        assert!(header.contains("3 issues"));

        let line2 = buf_line(&buf, 2, WIDTH);
        assert!(line2.contains("⎿ [high] Missing null check"));
        assert!(line2.contains("src/auth.rs:42"));
        // Location must be right-aligned (flush to col 80)
        assert!(line2.trim_end().ends_with("src/auth.rs:42"));
        let high_style = buf.cell((4, 2)).unwrap().style();
        assert_eq!(high_style.fg, Some(theme.red));
        let loc_cell_x = WIDTH - "src/auth.rs:42".len() as u16;
        let loc_style = buf.cell((loc_cell_x, 2)).unwrap().style();
        assert_eq!(loc_style.fg, Some(theme.dim));
    }

    #[test]
    fn wrapped_issue_rows_indent_on_continuation() {
        let theme = test_theme();
        let issues = vec![IssueListItem {
            severity: "high".to_string(),
            text: "The null check is missing in the auth handler which could cause a panic when the token is None"
                .to_string(),
            location: "src/auth.rs:42".to_string(),
        }];

        let buf = render_issue_list("rrmqxnps", "Review fix-auth-subtasks", &issues, &theme);

        let line2 = buf_line(&buf, 2, WIDTH);
        assert!(line2.contains("src/auth.rs:42"));

        let line3 = buf_line(&buf, 3, WIDTH);
        assert!(!line3.contains("⎿"));
        let indent = (LEFT_PAD as usize) + (TREE_PREFIX_WIDTH as usize) + "[high] ".len();
        assert!(line3.starts_with(&" ".repeat(indent)));
        let continuation_style = buf.cell((indent as u16, 3)).unwrap().style();
        assert_eq!(continuation_style.fg, Some(theme.red));
    }

    #[test]
    fn no_location_uses_full_width_text() {
        let theme = test_theme();
        let issues = vec![IssueListItem {
            severity: "medium".to_string(),
            text: "Consider using const for repeated string literals to improve clarity and readability."
                .to_string(),
            location: String::new(),
        }];

        let buf = render_issue_list("abc12345", "Review subtasks", &issues, &theme);
        let line2 = buf_line(&buf, 2, WIDTH);
        assert!(line2.contains("Consider using const"));
        let style_cell = buf.cell((4, 2)).unwrap().style();
        assert_eq!(style_cell.fg, Some(theme.yellow));
    }

    #[test]
    fn empty_issue_list() {
        let theme = test_theme();
        let buf = render_issue_list("rrmqxnps", "Review fix-auth-subtasks", &[], &theme);
        assert_eq!(buf.area().height, 1);
        let header = buf_line(&buf, 0, WIDTH);
        assert!(header.contains("[rrmqxnps]"));
        assert!(header.contains("Review fix-auth-subtasks"));
        assert!(header.contains("No issues"));
        let count_index = header.find("No issues").unwrap();
        let style_cell = buf.cell((count_index as u16, 0)).unwrap().style();
        assert_eq!(style_cell.fg, Some(theme.green));
    }

    #[test]
    fn issue_rows_count_is_used_for_height() {
        let theme = test_theme();
        let issues = vec![
            IssueListItem {
                severity: "low".to_string(),
                text: "Short".to_string(),
                location: String::new(),
            },
            IssueListItem {
                severity: "high".to_string(),
                text: "A much longer issue text that definitely needs wrapping in the test".to_string(),
                location: String::new(),
            },
            IssueListItem {
                severity: "medium".to_string(),
                text: "Another one".to_string(),
                location: String::new(),
            },
        ];

        let mut expected_rows = 0usize;
        for issue in &issues {
            let badge_len = issue.severity.len() + 2;
            let text_start = 1 + 2 + badge_len + 1;
            let width = (WIDTH.saturating_sub(text_start as u16)) as usize;
            expected_rows += wrap_words(&issue.text, width).len();
        }

        let buf = render_issue_list("abc123", "Review", &issues, &theme);
        assert_eq!(buf.area().height, (2 + expected_rows) as u16);
    }
}

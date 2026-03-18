//! Path line widget.
//!
//! Renders a `[repo_name] path` breadcrumb with dimmed prefix/directory
//! and normal filename.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;

/// A widget that renders a repo-prefixed file path breadcrumb.
///
/// Given prefix `"aiki"` and path `"ops/now/webhooks.md"`, renders
/// `[aiki] ops/now/` in [`Theme::dim_style`] and `webhooks.md` in
/// [`Theme::fg_style`].
pub struct PathLine<'a> {
    prefix: &'a str,
    path: &'a str,
    theme: &'a Theme,
}

impl<'a> PathLine<'a> {
    pub fn new(prefix: &'a str, path: &'a str, theme: &'a Theme) -> Self {
        Self { prefix, path, theme }
    }
}

impl Widget for PathLine<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut x = area.x;
        let max_x = area.x.saturating_add(area.width);

        // Leading space for left padding.
        if x < max_x {
            buf.set_string(x, area.y, " ", self.theme.fg_style());
            x = x.saturating_add(1);
        }

        // Render [prefix] in dim style (only if prefix is non-empty).
        if !self.prefix.is_empty() {
            // Opening bracket
            if x < max_x {
                buf.set_string(x, area.y, "[", self.theme.dim_style());
                x = x.saturating_add(1);
            }
            // Prefix text
            let prefix_len = self.prefix.len() as u16;
            let fits = prefix_len.min(max_x.saturating_sub(x));
            if fits > 0 {
                buf.set_string(x, area.y, &self.prefix[..fits as usize], self.theme.dim_style());
                x = x.saturating_add(fits);
            }
            // Closing bracket
            if x < max_x {
                buf.set_string(x, area.y, "]", self.theme.dim_style());
                x = x.saturating_add(1);
            }
        }

        // Space between prefix and path (only if there is a path).
        if !self.path.is_empty() && x < max_x {
            buf.set_string(x, area.y, " ", self.theme.dim_style());
            x = x.saturating_add(1);
        }

        let (dir, file) = match self.path.rfind('/') {
            Some(pos) => (&self.path[..=pos], &self.path[pos + 1..]),
            None => ("", self.path),
        };

        // Render directory portion in dim style.
        if !dir.is_empty() {
            let dir_len = dir.len() as u16;
            let fits = dir_len.min(max_x.saturating_sub(x));
            if fits > 0 {
                buf.set_string(x, area.y, &dir[..fits as usize], self.theme.dim_style());
                x = x.saturating_add(fits);
            }
        }

        // Render filename in fg style.
        if !file.is_empty() && x < max_x {
            let file_len = file.len() as u16;
            let fits = file_len.min(max_x.saturating_sub(x));
            if fits > 0 {
                buf.set_string(x, area.y, &file[..fits as usize], self.theme.fg_style());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    #[test]
    fn splits_directory_and_filename() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        PathLine::new("aiki", "ops/now/webhooks.md", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        // " [aiki] ops/now/webhooks.md"
        assert!(content.starts_with(" [aiki] ops/now/webhooks.md"), "got: {}", content);

        // Prefix (after leading space) should be dim.
        let bracket_cell = buf.cell((1, 0)).unwrap().style();
        assert_eq!(bracket_cell.fg, theme.dim_style().fg);

        // Directory portion: " [aiki] " is 8 chars, "ops/now/" starts at x=8.
        let dir_cell = buf.cell((8, 0)).unwrap().style();
        assert_eq!(dir_cell.fg, theme.dim_style().fg);

        // Filename starts at x=16 (" [aiki] ops/now/" is 16 chars).
        let file_cell = buf.cell((16, 0)).unwrap().style();
        assert_eq!(file_cell.fg, theme.fg_style().fg);
    }

    #[test]
    fn no_directory_component() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        PathLine::new("myrepo", "README.md", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        // " [myrepo] README.md"
        assert!(content.starts_with(" [myrepo] README.md"), "got: {}", content);

        // Filename (after " [myrepo] ") should be fg style. x=10.
        let file_cell = buf.cell((10, 0)).unwrap().style();
        assert_eq!(file_cell.fg, theme.fg_style().fg);
    }

    #[test]
    fn empty_path_shows_prefix_only() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        PathLine::new("aiki", "", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        // " [aiki]" with no trailing space (no path follows).
        assert!(content.starts_with(" [aiki]"), "got: {}", content);
        // No path content after prefix.
        assert!(!content.trim().ends_with('/'));
    }

    #[test]
    fn empty_prefix_shows_path_only() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        PathLine::new("", "ops/now/test.md", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        // " ops/now/test.md" (leading space, no brackets, just path)
        assert!(content.starts_with("  ops/now/test.md") || content.starts_with(" ops/now/test.md"),
            "got: {}", content);
        // Path should be present without any brackets
        assert!(!content.contains('['), "should not have brackets: {}", content);
    }

    #[test]
    fn zero_area_no_panic() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        PathLine::new("aiki", "ops/now/webhooks.md", &theme).render(area, &mut buf);
        // No panic on zero-sized area.
    }
}

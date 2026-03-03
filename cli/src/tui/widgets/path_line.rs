//! Path line widget.
//!
//! Renders a file path with a dimmed directory portion and normal filename.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;

/// A widget that renders a file path with dimmed directory and normal filename.
///
/// Given `ops/now/webhooks.md`, renders `ops/now/` in [`Theme::dim_style`] and
/// `webhooks.md` in [`Theme::fg_style`].
pub struct PathLine<'a> {
    path: &'a str,
    theme: &'a Theme,
}

impl<'a> PathLine<'a> {
    pub fn new(path: &'a str, theme: &'a Theme) -> Self {
        Self { path, theme }
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
        PathLine::new("ops/now/webhooks.md", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.starts_with(" ops/now/webhooks.md"));

        // Directory portion (after leading space) should be dim.
        // Leading space at x=0, "ops/now/" starts at x=1.
        let dir_cell = buf.cell((1, 0)).unwrap().style();
        assert_eq!(dir_cell.fg, theme.dim_style().fg);

        // Filename starts at x=9 ("ops/now/" is 8 chars + 1 leading space).
        let file_cell = buf.cell((9, 0)).unwrap().style();
        assert_eq!(file_cell.fg, theme.fg_style().fg);
    }

    #[test]
    fn no_directory_component() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        PathLine::new("README.md", &theme).render(area, &mut buf);

        let content: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.starts_with(" README.md"));

        // All text (after leading space) should be fg style.
        let file_cell = buf.cell((1, 0)).unwrap().style();
        assert_eq!(file_cell.fg, theme.fg_style().fg);
    }

    #[test]
    fn zero_area_no_panic() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        PathLine::new("ops/now/webhooks.md", &theme).render(area, &mut buf);
        // No panic on zero-sized area.
    }
}

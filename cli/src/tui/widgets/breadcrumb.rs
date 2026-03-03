//! Breadcrumb navigation widget.
//!
//! Renders path-like segments separated by dim ` > ` arrows.
//! The last segment is highlighted in bold high-contrast by default.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;

/// A single segment in a breadcrumb trail.
pub struct BreadcrumbSegment {
    pub text: String,
    pub style: Option<Style>,
}

/// Breadcrumb navigation widget.
///
/// Renders segments separated by dim ` > `. The last segment defaults to
/// [`Theme::hi_style`] (bold + high-contrast) unless it has a custom style.
/// Gracefully truncates when the terminal is too narrow.
pub struct Breadcrumb<'a> {
    segments: Vec<BreadcrumbSegment>,
    theme: &'a Theme,
}

const SEPARATOR: &str = " > ";

impl<'a> Breadcrumb<'a> {
    pub fn new(segments: Vec<BreadcrumbSegment>, theme: &'a Theme) -> Self {
        Self { segments, theme }
    }
}

impl Widget for Breadcrumb<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let sep_len = SEPARATOR.len() as u16;
        let dim_style = self.theme.dim_style();
        let hi_style = self.theme.hi_style();
        let last_idx = self.segments.len().saturating_sub(1);

        let mut x = area.x;
        let max_x = area.x.saturating_add(area.width);

        for (i, seg) in self.segments.into_iter().enumerate() {
            // Prepend separator for all segments after the first.
            if i > 0 {
                if x.saturating_add(sep_len) > max_x {
                    return;
                }
                buf.set_string(x, area.y, SEPARATOR, dim_style);
                x = x.saturating_add(sep_len);
            }

            let text_len = seg.text.len() as u16;
            if x.saturating_add(text_len) > max_x {
                return;
            }

            let style = seg.style.unwrap_or(if i == last_idx {
                hi_style
            } else {
                self.theme.text_style()
            });

            buf.set_string(x, area.y, &seg.text, style);
            x = x.saturating_add(text_len);
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
    fn empty_segments() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        Breadcrumb::new(vec![], &theme).render(area, &mut buf);
        // No panic, buffer unchanged from empty.
        let content: String = buf.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
        assert!(content.trim().is_empty());
    }

    #[test]
    fn single_segment() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        Breadcrumb::new(
            vec![BreadcrumbSegment { text: "home".into(), style: None }],
            &theme,
        )
        .render(area, &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
        assert!(content.starts_with("home"));
        // Single segment is also the last, so it gets hi_style fg color.
        let cell_style = buf.cell((0, 0)).unwrap().style();
        assert_eq!(cell_style.fg, theme.hi_style().fg);
    }

    #[test]
    fn multiple_segments() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        Breadcrumb::new(
            vec![
                BreadcrumbSegment { text: "aiki".into(), style: None },
                BreadcrumbSegment { text: "tasks".into(), style: None },
                BreadcrumbSegment { text: "build".into(), style: None },
            ],
            &theme,
        )
        .render(area, &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
        assert!(content.starts_with("aiki > tasks > build"));
    }

    #[test]
    fn truncation_on_narrow_width() {
        let theme = test_theme();
        // Only 10 chars wide — not enough for "aiki > tasks > build"
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        Breadcrumb::new(
            vec![
                BreadcrumbSegment { text: "aiki".into(), style: None },
                BreadcrumbSegment { text: "tasks".into(), style: None },
                BreadcrumbSegment { text: "build".into(), style: None },
            ],
            &theme,
        )
        .render(area, &mut buf);
        // Should not panic. Only what fits is rendered.
        let content: String = buf.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
        assert!(content.starts_with("aiki"));
        // "aiki" (4) + " > " (3) + "tasks" (5) = 12, won't fit in 10
        assert!(!content.contains("tasks"));
    }

    #[test]
    fn zero_area() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        Breadcrumb::new(
            vec![BreadcrumbSegment { text: "test".into(), style: None }],
            &theme,
        )
        .render(area, &mut buf);
        // No panic on zero-sized area.
    }

    #[test]
    fn custom_style_overrides_default() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        let custom = Style::default().fg(theme.red);
        Breadcrumb::new(
            vec![
                BreadcrumbSegment { text: "aiki".into(), style: None },
                BreadcrumbSegment { text: "error".into(), style: Some(custom) },
            ],
            &theme,
        )
        .render(area, &mut buf);
        // Last segment has custom style, not hi_style.
        // "aiki" (4) + " > " (3) = offset 7 for "error"
        let cell_style = buf.cell((7, 0)).unwrap().style();
        assert_eq!(cell_style.fg, custom.fg);
    }
}

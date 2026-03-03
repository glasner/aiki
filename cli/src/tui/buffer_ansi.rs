// Buffer to ANSI converter — renders a ratatui Buffer as an ANSI-escaped string.

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// Convert a ratatui `Buffer` to a printable ANSI-escaped string.
///
/// Walks every cell, emitting 24-bit foreground color escapes (`\x1b[38;2;r;g;bm`)
/// and bold (`\x1b[1m`). Resets style between cells when it changes. Trims trailing
/// spaces from each line for cleaner output.
pub fn buffer_to_ansi(buf: &Buffer) -> String {
    let area = buf.area();
    let mut out = String::new();

    for row in area.y..area.y + area.height {
        let mut last_style: Option<(Color, Modifier)> = None;

        for col in area.x..area.x + area.width {
            let cell = &buf[(col, row)];
            let style = (cell.fg, cell.modifier);

            if Some(style) != last_style {
                out.push_str("\x1b[0m");
                if let Color::Rgb(r, g, b) = cell.fg {
                    out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
                }
                if cell.modifier.contains(Modifier::BOLD) {
                    out.push_str("\x1b[1m");
                }
                last_style = Some(style);
            }

            out.push_str(cell.symbol());
        }

        // Reset at end of line
        out.push_str("\x1b[0m");

        // Trim trailing spaces
        let trimmed = out.trim_end_matches(' ');
        out.truncate(trimmed.len());

        if row < area.y + area.height - 1 {
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    #[test]
    fn styled_cells_produce_ansi_escapes() {
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_symbol("H");
        buf[(0, 0)].set_style(
            Style::default()
                .fg(Color::Rgb(255, 0, 0))
                .add_modifier(Modifier::BOLD),
        );
        buf[(1, 0)].set_symbol("i");
        buf[(1, 0)].set_style(Style::default().fg(Color::Rgb(0, 255, 0)));

        let result = buffer_to_ansi(&buf);

        // Red bold for 'H'
        assert!(result.contains("\x1b[38;2;255;0;0m"));
        assert!(result.contains("\x1b[1m"));
        assert!(result.contains("H"));

        // Green for 'i'
        assert!(result.contains("\x1b[38;2;0;255;0m"));
        assert!(result.contains("i"));

        // Reset appears
        assert!(result.contains("\x1b[0m"));
    }

    #[test]
    fn empty_buffer_produces_clean_output() {
        let area = Rect::new(0, 0, 3, 2);
        let buf = Buffer::empty(area);

        let result = buffer_to_ansi(&buf);

        // Should not panic and should produce some output (resets at minimum)
        assert!(!result.is_empty());
        // Should contain a newline between the two rows
        assert!(result.contains('\n'));
    }

    #[test]
    fn trailing_spaces_are_trimmed() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_symbol("A");
        buf[(1, 0)].set_symbol("B");
        // Cells 2..9 are default spaces

        let result = buffer_to_ansi(&buf);

        // The line should not end with spaces (after the final reset)
        let after_last_reset = result.rsplit("\x1b[0m").next().unwrap_or("");
        assert!(
            !after_last_reset.ends_with(' '),
            "trailing spaces should be trimmed, got: {:?}",
            result
        );
    }
}

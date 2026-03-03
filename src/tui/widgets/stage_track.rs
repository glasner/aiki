//! Stage pipeline status bar widget.
//!
//! Renders the build/review/fix pipeline as a single-line status bar
//! with phase symbols, progress counts, and elapsed times.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::tui::theme::{Theme, SYM_CHECK, SYM_FAILED, SYM_PENDING, SYM_RUNNING, SYM_STARTING};

/// State of a single pipeline phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PhaseState {
    /// Not yet started.
    Pending,
    /// Agent is spawning.
    Starting,
    /// Currently running.
    Active,
    /// Completed successfully.
    Done,
    /// Completed with failures.
    Failed,
}

/// Information about a single pipeline phase.
#[allow(dead_code)]
pub struct PhaseInfo {
    /// Phase name, e.g. "build", "review", "fix".
    pub name: &'static str,
    /// Current state of the phase.
    pub state: PhaseState,
    /// Number of completed items.
    pub completed: usize,
    /// Total number of items.
    pub total: usize,
    /// Elapsed time string, e.g. "2m28s".
    pub elapsed: Option<String>,
    /// Number of failed items.
    pub failed: usize,
}

/// Stage pipeline status bar widget.
///
/// Renders phases separated by dim `│` separators. Each phase shows
/// a state symbol, name, optional progress count, elapsed time, and
/// failure count.
#[allow(dead_code)]
pub struct StageTrack<'a> {
    phases: Vec<PhaseInfo>,
    theme: &'a Theme,
}

impl<'a> StageTrack<'a> {
    #[allow(dead_code)]
    pub fn new(phases: Vec<PhaseInfo>, theme: &'a Theme) -> Self {
        Self { phases, theme }
    }
}

#[allow(dead_code)]
const SEPARATOR: &str = "  │  ";

impl PhaseInfo {
    /// Build the text fragments for this phase.
    /// Returns a list of (text, style) pairs.
    #[allow(dead_code)]
    fn fragments(&self, theme: &Theme) -> Vec<(String, Style)> {
        let (symbol, color) = match self.state {
            PhaseState::Pending => (SYM_PENDING, theme.dim),
            PhaseState::Starting => (SYM_STARTING, theme.yellow),
            PhaseState::Active => (SYM_RUNNING, theme.yellow),
            PhaseState::Done => (SYM_CHECK, theme.green),
            PhaseState::Failed => (SYM_FAILED, theme.red),
        };

        let phase_style = Style::default().fg(color);
        let mut frags = vec![
            (format!("{symbol} "), phase_style),
            (self.name.to_string(), phase_style),
        ];

        // Show progress count for Active, Done, and Failed phases.
        if matches!(self.state, PhaseState::Active | PhaseState::Done | PhaseState::Failed) {
            frags.push((format!("  {}/{}", self.completed, self.total), phase_style));
        }

        // Show elapsed time for Active phases.
        if self.state == PhaseState::Active {
            if let Some(ref elapsed) = self.elapsed {
                frags.push((format!("  {elapsed}"), phase_style));
            }
        }

        // Show elapsed time for Done phases.
        if self.state == PhaseState::Done {
            if let Some(ref elapsed) = self.elapsed {
                frags.push((format!("  {elapsed}"), phase_style));
            }
        }

        // Show failure count for Failed phases.
        if self.state == PhaseState::Failed && self.failed > 0 {
            let fail_style = Style::default().fg(theme.red);
            frags.push((format!("  {} failed", self.failed), fail_style));
        }

        frags
    }
}

impl Widget for StageTrack<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let dim_style = self.theme.dim_style();
        let max_x = area.x.saturating_add(area.width);
        let mut x = area.x;

        // Leading space for left padding.
        if x < max_x {
            buf.set_string(x, area.y, " ", Style::default());
            x = x.saturating_add(1);
        }

        for (i, phase) in self.phases.iter().enumerate() {
            // Separator between phases.
            if i > 0 {
                let sep_len = SEPARATOR.chars().count() as u16;
                if x.saturating_add(sep_len) > max_x {
                    return;
                }
                buf.set_string(x, area.y, SEPARATOR, dim_style);
                x = x.saturating_add(sep_len);
            }

            for (text, style) in phase.fragments(self.theme) {
                let text_len = text.chars().count() as u16;
                if x.saturating_add(text_len) > max_x {
                    return;
                }
                buf.set_string(x, area.y, &text, style);
                x = x.saturating_add(text_len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    /// Helper: render a StageTrack and return the text content as a string.
    fn render_to_string(phases: Vec<PhaseInfo>, width: u16) -> String {
        let theme = test_theme();
        let area = Rect::new(0, 0, width, 1);
        let mut buf = Buffer::empty(area);
        StageTrack::new(phases, &theme).render(area, &mut buf);
        buf.content()
            .iter()
            .map(|c| {
                let s = c.symbol();
                if s.is_empty() { " ".to_string() } else { s.to_string() }
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn three_phases(
        build: PhaseState,
        review: PhaseState,
        fix: PhaseState,
    ) -> Vec<PhaseInfo> {
        vec![
            PhaseInfo {
                name: "build",
                state: build,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: review,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: fix,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ]
    }

    #[test]
    fn all_pending() {
        let content = render_to_string(
            three_phases(PhaseState::Pending, PhaseState::Pending, PhaseState::Pending),
            80,
        );
        assert!(content.contains("○ build"));
        assert!(content.contains("○ review"));
        assert!(content.contains("○ fix"));
        assert!(content.contains("│"));
    }

    #[test]
    fn build_active() {
        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Active,
                completed: 3,
                total: 6,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];
        let content = render_to_string(phases, 80);
        assert!(content.contains("▸ build"));
        assert!(content.contains("3/6"));
        assert!(content.contains("○ review"));
        assert!(content.contains("○ fix"));
    }

    #[test]
    fn build_done_review_active() {
        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Done,
                completed: 6,
                total: 6,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Active,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];
        let content = render_to_string(phases, 80);
        assert!(content.contains("✓ build"));
        assert!(content.contains("6/6"));
        assert!(content.contains("▸ review"));
        assert!(content.contains("○ fix"));
    }

    #[test]
    fn build_failed() {
        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Failed,
                completed: 8,
                total: 10,
                elapsed: Some("2m48s".to_string()),
                failed: 1,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];
        let content = render_to_string(phases, 80);
        assert!(content.contains("✗ build"));
        assert!(content.contains("8/10"));
        assert!(content.contains("1 failed"));
        assert!(content.contains("○ review"));
        assert!(content.contains("○ fix"));
    }

    #[test]
    fn all_done() {
        let content = render_to_string(
            three_phases(PhaseState::Done, PhaseState::Done, PhaseState::Done),
            80,
        );
        assert!(content.contains("✓ build"));
        assert!(content.contains("✓ review"));
        assert!(content.contains("✓ fix"));
    }

    #[test]
    fn zero_area() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        StageTrack::new(
            three_phases(PhaseState::Pending, PhaseState::Pending, PhaseState::Pending),
            &theme,
        )
        .render(area, &mut buf);
        // No panic on zero-sized area.
    }

    #[test]
    fn active_with_elapsed() {
        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Active,
                completed: 3,
                total: 6,
                elapsed: Some("0:34".to_string()),
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];
        let content = render_to_string(phases, 80);
        assert!(content.contains("▸ build"));
        assert!(content.contains("3/6"));
        assert!(content.contains("0:34"));
    }

    #[test]
    fn done_with_elapsed() {
        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Done,
                completed: 6,
                total: 6,
                elapsed: Some("2m28s".to_string()),
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Active,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];
        let content = render_to_string(phases, 80);
        assert!(content.contains("✓ build"));
        assert!(content.contains("6/6"));
        assert!(content.contains("2m28s"));
    }

    #[test]
    fn style_colors_correct() {
        let theme = test_theme();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        let phases = vec![
            PhaseInfo {
                name: "build",
                state: PhaseState::Done,
                completed: 6,
                total: 6,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "review",
                state: PhaseState::Active,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
            PhaseInfo {
                name: "fix",
                state: PhaseState::Pending,
                completed: 0,
                total: 0,
                elapsed: None,
                failed: 0,
            },
        ];

        StageTrack::new(phases, &theme).render(area, &mut buf);

        // Leading space + "✓" at position 1 should be green.
        // Note: ✓ is multi-byte but occupies one cell.
        let check_cell = buf.cell((1, 0)).unwrap();
        assert_eq!(check_cell.style().fg, Some(theme.green));
    }

    #[test]
    fn starting_phase() {
        let content = render_to_string(
            vec![
                PhaseInfo {
                    name: "build",
                    state: PhaseState::Starting,
                    completed: 0,
                    total: 0,
                    elapsed: None,
                    failed: 0,
                },
                PhaseInfo {
                    name: "review",
                    state: PhaseState::Pending,
                    completed: 0,
                    total: 0,
                    elapsed: None,
                    failed: 0,
                },
            ],
            80,
        );
        assert!(content.contains("⧗ build"), "got: {}", content);
        assert!(content.contains("○ review"), "got: {}", content);
    }

}

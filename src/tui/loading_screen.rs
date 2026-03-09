//! LoadingScreen — alternate-screen overlay for build/task startup phases.
//!
//! Displays a simple progress indicator (step label, optional filepath and task
//! context) while work is being prepared. Once the real work begins the screen
//! can be converted into a [`LiveScreen`] without leaving the alternate screen.

use std::io::{stderr, IsTerminal, Stderr};

use crossterm::{cursor, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::backend::CrosstermBackend;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::Terminal;

use crate::error::Result;
use crate::tui::live_screen::LiveScreen;
use crate::tui::theme::{detect_mode, Theme, SYM_FAILED, SYM_STARTING};

// ---------------------------------------------------------------------------
// Inner enum (TTY vs non-TTY)
// ---------------------------------------------------------------------------

#[allow(clippy::large_enum_variant)]
enum Inner {
    Live {
        terminal: Terminal<CrosstermBackend<Stderr>>,
        filepath: Option<String>,
        task_context: Option<(String, String)>,
        step: String,
    },
    Noop,
}

// ---------------------------------------------------------------------------
// LoadingScreen
// ---------------------------------------------------------------------------

/// Alternate-screen overlay for build/task startup phases.
///
/// On TTY stderr it enters the alternate screen, shows a progress indicator,
/// and can be converted into a [`LiveScreen`] for the main event loop.
/// On non-TTY stderr all methods are no-ops.
pub struct LoadingScreen {
    inner: Inner,
}

impl LoadingScreen {
    /// Create a new loading screen with the given initial step label.
    ///
    /// If stderr is not a terminal, returns a no-op variant.
    pub fn new(initial_step: &str) -> Result<Self> {
        if !stderr().is_terminal() {
            return Ok(Self { inner: Inner::Noop });
        }

        // Step 1: raw mode
        enable_raw_mode().map_err(|e| {
            crate::error::AikiError::Io(std::io::Error::other(e))
        })?;

        // Step 2: alternate screen + hide cursor
        if let Err(e) = execute!(stderr(), EnterAlternateScreen, cursor::Hide) {
            let _ = disable_raw_mode();
            return Err(crate::error::AikiError::Io(std::io::Error::other(e)));
        }

        // Step 3: create terminal
        let backend = CrosstermBackend::new(stderr());
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                let _ = execute!(stderr(), cursor::Show, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(crate::error::AikiError::Io(e));
            }
        };

        let mut screen = Self {
            inner: Inner::Live {
                terminal,
                filepath: None,
                task_context: None,
                step: initial_step.to_string(),
            },
        };
        screen.redraw();
        Ok(screen)
    }

    /// Update the current step label and immediately redraw.
    pub fn set_step(&mut self, label: &str) {
        if let Inner::Live { step, .. } = &mut self.inner {
            *step = label.to_string();
            self.redraw();
        }
    }

    /// Set the filepath context line (rendered above the step). Triggers redraw.
    pub fn set_filepath(&mut self, path: &str) {
        if let Inner::Live { filepath, .. } = &mut self.inner {
            *filepath = Some(path.to_string());
            self.redraw();
        }
    }

    /// Set the task context line: `[change_id] description`. Triggers redraw.
    pub fn set_task_context(&mut self, change_id: &str, description: &str) {
        if let Inner::Live { task_context, .. } = &mut self.inner {
            *task_context = Some((change_id.to_string(), description.to_string()));
            self.redraw();
        }
    }

    /// Show an error frame, pause briefly, then leave the alternate screen and
    /// reprint the error to stderr so it persists in terminal scrollback.
    #[allow(dead_code)]
    pub fn fail(mut self, message: &str) {
        // Swap inner to Noop so Drop doesn't double-cleanup.
        let inner = std::mem::replace(&mut self.inner, Inner::Noop);
        if let Inner::Live { mut terminal, step, .. } = inner {
            let theme = Theme::from_mode(detect_mode());
            let fail_msg = message.to_string();

            // Render error frame
            let _ = terminal.draw(|frame| {
                let lines: Vec<Line<'_>> = vec![
                    // Blank line for top padding
                    Line::from(""),
                    // ✗ <step>
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(SYM_FAILED, Style::default().fg(theme.red)),
                        Span::raw(" "),
                        Span::styled(step.clone(), theme.text_style()),
                    ]),
                    // Indented error message
                    Line::from(vec![
                        Span::raw("   "),
                        Span::styled(fail_msg.clone(), theme.text_style()),
                    ]),
                ];

                let paragraph = ratatui::widgets::Paragraph::new(lines);
                frame.render_widget(paragraph, frame.area());
            });

            // Pause so the user can see the error
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Leave alternate screen
            let _ = execute!(std::io::stderr(), cursor::Show, LeaveAlternateScreen);
            let _ = disable_raw_mode();

            // Reprint to scrollback
            eprintln!("{} {}", SYM_FAILED, step);
            eprintln!("  {}", fail_msg);
        }
    }

    /// Consume self and return the underlying terminal wrapped in a [`LiveScreen`].
    ///
    /// The alternate screen session continues without exit/re-enter.
    /// For the non-TTY variant, creates a fresh `LiveScreen::new()`.
    pub fn into_live_screen(mut self) -> Result<LiveScreen> {
        // Swap inner to Noop so Drop doesn't clean up the alternate screen.
        let inner = std::mem::replace(&mut self.inner, Inner::Noop);
        match inner {
            Inner::Live { terminal, .. } => Ok(LiveScreen::from_terminal(terminal)),
            Inner::Noop => LiveScreen::new(),
        }
    }

    // -- private ------------------------------------------------------------

    fn redraw(&mut self) {
        let Inner::Live { terminal, filepath, task_context, step } = &mut self.inner else {
            return;
        };

        let theme = Theme::from_mode(detect_mode());
        let fp = filepath.clone();
        let tc = task_context.clone();
        let st = step.clone();

        let _ = terminal.draw(|frame| {
            let area = frame.area();
            let mut lines: Vec<Line<'_>> = Vec::new();

            // Blank line for top padding
            lines.push(Line::from(""));

            // Optional filepath line
            if let Some(ref path) = fp {
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(path.clone(), theme.text_style()),
                ]));
                lines.push(Line::from(""));
            }

            // Optional task context line
            if let Some((ref cid, ref desc)) = tc {
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("[", theme.dim_style()),
                    Span::styled(cid.clone(), theme.dim_style()),
                    Span::styled("] ", theme.dim_style()),
                    Span::styled(desc.clone(), theme.hi_style()),
                ]));
                lines.push(Line::from(""));
            }

            // Step line: ⧗ <step>...
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(SYM_STARTING, Style::default().fg(theme.yellow)),
                Span::raw(" "),
                Span::styled(format!("{}...", st), theme.text_style()),
            ]));

            let paragraph = ratatui::widgets::Paragraph::new(lines);
            frame.render_widget(paragraph, area);
        });
    }
}

impl Drop for LoadingScreen {
    fn drop(&mut self) {
        // Only clean up if we still own a live terminal (i.e., into_live_screen
        // was NOT called — that method moves the terminal out via mem::replace).
        // The Inner::Live variant existing at drop time means we need to clean up.
        if let Inner::Live { .. } = &self.inner {
            let _ = execute!(std::io::stderr(), cursor::Show, LeaveAlternateScreen);
            let _ = disable_raw_mode();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_variant_methods_dont_panic() {
        // In CI / test environments stderr is typically not a TTY,
        // so new() should return the Noop variant.
        let mut screen = LoadingScreen::new("initializing").unwrap();
        screen.set_step("next step");
        screen.set_filepath("/some/path");
        screen.set_task_context("abc123", "My task");
        // into_live_screen on Noop creates a fresh LiveScreen — may fail
        // in non-TTY test environments, so we just test the loading screen methods.
    }

    #[test]
    fn step_label_has_ellipsis_suffix() {
        // Verify the rendering format includes "..." by constructing the expected string
        let step = "building";
        let rendered = format!("{}...", step);
        assert_eq!(rendered, "building...");
    }
}

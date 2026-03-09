//! LiveScreen — alternate-screen wrapper for ratatui rendering.
//!
//! Manages the terminal lifecycle (raw mode, alternate screen, cursor visibility)
//! and provides an event loop for live-updating TUI views.

use std::cell::Cell;
use std::io::{stderr, Stderr};
use std::time::Duration;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::Terminal;

use crate::error::Result;

/// Default polling interval in milliseconds.
const DEFAULT_POLL_INTERVAL_MS: u64 = 500;

// ---------------------------------------------------------------------------
// Thread-local guard
// ---------------------------------------------------------------------------

thread_local! {
    static LIVE_SCREEN_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Debug-only assertion that no `LiveScreen` is active on the current thread.
///
/// Use this before any direct `stderr` writes in the rendering path to catch
/// accidental bypasses during development.
#[macro_export]
macro_rules! debug_assert_no_live_screen {
    () => {
        debug_assert!(
            !$crate::tui::live_screen::is_live_screen_active(),
            "Direct stderr write while LiveScreen is active — use LiveScreen::draw() instead"
        );
    };
}

/// Returns whether a `LiveScreen` is active on the current thread.
#[doc(hidden)]
pub fn is_live_screen_active() -> bool {
    LIVE_SCREEN_ACTIVE.with(|f| f.get())
}

// ---------------------------------------------------------------------------
// ExitReason
// ---------------------------------------------------------------------------

/// Reason for the live-screen event loop to stop.
#[derive(Debug, Clone)]
pub enum ExitReason {
    /// Task reached terminal state (closed or stopped).
    TaskCompleted,
    /// User pressed Ctrl+C to detach.
    UserDetached,
    /// Agent process exited without task reaching terminal state.
    AgentExited {
        /// Captured stderr output from the agent (if any).
        stderr: String,
    },
    /// Monitor encountered persistent failures (e.g., poll errors).
    MonitorFailed { reason: String },
}

// ---------------------------------------------------------------------------
// LiveScreen
// ---------------------------------------------------------------------------

/// Alternate-screen terminal wrapper.
///
/// On creation enters raw mode + alternate screen and hides the cursor.
/// On drop (or explicit `restore_terminal()`) reverses all three steps.
pub struct LiveScreen {
    terminal: Terminal<CrosstermBackend<Stderr>>,
}

impl LiveScreen {
    /// Enter alternate screen mode.
    ///
    /// Each step rolls back previously-applied state on failure since `Drop`
    /// won't run for a partially constructed struct.
    pub fn new() -> Result<Self> {
        // Step 1: raw mode
        enable_raw_mode().map_err(|e| {
            crate::error::AikiError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

        // Step 2: alternate screen + hide cursor
        if let Err(e) = execute!(stderr(), EnterAlternateScreen, cursor::Hide) {
            let _ = disable_raw_mode();
            return Err(crate::error::AikiError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )));
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

        LIVE_SCREEN_ACTIVE.with(|f| f.set(true));

        Ok(Self { terminal })
    }

    /// Wrap an existing terminal. Assumes raw mode and alternate screen are already active.
    pub(crate) fn from_terminal(terminal: Terminal<CrosstermBackend<Stderr>>) -> Self {
        LIVE_SCREEN_ACTIVE.with(|f| f.set(true));
        Self { terminal }
    }

    /// Draw a single frame.
    pub fn draw<F>(&mut self, render_fn: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame),
    {
        self.terminal.draw(render_fn).map_err(crate::error::AikiError::Io)?;
        Ok(())
    }

    /// Run the event loop.
    ///
    /// `update_fn` is called on each iteration. It should:
    /// - Call `self.draw(...)` to render the current state.
    /// - Return `Ok(Some(reason))` to exit or `Ok(None)` to continue.
    ///
    /// The loop also handles:
    /// - `Ctrl+C` → `ExitReason::UserDetached`
    /// - `Resize` → automatic redraw by ratatui on next `draw()`
    pub fn run<F>(&mut self, mut update_fn: F) -> Result<ExitReason>
    where
        F: FnMut(&mut Self) -> Result<Option<ExitReason>>,
    {
        let poll_interval_ms = std::env::var("AIKI_STATUS_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS);
        let poll_interval = Duration::from_millis(poll_interval_ms);

        loop {
            // Poll for crossterm events
            if event::poll(poll_interval)? {
                let ev = event::read()?;
                match ev {
                    Event::Key(key) if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(ExitReason::UserDetached);
                    }
                    Event::Resize(_, _) => {
                        // ratatui will pick up the new size on the next draw()
                    }
                    _ => {}
                }
            }

            // Let the caller update state and optionally render
            if let Some(reason) = update_fn(self)? {
                return Ok(reason);
            }
        }
    }
}

impl Drop for LiveScreen {
    fn drop(&mut self) {
        restore_terminal();
    }
}

// ---------------------------------------------------------------------------
// Standalone restore (idempotent)
// ---------------------------------------------------------------------------

/// Restore the terminal to its normal state.
///
/// Idempotent — safe to call even if the terminal was never modified.
/// Extracted from `Drop` for testability and use in panic hooks.
pub(crate) fn restore_terminal() {
    #[cfg(test)]
    CLEANUP_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);

    let _ = execute!(std::io::stderr(), cursor::Show);
    let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
    let _ = disable_raw_mode();

    LIVE_SCREEN_ACTIVE.with(|f| f.set(false));
}

// ---------------------------------------------------------------------------
// BlitWidget
// ---------------------------------------------------------------------------

/// Adapter that stamps a pre-rendered `Buffer` into a ratatui frame.
///
/// Used to bridge the existing `render_workflow()` → `Buffer` pipeline into
/// a `LiveScreen::draw()` call without rewriting all widgets.
pub struct BlitWidget {
    buf: Buffer,
}

impl BlitWidget {
    /// Wrap a pre-rendered buffer.
    pub fn new(buf: Buffer) -> Self {
        Self { buf }
    }
}

impl Widget for BlitWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let src = &self.buf;
        let src_area = src.area();
        for y in 0..area.height.min(src_area.height) {
            for x in 0..area.width.min(src_area.width) {
                if let Some(src_cell) = src.cell((x, y)) {
                    if let Some(dst_cell) = buf.cell_mut((area.x + x, area.y + y)) {
                        *dst_cell = src_cell.clone();
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

#[cfg(test)]
static CLEANUP_CALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
impl LiveScreen {
    /// Create a `LiveScreen` without entering alternate screen or raw mode.
    ///
    /// Used for testing drop/cleanup behavior without real terminal side-effects.
    fn new_test() -> Self {
        LIVE_SCREEN_ACTIVE.with(|f| f.set(true));
        let backend = CrosstermBackend::new(stderr());
        let terminal = Terminal::new(backend).unwrap();
        Self { terminal }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Style};

    /// Create a `LiveScreen`-like terminal backed by `TestBackend` for tests.
    fn test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(width, height)).unwrap()
    }

    // -- Cleanup verification -----------------------------------------------

    #[test]
    fn drop_calls_restore_terminal() {
        CLEANUP_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);
        {
            let _screen = LiveScreen::new_test();
        } // dropped here
        assert!(CLEANUP_CALLED.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn drop_runs_on_panic() {
        CLEANUP_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _screen = LiveScreen::new_test();
            panic!("simulated panic");
        }));
        assert!(CLEANUP_CALLED.load(std::sync::atomic::Ordering::SeqCst));
    }

    // -- Rendering ----------------------------------------------------------

    #[test]
    fn renders_status_frame() {
        // Build a small "status" buffer simulating a rendered frame
        let text = "> task  0:42";
        let width = text.len() as u16;
        let mut src = Buffer::empty(Rect::new(0, 0, width, 1));
        for (i, ch) in text.chars().enumerate() {
            src[(i as u16, 0)].set_symbol(&ch.to_string());
        }

        let mut term = test_terminal(width + 4, 3);
        term.draw(|f| {
            f.render_widget(BlitWidget::new(src), f.area());
        })
        .unwrap();

        let buf = term.backend().buffer();
        for (i, ch) in text.chars().enumerate() {
            assert_eq!(buf[(i as u16, 0)].symbol(), &ch.to_string());
        }
    }

    // -- BlitWidget ---------------------------------------------------------

    #[test]
    fn blit_copies_cells() {
        let mut src = Buffer::empty(Rect::new(0, 0, 3, 2));
        src[(0, 0)].set_symbol("A");
        src[(1, 0)].set_symbol("B");
        src[(2, 0)].set_symbol("C");
        src[(0, 1)].set_symbol("D");
        src[(1, 1)].set_symbol("E");
        src[(2, 1)].set_symbol("F");

        let mut term = test_terminal(5, 3);
        term.draw(|f| {
            f.render_widget(BlitWidget::new(src), f.area());
        })
        .unwrap();

        let backend = term.backend();
        let buf = backend.buffer();
        assert_eq!(buf[(0, 0)].symbol(), "A");
        assert_eq!(buf[(1, 0)].symbol(), "B");
        assert_eq!(buf[(2, 0)].symbol(), "C");
        assert_eq!(buf[(0, 1)].symbol(), "D");
        assert_eq!(buf[(1, 1)].symbol(), "E");
        assert_eq!(buf[(2, 1)].symbol(), "F");
    }

    #[test]
    fn blit_clips_to_area() {
        // Source is 4×2 but destination area is 2×1
        let mut src = Buffer::empty(Rect::new(0, 0, 4, 2));
        src[(0, 0)].set_symbol("X");
        src[(1, 0)].set_symbol("Y");
        src[(2, 0)].set_symbol("Z");
        src[(3, 0)].set_symbol("W");

        let mut term = test_terminal(2, 1);
        term.draw(|f| {
            f.render_widget(BlitWidget::new(src), f.area());
        })
        .unwrap();

        let backend = term.backend();
        let buf = backend.buffer();
        assert_eq!(buf[(0, 0)].symbol(), "X");
        assert_eq!(buf[(1, 0)].symbol(), "Y");
    }

    #[test]
    fn blit_preserves_style() {
        let mut src = Buffer::empty(Rect::new(0, 0, 1, 1));
        src[(0, 0)].set_symbol("S");
        src[(0, 0)].set_style(Style::default().fg(Color::Rgb(255, 0, 0)));

        let mut term = test_terminal(1, 1);
        term.draw(|f| {
            f.render_widget(BlitWidget::new(src), f.area());
        })
        .unwrap();

        let backend = term.backend();
        let buf = backend.buffer();
        assert_eq!(buf[(0, 0)].symbol(), "S");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(255, 0, 0));
    }

    // -- Thread-local guard -------------------------------------------------

    #[test]
    fn live_screen_active_flag_default_false() {
        // In tests, no LiveScreen is created, so the flag should be false.
        assert!(!is_live_screen_active());
    }

    // -- restore_terminal ---------------------------------------------------

    #[test]
    fn restore_terminal_sets_cleanup_flag() {
        CLEANUP_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);
        restore_terminal();
        assert!(CLEANUP_CALLED.load(std::sync::atomic::Ordering::SeqCst));
    }
}

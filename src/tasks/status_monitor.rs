//! Real-time status monitoring for task execution
//!
//! Provides live terminal visualization of task progress during sync execution.
//! Shows subtasks and comments as they're created by the working agent.

use std::io::{stderr, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    cursor::MoveUp,
    terminal::{Clear, ClearType},
    ExecutableCommand,
};

use super::graph::{materialize_graph, TaskGraph};
use super::storage::read_events;
use super::types::{Task, TaskStatus};
use crate::agents::MonitoredChild;
use crate::error::Result;
use crate::tui;
use crate::tui::theme::{detect_mode, Theme};

/// Default polling interval in milliseconds
const DEFAULT_POLL_INTERVAL_MS: u64 = 500;

/// Reason for monitor to stop
#[derive(Debug, Clone)]
pub enum MonitorExitReason {
    /// Task reached terminal state (closed or stopped)
    TaskCompleted,
    /// User pressed Ctrl+C to detach
    UserDetached,
    /// Agent process exited without task reaching terminal state
    AgentExited {
        /// Captured stderr output from the agent (if any)
        stderr: String,
    },
}

/// Monitor for real-time task status updates
pub struct StatusMonitor {
    /// The root task being monitored
    task_id: String,
    /// Number of events at last poll (to detect changes)
    last_event_count: usize,
    /// Polling interval
    poll_interval: Duration,
    /// Flag to track if we've already rendered initial state
    has_rendered: bool,
    /// Atomic flag to signal when to stop (for Ctrl+C handling)
    stop_flag: Arc<AtomicBool>,
    /// Number of lines rendered in the last frame (for cursor-up redraw)
    last_line_count: u16,
}

impl StatusMonitor {
    /// Create a new status monitor for a task
    #[must_use]
    pub fn new(task_id: &str) -> Self {
        let poll_interval_ms = std::env::var("AIKI_STATUS_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS);

        Self {
            task_id: task_id.to_string(),
            last_event_count: 0,
            poll_interval: Duration::from_millis(poll_interval_ms),
            has_rendered: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            last_line_count: 0,
        }
    }

    /// Get a clone of the stop flag for signal handling
    #[must_use]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
    }

    /// Poll for new events and update display if state changed
    ///
    /// Returns Ok(true) if task reached terminal state (closed/stopped)
    pub fn poll_and_display(&mut self, cwd: &Path) -> Result<bool> {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);

        // Find the root task
        let root_task = match graph.tasks.get(&self.task_id) {
            Some(task) => task,
            None => return Ok(false), // Task not found yet, keep waiting
        };

        // Check if we should update display (new events since last poll)
        let should_render = events.len() != self.last_event_count || !self.has_rendered;

        if should_render {
            self.last_event_count = events.len();
            self.render_task_tree(&graph, root_task)?;
            self.has_rendered = true;
        }

        // Check if task reached terminal state
        let is_terminal = matches!(root_task.status, TaskStatus::Closed | TaskStatus::Stopped);

        Ok(is_terminal)
    }

    /// Monitor until task completion, detach, or agent exit (using MonitoredChild)
    ///
    /// This version properly handles zombie processes by using `try_wait()` on the
    /// child process instead of checking if the PID is alive with `kill(pid, 0)`.
    ///
    /// Returns the reason why monitoring stopped.
    pub fn monitor_until_complete_with_child(
        &mut self,
        cwd: &Path,
        child: &mut MonitoredChild,
    ) -> Result<MonitorExitReason> {
        // Initial render
        let _ = self.poll_and_display(cwd);

        loop {
            // Check stop flag (Ctrl+C)
            if self.stop_flag.load(Ordering::Relaxed) {
                self.render_detach_message()?;
                return Ok(MonitorExitReason::UserDetached);
            }

            // Sleep for poll interval
            std::thread::sleep(self.poll_interval);

            // Check if agent process exited using try_wait()
            // This properly handles zombie processes by calling wait() internally
            match child.try_wait() {
                Ok(Some(_exit_status)) => {
                    // Agent exited - capture stderr and do one final poll to check task status
                    let stderr = child.read_stderr();
                    match self.poll_and_display(cwd) {
                        Ok(true) => return Ok(MonitorExitReason::TaskCompleted),
                        _ => return Ok(MonitorExitReason::AgentExited { stderr }),
                    }
                }
                Ok(None) => {
                    // Process is still running, continue monitoring
                }
                Err(e) => {
                    // Error checking process status - treat as exited
                    eprintln!("\nError checking agent status: {}", e);
                    let stderr = child.read_stderr();
                    match self.poll_and_display(cwd) {
                        Ok(true) => return Ok(MonitorExitReason::TaskCompleted),
                        _ => return Ok(MonitorExitReason::AgentExited { stderr }),
                    }
                }
            }

            // Poll and update display
            match self.poll_and_display(cwd) {
                Ok(true) => return Ok(MonitorExitReason::TaskCompleted),
                Ok(false) => continue,
                Err(e) => {
                    // Log error but continue monitoring
                    eprintln!("\nError polling task: {}", e);
                    continue;
                }
            }
        }
    }

    /// Render the task tree to stderr
    ///
    /// Uses cursor-up movement to overwrite the previous frame in place.
    /// This is more reliable than `SavePosition`/`RestorePosition` which breaks
    /// when terminal output scrolls past the bottom of the visible area.
    fn render_task_tree(&mut self, graph: &TaskGraph, root_task: &Task) -> Result<()> {
        let mut stderr = stderr();

        // Move cursor up to overwrite the previous frame
        if self.has_rendered && self.last_line_count > 0 {
            stderr.execute(MoveUp(self.last_line_count))?;
            stderr.execute(Clear(ClearType::FromCursorDown))?;
        }

        // Resolve epic from build task's data
        let (epic, subtasks) = if let Some(epic_id) =
            root_task.data.get("epic").or_else(|| root_task.data.get("target"))
        {
            if let Some(epic_task) = graph.tasks.get(epic_id) {
                let subs = self.get_sorted_subtasks(graph, epic_id);
                (epic_task, subs)
            } else {
                (root_task, self.get_sorted_subtasks(graph, &root_task.id))
            }
        } else {
            (root_task, self.get_sorted_subtasks(graph, &root_task.id))
        };

        let plan_path = epic.data.get("plan").map(|s| s.as_str()).unwrap_or("");
        let subtask_refs: Vec<&Task> = subtasks.into_iter().collect();
        let theme = Theme::from_mode(detect_mode());
        let view = tui::builder::build_workflow_view(epic, &subtask_refs, plan_path, graph);
        let buf = tui::views::workflow::render_workflow(&view, &theme);
        let ansi = tui::buffer_ansi::buffer_to_ansi(&buf);

        let footer = " [Ctrl+C to detach]";
        writeln!(stderr, "{}", ansi)?;
        writeln!(stderr)?;
        writeln!(stderr, "{}", footer)?;
        stderr.flush()?;

        // Track total terminal lines so we can move back up on next render.
        // Must account for line wrapping: a logical line wider than the terminal
        // occupies multiple terminal rows.
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);

        let content_terminal_lines: u16 = count_terminal_lines(&ansi, term_width);
        // +1 for the empty line, +1 for the footer line
        self.last_line_count = content_terminal_lines + 2;
        self.has_rendered = true;
        Ok(())
    }

    /// Get sorted subtasks for a parent task (sorted by creation time)
    fn get_sorted_subtasks<'a>(&self, graph: &'a TaskGraph, parent_id: &str) -> Vec<&'a Task> {
        let mut subtasks = graph.children_of(parent_id);
        subtasks.sort_by_key(|t| t.created_at);
        subtasks
    }

    /// Render detach message when Ctrl+C is pressed
    fn render_detach_message(&self) -> Result<()> {
        let mut stderr = stderr();
        writeln!(stderr)?;
        writeln!(
            stderr,
            "Detached. Task {} still running. Use `aiki task show {}` to check status.",
            &self.task_id[..8.min(self.task_id.len())],
            self.task_id
        )?;
        stderr.flush()?;
        Ok(())
    }
}

/// Count how many terminal rows an ANSI string occupies, accounting for line wrapping.
///
/// Walks the string, tracking visible character width per logical line.
/// When a `\n` is encountered or the visible width exceeds `term_width`,
/// a new terminal row is counted. ANSI escape sequences (CSI codes like
/// `\x1b[0m`, `\x1b[38;2;r;g;bm`) are skipped as they don't occupy
/// visible space.
fn count_terminal_lines(ansi: &str, term_width: usize) -> u16 {
    if term_width == 0 {
        return ansi.chars().filter(|&c| c == '\n').count() as u16 + 1;
    }

    let mut terminal_lines: u16 = 1; // at least one line
    let mut col: usize = 0;
    let mut chars = ansi.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\n' {
            terminal_lines += 1;
            col = 0;
        } else if ch == '\x1b' {
            // Skip ANSI escape sequence: ESC [ ... final_byte
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Consume parameter bytes and intermediate bytes until final byte (0x40-0x7E)
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || next == 'm' {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break; // final byte
                        }
                    } else if next == ';' {
                        chars.next(); // parameter separator
                    } else {
                        break;
                    }
                }
            }
        } else {
            // Deferred wrapping: when cursor is at right margin (col == term_width),
            // writing a character wraps to the next line first.
            if col == term_width {
                terminal_lines += 1;
                col = 0;
            }
            col += 1;
        }
    }

    terminal_lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_monitor_new() {
        let monitor = StatusMonitor::new("test-task-id");
        assert_eq!(monitor.task_id, "test-task-id");
        assert_eq!(monitor.last_event_count, 0);
        assert!(!monitor.has_rendered);
    }

    #[test]
    fn test_count_terminal_lines_no_wrap() {
        // 3 logical lines, each shorter than terminal width
        assert_eq!(count_terminal_lines("abc\ndef\nghi", 80), 3);
    }

    #[test]
    fn test_count_terminal_lines_with_wrap() {
        // 10 chars in a 5-column terminal → 2 terminal rows (deferred wrap)
        assert_eq!(count_terminal_lines("1234567890", 5), 2);
    }

    #[test]
    fn test_count_terminal_lines_exact_width() {
        // Exactly 5 chars in a 5-column terminal → 1 row (deferred wrap:
        // cursor sits at right margin, only wraps when next char is written)
        assert_eq!(count_terminal_lines("12345", 5), 1);
    }

    #[test]
    fn test_count_terminal_lines_one_past_width() {
        // 6 chars in 5-column terminal → 2 rows (6th char triggers wrap)
        assert_eq!(count_terminal_lines("123456", 5), 2);
    }

    #[test]
    fn test_count_terminal_lines_ansi_ignored() {
        // ANSI codes should not count toward visible width
        let ansi = "\x1b[38;2;255;0;0mHi\x1b[0m";
        // Visible content is "Hi" (2 chars) - fits in 80 columns
        assert_eq!(count_terminal_lines(ansi, 80), 1);
    }

    #[test]
    fn test_count_terminal_lines_multiline_with_ansi() {
        let ansi = "\x1b[0mline1\x1b[0m\n\x1b[38;2;0;255;0mline2\x1b[0m";
        assert_eq!(count_terminal_lines(ansi, 80), 2);
    }

    #[test]
    fn test_count_terminal_lines_empty() {
        assert_eq!(count_terminal_lines("", 80), 1);
    }

    #[test]
    fn test_monitor_exit_reason_variants() {
        // Test that we can construct all variants
        let _completed = MonitorExitReason::TaskCompleted;
        let _detached = MonitorExitReason::UserDetached;
        let _exited = MonitorExitReason::AgentExited {
            stderr: "test error".to_string(),
        };

        // Test that AgentExited carries stderr
        let exit_reason = MonitorExitReason::AgentExited {
            stderr: "captured error".to_string(),
        };
        if let MonitorExitReason::AgentExited { stderr } = exit_reason {
            assert_eq!(stderr, "captured error");
        } else {
            panic!("Expected AgentExited variant");
        }
    }

}

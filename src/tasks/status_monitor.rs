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

        // Track total lines so we can move back up on next render.
        // ansi content lines + 1 empty line + 1 footer line
        let content_lines = ansi.chars().filter(|&c| c == '\n').count() as u16 + 1;
        self.last_line_count = content_lines + 2;
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

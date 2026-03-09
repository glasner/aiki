//! Real-time status monitoring for task execution
//!
//! Provides live terminal visualization of task progress during sync execution.
//! Shows subtasks and comments as they're created by the working agent.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;

use super::graph::{materialize_graph, TaskGraph};
use super::storage::read_events;
use super::types::{Task, TaskStatus};
use crate::agents::MonitoredChild;
use crate::error::Result;
use crate::tui;
use crate::tui::live_screen::{BlitWidget, ExitReason, LiveScreen};
use crate::tui::theme::{detect_mode, Theme};

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
    /// Monitor encountered persistent failures (e.g., poll errors)
    MonitorFailed { reason: String },
}

/// Monitor for real-time task status updates
pub struct StatusMonitor {
    /// The root task being monitored
    task_id: String,
    /// Number of events at last poll (to detect changes)
    last_event_count: usize,
    /// Flag to track if we've already rendered initial state
    has_rendered: bool,
    /// Atomic flag to signal when to stop (for Ctrl+C handling outside raw mode)
    stop_flag: Arc<AtomicBool>,
}

impl StatusMonitor {
    /// Create a new status monitor for a task
    #[must_use]
    pub fn new(task_id: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            last_event_count: 0,
            has_rendered: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new status monitor with an externally-owned stop flag.
    ///
    /// Used by `ScreenSession` where the stop flag is shared across monitors.
    #[must_use]
    pub fn new_with_stop_flag(task_id: &str, stop_flag: Arc<AtomicBool>) -> Self {
        Self {
            task_id: task_id.to_string(),
            last_event_count: 0,
            has_rendered: false,
            stop_flag,
        }
    }

    /// Get a clone of the stop flag for signal handling
    #[must_use]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
    }

    /// Poll for new events and return the view data if state changed.
    ///
    /// Returns `Ok((changed, is_terminal))` where `changed` indicates new events
    /// were seen and `is_terminal` indicates the task reached a terminal state.
    fn poll(&mut self, cwd: &Path) -> Result<(bool, bool)> {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);

        // Find the root task
        let root_task = match graph.tasks.get(&self.task_id) {
            Some(task) => task,
            None => return Ok((false, false)), // Task not found yet, keep waiting
        };

        // Check if we should update display (new events since last poll)
        let changed = events.len() != self.last_event_count || !self.has_rendered;

        if changed {
            self.last_event_count = events.len();
            self.has_rendered = true;
        }

        // Check if task reached terminal state
        let is_terminal = matches!(root_task.status, TaskStatus::Closed | TaskStatus::Stopped);

        Ok((changed, is_terminal))
    }

    /// Build the workflow view buffer from current task state.
    fn build_view(&self, cwd: &Path) -> Result<ratatui::buffer::Buffer> {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);

        let root_task = match graph.tasks.get(&self.task_id) {
            Some(task) => task,
            None => {
                // Return an empty buffer if task not found yet
                return Ok(ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(
                    0, 0, 80, 1,
                )));
            }
        };

        let (epic, subtasks, focus_task_id) = self.resolve_epic(&graph, root_task);

        let plan_path = epic.data.get("plan").map(|s| s.as_str()).unwrap_or("");
        let subtask_refs: Vec<&Task> = subtasks.into_iter().collect();
        let theme = Theme::from_mode(detect_mode());
        let view = tui::builder::build_workflow_view_focused(
            epic,
            &subtask_refs,
            plan_path,
            &graph,
            focus_task_id,
        );
        Ok(tui::views::workflow::render_workflow(&view, &theme))
    }

    /// Resolve epic from the running task.
    ///
    /// Build/orchestrator tasks store the epic id in data["epic"] or data["target"].
    /// Review tasks link to the epic via a "validates" edge.
    /// Fix tasks link to the epic via a "remediates" edge.
    fn resolve_epic<'a>(
        &self,
        graph: &'a TaskGraph,
        root_task: &'a Task,
    ) -> (&'a Task, Vec<&'a Task>, Option<&'a str>) {
        if let Some(epic_id) = root_task
            .data
            .get("epic")
            .or_else(|| root_task.data.get("target"))
        {
            if let Some(epic_task) = graph.tasks.get(epic_id) {
                let subs = self.get_sorted_subtasks(graph, epic_id);
                return (epic_task, subs, None);
            }
        } else if let Some(epic_id) = graph
            .edges
            .targets(&root_task.id, "validates")
            .first()
            .or_else(|| {
                graph
                    .edges
                    .targets(&root_task.id, "remediates")
                    .first()
            })
        {
            if let Some(epic_task) = graph.tasks.get(epic_id) {
                let subs = self.get_sorted_subtasks(graph, epic_id);
                return (epic_task, subs, Some(root_task.id.as_str()));
            }
        }

        (
            root_task,
            self.get_sorted_subtasks(graph, &root_task.id),
            None,
        )
    }

    /// Get sorted subtasks for a parent task (sorted by creation time)
    fn get_sorted_subtasks<'a>(&self, graph: &'a TaskGraph, parent_id: &str) -> Vec<&'a Task> {
        let mut subtasks = graph.children_of(parent_id);
        subtasks.sort_by_key(|t| t.created_at);
        subtasks
    }

    /// Run the event loop on an existing screen.
    ///
    /// Contains all the monitoring logic: stop flag checking, agent process
    /// exit detection with bounded reconciliation, task state polling, frame
    /// drawing, and error tracking. Used by both `monitor_on_screen` and
    /// `monitor_until_complete_with_child`.
    fn run_event_loop(
        &mut self,
        cwd: &Path,
        child: &mut MonitoredChild,
        screen: &mut LiveScreen,
    ) -> Result<MonitorExitReason> {
        let mut errors: Vec<String> = Vec::new();
        let mut consecutive_poll_failures: usize = 0;

        let exit_reason = screen.run(|screen| {
            // Check stop flag (for SIGINT received outside raw mode)
            if self.stop_flag.load(Ordering::Relaxed) {
                return Ok(Some(ExitReason::UserDetached));
            }

            // Check if agent process exited
            match child.try_wait() {
                Ok(Some(_exit_status)) => {
                    // Agent exited — do bounded reconciliation
                    const RECONCILE_RETRIES: usize = 5;
                    const RECONCILE_DELAY_MS: u64 = 200;

                    let stderr_output = child.read_stderr();
                    for _ in 0..RECONCILE_RETRIES {
                        match self.poll(cwd) {
                            Ok((_, is_terminal)) => {
                                let buf = self.build_view(cwd).ok();
                                draw_frame(screen, buf.as_ref())?;
                                if is_terminal {
                                    return Ok(Some(ExitReason::TaskCompleted));
                                }
                            }
                            Err(e) => {
                                errors.push(format!("Poll error during reconciliation: {}", e));
                            }
                        }
                        std::thread::sleep(Duration::from_millis(RECONCILE_DELAY_MS));
                    }
                    return Ok(Some(ExitReason::AgentExited {
                        stderr: stderr_output,
                    }));
                }
                Ok(None) => {
                    // Still running, continue
                }
                Err(e) => {
                    // Error checking process status — do bounded reconciliation
                    const RECONCILE_RETRIES: usize = 5;
                    const RECONCILE_DELAY_MS: u64 = 200;

                    errors.push(format!("Error checking agent status: {}", e));
                    let stderr_output = child.read_stderr();
                    for _ in 0..RECONCILE_RETRIES {
                        if let Ok((_, true)) = self.poll(cwd) {
                            return Ok(Some(ExitReason::TaskCompleted));
                        }
                        std::thread::sleep(Duration::from_millis(RECONCILE_DELAY_MS));
                    }
                    return Ok(Some(ExitReason::AgentExited {
                        stderr: stderr_output,
                    }));
                }
            }

            // Poll task state
            match self.poll(cwd) {
                Ok((_changed, is_terminal)) => {
                    consecutive_poll_failures = 0;
                    let buf = self.build_view(cwd).ok();
                    draw_frame(screen, buf.as_ref())?;
                    if is_terminal {
                        Ok(Some(ExitReason::TaskCompleted))
                    } else {
                        Ok(None)
                    }
                }
                Err(e) => {
                    consecutive_poll_failures += 1;
                    if consecutive_poll_failures >= 5 {
                        errors.push(format!("Persistent poll failure ({}x): {}", consecutive_poll_failures, e));
                        Ok(Some(ExitReason::MonitorFailed { reason: format!("Persistent poll failure ({}x): {}", consecutive_poll_failures, e) }))
                    } else {
                        // Silently retry — jj contention during agent shutdown is expected.
                        Ok(None)
                    }
                }
            }
        })?;

        // Print any errors collected during monitoring
        for err in &errors {
            eprintln!("{}", err); // stderr-ok: after monitor loop
        }

        // Convert ExitReason to MonitorExitReason
        match exit_reason {
            ExitReason::TaskCompleted => Ok(MonitorExitReason::TaskCompleted),
            ExitReason::UserDetached => Ok(MonitorExitReason::UserDetached),
            ExitReason::MonitorFailed { reason } => Ok(MonitorExitReason::MonitorFailed { reason }),
            ExitReason::AgentExited { stderr } => Ok(MonitorExitReason::AgentExited { stderr }),
        }
    }

    /// Monitor on an existing `LiveScreen` (caller-owned).
    ///
    /// Like `monitor_until_complete_with_child` but uses a shared screen
    /// instead of creating its own. The caller is responsible for screen
    /// lifecycle (creation and cleanup).
    pub fn monitor_on_screen(
        &mut self,
        cwd: &Path,
        child: &mut MonitoredChild,
        screen: &mut LiveScreen,
    ) -> Result<MonitorExitReason> {
        if self.stop_flag.load(Ordering::Relaxed) {
            return Ok(MonitorExitReason::UserDetached);
        }

        self.run_event_loop(cwd, child, screen)
    }

    /// Monitor until task completion, detach, or agent exit using LiveScreen.
    ///
    /// Enters alternate screen mode and runs an event loop that:
    /// - Polls crossterm events (resize, Ctrl+C)
    /// - Checks if agent process exited via `child.try_wait()`
    /// - On agent exit, does bounded reconciliation (up to 5 retries × 200ms)
    /// - Polls task state and draws the frame
    pub fn monitor_until_complete_with_child(
        &mut self,
        cwd: &Path,
        child: &mut MonitoredChild,
    ) -> Result<MonitorExitReason> {
        // Check stop flag before entering the live screen (covers startup window)
        if self.stop_flag.load(Ordering::Relaxed) {
            return Ok(MonitorExitReason::UserDetached);
        }

        // Enter alternate screen
        let mut screen = LiveScreen::new()?;

        // Run the event loop
        let result = self.run_event_loop(cwd, child, &mut screen);

        // Screen drops here, restoring the terminal.
        drop(screen);

        result
    }

    /// Like `monitor_until_complete_with_child` but uses a provided `LiveScreen`
    /// instead of creating a new one.
    ///
    /// Used when a `LoadingScreen` has already entered the alternate screen.
    /// The caller is responsible for screen cleanup (Drop).
    pub fn monitor_until_complete_with_child_on_screen(
        &mut self,
        cwd: &Path,
        child: &mut MonitoredChild,
        screen: &mut LiveScreen,
    ) -> Result<MonitorExitReason> {
        if self.stop_flag.load(Ordering::Relaxed) {
            return Ok(MonitorExitReason::UserDetached);
        }
        self.run_event_loop(cwd, child, screen)
    }
}

/// Draw a single frame with the workflow view and footer.
fn draw_frame(screen: &mut LiveScreen, buf: Option<&ratatui::buffer::Buffer>) -> Result<()> {
    screen.draw(|f| {
        let chunks = Layout::vertical([
            Constraint::Min(0),    // workflow view
            Constraint::Length(1), // footer
        ])
        .split(f.area());

        if let Some(buf) = buf {
            f.render_widget(BlitWidget::new(buf.clone()), chunks[0]);
        }

        let footer = Line::from(" [Ctrl+C to detach]").right_aligned();
        f.render_widget(footer, chunks[1]);
    })
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
        let _monitor_failed = MonitorExitReason::MonitorFailed {
            reason: "test failure".to_string(),
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

        // Test that MonitorFailed carries reason
        let exit_reason = MonitorExitReason::MonitorFailed {
            reason: "poll timeout".to_string(),
        };
        if let MonitorExitReason::MonitorFailed { reason } = exit_reason {
            assert_eq!(reason, "poll timeout");
        } else {
            panic!("Expected MonitorFailed variant");
        }
    }
}

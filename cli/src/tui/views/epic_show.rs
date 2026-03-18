//! Epic show view.
//!
//! Composes PathLine, EpicTree, and StageTrack into a single Buffer
//! representing the full epic detail view.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tasks::types::{Task, TaskOutcome, TaskStatus};
use crate::tui::theme::Theme;
use crate::tui::types::{EpicView, SubtaskLine, SubtaskStatus};
use crate::tui::widgets::epic_tree::EpicTree;
use crate::tui::widgets::path_line::PathLine;
use crate::tui::widgets::stage_track::{PhaseInfo, PhaseState, StageTrack};

/// Render the epic show view into a Buffer.
///
/// Composes three sections vertically:
/// - Row 0: PathLine (file path breadcrumb)
/// - Row 1: blank separator
/// - Rows 2..N: EpicTree (epic headline + subtask tree)
/// - Row N+1: blank separator
/// - Last row: StageTrack (build/review/fix pipeline)
#[allow(dead_code)]
pub fn render_epic_show(
    epic: &Task,
    subtasks: &[&Task],
    plan_path: &str,
    repo_name: &str,
    theme: &Theme,
) -> Buffer {
    let epic_view = task_to_epic_view(epic, subtasks);
    let error_lines = epic_view
        .subtasks
        .iter()
        .filter(|s| s.error.is_some())
        .count() as u16;
    let tree_height = if epic_view.collapsed {
        if epic_view.collapsed_summary.is_some() { 2 } else { 1 }
    } else {
        1 + epic_view.subtasks.len() as u16 + error_lines
    };
    // 1 (path) + 1 (blank) + tree_height + 1 (blank) + 1 (stage track)
    let height = 1 + 1 + tree_height + 1 + 1;
    let area = Rect::new(0, 0, 80, height);
    let mut buf = Buffer::empty(area);

    // Row 0: PathLine
    let path_area = Rect::new(0, 0, 80, 1);
    PathLine::new(repo_name, plan_path, theme).render(path_area, &mut buf);

    // Rows 2..N: EpicTree (row 1 is blank)
    let tree_area = Rect::new(0, 2, 80, tree_height);
    EpicTree::new(&epic_view, theme).render(tree_area, &mut buf);

    // Last row: StageTrack
    let stage_area = Rect::new(0, height - 1, 80, 1);
    let phases = compute_phases(epic, subtasks);
    StageTrack::new(phases, theme).render(stage_area, &mut buf);

    buf
}

/// Convert a Task and its subtasks to an EpicView.
#[allow(dead_code)]
fn task_to_epic_view(epic: &Task, subtasks: &[&Task]) -> EpicView {
    let short_id = if epic.id.len() >= 8 {
        epic.id[..8].to_string()
    } else {
        epic.id.clone()
    };

    let subtask_lines: Vec<SubtaskLine> = subtasks
        .iter()
        .map(|t| {
            let status = match t.status {
                TaskStatus::Closed => match t.closed_outcome {
                    Some(TaskOutcome::Done) | None => SubtaskStatus::Done,
                    Some(TaskOutcome::WontDo) => SubtaskStatus::Failed,
                },
                TaskStatus::InProgress => SubtaskStatus::Active,
                TaskStatus::Open => SubtaskStatus::Pending,
                TaskStatus::Stopped => SubtaskStatus::Failed,
            };

            let agent = t
                .data
                .get("agent_type")
                .cloned()
                .or_else(|| t.assignee.clone());

            let elapsed = format_elapsed_from_task(t);

            let error = match t.status {
                TaskStatus::Stopped => t.stopped_reason.clone(),
                TaskStatus::Closed if t.closed_outcome == Some(TaskOutcome::WontDo) => {
                    t.effective_summary().map(|s| s.to_string())
                }
                _ => None,
            };

            SubtaskLine {
                name: t.name.clone(),
                status,
                agent,
                elapsed,
                error,
            }
        })
        .collect();

    EpicView {
        short_id,
        name: epic.name.clone(),
        subtasks: subtask_lines,
        collapsed: false,
        collapsed_summary: None,
    }
}

/// Format elapsed time from a task's started_at to now or last event.
#[allow(dead_code)]
fn format_elapsed_from_task(task: &Task) -> Option<String> {
    let started = task.started_at?;
    let end = match task.status {
        TaskStatus::Closed => task
            .comments
            .last()
            .map(|c| c.timestamp)
            .unwrap_or(started),
        TaskStatus::InProgress => chrono::Utc::now(),
        TaskStatus::Stopped => task
            .comments
            .last()
            .map(|c| c.timestamp)
            .unwrap_or(started),
        TaskStatus::Open => return None,
    };

    let secs = (end - started).num_seconds().max(0);
    if secs == 0 {
        return None;
    }
    if secs < 60 {
        Some(format!("{}s", secs))
    } else {
        Some(format!("{}m{:02}", secs / 60, secs % 60))
    }
}

/// Derive pipeline phases from the epic and its subtasks.
///
/// - **Build phase**: based on subtask completion — Active if any in-progress,
///   Done if all closed successfully, Failed if any stopped/wont-do.
/// - **Review phase**: Pending (future: detect review tasks).
/// - **Fix phase**: Pending (future: detect fix tasks).
#[allow(dead_code)]
fn compute_phases(_epic: &Task, subtasks: &[&Task]) -> Vec<PhaseInfo> {
    let total = subtasks.len();
    let completed = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed && t.closed_outcome == Some(TaskOutcome::Done))
        .count();
    let failed = subtasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Stopped
                || (t.status == TaskStatus::Closed
                    && t.closed_outcome == Some(TaskOutcome::WontDo))
        })
        .count();
    let in_progress = subtasks
        .iter()
        .any(|t| t.status == TaskStatus::InProgress);

    let build_state = if total == 0 {
        PhaseState::Pending
    } else if failed > 0 {
        PhaseState::Failed
    } else if completed == total {
        PhaseState::Done
    } else if in_progress || completed > 0 {
        PhaseState::Active
    } else {
        PhaseState::Pending
    };

    vec![
        PhaseInfo {
            name: "build",
            state: build_state,
            completed,
            total,
            elapsed: None,
            failed,
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskOutcome, TaskPriority, TaskStatus};
    use chrono::Utc;
    use std::collections::HashMap;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    fn make_task(id: &str, name: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            slug: None,
            task_type: None,
            status,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            summary: None,
            turn_started: None,
                closed_at: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn buf_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut result = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    result.push_str(cell.symbol());
                } else {
                    result.push(' ');
                }
            }
            result.push('\n');
        }
        result
    }

    #[test]
    fn render_all_completed_subtasks() {
        let theme = test_theme();
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Deploy webhooks",
            TaskStatus::InProgress,
        );
        let mut t1 = make_task(
            "aaaabbbbccccddddeeeeffffgggghhhh",
            "Write handler",
            TaskStatus::Closed,
        );
        t1.closed_outcome = Some(TaskOutcome::Done);
        let mut t2 = make_task(
            "iiiijjjjkkkkllllmmmmnnnnoooopppp",
            "Add tests",
            TaskStatus::Closed,
        );
        t2.closed_outcome = Some(TaskOutcome::Done);

        let subtasks: Vec<&Task> = vec![&t1, &t2];
        let buf = render_epic_show(&epic, &subtasks, "ops/now/webhooks.md", "test-repo", &theme);
        let text = buf_text(&buf);

        // Path line should be present
        assert!(text.contains("ops/now/"), "Should contain path directory");
        assert!(text.contains("webhooks.md"), "Should contain path filename");

        // Epic tree should be present
        assert!(text.contains("Deploy webhooks"), "Should contain epic name");
        assert!(text.contains("Write handler"), "Should contain subtask 1");
        assert!(text.contains("Add tests"), "Should contain subtask 2");

        // Stage track should show build done
        assert!(text.contains("build"), "Should contain build phase");
        assert!(text.contains("2/2"), "Should show 2/2 completed");
        assert!(text.contains("review"), "Should contain review phase");
        assert!(text.contains("fix"), "Should contain fix phase");
    }

    #[test]
    fn render_in_progress_subtasks() {
        let theme = test_theme();
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Build feature",
            TaskStatus::InProgress,
        );
        let mut done = make_task(
            "aaaabbbbccccddddeeeeffffgggghhhh",
            "Step one",
            TaskStatus::Closed,
        );
        done.closed_outcome = Some(TaskOutcome::Done);
        let running = make_task(
            "iiiijjjjkkkkllllmmmmnnnnoooopppp",
            "Step two",
            TaskStatus::InProgress,
        );
        let pending = make_task(
            "qqqqrrrrssssttttuuuuvvvvwwwwxxxx",
            "Step three",
            TaskStatus::Open,
        );

        let subtasks: Vec<&Task> = vec![&done, &running, &pending];
        let buf = render_epic_show(&epic, &subtasks, "ops/now/feature.md", "test-repo", &theme);
        let text = buf_text(&buf);

        assert!(text.contains("Build feature"), "Should contain epic name");
        assert!(text.contains("Step one"), "Should contain completed subtask");
        assert!(text.contains("Step two"), "Should contain in-progress subtask");
        assert!(text.contains("Step three"), "Should contain pending subtask");

        // Build phase should be active with 1/3
        assert!(text.contains("1/3"), "Should show 1/3 progress");
    }

    #[test]
    fn render_empty_subtasks() {
        let theme = test_theme();
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Solo epic",
            TaskStatus::InProgress,
        );

        let subtasks: Vec<&Task> = vec![];
        let buf = render_epic_show(&epic, &subtasks, "ops/now/solo.md", "test-repo", &theme);
        let text = buf_text(&buf);

        // Should have path, epic header, and stage track
        assert!(text.contains("solo.md"), "Should contain path filename");
        assert!(text.contains("Solo epic"), "Should contain epic name");
        assert!(text.contains("build"), "Should contain build phase");
        assert!(text.contains("review"), "Should contain review phase");
    }

    #[test]
    fn compute_phases_all_done() {
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);
        let mut t2 = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Closed);
        t2.closed_outcome = Some(TaskOutcome::Done);

        let epic = make_task("c".repeat(32).as_str(), "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![&t1, &t2];
        let phases = compute_phases(&epic, &subtasks);

        assert_eq!(phases[0].state, PhaseState::Done);
        assert_eq!(phases[0].completed, 2);
        assert_eq!(phases[0].total, 2);
        assert_eq!(phases[1].state, PhaseState::Pending);
        assert_eq!(phases[2].state, PhaseState::Pending);
    }

    #[test]
    fn compute_phases_with_failure() {
        let mut done = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        done.closed_outcome = Some(TaskOutcome::Done);
        let stopped = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Stopped);

        let epic = make_task("c".repeat(32).as_str(), "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![&done, &stopped];
        let phases = compute_phases(&epic, &subtasks);

        assert_eq!(phases[0].state, PhaseState::Failed);
        assert_eq!(phases[0].completed, 1);
        assert_eq!(phases[0].failed, 1);
    }

    #[test]
    fn compute_phases_empty() {
        let epic = make_task("c".repeat(32).as_str(), "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![];
        let phases = compute_phases(&epic, &subtasks);

        assert_eq!(phases[0].state, PhaseState::Pending);
        assert_eq!(phases[0].total, 0);
    }
}

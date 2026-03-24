//! Shared graph query and rendering helpers used by all view functions.

use crate::tasks::types::TaskStatus;
use crate::tasks::{Task, TaskGraph};
use crate::tui::app::{Line, LineStyle, SubtaskStatus};
use crate::tui::components::{self, ChildLine, SubtaskData};

// ── Graph query helpers ────────────────────────────────────────────

/// Get ordered subtasks for a parent (by creation time).
/// Uses EdgeStore reverse lookup on "subtask-of" link kind.
pub fn get_subtasks<'a>(graph: &'a TaskGraph, parent_id: &str) -> Vec<&'a Task> {
    let mut children = graph.children_of(parent_id);
    children.sort_by_key(|t| t.created_at);
    children
}

/// Get subtasks that represent actual work items (excludes decompose, review, fix, orchestrator).
pub fn get_work_subtasks<'a>(graph: &'a TaskGraph, parent_id: &str) -> Vec<&'a Task> {
    let mut children = graph.children_of(parent_id);
    children.retain(|t| {
        !matches!(
            t.task_type.as_deref(),
            Some("decompose") | Some("review") | Some("fix") | Some("orchestrator")
        )
    });
    children.sort_by_key(|t| t.created_at);
    children
}

/// Find the decompose subtask of an epic/fix parent.
/// Looks for a child task with task_type == "decompose".
pub fn find_decompose_task<'a>(graph: &'a TaskGraph, parent_id: &str) -> Option<&'a Task> {
    graph
        .children_of(parent_id)
        .into_iter()
        .find(|t| t.task_type.as_deref() == Some("decompose"))
}

/// Whether decompose task exists and is not terminal.
pub fn decompose_in_progress(graph: &TaskGraph, parent_id: &str) -> bool {
    find_decompose_task(graph, parent_id)
        .map(|t| !t.is_terminal())
        .unwrap_or(false)
}

/// Find the review subtask in a build epic.
/// Looks for a child task with task_type == "review".
pub fn find_build_review<'a>(graph: &'a TaskGraph, epic_id: &str) -> Option<&'a Task> {
    graph
        .children_of(epic_id)
        .into_iter()
        .find(|t| t.task_type.as_deref() == Some("review"))
}

/// Find the review subtask in a fix parent.
pub fn find_fix_review<'a>(graph: &'a TaskGraph, fix_parent_id: &str) -> Option<&'a Task> {
    graph
        .children_of(fix_parent_id)
        .into_iter()
        .find(|t| t.task_type.as_deref() == Some("review"))
}

/// Find the regression review task (linked via "validates" to the original review).
pub fn find_regression_review<'a>(
    graph: &'a TaskGraph,
    review_id: &str,
) -> Option<&'a Task> {
    let referrers = graph.edges.referrers(review_id, "validates");
    referrers
        .iter()
        .filter_map(|id| graph.tasks.get(id.as_str()))
        .find(|t| t.task_type.as_deref() == Some("review"))
}

/// Find the fix parent task for a specific iteration.
/// Fix parents are children of the epic with task_type == "fix" and data["iteration"] matching.
pub fn find_fix_parent(graph: &TaskGraph, epic_id: &str, iteration: u16) -> Option<String> {
    graph
        .children_of(epic_id)
        .into_iter()
        .find(|t| {
            t.task_type.as_deref() == Some("fix")
                && t.data
                    .get("iteration")
                    .and_then(|v| v.parse::<u16>().ok())
                    == Some(iteration)
        })
        .map(|t| t.id.clone())
}

/// Get the review task ID associated with an epic.
pub fn review_id_for_epic(graph: &TaskGraph, epic_id: &str) -> String {
    find_build_review(graph, epic_id)
        .map(|t| t.id.clone())
        .unwrap_or_default()
}

/// Count fix iterations in the graph (number of fix children of the epic).
pub fn current_iteration(graph: &TaskGraph, epic_id: &str) -> u16 {
    let fix_count = graph
        .children_of(epic_id)
        .into_iter()
        .filter(|t| t.task_type.as_deref() == Some("fix"))
        .count();
    // First build is iteration 1, fix cycles start at 2
    (fix_count as u16) + 1
}

/// Check if all phases (including fix iterations) are done.
pub fn is_build_complete(graph: &TaskGraph, epic_id: &str) -> bool {
    let epic = match graph.tasks.get(epic_id) {
        Some(t) => t,
        None => return false,
    };
    epic.is_terminal()
}

/// Check if review found zero actionable issues.
pub fn no_actionable_issues(graph: &TaskGraph, review_id: &str) -> bool {
    graph
        .tasks
        .get(review_id)
        .map(|t| t.status == TaskStatus::Closed && extract_issues(t).is_empty())
        .unwrap_or(false)
}

/// Extract issues from a review task's comments.
/// Issues are comments with data["type"] == "issue".
pub fn extract_issues(task: &Task) -> Vec<Issue> {
    task.comments
        .iter()
        .filter(|c| {
            c.data
                .get("type")
                .map(|t| t == "issue")
                .unwrap_or(false)
        })
        .map(|c| Issue {
            title: c.text.clone(),
            severity: c
                .data
                .get("severity")
                .cloned()
                .unwrap_or_else(|| "medium".to_string()),
        })
        .collect()
}

pub struct Issue {
    pub title: String,
    pub severity: String,
}

// ── Rendering helpers ──────────────────────────────────────────────

/// Standard loading placeholder.
pub fn loading_lines() -> Vec<Line> {
    vec![Line {
        indent: 0,
        text: "Reading task graph...".to_string(),
        meta: None,
        style: LineStyle::PhaseHeader { active: true },
        group: 0,
        dimmed: false,
    }]
}

/// Format progress string: "2/5 subtasks completed" or "2/5 subtasks completed, 1 failed"
pub fn format_progress(subtasks: &[&Task]) -> String {
    let total = subtasks.len();
    let done = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed)
        .count();
    let failed = subtasks
        .iter()
        .filter(|t| t.status == TaskStatus::Stopped)
        .count();
    if failed > 0 {
        format!("{}/{} subtasks completed, {} failed", done, total, failed)
    } else {
        format!("{}/{} subtasks completed", done, total)
    }
}

/// Render a review phase + inline issue list (shared by build and fix).
pub fn review_phase_lines(group: u16, review: &Task, graph: &TaskGraph) -> Vec<Line> {
    let _ = graph; // available for future use
    let mut lines = vec![];
    let issues = extract_issues(review);
    let active = !review.is_terminal();

    let children = match review.status {
        TaskStatus::Open => vec![ChildLine::active("starting session...")],
        TaskStatus::InProgress => {
            vec![ChildLine::active_with_elapsed(
                review.latest_heartbeat(),
                review.elapsed_str(),
            )]
        }
        TaskStatus::Closed if !issues.is_empty() => vec![ChildLine::normal(
            &format!("Found {} issues", issues.len()),
            review.elapsed_str(),
        )],
        TaskStatus::Closed => {
            vec![ChildLine::done(&format!("{} approved", crate::tui::theme::SYM_CHECK), review.elapsed_str())]
        }
        _ => vec![],
    };

    lines.extend(components::phase(
        group,
        "review",
        review.agent_label(),
        active,
        children,
    ));

    if !issues.is_empty() {
        lines.extend(components::blank());
        let issue_texts: Vec<String> = issues.iter().map(|i| i.title.clone()).collect();
        lines.extend(components::issues(group, &issue_texts));
    }

    lines
}

// ── SubtaskData conversion ─────────────────────────────────────────

/// Convert a task to SubtaskData for rendering.
/// `in_active_lane` determines whether pending tasks show ○ (true) or ◌ (false).
pub fn subtask_data_from_task(task: &Task, in_active_lane: bool) -> SubtaskData {
    let status = match task.status {
        TaskStatus::Open => {
            if task.claimed_by_session.is_some() {
                SubtaskStatus::Assigned
            } else if in_active_lane {
                SubtaskStatus::Pending
            } else {
                SubtaskStatus::PendingUnassigned
            }
        }
        TaskStatus::Reserved => SubtaskStatus::Assigned,
        TaskStatus::InProgress => SubtaskStatus::Active,
        TaskStatus::Closed => SubtaskStatus::Done,
        TaskStatus::Stopped => SubtaskStatus::Failed,
    };
    SubtaskData {
        name: task.name.clone(),
        status,
        elapsed: task.elapsed_str(),
    }
}

impl From<&&Task> for SubtaskData {
    fn from(task: &&Task) -> Self {
        subtask_data_from_task(task, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::EdgeStore;
    use crate::tasks::types::{FastHashMap, TaskOutcome, TaskPriority, TaskStatus};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_task(id: &str, name: &str, status: TaskStatus, task_type: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            slug: None,
            task_type: task_type.map(|s| s.to_string()),
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
            closed_outcome: if status == TaskStatus::Closed {
                Some(TaskOutcome::Done)
            } else {
                None
            },
            summary: None,
            turn_started: None,
            closed_at: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn make_graph() -> TaskGraph {
        TaskGraph {
            tasks: FastHashMap::default(),
            edges: EdgeStore::default(),
            slug_index: FastHashMap::default(),
        }
    }

    #[test]
    fn get_work_subtasks_filters_non_work_tasks() {
        let mut graph = make_graph();

        let parent = make_task("parent1", "Epic", TaskStatus::InProgress, None);
        let work1 = make_task("work1", "Implement feature A", TaskStatus::Open, None);
        let work2 = make_task("work2", "Implement feature B", TaskStatus::InProgress, None);
        let decompose = make_task("decomp1", "Decompose", TaskStatus::Closed, Some("decompose"));
        let review = make_task("review1", "Review", TaskStatus::Open, Some("review"));
        let fix = make_task("fix1", "Fix issues", TaskStatus::Open, Some("fix"));
        let orchestrator = make_task("orch1", "Orchestrator", TaskStatus::Open, Some("orchestrator"));

        graph.tasks.insert("parent1".to_string(), parent);
        graph.tasks.insert("work1".to_string(), work1);
        graph.tasks.insert("work2".to_string(), work2);
        graph.tasks.insert("decomp1".to_string(), decompose);
        graph.tasks.insert("review1".to_string(), review);
        graph.tasks.insert("fix1".to_string(), fix);
        graph.tasks.insert("orch1".to_string(), orchestrator);

        graph.edges.add("work1", "parent1", "subtask-of");
        graph.edges.add("work2", "parent1", "subtask-of");
        graph.edges.add("decomp1", "parent1", "subtask-of");
        graph.edges.add("review1", "parent1", "subtask-of");
        graph.edges.add("fix1", "parent1", "subtask-of");
        graph.edges.add("orch1", "parent1", "subtask-of");

        // get_subtasks returns all 6 children
        let all = get_subtasks(&graph, "parent1");
        assert_eq!(all.len(), 6);

        // get_work_subtasks excludes decompose, review, fix, orchestrator
        let work = get_work_subtasks(&graph, "parent1");
        assert_eq!(work.len(), 2);
        let ids: Vec<&str> = work.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"work1"));
        assert!(ids.contains(&"work2"));
        assert!(!ids.iter().any(|id| ["decomp1", "review1", "fix1", "orch1"].contains(id)));
    }
}

//! WorkflowView builder — converts raw `Task` data into the `WorkflowView` data model.
//!
//! Bridges the gap between `Task` objects from the task graph and the
//! `WorkflowView` that TUI widgets consume.

use crate::tasks::graph::TaskGraph;
use crate::tasks::types::{Task, TaskOutcome, TaskStatus};
use crate::tui::types::{
    EpicView, FixChild, StageChild, StageState, StageView, SubStageView, SubtaskLine,
    SubtaskStatus, WorkflowView,
};

/// Build a complete `WorkflowView` from an epic task, its subtasks, and the task graph.
pub fn build_workflow_view(
    epic: &Task,
    subtasks: &[&Task],
    plan_path: &str,
    graph: &TaskGraph,
) -> WorkflowView {
    let stages = build_stages(epic, subtasks, plan_path, graph);
    let active_stage = active_stage_name(&stages);

    let epic_view = build_epic_view(epic, subtasks, &active_stage, &stages);

    WorkflowView {
        plan_path: plan_path.to_string(),
        epic: epic_view,
        stages,
        lane_dag: None,
    }
}

/// Convert a `Task` status to a `SubtaskStatus`.
fn task_to_subtask_status(task: &Task) -> SubtaskStatus {
    match task.status {
        TaskStatus::Closed => match task.closed_outcome {
            Some(TaskOutcome::Done) => SubtaskStatus::Done,
            Some(TaskOutcome::WontDo) => SubtaskStatus::Failed,
            None => SubtaskStatus::Done,
        },
        TaskStatus::InProgress => SubtaskStatus::Active,
        TaskStatus::Open => SubtaskStatus::Pending,
        TaskStatus::Stopped => SubtaskStatus::Failed,
    }
}

/// Convert a `Task` status to a `StageState`.
fn task_to_stage_state(task: &Task) -> StageState {
    match task.status {
        TaskStatus::Closed => match task.closed_outcome {
            Some(TaskOutcome::Done) => StageState::Done,
            Some(TaskOutcome::WontDo) => StageState::Failed,
            None => StageState::Done,
        },
        TaskStatus::InProgress => StageState::Active,
        TaskStatus::Open => StageState::Pending,
        TaskStatus::Stopped => StageState::Failed,
    }
}

/// Derive agent display string from task metadata.
fn agent_label(task: &Task) -> Option<String> {
    let agent = task
        .data
        .get("agent_type")
        .map(|s| s.as_str())
        .or(task.assignee.as_deref());

    match agent {
        Some(a) if a.contains("claude-code") || a == "cc" => Some("cc".to_string()),
        Some(a) if a.contains("cursor") || a == "cur" => Some("cur".to_string()),
        _ => None,
    }
}

/// Format elapsed time from started_at to now (in-progress) or last event.
fn format_elapsed(task: &Task) -> Option<String> {
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

/// Get the error text for a failed/stopped task.
fn error_text(task: &Task) -> Option<String> {
    match task.status {
        TaskStatus::Stopped => task.stopped_reason.clone(),
        TaskStatus::Closed if task.closed_outcome == Some(TaskOutcome::WontDo) => {
            task.effective_summary().map(|s| s.to_string())
        }
        _ => None,
    }
}

/// Convert a Task to a SubtaskLine.
fn task_to_subtask_line(task: &Task) -> SubtaskLine {
    SubtaskLine {
        name: task.name.clone(),
        status: task_to_subtask_status(task),
        agent: agent_label(task),
        elapsed: format_elapsed(task),
        error: error_text(task),
    }
}

/// Build the EpicView from the epic task and its subtasks.
fn build_epic_view(epic: &Task, subtasks: &[&Task], active_stage: &str, stages: &[StageView]) -> EpicView {
    let short_id = if epic.id.len() >= 8 {
        epic.id[..8].to_string()
    } else {
        epic.id.clone()
    };

    let subtask_lines: Vec<SubtaskLine> = subtasks.iter().map(|t| task_to_subtask_line(t)).collect();

    // Collapsed during review/fix stages, or when all stages are done
    let all_done = stages.iter().all(|s| s.state == StageState::Done);
    let collapsed = active_stage == "review" || active_stage == "fix" || all_done;

    let collapsed_summary = if collapsed {
        let total_elapsed = compute_total_elapsed(subtasks);
        Some(format!(
            "{} subtasks  {}",
            subtasks.len(),
            total_elapsed.unwrap_or_default()
        ))
    } else {
        None
    };

    EpicView {
        short_id,
        name: epic.name.clone(),
        subtasks: subtask_lines,
        collapsed,
        collapsed_summary,
    }
}

/// Compute total elapsed for all subtasks (for collapsed summary).
fn compute_total_elapsed(subtasks: &[&Task]) -> Option<String> {
    let mut total_secs: i64 = 0;
    for task in subtasks {
        if let Some(started) = task.started_at {
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
                TaskStatus::Open => continue,
            };
            total_secs += (end - started).num_seconds().max(0);
        }
    }

    if total_secs == 0 {
        return None;
    }
    if total_secs < 60 {
        Some(format!("{}s", total_secs))
    } else {
        Some(format!("{}m{:02}", total_secs / 60, total_secs % 60))
    }
}

/// Determine which stage is currently active.
fn active_stage_name(stages: &[StageView]) -> String {
    for stage in stages.iter().rev() {
        if stage.state == StageState::Active || stage.state == StageState::Failed {
            return stage.name.clone();
        }
    }
    // Default to build if nothing is active
    "build".to_string()
}

/// Build the list of stages from the task graph.
fn build_stages(
    epic: &Task,
    subtasks: &[&Task],
    plan_path: &str,
    graph: &TaskGraph,
) -> Vec<StageView> {
    let build_stage = build_build_stage(epic, subtasks, plan_path, graph);
    let review_stage = build_review_stage(epic, graph);
    let fix_stage = build_fix_stage(epic, graph);

    vec![build_stage, review_stage, fix_stage]
}

/// Build the "build" stage from subtask completion.
fn build_build_stage(
    epic: &Task,
    subtasks: &[&Task],
    plan_path: &str,
    graph: &TaskGraph,
) -> StageView {
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

    // Check if there's an active orchestrator or decompose task for this epic.
    // This covers the case where the build has started (orchestrator running or
    // decompose running) but no subtasks have been started/completed yet.
    let has_active_orchestrator = has_active_build_task(epic, plan_path, graph);

    let state = if total == 0 {
        if has_active_orchestrator { StageState::Active } else { StageState::Pending }
    } else if failed > 0 {
        StageState::Failed
    } else if completed == total {
        StageState::Done
    } else if in_progress || completed > 0 || has_active_orchestrator {
        StageState::Active
    } else {
        StageState::Pending
    };

    let progress = if total > 0 {
        Some(format!("{}/{}", completed, total))
    } else {
        None
    };

    // Look for decompose and build sub-stages
    let sub_stages = build_sub_stages(epic, plan_path, graph);

    // Compute elapsed from epic's build orchestrator or the epic itself
    let elapsed = find_orchestrator_elapsed(epic, plan_path, graph);

    StageView {
        name: "build".to_string(),
        state,
        progress,
        elapsed,
        sub_stages,
        children: Vec::new(),
    }
}

/// Check if there's an active orchestrator or decompose task for this epic.
fn has_active_build_task(epic: &Task, plan_path: &str, graph: &TaskGraph) -> bool {
    // Check orchestrators
    for orch_id in graph.edges.referrers(&epic.id, "orchestrates") {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            let orch_plan = orch_task.data.get("plan").map(|s| s.as_str()).unwrap_or("");
            if orch_plan == plan_path && orch_task.status == TaskStatus::InProgress {
                return true;
            }
        }
    }

    // Check decompose tasks (epic depends-on decompose)
    for dep_id in graph.edges.targets(&epic.id, "depends-on") {
        if let Some(dep_task) = graph.tasks.get(dep_id) {
            if dep_task.status == TaskStatus::InProgress {
                return true;
            }
        }
    }

    false
}

/// Find decompose and implement sub-stages within the build stage.
fn build_sub_stages(epic: &Task, plan_path: &str, graph: &TaskGraph) -> Vec<SubStageView> {
    let mut sub_stages = Vec::new();

    // Find decompose task via the epic's depends-on edges.
    // The decompose task is created separately (not as a child of the orchestrator)
    // and the epic has a "depends-on" link to it.
    for dep_id in graph.edges.targets(&epic.id, "depends-on") {
        if let Some(dep_task) = graph.tasks.get(dep_id) {
            if dep_task.task_type.as_deref() == Some("decompose") {
                sub_stages.push(SubStageView {
                    name: "decompose".to_string(),
                    state: task_to_stage_state(dep_task),
                    progress: None,
                    elapsed: format_elapsed(dep_task),
                });
                break;
            }
        }
    }

    // Find orchestrator task linked to this epic for the "implement" sub-stage
    let orchestrator_ids = graph.edges.referrers(&epic.id, "orchestrates");
    for orch_id in orchestrator_ids {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            let orch_plan = orch_task.data.get("plan").map(|s| s.as_str()).unwrap_or("");
            if orch_plan != plan_path {
                continue;
            }

            sub_stages.push(SubStageView {
                name: "implement".to_string(),
                state: task_to_stage_state(orch_task),
                progress: None,
                elapsed: format_elapsed(orch_task),
            });

            break; // Only one orchestrator per epic
        }
    }

    sub_stages
}

/// Find elapsed time from the orchestrator or decompose task.
fn find_orchestrator_elapsed(epic: &Task, plan_path: &str, graph: &TaskGraph) -> Option<String> {
    // Try orchestrator first
    let orchestrator_ids = graph.edges.referrers(&epic.id, "orchestrates");
    for orch_id in orchestrator_ids {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            let orch_plan = orch_task.data.get("plan").map(|s| s.as_str()).unwrap_or("");
            if orch_plan == plan_path {
                if let Some(elapsed) = format_elapsed(orch_task) {
                    return Some(elapsed);
                }
            }
        }
    }

    // Fall back to decompose task (via epic's depends-on)
    for dep_id in graph.edges.targets(&epic.id, "depends-on") {
        if let Some(dep_task) = graph.tasks.get(dep_id) {
            if dep_task.task_type.as_deref() == Some("decompose") {
                return format_elapsed(dep_task);
            }
        }
    }

    None
}

/// Build the "review" stage from review tasks in the graph.
fn build_review_stage(epic: &Task, graph: &TaskGraph) -> StageView {
    // Find review tasks that validate this epic
    let review_ids = graph.edges.referrers(&epic.id, "validates");

    if review_ids.is_empty() {
        return StageView {
            name: "review".to_string(),
            state: StageState::Pending,
            progress: None,
            elapsed: None,
            sub_stages: Vec::new(),
            children: Vec::new(),
        };
    }

    let mut state = StageState::Pending;
    let mut elapsed = None;
    let mut children = Vec::new();
    let mut progress = None;

    for review_id in review_ids {
        if let Some(review_task) = graph.tasks.get(review_id) {
            let review_state = task_to_stage_state(review_task);
            // When a higher-priority state is found, reset elapsed/progress
            // so they derive from the winning review (not a stale one).
            if stage_state_priority(review_state) > stage_state_priority(state) {
                state = review_state;
                elapsed = format_elapsed(review_task);
                progress = derive_review_progress(review_task);
            } else if elapsed.is_none() {
                elapsed = format_elapsed(review_task);
            }

            if progress.is_none() {
                progress = derive_review_progress(review_task);
            }

            // Add review subtasks as children
            let review_children = graph.children_of(review_id);
            for child in review_children {
                children.push(StageChild::Subtask(task_to_subtask_line(child)));
            }
        }
    }

    StageView {
        name: "review".to_string(),
        state,
        progress,
        elapsed,
        sub_stages: Vec::new(),
        children,
    }
}

/// Build the "fix" stage from fix tasks in the graph.
fn build_fix_stage(epic: &Task, graph: &TaskGraph) -> StageView {
    // Find fix tasks that remediate this epic
    let fix_ids = graph.edges.referrers(&epic.id, "remediates");

    if fix_ids.is_empty() {
        return StageView {
            name: "fix".to_string(),
            state: StageState::Pending,
            progress: None,
            elapsed: None,
            sub_stages: Vec::new(),
            children: Vec::new(),
        };
    }

    let mut state = StageState::Pending;
    let mut elapsed = None;
    let mut children = Vec::new();
    let mut remediation_completed = 0usize;
    let mut remediation_total = 0usize;
    let mut review_fix_count = 0u32;

    for fix_id in fix_ids {
        if let Some(fix_task) = graph.tasks.get(fix_id) {
            let fix_state = task_to_stage_state(fix_task);
            if stage_state_priority(fix_state) > stage_state_priority(state) {
                state = fix_state;
            }
            elapsed = elapsed.or_else(|| format_elapsed(fix_task));

            // Add fix subtasks as children
            let fix_children = graph.children_of(fix_id);
            for child in &fix_children {
                // Check if this is a review-fix quality gate or a remediation subtask
                if child.task_type.as_deref() == Some("review") {
                    review_fix_count += 1;
                    children.push(StageChild::Fix(FixChild::ReviewFix {
                        number: Some(review_fix_count),
                        state: task_to_stage_state(child),
                        result: child.effective_summary().map(|s| s.to_string()),
                        agent: agent_label(child),
                        elapsed: format_elapsed(child),
                    }));
                } else {
                    // Remediation subtask — counts toward progress
                    remediation_total += 1;
                    if child.status == TaskStatus::Closed
                        && child.closed_outcome == Some(TaskOutcome::Done)
                    {
                        remediation_completed += 1;
                    }
                    children.push(StageChild::Fix(FixChild::Subtask(task_to_subtask_line(
                        child,
                    ))));
                }
            }
        }
    }

    let progress = if remediation_total > 0 {
        Some(format!("{}/{}", remediation_completed, remediation_total))
    } else {
        None
    };

    StageView {
        name: "fix".to_string(),
        state,
        progress,
        elapsed,
        sub_stages: Vec::new(),
        children,
    }
}

/// Derive review progress string from review task data.
fn derive_review_progress(review_task: &Task) -> Option<String> {
    if review_task.data.get("approved").map(|s| s.as_str()) == Some("true") {
        return Some("approved".to_string());
    }
    if let Some(count) = review_task.data.get("issue_count") {
        if let Ok(n) = count.parse::<usize>() {
            if n > 0 {
                return if n == 1 {
                    Some("1 issue".to_string())
                } else {
                    Some(format!("{} issues", n))
                };
            }
        }
    }
    None
}

/// Priority ordering for stage states (higher = takes precedence).
/// Failed > Done because a failure in any part should surface.
fn stage_state_priority(state: StageState) -> u8 {
    match state {
        StageState::Pending => 0,
        StageState::Active => 2,
        StageState::Done => 1,
        StageState::Failed => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::{EdgeStore, TaskGraph};
    use crate::tasks::types::{FastHashMap, TaskOutcome, TaskPriority, TaskStatus};
    use chrono::Utc;
    use std::collections::HashMap;

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
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn empty_graph() -> TaskGraph {
        TaskGraph {
            tasks: FastHashMap::default(),
            edges: EdgeStore::new(),
            slug_index: FastHashMap::default(),
        }
    }

    #[test]
    fn basic_epic_conversion() {
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
        let t2 = make_task(
            "iiiijjjjkkkkllllmmmmnnnnoooopppp",
            "Add tests",
            TaskStatus::InProgress,
        );

        let subtasks: Vec<&Task> = vec![&t1, &t2];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/webhooks.md", &graph);

        assert_eq!(view.plan_path, "ops/now/webhooks.md");
        assert_eq!(view.epic.short_id, "abcdefgh");
        assert_eq!(view.epic.name, "Deploy webhooks");
        assert_eq!(view.epic.subtasks.len(), 2);
        assert_eq!(view.epic.subtasks[0].status, SubtaskStatus::Done);
        assert_eq!(view.epic.subtasks[0].name, "Write handler");
        assert_eq!(view.epic.subtasks[1].status, SubtaskStatus::Active);
        assert_eq!(view.epic.subtasks[1].name, "Add tests");
    }

    #[test]
    fn build_stage_progress_matches_subtask_completion() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Epic",
            TaskStatus::InProgress,
        );
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);
        let mut t2 = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Closed);
        t2.closed_outcome = Some(TaskOutcome::Done);
        let t3 = make_task("c".repeat(32).as_str(), "T3", TaskStatus::Open);

        let subtasks: Vec<&Task> = vec![&t1, &t2, &t3];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        assert_eq!(view.stages.len(), 3);
        // Build stage
        assert_eq!(view.stages[0].name, "build");
        assert_eq!(view.stages[0].state, StageState::Active);
        assert_eq!(view.stages[0].progress, Some("2/3".to_string()));
        // Review + fix should be pending
        assert_eq!(view.stages[1].name, "review");
        assert_eq!(view.stages[1].state, StageState::Pending);
        assert_eq!(view.stages[2].name, "fix");
        assert_eq!(view.stages[2].state, StageState::Pending);
    }

    #[test]
    fn build_stage_all_done() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Epic",
            TaskStatus::InProgress,
        );
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);
        let mut t2 = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Closed);
        t2.closed_outcome = Some(TaskOutcome::Done);

        let subtasks: Vec<&Task> = vec![&t1, &t2];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        assert_eq!(view.stages[0].state, StageState::Done);
        assert_eq!(view.stages[0].progress, Some("2/2".to_string()));
    }

    #[test]
    fn build_stage_with_failure() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Epic",
            TaskStatus::InProgress,
        );
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);
        let t2 = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Stopped);

        let subtasks: Vec<&Task> = vec![&t1, &t2];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        assert_eq!(view.stages[0].state, StageState::Failed);
    }

    #[test]
    fn empty_subtasks() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Solo",
            TaskStatus::InProgress,
        );
        let subtasks: Vec<&Task> = vec![];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/solo.md", &graph);

        assert_eq!(view.epic.subtasks.len(), 0);
        assert_eq!(view.stages[0].state, StageState::Pending);
        assert_eq!(view.stages[0].progress, None);
    }

    #[test]
    fn subtask_status_mapping() {
        // Closed + Done → Done
        let mut done = make_task("a".repeat(32).as_str(), "Done", TaskStatus::Closed);
        done.closed_outcome = Some(TaskOutcome::Done);
        assert_eq!(task_to_subtask_status(&done), SubtaskStatus::Done);

        // InProgress → Active
        let active = make_task("b".repeat(32).as_str(), "Active", TaskStatus::InProgress);
        assert_eq!(task_to_subtask_status(&active), SubtaskStatus::Active);

        // Open → Pending
        let pending = make_task("c".repeat(32).as_str(), "Pending", TaskStatus::Open);
        assert_eq!(task_to_subtask_status(&pending), SubtaskStatus::Pending);

        // Stopped → Failed
        let failed = make_task("d".repeat(32).as_str(), "Failed", TaskStatus::Stopped);
        assert_eq!(task_to_subtask_status(&failed), SubtaskStatus::Failed);

        // Closed + WontDo → Failed
        let mut wontdo = make_task("e".repeat(32).as_str(), "WontDo", TaskStatus::Closed);
        wontdo.closed_outcome = Some(TaskOutcome::WontDo);
        assert_eq!(task_to_subtask_status(&wontdo), SubtaskStatus::Failed);
    }

    #[test]
    fn agent_label_from_data() {
        let mut task = make_task("a".repeat(32).as_str(), "T", TaskStatus::Open);
        task.data
            .insert("agent_type".to_string(), "claude-code".to_string());
        assert_eq!(agent_label(&task), Some("cc".to_string()));

        let mut task2 = make_task("b".repeat(32).as_str(), "T", TaskStatus::Open);
        task2
            .data
            .insert("agent_type".to_string(), "cursor".to_string());
        assert_eq!(agent_label(&task2), Some("cur".to_string()));
    }

    #[test]
    fn agent_label_from_assignee() {
        let mut task = make_task("a".repeat(32).as_str(), "T", TaskStatus::Open);
        task.assignee = Some("cc".to_string());
        assert_eq!(agent_label(&task), Some("cc".to_string()));
    }

    #[test]
    fn error_text_stopped() {
        let mut task = make_task("a".repeat(32).as_str(), "T", TaskStatus::Stopped);
        task.stopped_reason = Some("Redis down".to_string());
        assert_eq!(error_text(&task), Some("Redis down".to_string()));
    }

    #[test]
    fn error_text_wontdo() {
        let mut task = make_task("a".repeat(32).as_str(), "T", TaskStatus::Closed);
        task.closed_outcome = Some(TaskOutcome::WontDo);
        task.summary = Some("Not needed".to_string());
        assert_eq!(error_text(&task), Some("Not needed".to_string()));
    }

    #[test]
    fn collapsed_during_review() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let review_id = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);

        let subtasks: Vec<&Task> = vec![&t1];

        // Build graph with a review task validating the epic
        let mut graph = empty_graph();
        let review_task = make_task(review_id, "Review epic", TaskStatus::InProgress);
        graph.tasks.insert(review_id.to_string(), review_task);
        graph.edges.add(review_id, epic_id, "validates");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Review is active → epic should be collapsed
        assert!(view.epic.collapsed);
        assert!(view.epic.collapsed_summary.is_some());
        assert!(view.epic.collapsed_summary.unwrap().contains("1 subtasks"));
    }

    #[test]
    fn expanded_during_build() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Epic",
            TaskStatus::InProgress,
        );
        let t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::InProgress);

        let subtasks: Vec<&Task> = vec![&t1];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Build is active → epic should be expanded
        assert!(!view.epic.collapsed);
        assert!(view.epic.collapsed_summary.is_none());
    }

    #[test]
    fn review_stage_with_children() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let review_id = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
        let child_id = "cccccccccccccccccccccccccccccccc";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![];

        let mut graph = empty_graph();
        let review_task = make_task(review_id, "Review", TaskStatus::InProgress);
        let review_child = make_task(child_id, "Explore Scope", TaskStatus::Closed);
        let mut review_child_done = review_child.clone();
        review_child_done.closed_outcome = Some(TaskOutcome::Done);

        graph
            .tasks
            .insert(review_id.to_string(), review_task);
        graph
            .tasks
            .insert(child_id.to_string(), review_child_done);
        graph.edges.add(review_id, epic_id, "validates");
        graph.edges.add(child_id, review_id, "subtask-of");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        assert_eq!(view.stages[1].name, "review");
        assert_eq!(view.stages[1].state, StageState::Active);
        assert_eq!(view.stages[1].children.len(), 1);
        match &view.stages[1].children[0] {
            StageChild::Subtask(line) => {
                assert_eq!(line.name, "Explore Scope");
                assert_eq!(line.status, SubtaskStatus::Done);
            }
            _ => panic!("Expected Subtask child"),
        }
    }

    #[test]
    fn not_collapsed_when_build_done_but_review_pending() {
        let epic = make_task(
            "abcdefghijklmnopqrstuvwxyzabcdef",
            "Epic",
            TaskStatus::InProgress,
        );
        let mut t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Closed);
        t1.closed_outcome = Some(TaskOutcome::Done);

        let subtasks: Vec<&Task> = vec![&t1];
        let graph = empty_graph();
        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Build done + review/fix pending → epic should NOT collapse
        assert_eq!(view.stages[0].state, StageState::Done);
        assert_eq!(view.stages[1].state, StageState::Pending);
        assert_eq!(view.stages[2].state, StageState::Pending);
        assert!(!view.epic.collapsed);
    }

    #[test]
    fn review_progress_from_winning_review() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let old_review_id = "oooooooooooooooooooooooooooooooo";
        let new_review_id = "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![];

        let mut graph = empty_graph();

        // Old review: closed/done with "approved"
        let mut old_review = make_task(old_review_id, "Review v1", TaskStatus::Closed);
        old_review.closed_outcome = Some(TaskOutcome::Done);
        old_review
            .data
            .insert("approved".to_string(), "true".to_string());

        // New review: in-progress with 3 issues
        let mut new_review = make_task(new_review_id, "Review v2", TaskStatus::InProgress);
        new_review
            .data
            .insert("issue_count".to_string(), "3".to_string());

        graph
            .tasks
            .insert(old_review_id.to_string(), old_review);
        graph
            .tasks
            .insert(new_review_id.to_string(), new_review);
        graph.edges.add(old_review_id, epic_id, "validates");
        graph.edges.add(new_review_id, epic_id, "validates");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // State should be Active (from new review, highest priority)
        assert_eq!(view.stages[1].state, StageState::Active);
        // Progress should come from the new review (3 issues), not old ("approved")
        assert_eq!(view.stages[1].progress, Some("3 issues".to_string()));
    }

    #[test]
    fn fix_stage_with_remediation_progress() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let fix_id = "ffffffffffffffffffffffffffffffff";
        let rem1_id = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
        let rem2_id = "ssssssssssssssssssssssssssssssss";
        let rf_id = "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![];

        let mut graph = empty_graph();
        let fix_task = make_task(fix_id, "Fix issues", TaskStatus::InProgress);

        let mut rem1 = make_task(rem1_id, "Fix null check", TaskStatus::Closed);
        rem1.closed_outcome = Some(TaskOutcome::Done);

        let rem2 = make_task(rem2_id, "Fix error handling", TaskStatus::InProgress);

        let mut review_fix = make_task(rf_id, "Review fix", TaskStatus::Open);
        review_fix.task_type = Some("review".to_string());

        graph.tasks.insert(fix_id.to_string(), fix_task);
        graph.tasks.insert(rem1_id.to_string(), rem1);
        graph.tasks.insert(rem2_id.to_string(), rem2);
        graph.tasks.insert(rf_id.to_string(), review_fix);

        graph.edges.add(fix_id, epic_id, "remediates");
        graph.edges.add(rem1_id, fix_id, "subtask-of");
        graph.edges.add(rem2_id, fix_id, "subtask-of");
        graph.edges.add(rf_id, fix_id, "subtask-of");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        assert_eq!(view.stages[2].name, "fix");
        assert_eq!(view.stages[2].state, StageState::Active);
        // Progress counts only remediation subtasks (not review-fix)
        assert_eq!(view.stages[2].progress, Some("1/2".to_string()));
        assert_eq!(view.stages[2].children.len(), 3);

        // Verify review-fix numbering is populated
        let review_fix_child = &view.stages[2].children[2];
        match review_fix_child {
            StageChild::Fix(FixChild::ReviewFix { number, .. }) => {
                assert_eq!(*number, Some(1));
            }
            _ => panic!("Expected ReviewFix child"),
        }
    }

    // ── Decompose sub-stage discovery ────────────────────────────

    #[test]
    fn decompose_substage_found_via_depends_on() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let decompose_id = "dddddddddddddddddddddddddddddddd";
        let orch_id = "oooooooooooooooooooooooooooooooo";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        let subtasks: Vec<&Task> = vec![];

        let mut graph = empty_graph();

        // Decompose task (linked via depends-on, NOT subtask-of orchestrator)
        let mut decompose = make_task(decompose_id, "Decompose plan", TaskStatus::Closed);
        decompose.task_type = Some("decompose".to_string());
        decompose.closed_outcome = Some(TaskOutcome::Done);
        decompose.started_at = Some(Utc::now() - chrono::Duration::seconds(12));
        graph.tasks.insert(decompose_id.to_string(), decompose);
        graph.edges.add(epic_id, decompose_id, "depends-on");

        // Orchestrator task
        let mut orch = make_task(orch_id, "Implement", TaskStatus::InProgress);
        orch.task_type = Some("orchestrator".to_string());
        orch.data.insert("plan".to_string(), "ops/now/test.md".to_string());
        orch.started_at = Some(Utc::now() - chrono::Duration::seconds(30));
        graph.tasks.insert(orch_id.to_string(), orch);
        graph.edges.add(orch_id, epic_id, "orchestrates");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Build stage should have 2 sub-stages: decompose + implement
        assert_eq!(view.stages[0].sub_stages.len(), 2);
        assert_eq!(view.stages[0].sub_stages[0].name, "decompose");
        assert_eq!(view.stages[0].sub_stages[0].state, StageState::Done);
        assert_eq!(view.stages[0].sub_stages[1].name, "implement");
        assert_eq!(view.stages[0].sub_stages[1].state, StageState::Active);
    }

    #[test]
    fn build_active_during_decompose() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let decompose_id = "dddddddddddddddddddddddddddddddd";

        let epic = make_task(epic_id, "Epic", TaskStatus::Open);
        let subtasks: Vec<&Task> = vec![]; // No subtasks yet

        let mut graph = empty_graph();

        // Decompose is in-progress
        let mut decompose = make_task(decompose_id, "Decompose", TaskStatus::InProgress);
        decompose.task_type = Some("decompose".to_string());
        decompose.started_at = Some(Utc::now() - chrono::Duration::seconds(5));
        graph.tasks.insert(decompose_id.to_string(), decompose);
        graph.edges.add(epic_id, decompose_id, "depends-on");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Build should be Active (not Pending), because decompose is running
        assert_eq!(view.stages[0].state, StageState::Active);
        // Sub-stages should show decompose as active
        assert_eq!(view.stages[0].sub_stages.len(), 1);
        assert_eq!(view.stages[0].sub_stages[0].name, "decompose");
        assert_eq!(view.stages[0].sub_stages[0].state, StageState::Active);
    }

    #[test]
    fn build_active_with_orchestrator_but_no_subtask_started() {
        let epic_id = "abcdefghijklmnopqrstuvwxyzabcdef";
        let orch_id = "oooooooooooooooooooooooooooooooo";

        let epic = make_task(epic_id, "Epic", TaskStatus::InProgress);
        // Subtasks exist but none started
        let t1 = make_task("a".repeat(32).as_str(), "T1", TaskStatus::Open);
        let t2 = make_task("b".repeat(32).as_str(), "T2", TaskStatus::Open);
        let subtasks: Vec<&Task> = vec![&t1, &t2];

        let mut graph = empty_graph();

        // Orchestrator is running
        let mut orch = make_task(orch_id, "Implement", TaskStatus::InProgress);
        orch.task_type = Some("orchestrator".to_string());
        orch.data.insert("plan".to_string(), "ops/now/test.md".to_string());
        orch.started_at = Some(Utc::now() - chrono::Duration::seconds(10));
        graph.tasks.insert(orch_id.to_string(), orch);
        graph.edges.add(orch_id, epic_id, "orchestrates");

        let view = build_workflow_view(&epic, &subtasks, "ops/now/test.md", &graph);

        // Build should be Active because orchestrator is running
        assert_eq!(view.stages[0].state, StageState::Active);
        assert_eq!(view.stages[0].progress, Some("0/2".to_string()));
    }
}

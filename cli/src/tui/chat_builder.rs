//! Chat builder — converts TaskGraph data into the Chat data model.
//!
//! Replaces the workflow builder with a narrative pipeline view.
//! Reads the TaskGraph directly and produces a flat list of Chat messages
//! that tell the story of a plan from creation to completion.

use crate::tasks::graph::TaskGraph;
use crate::tasks::lanes::{derive_lanes, lane_status, LaneStatus};
use crate::tasks::manager::get_subtasks;
use crate::tasks::types::{Task, TaskOutcome, TaskStatus};
use crate::tui::types::{
    BlockFooter, Chat, ChatChild, LaneSubtask, Message, MessageKind, Stage,
};

/// Build chat messages directly from the TaskGraph for a given plan.
pub fn build_pipeline_chat(graph: &TaskGraph, plan_path: &str) -> Chat {
    let mut messages = Vec::new();

    // Find the epic task for this plan (implements-plan link)
    let epic = match find_epic(graph, plan_path) {
        Some(e) => e,
        None => return Chat { messages },
    };

    // 1. Plan stage
    build_plan_messages(&mut messages, epic);

    // 2. Decompose
    build_decompose_messages(&mut messages, epic, graph);

    // 3. Build subtasks
    let subtasks = get_subtasks(graph, &epic.id);
    build_build_messages(&mut messages, epic, &subtasks, graph);

    // 4. Review
    build_review_messages(&mut messages, epic, graph);

    // 5. Fix pipeline
    build_fix_messages(&mut messages, epic, graph);

    // 6. Re-review
    build_rereview_messages(&mut messages, epic, graph);

    // 7. Summary
    build_summary_messages(&mut messages, epic, &subtasks, graph);

    Chat { messages }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Find the epic task that implements the given plan path.
fn find_epic<'a>(graph: &'a TaskGraph, plan_path: &str) -> Option<&'a Task> {
    // Look for tasks with implements-plan → plan_path
    for (id, task) in &graph.tasks {
        let targets = graph.edges.targets(id, "implements-plan");
        for target in targets {
            if target.ends_with(plan_path) || target == plan_path {
                return Some(task);
            }
        }
    }
    None
}

/// Convert task status to MessageKind.
fn task_to_kind(task: &Task) -> MessageKind {
    match task.status {
        TaskStatus::Closed => match task.closed_outcome {
            Some(TaskOutcome::Done) => MessageKind::Done,
            Some(TaskOutcome::WontDo) => MessageKind::Error,
            None => MessageKind::Done,
        },
        TaskStatus::InProgress => {
            if task.claimed_by_session.is_none() {
                MessageKind::Active // starting
            } else {
                MessageKind::Active
            }
        }
        TaskStatus::Open => MessageKind::Pending,
        TaskStatus::Stopped => MessageKind::Error,
    }
}

/// Derive agent display label from task metadata.
fn agent_label(task: &Task) -> Option<String> {
    let agent = task
        .data
        .get("agent_type")
        .map(|s| s.as_str())
        .or(task.assignee.as_deref());

    match agent {
        Some(a) if a.contains("claude-code") || a == "cc" || a == "claude" => {
            Some("claude".to_string())
        }
        Some(a) if a.contains("cursor") || a == "cur" => Some("cursor".to_string()),
        Some(a) if a.contains("codex") => Some("codex".to_string()),
        Some(a) if a.contains("gemini") => Some("gemini".to_string()),
        _ => None,
    }
}

/// Get elapsed seconds for a task.
fn elapsed_secs(task: &Task) -> i64 {
    let started = match task.started_at {
        Some(s) => s,
        None => return 0,
    };
    let end = match task.status {
        TaskStatus::Closed => task
            .closed_at
            .or_else(|| task.comments.last().map(|c| c.timestamp))
            .unwrap_or(started),
        TaskStatus::InProgress => chrono::Utc::now(),
        TaskStatus::Stopped => task.comments.last().map(|c| c.timestamp).unwrap_or(started),
        TaskStatus::Open => return 0,
    };
    (end - started).num_seconds().max(0)
}

/// Format seconds as elapsed string.
fn format_secs(secs: i64) -> Option<String> {
    if secs == 0 {
        return None;
    }
    if secs < 60 {
        Some(format!("{}s", secs))
    } else {
        Some(format!("{}m{:02}", secs / 60, secs % 60))
    }
}

/// Format elapsed time for a task.
fn format_elapsed(task: &Task) -> Option<String> {
    format_secs(elapsed_secs(task))
}

/// Build a BlockFooter from a task.
fn build_footer(task: &Task) -> BlockFooter {
    let agent = agent_label(task).unwrap_or_else(|| "agent".to_string());
    let model = task
        .data
        .get("model")
        .cloned()
        .unwrap_or_default();
    let context_pct = task
        .data
        .get("context_pct")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let cost = task
        .data
        .get("cost")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    BlockFooter {
        agent,
        model,
        context_pct,
        cost,
        elapsed: format_elapsed(task),
    }
}

/// Get error text for a failed task.
fn error_text(task: &Task) -> Option<String> {
    match task.status {
        TaskStatus::Stopped => task.stopped_reason.clone(),
        TaskStatus::Closed if task.closed_outcome == Some(TaskOutcome::WontDo) => {
            task.effective_summary().map(|s| s.to_string())
        }
        _ => None,
    }
}

// ── Plan stage ──────────────────────────────────────────────────────

fn build_plan_messages(messages: &mut Vec<Message>, epic: &Task) {
    // "Created plan" with timestamp
    let timestamp = epic.created_at.format("%H:%M").to_string();
    messages.push(Message {
        stage: Stage::Plan,
        kind: MessageKind::Meta,
        text: "Created plan".to_string(),
        meta: Some(timestamp),
        children: Vec::new(),
    });

    // Edit sessions — look for comments with "edit" data
    for comment in &epic.comments {
        if comment.data.get("type").map(|s| s.as_str()) == Some("edit") {
            let agent = comment
                .data
                .get("agent")
                .map(|a| format!("Edited with {}", a))
                .unwrap_or_else(|| "Edited".to_string());
            messages.push(Message {
                stage: Stage::Plan,
                kind: MessageKind::Meta,
                text: agent,
                meta: None,
                children: Vec::new(),
            });
        }
    }
}

// ── Decompose ───────────────────────────────────────────────────────

fn build_decompose_messages(messages: &mut Vec<Message>, epic: &Task, graph: &TaskGraph) {
    // Find decompose task via populated-by link
    let dep_ids = graph.edges.targets(&epic.id, "populated-by");
    if dep_ids.is_empty() {
        return;
    }

    for dep_id in dep_ids {
        let dep_task = match graph.tasks.get(dep_id) {
            Some(t) => t,
            None => continue,
        };

        match dep_task.status {
            TaskStatus::InProgress => {
                // Active: show as agent block
                messages.push(Message {
                    stage: Stage::Build,
                    kind: MessageKind::Active,
                    text: String::new(),
                    meta: None,
                    children: vec![ChatChild::AgentBlock {
                        task_name: "Decomposing plan".to_string(),
                        footer: build_footer(dep_task),
                    }],
                });
            }
            TaskStatus::Closed if dep_task.closed_outcome == Some(TaskOutcome::Done) => {
                let subtask_count = get_subtasks(graph, &epic.id).len();
                messages.push(Message {
                    stage: Stage::Build,
                    kind: MessageKind::Meta,
                    text: format!("Decomposed into {} subtasks", subtask_count),
                    meta: format_elapsed(dep_task),
                    children: Vec::new(),
                });
            }
            _ => {
                // Stopped/failed decompose
                messages.push(Message {
                    stage: Stage::Build,
                    kind: MessageKind::Error,
                    text: "Decompose failed".to_string(),
                    meta: format_elapsed(dep_task),
                    children: Vec::new(),
                });
            }
        }
        break; // Only first decompose
    }
}

// ── Build ───────────────────────────────────────────────────────────

fn build_build_messages(
    messages: &mut Vec<Message>,
    epic: &Task,
    subtasks: &[&Task],
    graph: &TaskGraph,
) {
    if subtasks.is_empty() {
        return;
    }

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
    let all_done = completed == total;
    let all_terminal = completed + failed == total;

    if all_done {
        // Collapsed: "Built N/N subtasks"
        let elapsed = compute_build_elapsed(epic, graph);
        messages.push(Message {
            stage: Stage::Build,
            kind: MessageKind::Done,
            text: format!("Built {}/{} subtasks", completed, total),
            meta: elapsed,
            children: Vec::new(),
        });
    } else if all_terminal && failed > 0 {
        // Build failed
        let elapsed = compute_build_elapsed(epic, graph);
        let mut children = Vec::new();
        for task in subtasks {
            let kind = task_to_kind(task);
            children.push(ChatChild::Subtask {
                name: task.name.clone(),
                status: kind,
                elapsed: format_elapsed(task),
                agent: agent_label(task),
                error: error_text(task),
            });
        }
        messages.push(Message {
            stage: Stage::Build,
            kind: MessageKind::Error,
            text: format!("Build failed: {} subtask{} errored", failed, if failed == 1 { "" } else { "s" }),
            meta: elapsed,
            children,
        });
    } else {
        // Active build: show lane blocks
        build_active_build(messages, epic, subtasks, graph);
    }
}

fn build_active_build(
    messages: &mut Vec<Message>,
    epic: &Task,
    subtasks: &[&Task],
    graph: &TaskGraph,
) {
    // Find orchestrator for lane derivation
    let orch_id = find_orchestrator(epic, graph);
    let mut children = Vec::new();

    if let Some(orch_id) = &orch_id {
        let decomposition = derive_lanes(graph, orch_id);
        let mut assigned_task_ids = std::collections::HashSet::new();

        for lane in &decomposition.lanes {
            let status = lane_status(lane, graph);

            // Collect all task IDs in this lane
            let lane_task_ids: Vec<&str> = lane
                .sessions
                .iter()
                .flat_map(|s| s.task_ids.iter())
                .map(|s| s.as_str())
                .collect();

            for tid in &lane_task_ids {
                assigned_task_ids.insert(tid.to_string());
            }

            match status {
                LaneStatus::Complete => {
                    // Collapsed: flat done subtasks
                    for tid in &lane_task_ids {
                        if let Some(task) = graph.tasks.get(*tid) {
                            children.push(ChatChild::Subtask {
                                name: task.name.clone(),
                                status: MessageKind::Done,
                                elapsed: format_elapsed(task),
                                agent: agent_label(task),
                                error: None,
                            });
                        }
                    }
                }
                LaneStatus::Failed | LaneStatus::Ready | LaneStatus::Blocked => {
                    // Active/blocked/failed: show as lane block
                    let lane_subtasks: Vec<LaneSubtask> = lane_task_ids
                        .iter()
                        .filter_map(|tid| graph.tasks.get(*tid))
                        .map(|task| LaneSubtask {
                            name: task.name.clone(),
                            status: task_to_kind(task),
                            elapsed: format_elapsed(task),
                            error: error_text(task),
                        })
                        .collect();

                    // Find the active task for footer
                    let active_task = lane_task_ids
                        .iter()
                        .filter_map(|tid| graph.tasks.get(*tid))
                        .find(|t| t.status == TaskStatus::InProgress);

                    let footer_task = active_task.or_else(|| {
                        lane_task_ids
                            .iter()
                            .filter_map(|tid| graph.tasks.get(*tid))
                            .last()
                    });

                    if let Some(ft) = footer_task {
                        children.push(ChatChild::LaneBlock {
                            subtasks: lane_subtasks,
                            footer: build_footer(ft),
                        });
                    }
                }
            }
        }

        // Unassigned subtasks (not in any lane)
        for task in subtasks {
            if !assigned_task_ids.contains(&task.id) {
                children.push(ChatChild::Subtask {
                    name: task.name.clone(),
                    status: task_to_kind(task),
                    elapsed: format_elapsed(task),
                    agent: agent_label(task),
                    error: error_text(task),
                });
            }
        }
    } else {
        // No orchestrator — check if any subtask is actively running
        let active_subtask = subtasks
            .iter()
            .find(|t| t.status == TaskStatus::InProgress);

        if let Some(active) = active_subtask {
            // Implicit single lane: wrap all subtasks in a LaneBlock
            let lane_subtasks: Vec<LaneSubtask> = subtasks
                .iter()
                .map(|task| LaneSubtask {
                    name: task.name.clone(),
                    status: task_to_kind(task),
                    elapsed: format_elapsed(task),
                    error: error_text(task),
                })
                .collect();

            children.push(ChatChild::LaneBlock {
                subtasks: lane_subtasks,
                footer: build_footer(active),
            });
        } else {
            // All done/pending — flat subtasks (collapsed form)
            for task in subtasks {
                children.push(ChatChild::Subtask {
                    name: task.name.clone(),
                    status: task_to_kind(task),
                    elapsed: format_elapsed(task),
                    agent: agent_label(task),
                    error: error_text(task),
                });
            }
        }
    }

    if !children.is_empty() {
        messages.push(Message {
            stage: Stage::Build,
            kind: MessageKind::Active,
            text: String::new(),
            meta: None,
            children,
        });
    }
}

/// Find the orchestrator task ID for this epic.
fn find_orchestrator<'a>(epic: &Task, graph: &'a TaskGraph) -> Option<String> {
    let orch_ids = graph.edges.referrers(&epic.id, "orchestrates");
    for orch_id in orch_ids {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            if orch_task.status == TaskStatus::InProgress
                || orch_task.status == TaskStatus::Closed
            {
                return Some(orch_id.clone());
            }
        }
    }
    None
}

/// Compute total build elapsed from decompose + orchestrator times.
fn compute_build_elapsed(epic: &Task, graph: &TaskGraph) -> Option<String> {
    let mut total_secs: i64 = 0;

    for dep_id in graph.edges.targets(&epic.id, "populated-by") {
        if let Some(dep_task) = graph.tasks.get(dep_id) {
            total_secs += elapsed_secs(dep_task);
            break;
        }
    }

    for orch_id in graph.edges.referrers(&epic.id, "orchestrates") {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            total_secs += elapsed_secs(orch_task);
            break;
        }
    }

    format_secs(total_secs)
}

// ── Review ──────────────────────────────────────────────────────────

fn build_review_messages(messages: &mut Vec<Message>, epic: &Task, graph: &TaskGraph) {
    let review_ids = graph.edges.referrers(&epic.id, "validates");
    if review_ids.is_empty() {
        return;
    }

    // Find the most relevant review (prefer active, then latest)
    let review_task = find_best_review(review_ids, graph);
    let review_task = match review_task {
        Some(t) => t,
        None => return,
    };

    match review_task.status {
        TaskStatus::InProgress => {
            messages.push(Message {
                stage: Stage::Review,
                kind: MessageKind::Active,
                text: String::new(),
                meta: None,
                children: vec![ChatChild::AgentBlock {
                    task_name: "Reviewing changes".to_string(),
                    footer: build_footer(review_task),
                }],
            });
        }
        TaskStatus::Closed => {
            let approved =
                review_task.data.get("approved").map(|s| s.as_str()) == Some("true");

            if approved {
                messages.push(Message {
                    stage: Stage::Review,
                    kind: MessageKind::Done,
                    text: "Review passed — approved".to_string(),
                    meta: format_elapsed(review_task),
                    children: Vec::new(),
                });
            } else {
                // Issues found
                let issues = collect_review_issues(review_task);
                let issue_count = issues.len();

                messages.push(Message {
                    stage: Stage::Review,
                    kind: MessageKind::Attention,
                    text: format!(
                        "Review found {} issue{}",
                        issue_count,
                        if issue_count == 1 { "" } else { "s" }
                    ),
                    meta: format_elapsed(review_task),
                    children: issues,
                });
            }
        }
        _ => {
            // Stopped/failed
            messages.push(Message {
                stage: Stage::Review,
                kind: MessageKind::Error,
                text: "Review failed".to_string(),
                meta: format_elapsed(review_task),
                children: Vec::new(),
            });
        }
    }
}

fn find_best_review<'a>(review_ids: &[String], graph: &'a TaskGraph) -> Option<&'a Task> {
    // Prefer in-progress, then latest closed
    let mut best: Option<&Task> = None;
    for id in review_ids {
        if let Some(task) = graph.tasks.get(id) {
            if task.status == TaskStatus::InProgress {
                return Some(task);
            }
            if best.is_none()
                || task.created_at
                    > best
                        .map(|b| b.created_at)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC)
            {
                best = Some(task);
            }
        }
    }
    best
}

/// Collect review issues from task comments.
fn collect_review_issues(review_task: &Task) -> Vec<ChatChild> {
    let mut issues = Vec::new();
    let mut number = 0;

    for comment in &review_task.comments {
        if comment.data.get("issue").map(|s| s.as_str()) == Some("true") {
            number += 1;
            let title = comment.text.clone();
            let location = comment.data.get("file").cloned();
            let description = comment.data.get("description").cloned();

            issues.push(ChatChild::Issue {
                number,
                title,
                location,
                description,
            });
        }
    }

    issues
}

// ── Fix ─────────────────────────────────────────────────────────────

fn build_fix_messages(messages: &mut Vec<Message>, epic: &Task, graph: &TaskGraph) {
    let fix_ids = graph.edges.referrers(&epic.id, "remediates");
    if fix_ids.is_empty() {
        return;
    }

    for fix_id in fix_ids {
        let fix_task = match graph.tasks.get(fix_id) {
            Some(t) => t,
            None => continue,
        };

        let fix_subtasks = get_subtasks(graph, fix_id);

        match fix_task.status {
            TaskStatus::InProgress => {
                if fix_subtasks.is_empty() {
                    // Planning fix
                    messages.push(Message {
                        stage: Stage::Fix,
                        kind: MessageKind::Active,
                        text: String::new(),
                        meta: None,
                        children: vec![ChatChild::AgentBlock {
                            task_name: "Planning fix".to_string(),
                            footer: build_footer(fix_task),
                        }],
                    });
                } else {
                    // Fix subtasks running
                    build_fix_subtask_messages(messages, fix_task, &fix_subtasks, graph);
                }
            }
            TaskStatus::Closed => {
                let total = fix_subtasks.len();
                let completed = fix_subtasks
                    .iter()
                    .filter(|t| {
                        t.status == TaskStatus::Closed
                            && t.closed_outcome == Some(TaskOutcome::Done)
                    })
                    .count();

                if total == 0 {
                    messages.push(Message {
                        stage: Stage::Fix,
                        kind: MessageKind::Done,
                        text: "Fixed issues".to_string(),
                        meta: format_elapsed(fix_task),
                        children: Vec::new(),
                    });
                } else {
                    messages.push(Message {
                        stage: Stage::Fix,
                        kind: MessageKind::Done,
                        text: format!("Fixed {}/{} subtasks", completed, total),
                        meta: format_elapsed(fix_task),
                        children: Vec::new(),
                    });
                }

                // Check for review-fix
                build_review_fix_messages(messages, fix_task, graph);
            }
            _ => {
                messages.push(Message {
                    stage: Stage::Fix,
                    kind: MessageKind::Error,
                    text: "Fix failed".to_string(),
                    meta: format_elapsed(fix_task),
                    children: Vec::new(),
                });
            }
        }
        break; // Only handle first fix
    }
}

fn build_fix_subtask_messages(
    messages: &mut Vec<Message>,
    _fix_task: &Task,
    fix_subtasks: &[&Task],
    _graph: &TaskGraph,
) {
    let mut children = Vec::new();

    for task in fix_subtasks {
        // Skip review-fix tasks
        if task.task_type.as_deref() == Some("review") {
            if task.status == TaskStatus::InProgress {
                children.push(ChatChild::AgentBlock {
                    task_name: "Reviewing fix".to_string(),
                    footer: build_footer(task),
                });
            }
            continue;
        }

        children.push(ChatChild::Subtask {
            name: task.name.clone(),
            status: task_to_kind(task),
            elapsed: format_elapsed(task),
            agent: agent_label(task),
            error: error_text(task),
        });
    }

    if !children.is_empty() {
        messages.push(Message {
            stage: Stage::Fix,
            kind: MessageKind::Active,
            text: String::new(),
            meta: None,
            children,
        });
    }
}

fn build_review_fix_messages(messages: &mut Vec<Message>, fix_task: &Task, graph: &TaskGraph) {
    let fix_children = graph.children_of(&fix_task.id);
    for child in &fix_children {
        if child.task_type.as_deref() != Some("review") {
            continue;
        }

        match child.status {
            TaskStatus::InProgress => {
                messages.push(Message {
                    stage: Stage::Fix,
                    kind: MessageKind::Active,
                    text: String::new(),
                    meta: None,
                    children: vec![ChatChild::AgentBlock {
                        task_name: "Reviewing fix".to_string(),
                        footer: build_footer(child),
                    }],
                });
            }
            TaskStatus::Closed => {
                let approved =
                    child.data.get("approved").map(|s| s.as_str()) == Some("true");
                if approved {
                    messages.push(Message {
                        stage: Stage::Fix,
                        kind: MessageKind::Done,
                        text: "Review-fix passed".to_string(),
                        meta: format_elapsed(child),
                        children: Vec::new(),
                    });
                }
            }
            _ => {}
        }
        break;
    }
}

// ── Re-review ───────────────────────────────────────────────────────

fn build_rereview_messages(messages: &mut Vec<Message>, epic: &Task, graph: &TaskGraph) {
    // Re-review is a second validates link (after fix)
    let review_ids = graph.edges.referrers(&epic.id, "validates");
    if review_ids.len() < 2 {
        return;
    }

    // The re-review is the latest review task (created after the first)
    let mut reviews: Vec<&Task> = review_ids
        .iter()
        .filter_map(|id| graph.tasks.get(id))
        .collect();
    reviews.sort_by_key(|t| t.created_at);

    if let Some(rereview) = reviews.last() {
        // Only show if this is actually a re-review (not the first review)
        if reviews.len() < 2 {
            return;
        }

        match rereview.status {
            TaskStatus::InProgress => {
                messages.push(Message {
                    stage: Stage::ReReview,
                    kind: MessageKind::Active,
                    text: String::new(),
                    meta: None,
                    children: vec![ChatChild::AgentBlock {
                        task_name: "Re-reviewing changes".to_string(),
                        footer: build_footer(rereview),
                    }],
                });
            }
            TaskStatus::Closed => {
                let approved =
                    rereview.data.get("approved").map(|s| s.as_str()) == Some("true");
                if approved {
                    messages.push(Message {
                        stage: Stage::ReReview,
                        kind: MessageKind::Done,
                        text: "Re-review passed — approved".to_string(),
                        meta: format_elapsed(rereview),
                        children: Vec::new(),
                    });
                } else {
                    let issues = collect_review_issues(rereview);
                    let issue_count = issues.len();
                    messages.push(Message {
                        stage: Stage::ReReview,
                        kind: MessageKind::Attention,
                        text: format!(
                            "Re-review found {} issue{}",
                            issue_count,
                            if issue_count == 1 { "" } else { "s" }
                        ),
                        meta: format_elapsed(rereview),
                        children: issues,
                    });
                }
            }
            _ => {
                messages.push(Message {
                    stage: Stage::ReReview,
                    kind: MessageKind::Error,
                    text: "Re-review failed".to_string(),
                    meta: format_elapsed(rereview),
                    children: Vec::new(),
                });
            }
        }
    }
}

// ── Summary ─────────────────────────────────────────────────────────

fn build_summary_messages(
    messages: &mut Vec<Message>,
    epic: &Task,
    subtasks: &[&Task],
    graph: &TaskGraph,
) {
    // Only show summary when the entire pipeline is done
    let all_subtasks_done = !subtasks.is_empty()
        && subtasks.iter().all(|t| {
            t.status == TaskStatus::Closed && t.closed_outcome == Some(TaskOutcome::Done)
        });

    if !all_subtasks_done {
        return;
    }

    // Check review is done
    let review_ids = graph.edges.referrers(&epic.id, "validates");
    let reviews_done = review_ids.iter().all(|id| {
        graph
            .tasks
            .get(id)
            .map_or(true, |t| t.status == TaskStatus::Closed)
    });

    if !reviews_done {
        return;
    }

    // Count iterations (number of review rounds)
    let iterations = review_ids.len();

    // Compute total elapsed
    let total_secs = compute_total_elapsed_secs(epic, subtasks, graph);
    let total_elapsed = format_secs(total_secs).unwrap_or_else(|| "0s".to_string());

    let summary_text = if iterations == 0 {
        format!("Done, {} total", total_elapsed)
    } else {
        format!(
            "Done in {} iteration{}, {} total",
            iterations,
            if iterations == 1 { "" } else { "s" },
            total_elapsed
        )
    };

    messages.push(Message {
        stage: Stage::Summary,
        kind: MessageKind::Summary,
        text: summary_text,
        meta: None,
        children: Vec::new(),
    });

    // Agent summary
    let mut agent_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for task in subtasks {
        if let Some(agent) = agent_label(task) {
            *agent_counts.entry(agent).or_default() += 1;
        }
    }

    if !agent_counts.is_empty() {
        let mut agents: Vec<String> = agent_counts
            .iter()
            .map(|(name, count)| format!("{} ×{}", name, count))
            .collect();
        agents.sort();

        messages.push(Message {
            stage: Stage::Summary,
            kind: MessageKind::Meta,
            text: format!("Agents: {}", agents.join(", ")),
            meta: None,
            children: Vec::new(),
        });
    }
}

fn compute_total_elapsed_secs(epic: &Task, subtasks: &[&Task], graph: &TaskGraph) -> i64 {
    let mut total: i64 = 0;

    // Decompose time
    for dep_id in graph.edges.targets(&epic.id, "populated-by") {
        if let Some(dep_task) = graph.tasks.get(dep_id) {
            total += elapsed_secs(dep_task);
            break;
        }
    }

    // Orchestrator time
    for orch_id in graph.edges.referrers(&epic.id, "orchestrates") {
        if let Some(orch_task) = graph.tasks.get(orch_id) {
            total += elapsed_secs(orch_task);
            break;
        }
    }

    // Subtask time (max of concurrent tasks rather than sum for accuracy)
    // For simplicity, just use the orchestrator/decompose time above
    let _ = subtasks; // Used via orchestrator elapsed

    // Review time
    for review_id in graph.edges.referrers(&epic.id, "validates") {
        if let Some(review_task) = graph.tasks.get(review_id) {
            total += elapsed_secs(review_task);
        }
    }

    // Fix time
    for fix_id in graph.edges.referrers(&epic.id, "remediates") {
        if let Some(fix_task) = graph.tasks.get(fix_id) {
            total += elapsed_secs(fix_task);
        }
    }

    total
}

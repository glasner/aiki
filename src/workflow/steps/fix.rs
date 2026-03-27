use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::tasks::runner::{handle_session_result, task_run, task_run_on_session, TaskRunOptions};
use crate::jj::get_working_copy_change_id;
use crate::tasks::{
    generate_task_id, materialize_graph, read_events, write_event, write_link_event,
    write_link_event_with_autorun, TaskEvent, TaskPriority,
};

use crate::workflow::steps::review::{ReviewScope, ReviewScopeKind};
// TODO: tech debt — create_from_template/TemplateTaskParams live in commands::task but
// should move to tasks::templates to eliminate workflow→commands coupling. The function
// is ~300 lines with private helpers (create_subtasks_from_entries, etc.) and 8+ callers
// across commands/, making the move non-trivial.
use crate::commands::task::{create_from_template, TemplateTaskParams};

/// Run a task with optional TUI display.
pub(crate) fn run_task_with_show_tui(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
    show_tui: bool,
) -> Result<()> {
    if show_tui {
        let result = task_run_on_session(cwd, task_id, options, true)?;
        handle_session_result(cwd, task_id, result, true)?;
    } else {
        task_run(cwd, task_id, options.quiet())?;
    }
    Ok(())
}

/// Create the fix-parent task (container for fix subtasks, like an epic).
///
/// Emits `remediates` link to the review task and `fixes` links to the
/// reviewed targets.
pub(crate) fn create_fix_parent(
    cwd: &Path,
    review_id: &str,
    scope: &ReviewScope,
    assignee: &Option<String>,
    autorun: bool,
) -> Result<String> {
    let fix_parent_id = generate_task_id("fix-parent");
    let working_copy = get_working_copy_change_id(cwd);

    let name = format!("Fix: {}", scope.name());
    let mut data = HashMap::new();
    data.insert("review".to_string(), review_id.to_string());

    // Add scope data
    for (k, v) in scope.to_data() {
        data.insert(k, v);
    }

    let event = TaskEvent::Created {
        task_id: fix_parent_id.clone(),
        name,
        slug: None,
        task_type: None,
        priority: TaskPriority::P2,
        assignee: assignee.clone(),
        sources: vec![format!("task:{}", review_id)],
        template: None,
        working_copy,
        instructions: None,
        data,
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &event)?;

    // Emit remediates link: fix-parent remediates the review task
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let autorun_opt = if autorun { Some(true) } else { None };
    write_link_event_with_autorun(
        cwd,
        &graph,
        "remediates",
        &fix_parent_id,
        review_id,
        autorun_opt,
    )?;

    // Emit fixes link to the target(s) that were reviewed
    let reviewed_targets = graph.edges.targets(review_id, "validates");
    for target in reviewed_targets {
        write_link_event(cwd, &graph, "fixes", &fix_parent_id, target)?;
    }

    // Add fix-parent as subtask of the original task (epic) so that
    // `task diff <epic>` includes fix changes in the 2-stage review.
    if scope.kind == ReviewScopeKind::Task {
        let events = read_events(cwd)?;
        let graph = materialize_graph(&events);
        write_link_event(cwd, &graph, "subtask-of", &fix_parent_id, &scope.id)?;
    }

    Ok(fix_parent_id)
}

/// Create a plan-fix task from the `fix` template.
pub(crate) fn create_plan_fix_task(
    cwd: &Path,
    review_id: &str,
    fix_parent_id: &str,
    assignee: &Option<String>,
    template_override: Option<&str>,
) -> Result<String> {
    let mut data = HashMap::new();
    data.insert("review".to_string(), review_id.to_string());
    data.insert("target".to_string(), fix_parent_id.to_string());

    let params = TemplateTaskParams {
        template_name: template_override.unwrap_or("fix").to_string(),
        data,
        sources: vec![format!("task:{}", review_id)],
        assignee: assignee.clone(),
        ..Default::default()
    };

    create_from_template(cwd, params)
}

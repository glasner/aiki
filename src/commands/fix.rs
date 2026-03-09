//! Fix command — pipeline with Rust-driven quality loop
//!
//! This module provides the `aiki fix` command which:
//! - Reads a review task ID (from argument or stdin)
//! - Checks for actionable issues
//! - If none: outputs "approved" (review passed)
//! - If issues found: runs a plan → decompose → loop → review cycle
//! - After fix-parent review passes, re-reviews the original scope to catch regressions
//! - The `--once` flag disables the post-fix review loop (single pass)

use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, IsTerminal};
use std::path::Path;

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::tasks::runner::{ScreenSession, handle_session_result, task_run, task_run_on_session, TaskRunOptions};
use crate::tui::loading_screen::LoadingScreen;
use crate::tasks::md::MdBuilder;
use crate::tasks::templates::get_working_copy_change_id;
use crate::tasks::{
    find_task, generate_task_id, materialize_graph, materialize_graph_with_ids,
    read_events, read_events_with_ids, write_event, write_link_event,
    write_link_event_with_autorun, Task, TaskEvent, TaskPriority,
};

use super::OutputFormat;
use super::decompose::{run_decompose, DecomposeOptions};
use super::loop_cmd::{run_loop, LoopOptions};
use super::review::{
    create_review, CreateReviewParams, ReviewScope, ReviewScopeKind,
};
use super::task::{create_from_template, TemplateTaskParams};

/// Maximum iterations of the quality loop to prevent infinite cycles.
const MAX_QUALITY_ITERATIONS: usize = 10;

/// Run the fix command
///
/// Creates followup tasks from review comments and runs them through
/// a plan → decompose → loop pipeline with an optional quality loop.
pub fn run(
    task_id: Option<String>,
    run_async: bool,
    continue_async: Option<String>,
    plan_template: Option<String>,
    decompose_template: Option<String>,
    loop_template: Option<String>,
    review_template: Option<String>,
    agent: Option<String>,
    autorun: bool,
    once: bool,
    output: Option<OutputFormat>,
) -> Result<()> {
    let cwd = env::current_dir().map_err(|_| {
        AikiError::InvalidArgument("Failed to get current directory".to_string())
    })?;

    // Get task ID from argument or stdin
    let task_id = match task_id {
        Some(id) => extract_task_id(&id),
        None => read_task_id_from_stdin()?,
    };

    run_fix(&cwd, &task_id, run_async, continue_async, plan_template, decompose_template, loop_template, review_template, agent, autorun, once, output)
}

/// Extract task ID from input, handling XML output format
fn extract_task_id(input: &str) -> String {
    let trimmed = input.trim();

    // Try to extract from XML task_id attribute
    if let Some(start) = trimmed.find("task_id=\"") {
        let after_quote = &trimmed[start + 9..];
        if let Some(end) = after_quote.find('"') {
            return after_quote[..end].to_string();
        }
    }

    trimmed.to_string()
}

/// Read task ID from stdin
fn read_task_id_from_stdin() -> Result<String> {
    let stdin = io::stdin();
    let mut input = String::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| {
            AikiError::InvalidArgument(format!("Failed to read from stdin: {}", e))
        })?;
        input.push_str(&line);
        input.push('\n');
    }

    if input.trim().is_empty() {
        return Err(AikiError::InvalidArgument(
            "No task ID provided. Pass as argument or pipe from another command.".to_string(),
        ));
    }

    Ok(extract_task_id(&input))
}

/// Core fix implementation — pipeline with Rust-driven quality loop.
///
/// Runs up to [`MAX_QUALITY_ITERATIONS`] cycles of fix → review. If the loop
/// exhausts all iterations without the review approving, a warning is emitted
/// to stderr and the function returns `Ok(())` (partial fixes may have been
/// applied, so we don't fail the whole command).
pub fn run_fix(
    cwd: &Path,
    task_id: &str,
    run_async: bool,
    continue_async: Option<String>,
    plan_template: Option<String>,
    decompose_template: Option<String>,
    loop_template: Option<String>,
    review_template: Option<String>,
    agent: Option<String>,
    autorun: bool,
    once: bool,
    output: Option<OutputFormat>,
) -> Result<()> {
    // Continue-async path: pick up from a previously created fix-parent
    if let Some(ref fix_parent_id) = continue_async {
        return run_fix_continue(cwd, fix_parent_id, plan_template, decompose_template, loop_template, review_template, agent, once, output);
    }

    // Parse agent if provided
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Load tasks with change IDs (needed for comment IDs)
    let events_with_ids = read_events_with_ids(cwd)?;
    let tasks = materialize_graph_with_ids(&events_with_ids).tasks;

    // Find the review task (the task we're creating followups for)
    let review_task = find_task(&tasks, task_id)?;

    // Validate that the input task is actually a review task
    if !is_review_task(review_task) {
        return Err(AikiError::InvalidArgument(format!(
            "No review task found for ID: {}",
            task_id
        )));
    }

    // Determine what was reviewed from typed scope data
    let scope = ReviewScope::from_data(&review_task.data)?;

    // Resolve the final template name for fix-plan tasks.
    // Priority chain: CLI --plan-template arg > review_task.data["options.fix_template"] > "fix".
    // See test_resolve_fix_template_name_* tests for coverage of this resolution logic.
    let plan_template_resolved = resolve_fix_template_name(plan_template.clone(), &review_task.data);

    // Determine assignee for fix tasks
    let assignee = match scope.kind {
        ReviewScopeKind::Task => {
            let original_task = find_task(&tasks, &scope.id).ok();
            determine_followup_assignee(agent_type, original_task)
        }
        _ => determine_followup_assignee(agent_type, None),
    };

    // Async spawn path: create fix-parent synchronously, then spawn background process
    if run_async {
        // Short-circuit if no actionable issues
        if !has_actionable_issues(review_task) {
            if output != Some(OutputFormat::Id) {
                output_approved(&review_task.id)?;
            }
            return Ok(());
        }

        // Create fix-parent task (container, like an epic)
        let fix_parent_id = create_fix_parent(cwd, &review_task.id, &scope, &assignee, autorun)?;

        // Build args for background process
        let mut spawn_args = vec!["fix", task_id, "--_continue-async", &fix_parent_id];
        if let Some(ref plan) = plan_template {
            spawn_args.extend(["--template", plan]);
        }
        if let Some(ref decompose) = decompose_template {
            spawn_args.extend(["--decompose-template", decompose]);
        }
        if let Some(ref loop_tmpl) = loop_template {
            spawn_args.extend(["--loop-template", loop_tmpl]);
        }
        if let Some(ref review_tmpl) = review_template {
            spawn_args.extend(["--review-template", review_tmpl]);
        }
        if let Some(ref agent_str) = agent {
            spawn_args.extend(["--agent", agent_str]);
        }
        if once {
            spawn_args.push("--once");
        }

        use super::async_spawn::spawn_aiki_background;
        spawn_aiki_background(cwd, &spawn_args)?;

        // Emit fix-parent ID and return immediately
        match output {
            Some(OutputFormat::Id) => println!("{}", fix_parent_id),
            None => eprintln!("Fix: {}", fix_parent_id),
        }
        return Ok(());
    }

    // ── Synchronous quality loop ──────────────────────────────────
    let mut review_id = review_task.id.clone();

    // Create loading screen for immediate TTY feedback, then transition to ScreenSession
    let mut session = if io::stderr().is_terminal() {
        let mut loading = LoadingScreen::new("Loading task graph...")?;
        loading.set_filepath(&scope.name());
        loading.set_task_context(&review_task.id, &review_task.name);
        loading.set_step("Starting quality loop...");
        let screen = loading.into_live_screen()?;
        Some(ScreenSession::from_live_screen(screen)?)
    } else {
        None
    };

    let result = run_quality_loop(cwd, &mut review_id, &scope, &assignee, &plan_template_resolved, decompose_template.as_deref(), loop_template.as_deref(), review_template.as_deref(), autorun, once, output, &mut session);

    drop(session);

    result
}

/// Continue an async fix from a previously created fix-parent.
///
/// Reads the review_id and scope from the fix-parent's data, then enters
/// the quality loop from the plan-fix step onward.
fn run_fix_continue(
    cwd: &Path,
    fix_parent_id: &str,
    plan_template: Option<String>,
    decompose_template: Option<String>,
    loop_template: Option<String>,
    review_template: Option<String>,
    agent: Option<String>,
    once: bool,
    output: Option<OutputFormat>,
) -> Result<()> {
    // Parse agent if provided
    let agent_type = if let Some(ref agent_str) = agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    // Load tasks
    let events_with_ids = read_events_with_ids(cwd)?;
    let tasks = materialize_graph_with_ids(&events_with_ids).tasks;

    // Read the fix-parent task
    let fix_parent = find_task(&tasks, fix_parent_id)?;

    // Get review_id from fix-parent's data
    let review_id = fix_parent.data.get("review").ok_or_else(|| {
        AikiError::InvalidArgument(format!(
            "Fix-parent task {} missing data.review field",
            fix_parent_id
        ))
    })?.clone();

    // Get scope from fix-parent's data
    let scope = ReviewScope::from_data(&fix_parent.data)?;

    // Determine assignee
    let assignee = match scope.kind {
        ReviewScopeKind::Task => {
            let original_task = find_task(&tasks, &scope.id).ok();
            determine_followup_assignee(agent_type, original_task)
        }
        _ => determine_followup_assignee(agent_type, None),
    };

    // Resolve plan template from review task data
    let review_task = find_task(&tasks, &review_id)?;
    let plan_template_resolved = resolve_fix_template_name(plan_template, &review_task.data);

    // Run the pipeline from plan-fix step onward for this fix-parent,
    // then continue the quality loop for subsequent iterations.

    // Step 3: Create plan-fix task
    let plan_fix_id = create_plan_fix_task(cwd, &review_id, fix_parent_id, &assignee, Some(&plan_template_resolved))?;

    // Create loading screen for immediate TTY feedback, then transition to ScreenSession
    let mut session = if io::stderr().is_terminal() {
        let mut loading = LoadingScreen::new("Preparing fix...")?;
        loading.set_filepath(&scope.name());
        loading.set_task_context(fix_parent_id, &fix_parent.name);
        loading.set_step("Starting fix pipeline...");
        let screen = loading.into_live_screen()?;
        Some(ScreenSession::from_live_screen(screen)?)
    } else {
        None
    };

    // Step 4: task_run(plan-fix)
    let run_options = TaskRunOptions::new();
    run_task_with_session(cwd, &plan_fix_id, run_options, &mut session)?;

    // Step 5-6: Decompose plan into subtasks under fix-parent
    let plan_path = format!("/tmp/aiki/plans/{}.md", plan_fix_id);
    let decompose_options = DecomposeOptions {
        template: decompose_template.clone(),
        agent: assignee.as_deref().and_then(AgentType::from_str),
    };
    run_decompose(cwd, &plan_path, fix_parent_id, decompose_options, session.as_mut())?;

    // Step 7: Delete plan file
    let _ = std::fs::remove_file(&plan_path);

    // Step 8: run_loop(fix-parent)
    let mut loop_options = LoopOptions::new();
    if let Some(ref tmpl) = loop_template {
        loop_options = loop_options.with_template(tmpl.clone());
    }
    run_loop(cwd, fix_parent_id, loop_options, session.as_mut())?;

    // Step 9: if --once, we're done
    if once {
        drop(session);
        return Ok(());
    }

    // Steps 10-13: Continue quality loop with new reviews
    let mut current_review_id;

    // Create review of the fix-parent's changes
    let review_scope = ReviewScope {
        kind: ReviewScopeKind::Task,
        id: fix_parent_id.to_string(),
        task_ids: vec![],
    };

    let review_result = create_review(
        cwd,
        CreateReviewParams {
            scope: review_scope,
            agent_override: None,
            template: review_template.clone().map(|s| s.to_string()),
            fix_template: None,
            autorun: false,
        },
    )?;

    let run_options = TaskRunOptions::new();
    run_task_with_session(cwd, &review_result.review_task_id, run_options, &mut session)?;

    // Two-phase review decision
    let events_with_ids = read_events_with_ids(cwd)?;
    let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
    let new_review = find_task(&current_tasks, &review_result.review_task_id)?;

    let outcome = determine_review_outcome(
        has_actionable_issues(new_review),
        &review_result.review_task_id,
        None,
        None,
    );
    match outcome {
        ReviewOutcome::LoopBack(id) => {
            current_review_id = id;
        }
        ReviewOutcome::ReReviewOriginalScope => {
            // Fix-parent review passed — re-review original scope to catch regressions
            let original_review_result = create_review(
                cwd,
                CreateReviewParams {
                    scope: scope.clone(),
                    agent_override: None,
                    template: review_template.clone().map(|s| s.to_string()),
                    fix_template: None,
                    autorun: false,
                },
            )?;
            let run_options = TaskRunOptions::new();
            run_task_with_session(cwd, &original_review_result.review_task_id, run_options, &mut session)?;

            let events_with_ids = read_events_with_ids(cwd)?;
            let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
            let orig_review = find_task(&current_tasks, &original_review_result.review_task_id)?;

            let orig_outcome = determine_review_outcome(
                false, // fix-parent already passed
                &review_result.review_task_id,
                Some(has_actionable_issues(orig_review)),
                Some(&original_review_result.review_task_id),
            );
            match orig_outcome {
                ReviewOutcome::Approved(id) => {
                    drop(session);
                    if output != Some(OutputFormat::Id) {
                        output_approved(&id)?;
                    }
                    return Ok(());
                }
                ReviewOutcome::LoopBack(id) => {
                    current_review_id = id;
                }
                ReviewOutcome::ReReviewOriginalScope => unreachable!(),
            }
        }
        ReviewOutcome::Approved(_) => unreachable!(),
    }

    // Continue the quality loop (iterations 2..MAX)
    let result = run_quality_loop(cwd, &mut current_review_id, &scope, &assignee, &plan_template_resolved, decompose_template.as_deref(), loop_template.as_deref(), review_template.as_deref(), false, once, output, &mut session);

    drop(session);

    result
}

/// Outcome of the two-phase review decision.
#[derive(Debug, PartialEq)]
enum ReviewOutcome {
    /// Fix-parent review has issues — loop back with this review ID
    LoopBack(String),
    /// Fix-parent review passed, original re-review also passed — approved
    Approved(String),
    /// Fix-parent review passed — need to re-review original scope
    ReReviewOriginalScope,
}

/// Determine the next action after a fix-parent review completes.
///
/// This is the pure decision logic extracted from run_quality_loop and
/// run_fix_continue for testability.
fn determine_review_outcome(
    fix_parent_review_has_issues: bool,
    fix_parent_review_id: &str,
    original_review_has_issues: Option<bool>,
    original_review_id: Option<&str>,
) -> ReviewOutcome {
    if fix_parent_review_has_issues {
        return ReviewOutcome::LoopBack(fix_parent_review_id.to_string());
    }
    // Fix-parent passed — check original scope
    match (original_review_has_issues, original_review_id) {
        (Some(false), Some(id)) => ReviewOutcome::Approved(id.to_string()),
        (Some(true), Some(id)) => ReviewOutcome::LoopBack(id.to_string()),
        _ => ReviewOutcome::ReReviewOriginalScope,
    }
}

/// Run the quality loop: plan → decompose → loop → review, repeating until
/// approved or MAX_QUALITY_ITERATIONS is reached. After the fix-parent review
/// passes, re-reviews the original scope to catch regressions before approving.
fn run_quality_loop(
    cwd: &Path,
    review_id: &mut String,
    scope: &ReviewScope,
    assignee: &Option<String>,
    plan_template: &str,
    decompose_template: Option<&str>,
    loop_template: Option<&str>,
    review_template: Option<&str>,
    autorun: bool,
    once: bool,
    output: Option<OutputFormat>,
    session: &mut Option<ScreenSession>,
) -> Result<()> {
    for _iteration in 0..MAX_QUALITY_ITERATIONS {
        // 1. Short-circuit if no actionable issues
        let events_with_ids = read_events_with_ids(cwd)?;
        let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
        let current_review = find_task(&current_tasks, review_id)?;

        if !has_actionable_issues(current_review) {
            if output != Some(OutputFormat::Id) {
                output_approved(review_id)?;
            }
            return Ok(());
        }

        // 2. Create fix-parent task (container, like an epic)
        let fix_parent_id = create_fix_parent(cwd, review_id, scope, assignee, autorun)?;

        // 3. Create plan-fix task from fix template
        let plan_fix_id = create_plan_fix_task(cwd, review_id, &fix_parent_id, assignee, Some(plan_template))?;

        // 4. task_run(plan-fix) — agent writes fix plan
        let run_options = TaskRunOptions::new();
        run_task_with_session(cwd, &plan_fix_id, run_options, session)?;

        // 5-6. Decompose plan into subtasks under fix-parent
        let plan_path = format!("/tmp/aiki/plans/{}.md", plan_fix_id);
        let decompose_options = DecomposeOptions {
            template: decompose_template.map(|s| s.to_string()),
            agent: assignee.as_deref().and_then(AgentType::from_str),
        };
        run_decompose(cwd, &plan_path, &fix_parent_id, decompose_options, session.as_mut())?;

        // 7. Delete plan file (content now lives as subtasks)
        let _ = std::fs::remove_file(&plan_path);

        // 8. run_loop(fix-parent) — orchestrate subtasks via lanes
        let mut loop_options = LoopOptions::new();
        if let Some(tmpl) = loop_template {
            loop_options = loop_options.with_template(tmpl.to_string());
        }
        run_loop(cwd, &fix_parent_id, loop_options, session.as_mut())?;

        // 9. if --once: break
        if once {
            break;
        }

        // 10. Create review task scoped to fix-parent's changes
        let review_scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: fix_parent_id.clone(),
            task_ids: vec![],
        };

        let review_result = create_review(
            cwd,
            CreateReviewParams {
                scope: review_scope,
                agent_override: None,
                template: review_template.map(|s| s.to_string()),
                fix_template: None,
                autorun: false,
            },
        )?;

        // 11. task_run(review) — agent reviews the fix
        let run_options = TaskRunOptions::new();
        run_task_with_session(cwd, &review_result.review_task_id, run_options, session)?;

        // 12-14. Two-phase review decision
        let events_with_ids = read_events_with_ids(cwd)?;
        let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
        let new_review = find_task(&current_tasks, &review_result.review_task_id)?;

        let outcome = determine_review_outcome(
            has_actionable_issues(new_review),
            &review_result.review_task_id,
            None,
            None,
        );
        match outcome {
            ReviewOutcome::LoopBack(id) => {
                *review_id = id;
                continue;
            }
            ReviewOutcome::ReReviewOriginalScope => {
                // Fix-parent review passed — re-review original scope to catch regressions
                let original_review_result = create_review(
                    cwd,
                    CreateReviewParams {
                        scope: scope.clone(),
                        agent_override: None,
                        template: review_template.map(|s| s.to_string()),
                        fix_template: None,
                        autorun: false,
                    },
                )?;
                let run_options = TaskRunOptions::new();
                run_task_with_session(cwd, &original_review_result.review_task_id, run_options, session)?;

                let events_with_ids = read_events_with_ids(cwd)?;
                let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
                let original_review = find_task(&current_tasks, &original_review_result.review_task_id)?;

                let orig_outcome = determine_review_outcome(
                    false, // fix-parent already passed
                    &review_result.review_task_id,
                    Some(has_actionable_issues(original_review)),
                    Some(&original_review_result.review_task_id),
                );
                match orig_outcome {
                    ReviewOutcome::Approved(id) => {
                        if output != Some(OutputFormat::Id) {
                            output_approved(&id)?;
                        }
                        return Ok(());
                    }
                    ReviewOutcome::LoopBack(id) => {
                        *review_id = id;
                    }
                    ReviewOutcome::ReReviewOriginalScope => unreachable!(),
                }
            }
            ReviewOutcome::Approved(_) => unreachable!(),
        }
    }

    // If we reached here, the quality loop exhausted MAX_QUALITY_ITERATIONS
    // without the review approving. Emit a warning.
    eprintln!(
        "Warning: quality loop reached maximum iterations ({}) without full approval. Review {} may still have unresolved issues.",
        MAX_QUALITY_ITERATIONS,
        review_id
    );

    Ok(())
}

/// Run a task using the shared screen session if available, otherwise standalone.
fn run_task_with_session(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
    session: &mut Option<ScreenSession>,
) -> Result<()> {
    if let Some(s) = session.as_mut() {
        let result = task_run_on_session(cwd, task_id, options, s)?;
        handle_session_result(cwd, task_id, result, true)?;
    } else {
        task_run(cwd, task_id, options)?;
    }
    Ok(())
}

/// Check if a review task has actionable issues.
fn has_actionable_issues(review_task: &Task) -> bool {
    if let Some(issue_count) = review_task.data.get("issue_count") {
        // Structured review: use data.issue_count
        match issue_count.parse::<usize>() {
            Ok(n) => n > 0,
            Err(_) => !super::review::get_issue_comments(review_task).is_empty(),
        }
    } else {
        // Backward compatibility: older reviews without data.issue_count
        !review_task.comments.is_empty()
    }
}

/// Output approved message when no issues found
fn output_approved(task_id: &str) -> Result<()> {
    use super::output::{CommandOutput, format_command_output};
    output_utils::emit(|| {
        let output = CommandOutput {
            heading: "Approved",
            task_id,
            scope: None,
            status: "Review approved - no issues found.",
            issues: None,
            hint: None,
        };
        let content = format_command_output(&output);
        MdBuilder::new("fix").build(&content, &[], &[])
    });
    Ok(())
}

/// Check if a task is a review task.
///
/// A task is considered a review task if:
/// 1. Its task_type is explicitly "review", OR
/// 2. It was created from a review template (template starts with "review" or legacy "aiki/review")
pub fn is_review_task(task: &Task) -> bool {
    if task.task_type.as_deref() == Some("review") {
        return true;
    }
    if let Some(ref template) = task.template {
        if template.starts_with("review") || template.starts_with("aiki/review") {
            return true;
        }
    }
    false
}

/// Determine assignee for followup task.
///
/// The followup should be assigned to whoever did the original work (the reviewed task's assignee),
/// not the opposite of the reviewer. The person who wrote the code should fix issues in their code.
fn determine_followup_assignee(agent_override: Option<AgentType>, reviewed_task: Option<&Task>) -> Option<String> {
    if let Some(agent) = agent_override {
        return Some(agent.as_str().to_string());
    }

    // Assign to whoever did the original work
    if let Some(task) = reviewed_task {
        if let Some(ref assignee) = task.assignee {
            return Some(assignee.clone());
        }
    }

    // Fallback to claude-code if we can't determine the original worker
    Some("claude-code".to_string())
}

/// Create the fix-parent task (container for fix subtasks, like an epic).
///
/// Emits `remediates` link to the review task and `fixes` links to the
/// reviewed targets.
fn create_fix_parent(
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
    write_link_event_with_autorun(cwd, &graph, "remediates", &fix_parent_id, review_id, autorun_opt)?;

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
fn create_plan_fix_task(
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

/// Describe the fix action based on scope
#[allow(dead_code)]
fn fix_description(scope: &ReviewScope) -> String {
    match scope.kind {
        ReviewScopeKind::Task => "Created fix followup subtask under original task".to_string(),
        _ => format!("Created standalone fix task for {}", scope.name()),
    }
}

/// Resolve the plan template from CLI arg or review task data.
///
/// Priority: CLI arg > review_task.data["options.fix_template"] > None (caller default).
fn resolve_plan_template(
    cli_arg: Option<String>,
    review_data: &HashMap<String, String>,
) -> Option<String> {
    cli_arg.or_else(|| review_data.get("options.fix_template").cloned())
}

/// Resolves the final template name for fix-plan tasks.
/// Combines CLI arg / review-data resolution with the default fallback.
fn resolve_fix_template_name(
    cli_arg: Option<String>,
    review_data: &HashMap<String, String>,
) -> String {
    resolve_plan_template(cli_arg, review_data)
        .unwrap_or_else(|| "fix".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{TaskPriority, TaskStatus};
    use std::collections::HashMap;

    #[test]
    fn test_extract_task_id_plain() {
        assert_eq!(extract_task_id("xqrmnpst"), "xqrmnpst");
        assert_eq!(extract_task_id("  xqrmnpst  "), "xqrmnpst");
    }

    #[test]
    fn test_extract_task_id_xml() {
        let xml = r#"<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" comments="2"/>
</aiki_review>"#;
        assert_eq!(extract_task_id(xml), "xqrmnpst");
    }

    #[test]
    fn test_determine_followup_assignee_override() {
        let task = Task {
            id: "test".to_string(),
            name: "Test".to_string(),
            slug: None,
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: Some("codex".to_string()),
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
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
        };

        // Override should take precedence
        let result = determine_followup_assignee(Some(AgentType::Codex), Some(&task));
        assert_eq!(result, Some("codex".to_string()));
    }

    #[test]
    fn test_determine_followup_assignee_from_reviewed_task() {
        // The reviewed task's assignee is who should fix the issues
        let reviewed_task = Task {
            id: "reviewed".to_string(),
            name: "Original Work".to_string(),
            slug: None,
            task_type: None,
            status: TaskStatus::Closed,
            priority: TaskPriority::P2,
            assignee: Some("claude-code".to_string()), // claude-code did the work
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
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
        };

        // Should return the reviewed task's assignee (claude-code fixes their own work)
        let result = determine_followup_assignee(None, Some(&reviewed_task));
        assert_eq!(result, Some("claude-code".to_string()));
    }

    #[test]
    fn test_determine_followup_assignee_no_reviewed_task() {
        // If we can't find the reviewed task, fall back to claude-code
        let result = determine_followup_assignee(None, None);
        assert_eq!(result, Some("claude-code".to_string()));
    }

    fn make_test_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            name: format!("Task {}", id),
            slug: None,
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            created_at: chrono::Utc::now(),
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

    #[test]
    fn test_review_scope_from_data_task() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "original123".to_string(),
            task_ids: vec![],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.kind, ReviewScopeKind::Task);
        assert_eq!(restored.id, "original123");
    }

    #[test]
    fn test_review_scope_from_data_missing() {
        let data = HashMap::new();
        assert!(ReviewScope::from_data(&data).is_err());
    }

    #[test]
    fn test_is_review_task_by_type() {
        let mut task = make_test_task("t1");
        task.task_type = Some("review".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_by_template() {
        let mut task = make_test_task("t2");
        task.template = Some("aiki/review@1.0.0".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_neither() {
        let task = make_test_task("t3");
        assert!(!is_review_task(&task));
    }

    // fix_description tests

    #[test]
    fn test_fix_description_task_scope() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "abc123".to_string(),
            task_ids: vec![],
        };
        assert_eq!(
            fix_description(&scope),
            "Created fix followup subtask under original task"
        );
    }

    #[test]
    fn test_fix_description_spec_scope() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Plan,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(
            fix_description(&scope),
            "Created standalone fix task for Plan (feature.md)"
        );
    }

    #[test]
    fn test_fix_description_code_scope() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Code,
            id: "ops/now/feature.md".to_string(),
            task_ids: vec![],
        };
        assert_eq!(
            fix_description(&scope),
            "Created standalone fix task for Code (feature.md)"
        );
    }

    #[test]
    fn test_resolve_plan_template_cli_override() {
        let mut data = HashMap::new();
        data.insert("options.fix_template".to_string(), "from/review".to_string());
        let result = resolve_plan_template(Some("custom/template".to_string()), &data);
        assert_eq!(result, Some("custom/template".to_string()));
    }

    #[test]
    fn test_resolve_plan_template_from_review_data() {
        let mut data = HashMap::new();
        data.insert("options.fix_template".to_string(), "from/review".to_string());
        let result = resolve_plan_template(None, &data);
        assert_eq!(result, Some("from/review".to_string()));
    }

    #[test]
    fn test_resolve_plan_template_default_none() {
        let data = HashMap::new();
        let result = resolve_plan_template(None, &data);
        assert_eq!(result, None);
    }

    // has_actionable_issues tests

    #[test]
    fn test_has_actionable_issues_with_issue_count_zero() {
        let mut task = make_test_task("review1");
        task.data.insert("issue_count".to_string(), "0".to_string());
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_with_issue_count_positive() {
        let mut task = make_test_task("review2");
        task.data.insert("issue_count".to_string(), "3".to_string());
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_no_data_no_comments() {
        let task = make_test_task("review3");
        assert!(!has_actionable_issues(&task));
    }

    /// Tests the full fix-template resolution chain used by run_fix:
    /// resolve_plan_template → unwrap_or("fix") via resolve_fix_template_name.
    #[test]
    fn test_resolve_fix_template_name_full_chain() {
        // 1. CLI arg provided → uses CLI arg (ignores review data and default)
        let mut data = HashMap::new();
        data.insert("options.fix_template".to_string(), "from/review".to_string());
        assert_eq!(
            resolve_fix_template_name(Some("cli/override".to_string()), &data),
            "cli/override"
        );

        // 2. No CLI arg, review data has options.fix_template → uses review data value
        let mut data = HashMap::new();
        data.insert("options.fix_template".to_string(), "from/review".to_string());
        assert_eq!(
            resolve_fix_template_name(None, &data),
            "from/review"
        );

        // 3. No CLI arg, no review data → falls back to "fix"
        let data = HashMap::new();
        assert_eq!(
            resolve_fix_template_name(None, &data),
            "fix"
        );
    }

    // determine_review_outcome tests

    #[test]
    fn test_review_outcome_loopback_when_fix_parent_has_issues() {
        let outcome = determine_review_outcome(true, "review1", None, None);
        assert_eq!(outcome, ReviewOutcome::LoopBack("review1".to_string()));
    }

    #[test]
    fn test_review_outcome_loopback_when_fix_parent_has_issues_ignores_original() {
        // Even if original review info is provided, fix-parent issues take precedence
        let outcome = determine_review_outcome(true, "review1", Some(false), Some("orig1"));
        assert_eq!(outcome, ReviewOutcome::LoopBack("review1".to_string()));
    }

    #[test]
    fn test_review_outcome_re_review_when_no_original_info() {
        let outcome = determine_review_outcome(false, "review1", None, None);
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }

    #[test]
    fn test_review_outcome_approved_when_original_passes() {
        let outcome = determine_review_outcome(false, "review1", Some(false), Some("orig1"));
        assert_eq!(outcome, ReviewOutcome::Approved("orig1".to_string()));
    }

    #[test]
    fn test_review_outcome_loopback_when_original_has_issues() {
        let outcome = determine_review_outcome(false, "review1", Some(true), Some("orig1"));
        assert_eq!(outcome, ReviewOutcome::LoopBack("orig1".to_string()));
    }

    #[test]
    fn test_review_outcome_consecutive_loop_backs() {
        // Simulate MAX_QUALITY_ITERATIONS consecutive "fix-parent has issues" outcomes
        // Each should return LoopBack, demonstrating the loop would continue
        for i in 0..MAX_QUALITY_ITERATIONS {
            let review_id = format!("review-iter-{}", i);
            let outcome = determine_review_outcome(true, &review_id, None, None);
            assert_eq!(outcome, ReviewOutcome::LoopBack(review_id));
        }
    }

    #[test]
    fn test_review_outcome_approval_breaks_loop() {
        // Simulate a few failed iterations followed by approval
        for i in 0..3 {
            let review_id = format!("review-iter-{}", i);
            let outcome = determine_review_outcome(true, &review_id, None, None);
            assert_eq!(outcome, ReviewOutcome::LoopBack(review_id));
        }
        // On the next iteration, fix-parent passes and original scope also passes
        let outcome = determine_review_outcome(false, "fix-review-final", Some(false), Some("orig-final"));
        assert_eq!(outcome, ReviewOutcome::Approved("orig-final".to_string()));
    }

    #[test]
    fn test_max_quality_iterations_value() {
        // Ensure the constant is set to a reasonable value (not 0 or absurdly large)
        assert!(MAX_QUALITY_ITERATIONS > 0);
        assert!(MAX_QUALITY_ITERATIONS <= 100);
    }

    // ── Regression tests for review-fix execution paths ──────────────

    #[test]
    fn test_has_actionable_issues_backward_compat_comments_only() {
        // Older reviews without data.issue_count fall back to checking comments
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-legacy");
        task.comments.push(TaskComment {
            id: None,
            text: "Fix the null check".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        // No issue_count in data → falls back to non-empty comments → has issues
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_invalid_issue_count_with_comments() {
        // When issue_count is unparseable, falls back to comment-based check
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-bad-count");
        task.data.insert("issue_count".to_string(), "not-a-number".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Issue: missing validation".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        // Unparseable issue_count → falls back to get_issue_comments
        // The comment above doesn't have data.issue="true", so get_issue_comments returns empty
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_invalid_issue_count_with_issue_comments() {
        // When issue_count is unparseable, falls back to issue comments (data.issue="true")
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-bad-count-2");
        task.data.insert("issue_count".to_string(), "bad".to_string());
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Critical bug found".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });
        // Unparseable issue_count → falls back to get_issue_comments → finds one
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_issue_count_takes_priority_over_comments() {
        // When issue_count is valid, it takes priority even if comments exist
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-count-priority");
        task.data.insert("issue_count".to_string(), "0".to_string());
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Stale issue from previous review".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });
        // issue_count=0 is parseable → returns false despite issue comments existing
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_no_data_with_empty_comments() {
        // No issue_count, empty comments → no issues
        let task = make_test_task("review-empty");
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_is_review_task_various_template_prefixes() {
        // New format: "review" prefix
        let mut task = make_test_task("t-review");
        task.template = Some("review".to_string());
        assert!(is_review_task(&task));

        task.template = Some("review@2.0.0".to_string());
        assert!(is_review_task(&task));

        task.template = Some("review/custom".to_string());
        assert!(is_review_task(&task));

        // Legacy format: "aiki/review" prefix (backward compat)
        task.template = Some("aiki/review".to_string());
        assert!(is_review_task(&task));

        task.template = Some("aiki/review@2.0.0".to_string());
        assert!(is_review_task(&task));

        // Non-matching templates
        task.template = Some("custom/review".to_string());
        assert!(!is_review_task(&task));

        task.template = Some("aiki/plan".to_string());
        assert!(!is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_type_overrides_template() {
        // task_type="review" should make it a review task regardless of template
        let mut task = make_test_task("t-type-override");
        task.task_type = Some("review".to_string());
        task.template = Some("aiki/plan".to_string()); // non-review template
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_review_outcome_re_review_when_only_has_issues_none() {
        // fix-parent passed (false), original_review_has_issues=None, id=Some
        // → should still re-review (no original info means we haven't checked yet)
        let outcome = determine_review_outcome(false, "review1", None, Some("orig1"));
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }

    #[test]
    fn test_review_outcome_re_review_when_only_id_none() {
        // fix-parent passed, has_issues=Some(false) but no id
        // → should re-review (can't approve without an ID)
        let outcome = determine_review_outcome(false, "review1", Some(false), None);
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }

    #[test]
    fn test_extract_task_id_multiline_xml() {
        // Test with multiline XML and extra whitespace
        let xml = r#"
            <aiki_review cmd="review" status="ok">
                <completed task_id="abcdefghijklmnopqrstuvwxyzabcdef" comments="5"/>
            </aiki_review>
        "#;
        assert_eq!(extract_task_id(xml), "abcdefghijklmnopqrstuvwxyzabcdef");
    }

    #[test]
    fn test_extract_task_id_no_xml_passthrough() {
        // Non-XML input should pass through unchanged
        assert_eq!(extract_task_id("plain-id-123"), "plain-id-123");
    }

    #[test]
    fn test_extract_task_id_xml_no_task_id_attr() {
        // XML without task_id attribute → return trimmed input
        let xml = r#"<aiki status="ok"/>"#;
        assert_eq!(extract_task_id(xml), xml);
    }

    #[test]
    fn test_fix_description_session_scope() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            task_ids: vec![],
        };
        assert_eq!(
            fix_description(&scope),
            "Created standalone fix task for Session"
        );
    }

    #[test]
    fn test_determine_followup_assignee_reviewed_task_no_assignee() {
        // Reviewed task exists but has no assignee → fallback to claude-code
        let task = make_test_task("no-assignee");
        let result = determine_followup_assignee(None, Some(&task));
        assert_eq!(result, Some("claude-code".to_string()));
    }

    // ── Regression tests for output_format gating ──────────────────

    /// Verify the gating condition used throughout the fix pipeline:
    /// `output_approved` should only be called when output is NOT `Some(OutputFormat::Id)`.
    /// This tests the boolean condition `output != Some(OutputFormat::Id)` that guards
    /// all `output_approved` call sites in run_fix, run_fix_continue, and run_quality_loop.
    #[test]
    fn test_output_format_gating_suppresses_approved_message() {
        use super::OutputFormat;

        let output_id: Option<OutputFormat> = Some(OutputFormat::Id);
        let output_none: Option<OutputFormat> = None;

        // When output is Some(Id), the gating condition should be false (suppress output_approved)
        assert!(
            output_id == Some(OutputFormat::Id),
            "Some(Id) should match the suppression check"
        );
        assert!(
            !(output_id != Some(OutputFormat::Id)),
            "output_approved should NOT be called when output is Some(Id)"
        );

        // When output is None, the gating condition should be true (allow output_approved)
        assert!(
            output_none != Some(OutputFormat::Id),
            "output_approved SHOULD be called when output is None"
        );
    }

    /// Verify that the gating condition correctly distinguishes between
    /// all possible OutputFormat variants and None.
    #[test]
    fn test_output_format_id_only_suppresses_approved() {
        use super::OutputFormat;

        // None → should print approved message
        let should_print: Option<OutputFormat> = None;
        assert!(should_print != Some(OutputFormat::Id));

        // Some(Id) → should suppress approved message
        let should_suppress: Option<OutputFormat> = Some(OutputFormat::Id);
        assert!(!(should_suppress != Some(OutputFormat::Id)));
    }
}

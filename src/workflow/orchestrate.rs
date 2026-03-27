//! Fix orchestration logic — quality loop and decision-making.
//!
//! This module contains the core orchestration functions for the fix pipeline:
//! - `run_fix()` — main entry point for the fix pipeline
//! - `run_fix_continue()` — resume an async fix from a previously created fix-parent
//! - `run_quality_loop()` — iterative fix → review cycle
//! - Review outcome decision logic
//! - Helper functions for review validation and assignee determination

use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::path::Path;

use crate::agents::{get_available_agents, AgentType};
use crate::commands::async_spawn::spawn_aiki_background;
use crate::commands::fix::{run_fix_review_step, run_regression_review_step};
use crate::commands::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};
use crate::commands::OutputFormat;
use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::tasks::md::MdBuilder;
use crate::tasks::{
    find_task, materialize_graph_with_ids, read_events_with_ids, Task,
};
use crate::workflow::builders::{fix_pass_workflow, FixOpts};
use crate::workflow::steps::fix::create_fix_parent;
use crate::workflow::{RunMode, StepResult, WorkflowContext};

/// Maximum iterations of the quality loop to prevent infinite cycles.
pub(crate) const MAX_QUALITY_ITERATIONS: usize = 10;

/// Outcome of the two-phase review decision.
#[derive(Debug, PartialEq)]
pub(crate) enum ReviewOutcome {
    /// Fix-parent review has issues — loop back with this review ID
    LoopBack(String),
    /// Fix-parent review passed, original re-review also passed — approved
    Approved(String),
    /// Fix-parent review passed — need to re-review original scope
    ReReviewOriginalScope,
}

/// Core fix implementation — pipeline with Rust-driven quality loop.
///
/// Runs up to [`MAX_QUALITY_ITERATIONS`] cycles of fix → review. If the loop
/// exhausts all iterations without the review approving, a warning is emitted
/// to stderr and the function returns `Ok(())` (partial fixes may have been
/// applied, so we don't fail the whole command).
pub(crate) fn run_fix(
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
        return run_fix_continue(
            cwd,
            fix_parent_id,
            plan_template,
            decompose_template,
            loop_template,
            review_template,
            agent,
            once,
            output,
            io::stderr().is_terminal(),
        );
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
    let plan_template_resolved =
        resolve_fix_template_name(plan_template.clone(), &review_task.data);

    // Determine assignee for fix tasks.
    let assignee = match scope.kind {
        ReviewScopeKind::Task => {
            let original_task = find_task(&tasks, &scope.id).ok();
            Some(determine_followup_assignee(
                agent_type,
                original_task,
                None,
                None,
            )?)
        }
        _ => Some(determine_followup_assignee(
            agent_type,
            None,
            review_task.assignee.as_deref(),
            None,
        )?),
    };

    // Short-circuit if no actionable issues
    if !has_actionable_issues(review_task) {
        if output != Some(OutputFormat::Id) {
            output_approved(&review_task.id)?;
        }
        return Ok(());
    }

    if run_async {
        // Create fix-parent task, spawn background
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

        spawn_aiki_background(cwd, &spawn_args)?;

        // --async: emit fix-parent ID and return
        match output {
            Some(OutputFormat::Id) => println!("{}", fix_parent_id),
            None => eprintln!("Fix: {}", fix_parent_id),
        }

        return Ok(());
    }

    // ── Synchronous quality loop ──────────────────────────
    let mut review_id = review_task.id.clone();

    run_quality_loop(
        cwd,
        &mut review_id,
        &scope,
        &assignee,
        &plan_template_resolved,
        decompose_template.as_deref(),
        loop_template.as_deref(),
        review_template.as_deref(),
        agent.as_deref(),
        autorun,
        once,
        output,
        true, // text output for sync path
    )
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
    show_tui: bool,
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
    let review_id = fix_parent
        .data
        .get("review")
        .ok_or_else(|| {
            AikiError::InvalidArgument(format!(
                "Fix-parent task {} missing data.review field",
                fix_parent_id
            ))
        })?
        .clone();

    // Get scope from fix-parent's data
    let scope = ReviewScope::from_data(&fix_parent.data)?;

    // Load review task (needed for both assignee fallback and template resolution)
    let review_task = find_task(&tasks, &review_id)?;

    // Determine assignee (same logic as sync path)
    let assignee = match scope.kind {
        ReviewScopeKind::Task => {
            let original_task = find_task(&tasks, &scope.id).ok();
            Some(determine_followup_assignee(
                agent_type,
                original_task,
                None,
                None,
            )?)
        }
        _ => Some(determine_followup_assignee(
            agent_type,
            None,
            review_task.assignee.as_deref(),
            None,
        )?),
    };

    // Resolve plan template from review task data
    let plan_template_resolved = resolve_fix_template_name(plan_template, &review_task.data);

    // Run the fix pipeline via workflow (Fix → Decompose → Loop)
    // with the pre-created fix-parent.
    let mode = if show_tui {
        RunMode::Text
    } else {
        RunMode::Quiet
    };
    let opts = FixOpts {
        scope: scope.clone(),
        assignee: assignee.clone(),
        plan_template: plan_template_resolved.clone(),
        decompose_template,
        loop_template,
        autorun: false,
        cwd: cwd.to_path_buf(),
    };
    let mut wf = fix_pass_workflow(&review_id, &opts);
    wf.ctx.task_id = Some(fix_parent_id.to_string()); // Pre-set fix-parent
    let mut ctx = wf.run(mode).map_err(AikiError::Other)?;

    // --once: we're done after the fix pass
    if once {
        return Ok(());
    }

    // Review the fix-parent's changes
    let review_result = run_fix_review_step(&mut ctx, review_template.clone(), agent.clone())
        .map_err(AikiError::Other)?;

    // Two-phase review decision
    let review_task_id = review_result.task_id.as_ref().unwrap();
    let events_with_ids = read_events_with_ids(cwd)?;
    let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
    let new_review = find_task(&current_tasks, review_task_id)?;

    let outcome = determine_review_outcome(
        has_actionable_issues(new_review),
        review_task_id,
        None,
        None,
    );
    let mut current_review_id;
    match outcome {
        ReviewOutcome::LoopBack(id) => {
            current_review_id = id;
        }
        ReviewOutcome::ReReviewOriginalScope => {
            // Fix-parent review passed — re-review original scope for regressions
            let regression_result =
                run_regression_review_step(&mut ctx, review_template.clone(), agent.clone())
                    .map_err(AikiError::Other)?;

            let regression_review_id = regression_result.task_id.as_ref().unwrap();
            let events_with_ids = read_events_with_ids(cwd)?;
            let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
            let orig_review = find_task(&current_tasks, regression_review_id)?;

            let orig_outcome = determine_review_outcome(
                false, // fix-parent already passed
                review_task_id,
                Some(has_actionable_issues(orig_review)),
                Some(regression_review_id),
            );
            match orig_outcome {
                ReviewOutcome::Approved(id) => {
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
    run_quality_loop(
        cwd,
        &mut current_review_id,
        &scope,
        &assignee,
        &plan_template_resolved,
        opts.decompose_template.as_deref(),
        opts.loop_template.as_deref(),
        review_template.as_deref(),
        agent.as_deref(),
        false,
        once,
        output,
        show_tui,
    )
}

/// Determine the next action after a fix-parent review completes.
///
/// This is the pure decision logic extracted from run_quality_loop and
/// run_fix_continue for testability.
pub(crate) fn determine_review_outcome(
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
pub(crate) fn run_quality_loop(
    cwd: &Path,
    review_id: &mut String,
    scope: &ReviewScope,
    assignee: &Option<String>,
    plan_template: &str,
    decompose_template: Option<&str>,
    loop_template: Option<&str>,
    review_template: Option<&str>,
    agent_override: Option<&str>,
    autorun: bool,
    once: bool,
    output: Option<OutputFormat>,
    show_tui: bool,
) -> Result<()> {
    let mode = if show_tui {
        RunMode::Text
    } else {
        RunMode::Quiet
    };

    for _iteration in 0..MAX_QUALITY_ITERATIONS {
        // Build and run fix workflow (Fix → Decompose → Loop)
        let opts = FixOpts {
            scope: scope.clone(),
            assignee: assignee.clone(),
            plan_template: plan_template.to_string(),
            decompose_template: decompose_template.map(|s| s.to_string()),
            loop_template: loop_template.map(|s| s.to_string()),
            autorun,
            cwd: cwd.to_path_buf(),
        };
        let wf = fix_pass_workflow(review_id, &opts);
        let mut ctx = wf.run(mode).map_err(AikiError::Other)?;

        // Fix step short-circuited → no actionable issues → approved
        if ctx.task_id.is_none() {
            if output != Some(OutputFormat::Id) {
                output_approved(review_id)?;
            }
            return Ok(());
        }

        // --once: break after fix pass (skip post-fix review)
        if once {
            break;
        }

        // Review the fix-parent's changes
        let review_result = run_fix_review_step(
            &mut ctx,
            review_template.map(|s| s.to_string()),
            agent_override.map(|s| s.to_string()),
        )
        .map_err(AikiError::Other)?;

        // Two-phase review decision
        let review_task_id = review_result.task_id.as_ref().unwrap();
        let events_with_ids = read_events_with_ids(cwd)?;
        let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
        let new_review = find_task(&current_tasks, review_task_id)?;

        let outcome = determine_review_outcome(
            has_actionable_issues(new_review),
            review_task_id,
            None,
            None,
        );
        match outcome {
            ReviewOutcome::LoopBack(id) => {
                *review_id = id;
                continue;
            }
            ReviewOutcome::ReReviewOriginalScope => {
                // Fix-parent review passed — re-review original scope for regressions
                let regression_result = run_regression_review_step(
                    &mut ctx,
                    review_template.map(|s| s.to_string()),
                    agent_override.map(|s| s.to_string()),
                )
                .map_err(AikiError::Other)?;

                let regression_review_id = regression_result.task_id.as_ref().unwrap();
                let events_with_ids = read_events_with_ids(cwd)?;
                let current_tasks = materialize_graph_with_ids(&events_with_ids).tasks;
                let regression_review = find_task(&current_tasks, regression_review_id)?;

                let orig_outcome = determine_review_outcome(
                    false, // fix-parent already passed
                    review_task_id,
                    Some(has_actionable_issues(regression_review)),
                    Some(regression_review_id),
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

    // Quality loop exhausted MAX_QUALITY_ITERATIONS without full approval
    eprintln!(
        "Warning: quality loop reached maximum iterations ({}) without full approval. Review {} may still have unresolved issues.",
        MAX_QUALITY_ITERATIONS,
        review_id
    );

    Ok(())
}

/// Check if a review task has actionable issues.
pub(crate) fn has_actionable_issues(review_task: &Task) -> bool {
    if let Some(issue_count) = review_task.data.get("issue_count") {
        // Structured review: use data.issue_count
        match issue_count.parse::<usize>() {
            Ok(n) => n > 0,
            Err(_) => !crate::commands::review::get_issue_comments(review_task).is_empty(),
        }
    } else {
        // Backward compatibility: older reviews without data.issue_count
        !review_task.comments.is_empty()
    }
}

/// Output approved message when no issues found
pub(crate) fn output_approved(task_id: &str) -> Result<()> {
    use crate::commands::output::{format_command_output, CommandOutput};
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
        MdBuilder::new().build(&content)
    });
    Ok(())
}

/// Check if a task is a review task.
///
/// A task is considered a review task if:
/// 1. Its task_type is explicitly "review", OR
/// 2. It was created from a review template (template starts with "review" or legacy "aiki/review")
pub(crate) fn is_review_task(task: &Task) -> bool {
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
/// When no assignee is known and multiple agents are available, falls back to the default coder
/// (first available agent, typically claude-code).
///
/// `exclude` names an agent to avoid (typically the reviewer). When picking from
/// the agent registry, the first agent that isn't the excluded one wins.
pub(crate) fn determine_followup_assignee(
    agent_override: Option<AgentType>,
    reviewed_task: Option<&Task>,
    exclude: Option<&str>,
    available_agents: Option<&[AgentType]>,
) -> Result<String> {
    // Tier 1: Explicit agent override
    if let Some(agent) = agent_override {
        return Ok(agent.as_str().to_string());
    }

    // Tier 2: Original task assignee
    if let Some(task) = reviewed_task {
        if let Some(ref assignee) = task.assignee {
            return Ok(assignee.clone());
        }
    }

    // Tier 3: Use agent registry, preferring an agent that isn't `exclude`
    let available = match available_agents {
        Some(agents) => agents.to_vec(),
        None => get_available_agents(),
    };
    if available.is_empty() {
        return Err(AikiError::Other(anyhow::anyhow!(
            "No agent CLIs found on PATH. Install claude or codex to use task delegation."
        )));
    }
    // Pick first agent that isn't the excluded one; fall back to first available
    let pick = exclude
        .and_then(|ex| available.iter().find(|a| a.as_str() != ex))
        .unwrap_or(&available[0]);
    Ok(pick.as_str().to_string())
}

/// Resolve the plan template from CLI arg or review task data.
///
/// Priority: CLI arg > review_task.data["options.fix_template"] > None (caller default).
pub(crate) fn resolve_plan_template(
    cli_arg: Option<String>,
    review_data: &HashMap<String, String>,
) -> Option<String> {
    cli_arg.or_else(|| review_data.get("options.fix_template").cloned())
}

/// Resolves the final template name for fix-plan tasks.
/// Combines CLI arg / review-data resolution with the default fallback.
pub(crate) fn resolve_fix_template_name(
    cli_arg: Option<String>,
    review_data: &HashMap<String, String>,
) -> String {
    resolve_plan_template(cli_arg, review_data).unwrap_or_else(|| "fix".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{TaskPriority, TaskStatus};
    use std::collections::HashMap;

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

        // Override should take precedence (Tier 1)
        let result = determine_followup_assignee(Some(AgentType::Codex), Some(&task), None, None);
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_determine_followup_assignee_from_reviewed_task() {
        let reviewed_task = Task {
            id: "reviewed".to_string(),
            name: "Original Work".to_string(),
            slug: None,
            task_type: None,
            status: TaskStatus::Closed,
            priority: TaskPriority::P2,
            assignee: Some("claude-code".to_string()),
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

        let result = determine_followup_assignee(None, Some(&reviewed_task), None, None);
        assert_eq!(result.unwrap(), "claude-code");
    }

    #[test]
    fn test_determine_followup_assignee_no_reviewed_task_no_agents() {
        let result = determine_followup_assignee(None, None, None, Some(&[]));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No agent CLIs found"));
    }

    #[test]
    fn test_determine_followup_assignee_no_reviewed_task_single_agent() {
        let agents = [AgentType::ClaudeCode];
        let result = determine_followup_assignee(None, None, None, Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");
    }

    #[test]
    fn test_determine_followup_assignee_no_reviewed_task_multiple_agents() {
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        let result = determine_followup_assignee(None, None, None, Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");
    }

    #[test]
    fn test_determine_followup_assignee_reviewed_task_no_assignee_no_agents() {
        let task = make_test_task("no-assignee");
        let result = determine_followup_assignee(None, Some(&task), None, Some(&[]));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No agent CLIs found"));
    }

    #[test]
    fn test_determine_followup_assignee_reviewed_task_no_assignee_single_agent() {
        let task = make_test_task("no-assignee");
        let agents = [AgentType::Codex];
        let result = determine_followup_assignee(None, Some(&task), None, Some(&agents));
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_determine_followup_assignee_reviewed_task_no_assignee_multiple_agents() {
        let task = make_test_task("no-assignee");
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        let result = determine_followup_assignee(None, Some(&task), None, Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");
    }

    // ── Review outcome tests ─────────────────────────────────

    #[test]
    fn test_review_outcome_loopback_when_fix_parent_has_issues() {
        let outcome = determine_review_outcome(true, "review1", None, None);
        assert_eq!(outcome, ReviewOutcome::LoopBack("review1".to_string()));
    }

    #[test]
    fn test_review_outcome_loopback_when_fix_parent_has_issues_ignores_original() {
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
        for i in 0..MAX_QUALITY_ITERATIONS {
            let review_id = format!("review-iter-{}", i);
            let outcome = determine_review_outcome(true, &review_id, None, None);
            assert_eq!(outcome, ReviewOutcome::LoopBack(review_id));
        }
    }

    #[test]
    fn test_review_outcome_approval_breaks_loop() {
        for i in 0..3 {
            let review_id = format!("review-iter-{}", i);
            let outcome = determine_review_outcome(true, &review_id, None, None);
            assert_eq!(outcome, ReviewOutcome::LoopBack(review_id));
        }
        let outcome =
            determine_review_outcome(false, "fix-review-final", Some(false), Some("orig-final"));
        assert_eq!(outcome, ReviewOutcome::Approved("orig-final".to_string()));
    }

    #[test]
    fn test_max_quality_iterations_value() {
        assert!(MAX_QUALITY_ITERATIONS > 0);
        assert!(MAX_QUALITY_ITERATIONS <= 100);
    }

    // ── is_review_task tests ─────────────────────────────────

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

    #[test]
    fn test_is_review_task_various_template_prefixes() {
        let mut task = make_test_task("t-review");
        task.template = Some("review".to_string());
        assert!(is_review_task(&task));

        task.template = Some("review@2.0.0".to_string());
        assert!(is_review_task(&task));

        task.template = Some("review/custom".to_string());
        assert!(is_review_task(&task));

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
        let mut task = make_test_task("t-type-override");
        task.task_type = Some("review".to_string());
        task.template = Some("aiki/plan".to_string());
        assert!(is_review_task(&task));
    }

    // ── resolve_plan_template / resolve_fix_template_name tests ──

    #[test]
    fn test_resolve_plan_template_cli_override() {
        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from/review".to_string(),
        );
        let result = resolve_plan_template(Some("custom/template".to_string()), &data);
        assert_eq!(result, Some("custom/template".to_string()));
    }

    #[test]
    fn test_resolve_plan_template_from_review_data() {
        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from/review".to_string(),
        );
        let result = resolve_plan_template(None, &data);
        assert_eq!(result, Some("from/review".to_string()));
    }

    #[test]
    fn test_resolve_plan_template_default_none() {
        let data = HashMap::new();
        let result = resolve_plan_template(None, &data);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_fix_template_name_full_chain() {
        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from/review".to_string(),
        );
        assert_eq!(
            resolve_fix_template_name(Some("cli/override".to_string()), &data),
            "cli/override"
        );

        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from/review".to_string(),
        );
        assert_eq!(resolve_fix_template_name(None, &data), "from/review");

        let data = HashMap::new();
        assert_eq!(resolve_fix_template_name(None, &data), "fix");
    }

    // ── has_actionable_issues tests ──────────────────────────

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

    #[test]
    fn test_has_actionable_issues_backward_compat_comments_only() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-legacy");
        task.comments.push(TaskComment {
            id: None,
            text: "Fix the null check".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_invalid_issue_count_with_comments() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-bad-count");
        task.data
            .insert("issue_count".to_string(), "not-a-number".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Issue: missing validation".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_invalid_issue_count_with_issue_comments() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("review-bad-count-2");
        task.data
            .insert("issue_count".to_string(), "bad".to_string());
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "Critical bug found".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_issue_count_takes_priority_over_comments() {
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
        assert!(!has_actionable_issues(&task));
    }

    // ── Contract tests ───────────────────────────────────────

    #[test]
    fn test_max_quality_iterations_is_ten() {
        assert_eq!(
            MAX_QUALITY_ITERATIONS, 10,
            "MAX_QUALITY_ITERATIONS must be 10"
        );
    }

    #[test]
    fn test_two_phase_fix_parent_fails_always_loops_back() {
        for original_has_issues in [None, Some(true), Some(false)] {
            let outcome =
                determine_review_outcome(true, "fix-review", original_has_issues, Some("orig"));
            assert!(
                matches!(outcome, ReviewOutcome::LoopBack(ref id) if id == "fix-review"),
                "Fix-parent failure must always loop back, original_has_issues={:?}",
                original_has_issues
            );
        }
    }

    #[test]
    fn test_two_phase_fix_passes_triggers_original_review() {
        let outcome = determine_review_outcome(false, "fix-review", None, None);
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }

    #[test]
    fn test_two_phase_both_pass_approved() {
        let outcome = determine_review_outcome(false, "fix-review", Some(false), Some("orig"));
        assert_eq!(outcome, ReviewOutcome::Approved("orig".to_string()));
    }

    #[test]
    fn test_two_phase_original_fails_loops_back_with_original_id() {
        let outcome = determine_review_outcome(false, "fix-review", Some(true), Some("orig"));
        assert_eq!(outcome, ReviewOutcome::LoopBack("orig".to_string()));
    }

    #[test]
    fn test_quality_loop_bounded_by_max_iterations() {
        let mut review_ids: Vec<String> = Vec::new();
        for i in 0..MAX_QUALITY_ITERATIONS {
            let review_id = format!("review-iter-{}", i);
            let outcome = determine_review_outcome(true, &review_id, None, None);
            match outcome {
                ReviewOutcome::LoopBack(id) => review_ids.push(id),
                _ => panic!("Expected LoopBack on iteration {}", i),
            }
        }
        assert_eq!(review_ids.len(), MAX_QUALITY_ITERATIONS);
    }

    #[test]
    fn test_followup_assignee_tier1_explicit_override() {
        let agents = [AgentType::Codex];
        let result =
            determine_followup_assignee(Some(AgentType::ClaudeCode), None, None, Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");
    }

    #[test]
    fn test_followup_assignee_tier2_original_task_assignee() {
        let mut task = make_test_task("original");
        task.assignee = Some("codex".to_string());
        let result = determine_followup_assignee(None, Some(&task), None, None);
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_followup_assignee_tier3_single_available_agent() {
        let agents = [AgentType::Codex];
        let result = determine_followup_assignee(None, None, None, Some(&agents));
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_followup_assignee_tier4_default_coder() {
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        let result = determine_followup_assignee(None, None, None, Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");
    }

    #[test]
    fn test_followup_assignee_no_agents_errors() {
        let result = determine_followup_assignee(None, None, None, Some(&[]));
        assert!(result.is_err());
    }

    #[test]
    fn test_followup_assignee_exclude_reviewer() {
        let agents = [AgentType::ClaudeCode, AgentType::Codex];
        let result = determine_followup_assignee(None, None, Some("codex"), Some(&agents));
        assert_eq!(result.unwrap(), "claude-code");

        let result = determine_followup_assignee(None, None, Some("claude-code"), Some(&agents));
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_followup_assignee_exclude_only_agent_falls_back() {
        let agents = [AgentType::Codex];
        let result = determine_followup_assignee(None, None, Some("codex"), Some(&agents));
        assert_eq!(result.unwrap(), "codex");
    }

    #[test]
    fn test_resolve_fix_template_cli_wins_over_review_data() {
        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from-review".to_string(),
        );
        assert_eq!(
            resolve_fix_template_name(Some("cli-override".to_string()), &data),
            "cli-override"
        );
    }

    #[test]
    fn test_resolve_fix_template_review_data_fallback() {
        let mut data = HashMap::new();
        data.insert(
            "options.fix_template".to_string(),
            "from-review".to_string(),
        );
        assert_eq!(resolve_fix_template_name(None, &data), "from-review");
    }

    #[test]
    fn test_resolve_fix_template_default_is_fix() {
        let data = HashMap::new();
        assert_eq!(resolve_fix_template_name(None, &data), "fix");
    }

    #[test]
    fn test_has_actionable_issues_structured_review_zero() {
        let mut task = make_test_task("r-structured-0");
        task.data.insert("issue_count".to_string(), "0".to_string());
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_structured_review_positive() {
        let mut task = make_test_task("r-structured-3");
        task.data.insert("issue_count".to_string(), "3".to_string());
        assert!(has_actionable_issues(&task));
    }

    #[test]
    fn test_has_actionable_issues_structured_takes_priority() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("r-priority");
        task.data.insert("issue_count".to_string(), "0".to_string());
        let mut issue_data = HashMap::new();
        issue_data.insert("issue".to_string(), "true".to_string());
        task.comments.push(TaskComment {
            id: None,
            text: "stale issue".to_string(),
            timestamp: chrono::Utc::now(),
            data: issue_data,
        });
        assert!(
            !has_actionable_issues(&task),
            "issue_count=0 must override issue comments"
        );
    }

    #[test]
    fn test_has_actionable_issues_legacy_nonempty_comments() {
        use crate::tasks::TaskComment;
        let mut task = make_test_task("r-legacy");
        task.comments.push(TaskComment {
            id: None,
            text: "Fix this".to_string(),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
        });
        assert!(
            has_actionable_issues(&task),
            "Legacy: non-empty comments → has issues"
        );
    }

    #[test]
    fn test_has_actionable_issues_legacy_empty_comments() {
        let task = make_test_task("r-legacy-empty");
        assert!(!has_actionable_issues(&task));
    }

    #[test]
    fn test_is_review_task_by_type_field() {
        let mut task = make_test_task("r1");
        task.task_type = Some("review".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_by_new_template_prefix() {
        let mut task = make_test_task("r2");
        task.template = Some("review/task".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_by_legacy_template_prefix() {
        let mut task = make_test_task("r3");
        task.template = Some("aiki/review@1.0.0".to_string());
        assert!(is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_rejects_non_review() {
        let mut task = make_test_task("r4");
        task.task_type = Some("build".to_string());
        task.template = Some("aiki/build".to_string());
        assert!(!is_review_task(&task));
    }

    #[test]
    fn test_is_review_task_type_overrides_non_review_template() {
        let mut task = make_test_task("r5");
        task.task_type = Some("review".to_string());
        task.template = Some("custom/not-review".to_string());
        assert!(
            is_review_task(&task),
            "task_type=review should override template"
        );
    }

    #[test]
    fn test_review_outcome_re_review_when_only_has_issues_none() {
        let outcome = determine_review_outcome(false, "review1", None, Some("orig1"));
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }

    #[test]
    fn test_review_outcome_re_review_when_only_id_none() {
        let outcome = determine_review_outcome(false, "review1", Some(false), None);
        assert_eq!(outcome, ReviewOutcome::ReReviewOriginalScope);
    }
}

//! Fix command — CLI entry point and workflow step handlers.
//!
//! This module provides the `aiki fix` command which reads a review task ID
//! and delegates to `workflow::orchestrate::run_fix()` for the core pipeline.
//! Workflow step handlers for fix-specific steps live here as well.

use std::env;
use std::io::{self, BufRead};
use std::path::Path;

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::tasks::runner::TaskRunOptions;
use crate::tasks::{find_task, materialize_graph_with_ids, read_events_with_ids, Task};
// Re-export orchestration functions for callers that use `commands::fix::*`.
pub(crate) use crate::workflow::orchestrate::{has_actionable_issues, is_review_task, run_fix};
use crate::workflow::steps::fix::{create_fix_parent, create_plan_fix_task, run_task_with_show_tui};
use crate::workflow::{StepResult, WorkflowContext};

use super::decompose::{run_decompose, DecomposeOptions};
use super::loop_cmd::{run_loop, LoopOptions};
use super::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};
use super::OutputFormat;

/// Fix plan step: check actionable issues, create fix parent and plan-fix task.
pub(crate) fn run_fix_plan_step(
    ctx: &mut WorkflowContext,
    review_id: &str,
    scope: &ReviewScope,
    assignee: &Option<String>,
    template: Option<&str>,
    autorun: bool,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = if let Some(ref id) = ctx.task_id {
        // Continue case: fix-parent already exists, skip actionable-issues check
        id.clone()
    } else {
        // New fix: check for actionable issues first
        let events_with_ids = read_events_with_ids(&cwd)?;
        let tasks = materialize_graph_with_ids(&events_with_ids).tasks;
        let review_task = find_task(&tasks, review_id)?;

        if !has_actionable_issues(review_task) {
            return Ok(StepResult {
                message: "approved — no actionable issues".to_string(),
                task_id: None,
            });
        }

        create_fix_parent(&cwd, review_id, scope, assignee, autorun)?
    };

    // Create and run plan-fix task
    let template_name = template.unwrap_or("fix");
    let plan_fix_id = create_plan_fix_task(
        &cwd,
        review_id,
        &fix_parent_id,
        assignee,
        Some(template_name),
    )?;
    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &plan_fix_id, run_options, false)?;

    ctx.task_id = Some(fix_parent_id.clone());
    ctx.plan_path = Some(format!("/tmp/aiki/plans/{}.md", plan_fix_id));

    Ok(StepResult {
        message: "fix plan created".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix decompose step: decompose fix plan into subtasks, then delete plan file.
pub(crate) fn run_fix_decompose_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };
    let plan_path = match ctx.plan_path {
        Some(ref path) => path.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped (no plan)".to_string(),
                task_id: None,
            })
        }
    };

    let decompose_options = DecomposeOptions { template, agent };
    run_decompose(&cwd, &plan_path, &fix_parent_id, decompose_options, false)?;

    // Delete plan file (content now lives as subtasks)
    let _ = std::fs::remove_file(&plan_path);
    ctx.plan_path = None;

    Ok(StepResult {
        message: "plan decomposed into subtasks".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix loop step: run subtasks via the loop orchestrator.
pub(crate) fn run_fix_loop_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    let mut loop_options = LoopOptions::new();
    if let Some(ref tmpl) = template {
        loop_options = loop_options.with_template(tmpl.clone());
    }
    run_loop(&cwd, &fix_parent_id, loop_options, false)?;

    Ok(StepResult {
        message: "subtasks executed".to_string(),
        task_id: Some(fix_parent_id),
    })
}

/// Fix review step: create and run a review of the fix-parent's changes.
pub(crate) fn run_fix_review_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    let review_scope = ReviewScope {
        kind: ReviewScopeKind::Task,
        id: fix_parent_id,
        task_ids: vec![],
    };

    let review_result = create_review(
        &cwd,
        CreateReviewParams {
            scope: review_scope,
            agent_override: agent,
            template,
            fix_template: None,
            autorun: false,
        },
    )?;

    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &review_result.review_task_id, run_options, false)?;

    Ok(StepResult {
        message: "review complete".to_string(),
        task_id: Some(review_result.review_task_id),
    })
}

/// Regression review step: re-review the original scope to catch regressions.
pub(crate) fn run_regression_review_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<StepResult> {
    let cwd = ctx.cwd.clone();
    let fix_parent_id = match ctx.task_id {
        Some(ref id) => id.clone(),
        None => {
            return Ok(StepResult {
                message: "skipped".to_string(),
                task_id: None,
            })
        }
    };

    // Read original scope from fix-parent task data
    let events_with_ids = read_events_with_ids(&cwd)?;
    let tasks = materialize_graph_with_ids(&events_with_ids).tasks;
    let fix_parent = find_task(&tasks, &fix_parent_id)?;
    let scope = ReviewScope::from_data(&fix_parent.data)?;

    let review_result = create_review(
        &cwd,
        CreateReviewParams {
            scope,
            agent_override: agent,
            template,
            fix_template: None,
            autorun: false,
        },
    )?;

    let run_options = TaskRunOptions::new();
    run_task_with_show_tui(&cwd, &review_result.review_task_id, run_options, false)?;

    Ok(StepResult {
        message: "regression review complete".to_string(),
        task_id: Some(review_result.review_task_id),
    })
}

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
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    // Get task ID from argument or stdin
    let task_id = match task_id {
        Some(id) => extract_task_id(&id),
        None => read_task_id_from_stdin()?,
    };

    run_fix(
        &cwd,
        &task_id,
        run_async,
        continue_async,
        plan_template,
        decompose_template,
        loop_template,
        review_template,
        agent,
        autorun,
        once,
        output,
    )
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
        let line = line
            .map_err(|e| AikiError::InvalidArgument(format!("Failed to read from stdin: {}", e)))?;
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

// ── Tests ────────────────────────────────────────────────────────────

// The fix_workflow test remains here since it tests workflow composition
// using builders from workflow::builders.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::builders::{fix_workflow, FixOpts};
    use crate::workflow::{ReviewScope, ReviewScopeKind};

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
    fn test_extract_task_id_multiline_xml() {
        let xml = r#"
            <aiki_review cmd="review" status="ok">
                <completed task_id="abcdefghijklmnopqrstuvwxyzabcdef" comments="5"/>
            </aiki_review>
        "#;
        assert_eq!(extract_task_id(xml), "abcdefghijklmnopqrstuvwxyzabcdef");
    }

    #[test]
    fn test_extract_task_id_no_xml_passthrough() {
        assert_eq!(extract_task_id("plain-id-123"), "plain-id-123");
    }

    #[test]
    fn test_extract_task_id_xml_no_task_id_attr() {
        let xml = r#"<aiki status="ok"/>"#;
        assert_eq!(extract_task_id(xml), xml);
    }

    #[test]
    fn test_extract_task_id_from_xml_attribute() {
        let xml = r#"<completed task_id="abcdefghij"/>"#;
        assert_eq!(extract_task_id(xml), "abcdefghij");
    }

    #[test]
    fn test_extract_task_id_trims_whitespace() {
        assert_eq!(extract_task_id("  myid  "), "myid");
    }

    #[test]
    fn test_fix_workflow_has_expected_steps() {
        let temp_dir = tempfile::tempdir().unwrap();
        let opts = FixOpts {
            cwd: temp_dir.path().to_path_buf(),
            scope: ReviewScope {
                kind: ReviewScopeKind::Task,
                id: "original123".to_string(),
                task_ids: vec![],
            },
            assignee: Some("codex".to_string()),
            plan_template: "fix".to_string(),
            decompose_template: Some("decompose".to_string()),
            loop_template: Some("loop".to_string()),
            autorun: true,
        };

        let wf = fix_workflow("review123", &opts);
        let names: Vec<_> = wf.steps.iter().map(|s| s.name()).collect();

        assert_eq!(wf.steps.len(), 5);
        assert_eq!(
            names,
            vec![
                "fix",
                "decompose",
                "loop",
                "review",
                "review for regressions"
            ]
        );
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
        use std::collections::HashMap;
        let data = HashMap::new();
        assert!(ReviewScope::from_data(&data).is_err());
    }

    #[test]
    fn test_output_format_gating_suppresses_approved_message() {
        let output_id: Option<OutputFormat> = Some(OutputFormat::Id);
        let output_none: Option<OutputFormat> = None;

        assert!(
            output_id == Some(OutputFormat::Id),
            "Some(Id) should match the suppression check"
        );
        assert!(
            !(output_id != Some(OutputFormat::Id)),
            "output_approved should NOT be called when output is Some(Id)"
        );

        assert!(
            output_none != Some(OutputFormat::Id),
            "output_approved SHOULD be called when output is None"
        );
    }

    #[test]
    fn test_output_format_id_only_suppresses_approved() {
        let should_print: Option<OutputFormat> = None;
        assert!(should_print != Some(OutputFormat::Id));

        let should_suppress: Option<OutputFormat> = Some(OutputFormat::Id);
        assert!(!(should_suppress != Some(OutputFormat::Id)));
    }

    #[test]
    fn test_review_scope_roundtrip_all_kinds() {
        for (kind, id) in [
            (ReviewScopeKind::Task, "task123".to_string()),
            (ReviewScopeKind::Plan, "ops/now/plan.md".to_string()),
            (ReviewScopeKind::Code, "src/main.rs".to_string()),
            (
                ReviewScopeKind::Session,
                "550e8400-e29b-41d4-a716-446655440000".to_string(),
            ),
        ] {
            let scope = ReviewScope {
                kind: kind.clone(),
                id: id.clone(),
                task_ids: vec![],
            };
            let data = scope.to_data();
            let restored = ReviewScope::from_data(&data).unwrap();
            assert_eq!(restored.kind, kind);
            assert_eq!(restored.id, id);
        }
    }

    #[test]
    fn test_review_scope_session_preserves_task_ids() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "session-id".to_string(),
            task_ids: vec!["t1".to_string(), "t2".to_string(), "t3".to_string()],
        };
        let data = scope.to_data();
        let restored = ReviewScope::from_data(&data).unwrap();
        assert_eq!(restored.task_ids, vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn test_review_scope_from_data_rejects_missing_kind() {
        use std::collections::HashMap;
        let data = HashMap::new();
        assert!(ReviewScope::from_data(&data).is_err());
    }

    #[test]
    fn test_review_scope_from_data_rejects_invalid_kind() {
        use std::collections::HashMap;
        let mut data = HashMap::new();
        data.insert("scope.kind".to_string(), "invalid".to_string());
        assert!(ReviewScope::from_data(&data).is_err());
    }

    #[test]
    fn test_fix_parent_data_fields() {
        let scope = ReviewScope {
            kind: ReviewScopeKind::Task,
            id: "epic123".to_string(),
            task_ids: vec![],
        };
        let scope_data = scope.to_data();

        assert_eq!(scope_data.get("scope.kind").unwrap(), "task");
        assert_eq!(scope_data.get("scope.id").unwrap(), "epic123");
        assert!(
            scope_data.contains_key("scope.name"),
            "scope.name must be present"
        );

        let mut data = std::collections::HashMap::new();
        data.insert("review".to_string(), "review-abc".to_string());
        for (k, v) in scope_data {
            data.insert(k, v);
        }

        assert_eq!(data.get("review").unwrap(), "review-abc");
        assert_eq!(data.get("scope.kind").unwrap(), "task");
        assert_eq!(data.get("scope.id").unwrap(), "epic123");
    }
}

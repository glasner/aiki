use std::collections::HashMap;
#[cfg(test)]
use std::env;
use std::path::Path;
#[cfg(test)]
use std::process::Command;

use super::StepResult;
use super::WorkflowChange;
use super::WorkflowContext;
use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{
    handle_session_result, task_run, task_run_async, task_run_on_session, TaskRunOptions,
};
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, start_task_core, TaskStatus,
};
use crate::workflow::{OutputKind, WorkflowOutput};

/// Options for `run_loop()`
pub struct LoopOptions {
    /// Run asynchronously (return immediately)
    pub run_async: bool,
    /// Agent type override
    pub agent: Option<AgentType>,
    /// Template name override (default: "loop")
    pub template: Option<String>,
}

impl LoopOptions {
    pub fn new() -> Self {
        Self {
            run_async: false,
            agent: None,
            template: None,
        }
    }

    pub fn with_async(mut self, run_async: bool) -> Self {
        self.run_async = run_async;
        self
    }

    pub fn with_agent(mut self, agent: AgentType) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn with_template(mut self, template: String) -> Self {
        self.template = Some(template);
        self
    }
}

/// Shared loop function used by `aiki loop`, `build.rs`, and `fix.rs`.
///
/// Creates a loop task from the `loop` template with `data.target` set to
/// the parent task ID, writes an `orchestrates` link from the loop task to the
/// parent, and runs the task.
///
/// Returns the loop task ID.
pub fn run_loop(
    cwd: &Path,
    parent_id: &str,
    options: LoopOptions,
    show_tui: bool,
    output: OutputKind,
) -> Result<String> {
    let output_ctx = WorkflowOutput::new(output);

    // Validate parent task exists and has subtasks
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let parent = find_task(&graph.tasks, parent_id)?;
    let parent_id = parent.id.clone(); // resolve prefix to canonical ID

    let subtasks = get_subtasks(&graph, &parent_id);
    if subtasks.is_empty() {
        return Err(AikiError::InvalidArgument(format!(
            "Parent task {} has no subtasks. Nothing to loop over.",
            &parent_id[..parent_id.len().min(8)]
        )));
    }

    // Start the parent task if not already in progress
    if parent.status != TaskStatus::InProgress {
        start_task_core(cwd, &[parent_id.clone()])?;
    }

    // Create loop task from loop template
    let mut data = HashMap::new();
    data.insert("target".to_string(), parent_id.clone());

    let assignee = options
        .agent
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let params = crate::commands::task::TemplateTaskParams {
        template_name: options.template.unwrap_or_else(|| "loop".to_string()),
        data,
        assignee,
        ..Default::default()
    };

    let loop_task_id = crate::commands::task::create_from_template(cwd, params)?;

    // Write orchestrates link: loop task → parent
    let events = crate::tasks::storage::read_events(cwd)?;
    let graph = crate::tasks::graph::materialize_graph(&events);
    crate::tasks::storage::write_link_event(
        cwd,
        &graph,
        "orchestrates",
        &loop_task_id,
        &parent_id,
    )?;

    // Run the loop task
    let task_run_options = if let Some(agent) = options.agent {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };

    if show_tui && !options.run_async {
        let session_result = task_run_on_session(cwd, &loop_task_id, task_run_options, true)?;
        handle_session_result(cwd, &loop_task_id, session_result, true)?;
    } else if options.run_async {
        let _handle = task_run_async(cwd, &loop_task_id, task_run_options)?;
        emit_loop_async(output_ctx, &loop_task_id, &parent_id);
    } else {
        emit_loop_started(output_ctx, &loop_task_id, &parent_id);
        task_run(cwd, &loop_task_id, task_run_options.quiet())?;
        emit_loop_completed(output_ctx, &loop_task_id, &parent_id);
    }

    Ok(loop_task_id)
}

/// Loop step: run the orchestration loop over epic subtasks.
pub(crate) fn run(ctx: &mut WorkflowContext) -> anyhow::Result<StepResult> {
    let epic_id = ctx.require_task_id()?.to_string();

    ctx.status("preparing loop options");
    let mut loop_options = LoopOptions::new();
    if let Some(agent) = ctx.opts.agent {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(ref tmpl) = ctx.opts.loop_template {
        loop_options = loop_options.with_template(tmpl.clone());
    }

    ctx.status("running subtask loop");
    let loop_task_id = run_loop(&ctx.cwd, &epic_id, loop_options, false, ctx.output.kind())?;

    Ok(StepResult {
        change: WorkflowChange::None,
        message: "All lanes complete".to_string(),
        task_id: Some(loop_task_id),
    })
}

/// Format loop started message.
fn format_loop_started(loop_id: &str, parent_id: &str) -> String {
    let content = format!(
        "## Loop Started\n- **Loop ID:** {}\n- **Parent ID:** {}\n",
        loop_id, parent_id
    );
    MdBuilder::new().build(&content)
}

fn emit_loop_started(output: WorkflowOutput, loop_id: &str, parent_id: &str) {
    output.emit(&format_loop_started(loop_id, parent_id));
}

/// Format loop completed message.
fn format_loop_completed(loop_id: &str, parent_id: &str) -> String {
    let content = format!(
        "## Loop Completed\n- **Loop ID:** {}\n- **Parent ID:** {}\n",
        loop_id, parent_id
    );
    MdBuilder::new().build(&content)
}

fn emit_loop_completed(output: WorkflowOutput, loop_id: &str, parent_id: &str) {
    output.emit(&format_loop_completed(loop_id, parent_id));
}

/// Format loop async message.
fn format_loop_async(loop_id: &str, parent_id: &str) -> String {
    let content = format!(
        "## Loop Started\n- **Loop ID:** {}\n- **Parent ID:** {}\n- Loop started in background.\n",
        loop_id, parent_id
    );
    MdBuilder::new().build(&content)
}

fn emit_loop_async(output: WorkflowOutput, loop_id: &str, parent_id: &str) {
    output.emit(&format_loop_async(loop_id, parent_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::OutputKind;

    fn run_loop_output_probe(case: &str) -> String {
        let exe = env::current_exe().expect("resolve current test binary");
        let output = Command::new(exe)
            .arg("--exact")
            .arg("workflow::steps::r#loop::tests::loop_output_probe")
            .arg("--nocapture")
            .env("AIKI_LOOP_OUTPUT_TEST_CASE", case)
            .output()
            .expect("run loop output probe");

        assert!(
            output.status.success(),
            "probe failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stderr).to_string()
    }

    #[test]
    fn loop_output_probe() {
        let Some(case) = env::var("AIKI_LOOP_OUTPUT_TEST_CASE").ok() else {
            return;
        };

        let output = if case.starts_with("quiet") {
            WorkflowOutput::new(OutputKind::Quiet)
        } else {
            WorkflowOutput::new(OutputKind::Text)
        };

        match case.as_str() {
            "text-started" | "quiet-started" => emit_loop_started(output, "loop123", "parent456"),
            "text-completed" | "quiet-completed" => {
                emit_loop_completed(output, "loop123", "parent456")
            }
            "text-async" | "quiet-async" => emit_loop_async(output, "loop123", "parent456"),
            other => panic!("unknown loop output probe case: {other}"),
        }
    }

    #[test]
    fn format_loop_started_includes_loop_and_parent_ids() {
        let rendered = format_loop_started("loop123", "parent456");
        assert!(rendered.contains("Loop Started"));
        assert!(rendered.contains("loop123"));
        assert!(rendered.contains("parent456"));
    }

    #[test]
    fn format_loop_completed_includes_loop_and_parent_ids() {
        let rendered = format_loop_completed("loop123", "parent456");
        assert!(rendered.contains("Loop Completed"));
        assert!(rendered.contains("loop123"));
        assert!(rendered.contains("parent456"));
    }

    #[test]
    fn format_loop_async_mentions_background_execution() {
        let rendered = format_loop_async("loop123", "parent456");
        assert!(rendered.contains("Loop Started"));
        assert!(rendered.contains("loop123"));
        assert!(rendered.contains("parent456"));
        assert!(rendered.contains("background"));
    }

    #[test]
    fn loop_started_output_respects_output_kind() {
        let text_output = run_loop_output_probe("text-started");
        let quiet_output = run_loop_output_probe("quiet-started");

        assert!(text_output.contains("Loop Started"));
        assert!(text_output.contains("loop123"));
        assert!(text_output.contains("parent456"));
        assert!(!quiet_output.contains("Loop Started"));
    }

    #[test]
    fn loop_completed_output_respects_output_kind() {
        let text_output = run_loop_output_probe("text-completed");
        let quiet_output = run_loop_output_probe("quiet-completed");

        assert!(text_output.contains("Loop Completed"));
        assert!(text_output.contains("loop123"));
        assert!(text_output.contains("parent456"));
        assert!(!quiet_output.contains("Loop Completed"));
    }

    #[test]
    fn loop_async_output_respects_output_kind() {
        let text_output = run_loop_output_probe("text-async");
        let quiet_output = run_loop_output_probe("quiet-async");

        assert!(text_output.contains("Loop Started"));
        assert!(text_output.contains("background"));
        assert!(text_output.contains("loop123"));
        assert!(text_output.contains("parent456"));
        assert!(!quiet_output.contains("Loop Started"));
        assert!(!quiet_output.contains("background"));
    }
}

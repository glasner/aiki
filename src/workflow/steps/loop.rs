use std::collections::HashMap;
use std::path::Path;

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{
    handle_session_result, task_run, task_run_async, task_run_on_session, TaskRunOptions,
};
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, start_task_core, TaskStatus,
};
use crate::workflow::{StepResult, WorkflowContext};

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
) -> Result<String> {
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
        output_loop_async(&loop_task_id, &parent_id)?;
    } else {
        output_loop_started(&loop_task_id, &parent_id)?;
        task_run(cwd, &loop_task_id, task_run_options.quiet())?;
        output_loop_completed(&loop_task_id, &parent_id)?;
    }

    Ok(loop_task_id)
}

/// Loop step: run the orchestration loop over epic subtasks.
pub(crate) fn run_loop_step(
    ctx: &mut WorkflowContext,
    template: Option<String>,
    agent: Option<AgentType>,
) -> anyhow::Result<StepResult> {
    let epic_id = ctx
        .task_id
        .as_ref()
        .ok_or_else(|| AikiError::InvalidArgument("No epic ID in workflow context".to_string()))?
        .clone();

    let mut loop_options = LoopOptions::new();
    if let Some(agent) = agent {
        loop_options = loop_options.with_agent(agent);
    }
    if let Some(tmpl) = template {
        loop_options = loop_options.with_template(tmpl);
    }

    let loop_task_id = run_loop(&ctx.cwd, &epic_id, loop_options, false)?;

    Ok(StepResult {
        message: "All lanes complete".to_string(),
        task_id: Some(loop_task_id),
    })
}

/// Output loop started message to stderr
fn output_loop_started(loop_id: &str, parent_id: &str) -> Result<()> {
    output_utils::emit(|| {
        let content = format!(
            "## Loop Started\n- **Loop ID:** {}\n- **Parent ID:** {}\n",
            loop_id, parent_id
        );
        MdBuilder::new().build(&content)
    });
    Ok(())
}

/// Output loop completed message to stderr
fn output_loop_completed(loop_id: &str, parent_id: &str) -> Result<()> {
    output_utils::emit(|| {
        let content = format!(
            "## Loop Completed\n- **Loop ID:** {}\n- **Parent ID:** {}\n",
            loop_id, parent_id
        );
        MdBuilder::new().build(&content)
    });
    Ok(())
}

/// Output loop async message to stderr
fn output_loop_async(loop_id: &str, parent_id: &str) -> Result<()> {
    output_utils::emit(|| {
        let content = format!(
            "## Loop Started\n- **Loop ID:** {}\n- **Parent ID:** {}\n- Loop started in background.\n",
            loop_id, parent_id
        );
        MdBuilder::new().build(&content)
    });
    Ok(())
}

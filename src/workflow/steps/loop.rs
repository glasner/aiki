use std::collections::HashMap;
#[cfg(test)]
use std::env;
use std::path::Path;
#[cfg(test)]
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::StepResult;
use super::WorkflowChange;
use super::WorkflowContext;
use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{
    finalize_agent_run, handle_session_result, prepare_task_run, rollback_if_still_reserved,
    task_run, task_run_async, task_run_on_session, TaskRunOptions,
};
use crate::tasks::types::TaskOutcome;
use crate::tasks::{
    find_task, get_subtasks, materialize_graph, read_events, start_task_core, TaskEvent,
    TaskStatus,
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
/// When `ctx` is provided with an active `event_rx`, uses spawn_monitored +
/// event drain loop to show subtask progress in real-time. Otherwise falls
/// back to `task_run()` or TUI.
///
/// Returns the loop task ID.
pub fn run_loop(
    cwd: &Path,
    parent_id: &str,
    options: LoopOptions,
    show_tui: bool,
    output: OutputKind,
    ctx: Option<&mut WorkflowContext>,
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
    } else if let Some(ctx) = ctx {
        if ctx.event_rx.is_some() {
            spawn_drain_finalize(cwd, &loop_task_id, &task_run_options, ctx)?;
        } else {
            emit_loop_started(output_ctx, &loop_task_id, &parent_id);
            task_run(cwd, &loop_task_id, task_run_options.quiet())?;
            emit_loop_completed(output_ctx, &loop_task_id, &parent_id);
        }
    } else {
        emit_loop_started(output_ctx, &loop_task_id, &parent_id);
        task_run(cwd, &loop_task_id, task_run_options.quiet())?;
        emit_loop_completed(output_ctx, &loop_task_id, &parent_id);
    }

    Ok(loop_task_id)
}

/// Spawn the loop agent via `spawn_monitored`, drain task events to show
/// subtask progress in real-time, and finalize the agent run.
fn spawn_drain_finalize(
    cwd: &Path,
    loop_task_id: &str,
    run_options: &TaskRunOptions,
    ctx: &mut WorkflowContext,
) -> Result<()> {
    let prepared = prepare_task_run(cwd, loop_task_id, run_options, |_| {})?;

    let mut agent_handle = match prepared.runtime.spawn_monitored(&prepared.spawn_options) {
        Ok(handle) => handle,
        Err(e) => {
            rollback_if_still_reserved(cwd, &prepared.task_id, &e);
            return Err(e);
        }
    };

    let output = ctx.output;

    if let Some(ref rx) = ctx.event_rx {
        let drain = |rx: &crossbeam_channel::Receiver<TaskEvent>,
                     task_names: &mut HashMap<String, String>| {
            for event in rx.try_iter() {
                match &event {
                    TaskEvent::Created { task_id, name, .. } => {
                        // Tasks created during loop (not seen during decompose)
                        task_names.insert(task_id.clone(), name.clone());
                    }
                    TaskEvent::Started { task_ids, .. } => {
                        for id in task_ids {
                            let name =
                                task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                            output.emit(&format!("  \u{25b8} {}", name));
                        }
                    }
                    TaskEvent::Closed {
                        task_ids, outcome, ..
                    } => {
                        for id in task_ids {
                            let name =
                                task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                            match outcome {
                                TaskOutcome::Done => {
                                    output.emit(&format!("  \u{2714} {} \u{2014} done", name));
                                }
                                TaskOutcome::WontDo => {
                                    output.emit(&format!("  \u{2298} {} \u{2014} skipped", name));
                                }
                            }
                        }
                    }
                    TaskEvent::Stopped { task_ids, .. } => {
                        for id in task_ids {
                            let name =
                                task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                            output.emit(&format!("  \u{2718} {} \u{2014} failed", name));
                        }
                    }
                    _ => {}
                }
            }
        };

        while agent_handle
            .try_wait()
            .map_err(|e| AikiError::AgentSpawnFailed(format!("try_wait failed: {}", e)))?
            .is_none()
        {
            drain(rx, &mut ctx.task_names);
            thread::sleep(Duration::from_millis(100));
        }
        // Final drain for events that arrived after agent finished
        drain(rx, &mut ctx.task_names);
    }

    // Read any diagnostic output
    let proc_output = agent_handle.read_output();
    if !proc_output.stderr.is_empty() {
        ctx.emit(&format!("  agent stderr: {}", proc_output.stderr));
    }

    finalize_agent_run(cwd, loop_task_id)?;

    Ok(())
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
    let cwd = ctx.cwd.clone();
    let output_kind = ctx.output.kind();
    let loop_task_id = run_loop(&cwd, &epic_id, loop_options, false, output_kind, Some(ctx))?;

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

    /// Verify the loop drain logic emits correct output for Started/Closed/Stopped events.
    #[test]
    fn loop_drain_emits_correct_output_for_task_lifecycle() {
        use crate::tasks::types::{TaskOutcome, TaskPriority};

        let (tx, rx) = crossbeam_channel::unbounded();
        let mut task_names: HashMap<String, String> = HashMap::new();
        // Pre-populate task_names (simulating decompose step seeding)
        task_names.insert("sub_001".to_string(), "Fix auth bug".to_string());
        task_names.insert("sub_002".to_string(), "Add error handling".to_string());
        task_names.insert("sub_003".to_string(), "Remove deprecated API".to_string());

        let now = chrono::Utc::now();

        // Created event for a new task not seen during decompose
        tx.send(TaskEvent::Created {
            task_id: "sub_004".to_string(),
            name: "New runtime task".to_string(),
            slug: None,
            task_type: None,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: HashMap::new(),
            timestamp: now,
        })
        .unwrap();

        // Started event
        tx.send(TaskEvent::Started {
            task_ids: vec!["sub_001".to_string()],
            agent_type: "claude-code".to_string(),
            session_id: None,
            turn_id: None,
            working_copy: None,
            timestamp: now,
        })
        .unwrap();

        // Closed event (Done)
        tx.send(TaskEvent::Closed {
            task_ids: vec!["sub_001".to_string()],
            outcome: TaskOutcome::Done,
            confidence: None,
            summary: None,
            session_id: None,
            turn_id: None,
            timestamp: now,
        })
        .unwrap();

        // Closed event (WontDo)
        tx.send(TaskEvent::Closed {
            task_ids: vec!["sub_002".to_string()],
            outcome: TaskOutcome::WontDo,
            confidence: None,
            summary: None,
            session_id: None,
            turn_id: None,
            timestamp: now,
        })
        .unwrap();

        // Stopped event
        tx.send(TaskEvent::Stopped {
            task_ids: vec!["sub_003".to_string()],
            reason: Some("Agent crashed".to_string()),
            session_id: None,
            turn_id: None,
            timestamp: now,
        })
        .unwrap();

        drop(tx);

        // Collect emitted output by running the drain logic
        let mut emitted: Vec<String> = Vec::new();
        for event in rx.try_iter() {
            match &event {
                TaskEvent::Created { task_id, name, .. } => {
                    task_names.insert(task_id.clone(), name.clone());
                }
                TaskEvent::Started { task_ids, .. } => {
                    for id in task_ids {
                        let name = task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                        emitted.push(format!("  \u{25b8} {}", name));
                    }
                }
                TaskEvent::Closed {
                    task_ids, outcome, ..
                } => {
                    for id in task_ids {
                        let name = task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                        match outcome {
                            TaskOutcome::Done => {
                                emitted.push(format!("  \u{2714} {} \u{2014} done", name));
                            }
                            TaskOutcome::WontDo => {
                                emitted.push(format!("  \u{2298} {} \u{2014} skipped", name));
                            }
                        }
                    }
                }
                TaskEvent::Stopped { task_ids, .. } => {
                    for id in task_ids {
                        let name = task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                        emitted.push(format!("  \u{2718} {} \u{2014} failed", name));
                    }
                }
                _ => {}
            }
        }

        // Verify Created populated task_names
        assert_eq!(task_names.get("sub_004").unwrap(), "New runtime task");

        // Verify output matches Text Output Spec
        assert_eq!(emitted.len(), 4);
        assert_eq!(emitted[0], "  \u{25b8} Fix auth bug");
        assert_eq!(emitted[1], "  \u{2714} Fix auth bug \u{2014} done");
        assert_eq!(emitted[2], "  \u{2298} Add error handling \u{2014} skipped");
        assert_eq!(emitted[3], "  \u{2718} Remove deprecated API \u{2014} failed");
    }

    /// Verify that unknown task IDs fall back to the raw ID in output.
    #[test]
    fn loop_drain_uses_raw_id_for_unknown_tasks() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut task_names: HashMap<String, String> = HashMap::new();

        let now = chrono::Utc::now();

        tx.send(TaskEvent::Started {
            task_ids: vec!["unknown_id".to_string()],
            agent_type: "claude-code".to_string(),
            session_id: None,
            turn_id: None,
            working_copy: None,
            timestamp: now,
        })
        .unwrap();

        drop(tx);

        let mut emitted: Vec<String> = Vec::new();
        for event in rx.try_iter() {
            if let TaskEvent::Started { task_ids, .. } = &event {
                for id in task_ids {
                    let name = task_names.get(id).map(|s| s.as_str()).unwrap_or(id);
                    emitted.push(format!("  \u{25b8} {}", name));
                }
            }
        }

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0], "  \u{25b8} unknown_id");
    }
}

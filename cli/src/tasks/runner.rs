//! Task execution runner
//!
//! This module provides functions for executing tasks via agent sessions,
//! including blocking and background (async) execution modes.

use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::Ordering;

use crate::agents::{
    detect_agent_from_process_tree, get_runtime, AgentSessionResult, AgentSpawnOptions, AgentType,
    Assignee, BackgroundHandle,
};
use crate::error::{AikiError, Result};
use crate::session::find_active_session;
use crate::tasks::{
    find_task,
    materialize_graph,
    read_events,
    status_monitor::{MonitorExitReason, StatusMonitor},
    types::{Task, TaskEvent, TaskStatus},
    write_event,
    md::MdBuilder,
    TaskGraph,
};

/// Options for running a task
#[derive(Debug, Clone)]
pub struct TaskRunOptions {
    /// Override the task's assignee agent
    pub agent_override: Option<AgentType>,
    /// Suppress real-time status updates
    pub quiet: bool,
    /// Ordered chain of task IDs for needs-context session execution
    pub chain_task_ids: Option<Vec<String>>,
}

impl Default for TaskRunOptions {
    fn default() -> Self {
        Self {
            agent_override: None,
            quiet: false,
            chain_task_ids: None,
        }
    }
}

impl TaskRunOptions {
    /// Create new task run options
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an agent override
    #[must_use]
    pub fn with_agent(mut self, agent: AgentType) -> Self {
        self.agent_override = Some(agent);
        self
    }

    /// Suppress informational output (spawning, completion messages)
    #[must_use]
    #[allow(dead_code)]
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Set chain task IDs for needs-context session execution
    #[must_use]
    pub fn with_chain(mut self, chain: Vec<String>) -> Self {
        self.chain_task_ids = Some(chain);
        self
    }
}

/// Resolve which agent type to use for running a task.
///
/// Resolution order:
/// 1. Explicit `--agent` override from options
/// 2. Task's assignee field (if set to an agent)
/// 3. Active session's agent type (cheap file lookup, authoritative)
/// 4. Process tree detection (expensive, fallback for non-session contexts)
fn resolve_agent_type(
    cwd: &Path,
    task_id: &str,
    task: &Task,
    options: &TaskRunOptions,
) -> Result<AgentType> {
    // 1. Explicit override
    if let Some(agent) = options.agent_override {
        return Ok(agent);
    }

    // 2. Task assignee
    if let Some(ref assignee_str) = task.assignee {
        match Assignee::from_str(assignee_str) {
            Some(Assignee::Agent(agent)) => return Ok(agent),
            Some(Assignee::Human) => {
                return Err(AikiError::TaskNoAssignee(format!(
                    "Task '{}' is assigned to human, use --agent to specify an agent",
                    task_id
                )));
            }
            Some(Assignee::Unassigned) | None => {}
        }
    }

    // 3. Active session (cheap file lookup, authoritative for aiki-managed sessions)
    if let Some(session) = find_active_session(cwd) {
        return Ok(session.agent_type);
    }

    // 4. Process tree detection (fallback for non-session contexts)
    if let Some(agent) = detect_agent_from_process_tree() {
        return Ok(agent);
    }

    Err(AikiError::TaskNoAssignee(task_id.to_string()))
}

/// Run a task by spawning an agent session
///
/// This function:
/// 1. Loads the task from the aiki/tasks branch
/// 2. Validates the task can be run (not closed)
/// 3. Determines which agent to use (from options or task assignee)
/// 4. Spawns the agent session with task context
/// 5. Shows real-time status updates while waiting (if TTY)
/// 6. Handles the result and updates task state
pub fn task_run(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    let chain_task_ids = options.chain_task_ids.clone();

    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    // Find the task
    let task = find_task(&tasks, task_id)?;
    let task_id = &task.id; // rebind to canonical ID

    // Validate task can be run
    if task.status == TaskStatus::Closed {
        return Err(AikiError::TaskAlreadyClosed(task_id.to_string()));
    }

    // Determine which agent to use
    let agent_type = resolve_agent_type(cwd, task_id, &task, &options)?;

    // Get runtime for the agent
    let runtime = get_runtime(agent_type).ok_or_else(|| {
        AikiError::AgentNotSupported(agent_type.as_str().to_string())
    })?;

    // Emit Started event before spawning to transition task to InProgress immediately.
    // The agent will later claim the task with its session_id via `aiki task start`.
    // This eliminates the visual gap where the task shows as pending while the agent boots.
    if task.status == TaskStatus::Open {
        let pre_start = TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: agent_type.as_str().to_string(),
            session_id: None,
            turn_id: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &pre_start)?;
    }

    // Print status to stderr to avoid corrupting piped output
    if !options.quiet {
        eprintln!(
            "Spawning {} agent session for task {}...",
            agent_type.display_name(),
            task_id
        );
    }

    // Build spawn options with parent session UUID for workspace isolation chaining
    let parent_uuid = find_active_session(cwd).map(|s| s.session_id);
    let mut spawn_options = AgentSpawnOptions::new(cwd, task_id)
        .with_parent_session_uuid(parent_uuid);

    // Pass chain IDs for needs-context session scoping
    if let Some(chain) = chain_task_ids {
        spawn_options = spawn_options.with_chain(chain);
    }

    // Check if we should show live status updates
    let show_status = std::io::stderr().is_terminal() && !options.quiet;

    // Spawn agent session and optionally monitor status
    let result = if show_status {
        // Run agent in background thread while monitoring status in foreground
        run_with_status_monitor(cwd, task_id, runtime.as_ref(), &spawn_options)?
    } else {
        // Simple blocking spawn (no status display)
        runtime.spawn_blocking(&spawn_options)?
    };

    // Handle result - the agent is responsible for claiming and closing the task
    // We just need to handle failures where the agent didn't complete properly
    match &result {
        AgentSessionResult::Completed { summary } => {
            // Agent completed successfully - it should have closed the task itself
            if !options.quiet {
                eprintln!("Task run complete");
                if !summary.is_empty() {
                    eprintln!("Summary: {}", summary);
                }
            }
        }
        AgentSessionResult::Stopped { reason } => {
            // Agent stopped - emit Stopped event if task is not already closed
            let refreshed_events = read_events(cwd)?;
            let mut refreshed_graph = materialize_graph(&refreshed_events);
            if let Ok(refreshed_task) = find_task(&refreshed_graph.tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let is_orchestrator = refreshed_task.is_orchestrator();
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(reason.clone()),
                        session_id: None,
                        turn_id: None,
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;

                    // Cascade-close subtasks if this is an orchestrator task
                    if is_orchestrator {
                        use crate::tasks::manager::get_all_unclosed_descendants;
                        use crate::commands::task::cascade_close_tasks;
                        let unclosed = get_all_unclosed_descendants(&refreshed_graph, task_id);
                        if !unclosed.is_empty() {
                            let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
                            cascade_close_tasks(cwd, &mut refreshed_graph.tasks, &cascade_ids, crate::tasks::types::TaskOutcome::WontDo, "Parent orchestrator stopped")?;
                        }
                    }
                }
            }
            eprintln!("Task {} stopped: {}", task_id, reason);
        }
        AgentSessionResult::Detached => {
            // User detached via Ctrl+C - agent continues running in background
            // Do NOT emit TaskEvent::Stopped since the agent is still working
            eprintln!(
                "Detached. Task {} still running. Use `aiki task show {}` to check status.",
                &task_id[..8.min(task_id.len())],
                task_id
            );
        }
        AgentSessionResult::Failed { error } => {
            // Agent failed - emit Stopped event even if task never reached InProgress
            // This handles spawn failures where the agent never claimed the task
            let refreshed_events = read_events(cwd)?;
            let mut refreshed_graph = materialize_graph(&refreshed_events);
            if let Ok(refreshed_task) = find_task(&refreshed_graph.tasks, task_id) {
                if refreshed_task.status != TaskStatus::Closed {
                    let is_orchestrator = refreshed_task.is_orchestrator();
                    let stop_event = TaskEvent::Stopped {
                        task_ids: vec![task_id.to_string()],
                        reason: Some(format!("Session failed: {}", error)),
                        session_id: None,
                        turn_id: None,
                        timestamp: chrono::Utc::now(),
                    };
                    write_event(cwd, &stop_event)?;

                    // Cascade-close subtasks if this is an orchestrator task
                    if is_orchestrator {
                        use crate::tasks::manager::get_all_unclosed_descendants;
                        use crate::commands::task::cascade_close_tasks;
                        let unclosed = get_all_unclosed_descendants(&refreshed_graph, task_id);
                        if !unclosed.is_empty() {
                            let cascade_ids: Vec<String> = unclosed.iter().map(|t| t.id.clone()).collect();
                            cascade_close_tasks(cwd, &mut refreshed_graph.tasks, &cascade_ids, crate::tasks::types::TaskOutcome::WontDo, "Parent orchestrator failed")?;
                        }
                    }
                }
            }
            return Err(AikiError::AgentSpawnFailed(error.clone()));
        }
    }

    Ok(())
}

/// Run agent with real-time status monitoring
///
/// Spawns the agent in background and monitors status in the foreground.
/// Ctrl+C signals the monitor to stop (detach), but the agent continues running.
fn run_with_status_monitor(
    cwd: &Path,
    task_id: &str,
    runtime: &dyn crate::agents::AgentRuntime,
    spawn_options: &AgentSpawnOptions,
) -> Result<AgentSessionResult> {
    // Spawn agent with child handle for proper exit detection
    // This properly handles zombie processes by using try_wait() instead of kill(pid, 0)
    let mut monitored_child = runtime.spawn_monitored(spawn_options)?;

    // Create status monitor (no longer needs PID since we check exit ourselves)
    let mut monitor = StatusMonitor::new(task_id);
    let stop_flag = monitor.stop_flag();

    // Set up Ctrl+C handler to signal monitor to stop
    let stop_flag_ctrlc = stop_flag.clone();
    let _ = ctrlc::set_handler(move || {
        stop_flag_ctrlc.store(true, Ordering::Relaxed);
    });

    // Run status monitor until task completes, user detaches, or agent exits
    let exit_reason = monitor.monitor_until_complete_with_child(cwd, &mut monitored_child)?;

    match exit_reason {
        MonitorExitReason::UserDetached => {
            // User detached via Ctrl+C - agent continues running in background
            Ok(AgentSessionResult::detached())
        }
        MonitorExitReason::AgentExited { stderr } => {
            // Agent exited without task reaching terminal state - check final status
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;

            if let Some(task) = tasks.get(task_id) {
                match task.status {
                    TaskStatus::Closed => {
                        // Task was closed right before agent exited
                        let summary = task
                            .effective_summary()
                            .unwrap_or_default()
                            .to_string();
                        Ok(AgentSessionResult::Completed { summary })
                    }
                    TaskStatus::Stopped => {
                        // Task was stopped
                        let reason = task
                            .stopped_reason
                            .clone()
                            .unwrap_or_else(|| "Task stopped".to_string());
                        Ok(AgentSessionResult::Stopped { reason })
                    }
                    _ => {
                        // Agent crashed without completing task - include stderr if available
                        let error = if stderr.trim().is_empty() {
                            "Agent process exited without completing task".to_string()
                        } else {
                            format!(
                                "Agent process exited without completing task:\n{}",
                                stderr.trim()
                            )
                        };
                        Ok(AgentSessionResult::Failed { error })
                    }
                }
            } else {
                let error = if stderr.trim().is_empty() {
                    "Task not found after agent exit".to_string()
                } else {
                    format!("Task not found after agent exit:\n{}", stderr.trim())
                };
                Ok(AgentSessionResult::Failed { error })
            }
        }
        MonitorExitReason::TaskCompleted => {
            // Task reached terminal state - check final status
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;

            if let Some(task) = tasks.get(task_id) {
                match task.status {
                    TaskStatus::Closed => {
                        let summary = task
                            .effective_summary()
                            .unwrap_or_default()
                            .to_string();
                        Ok(AgentSessionResult::Completed { summary })
                    }
                    TaskStatus::Stopped => {
                        let reason = task
                            .stopped_reason
                            .clone()
                            .unwrap_or_else(|| "Task stopped".to_string());
                        Ok(AgentSessionResult::Stopped { reason })
                    }
                    _ => {
                        // Shouldn't happen but handle gracefully
                        Ok(AgentSessionResult::Completed {
                            summary: String::new(),
                        })
                    }
                }
            } else {
                Ok(AgentSessionResult::Failed {
                    error: "Task not found after completion".to_string(),
                })
            }
        }
    }
}

/// Run a task and output result
///
/// Wrapper around `task_run` that outputs formatted results.
pub fn run_task_with_output(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run(cwd, task_id, options) {
        Ok(()) => {
            let md = MdBuilder::new("run")
                .build(&format!("## Run Completed\n- **Task:** {}\n", task_id), &[], &[]);
            println!("{}", md);
            Ok(())
        }
        Err(e) => {
            let md = MdBuilder::new("run")
                .error()
                .build_error(&e.to_string());
            println!("{}", md);
            Err(e)
        }
    }
}

/// Spawn a task in the background and return immediately
///
/// This function:
/// 1. Validates the task can be run
/// 2. Determines which agent to use
/// 3. Spawns the agent process in the background (with AIKI_TASK env var)
/// 4. Returns the background handle
///
/// The spawned agent creates a session with task field, which allows
/// `terminate_background_task` to find and terminate it later.
pub fn task_run_async(
    cwd: &Path,
    task_id: &str,
    options: TaskRunOptions,
) -> Result<BackgroundHandle> {
    // Load task from events
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    // Find the task
    let task = find_task(&tasks, task_id)?;
    let task_id = &task.id; // rebind to canonical ID

    // Validate task can be run
    if task.status == TaskStatus::Closed {
        return Err(AikiError::TaskAlreadyClosed(task_id.to_string()));
    }

    // Determine which agent to use
    let agent_type = resolve_agent_type(cwd, task_id, &task, &options)?;

    // Get runtime for the agent
    let runtime = get_runtime(agent_type)
        .ok_or_else(|| AikiError::AgentNotSupported(agent_type.as_str().to_string()))?;

    // Emit Started event before spawning to transition task to InProgress immediately.
    if task.status == TaskStatus::Open {
        let pre_start = TaskEvent::Started {
            task_ids: vec![task_id.to_string()],
            agent_type: agent_type.as_str().to_string(),
            session_id: None,
            turn_id: None,
            timestamp: chrono::Utc::now(),
        };
        write_event(cwd, &pre_start)?;
    }

    // Build spawn options with parent session UUID for workspace isolation chaining
    let parent_uuid = find_active_session(cwd).map(|s| s.session_id);
    let spawn_options = AgentSpawnOptions::new(cwd, task_id)
        .with_parent_session_uuid(parent_uuid);

    // Spawn agent session in background
    // The agent inherits AIKI_TASK env var which gets recorded in its session file
    // This allows terminate_background_task to find and kill it later
    let handle = match runtime.spawn_background(&spawn_options) {
        Ok(h) => h,
        Err(e) => {
            // Compensate: emit Stopped so the task doesn't get stuck in InProgress
            if task.status == TaskStatus::Open {
                let rollback = TaskEvent::Stopped {
                    task_ids: vec![task_id.to_string()],
                    reason: Some(format!("Spawn failed: {}", e)),
                    session_id: None,
                    turn_id: None,
                    timestamp: chrono::Utc::now(),
                };
                let _ = write_event(cwd, &rollback); // best-effort
            }
            return Err(e);
        }
    };

    Ok(handle)
}

/// Run a task asynchronously and output XML result
///
/// Wrapper around `task_run_async` that outputs formatted results.
pub fn run_task_async_with_output(cwd: &Path, task_id: &str, options: TaskRunOptions) -> Result<()> {
    match task_run_async(cwd, task_id, options) {
        Ok(handle) => {
            let md = MdBuilder::new("run").build(
                &format!(
                    "## Run Started\n- **Task:** {}\n- Task started asynchronously.\n",
                    handle.task_id
                ),
                &[],
                &[],
            );
            println!("{}", md);
            Ok(())
        }
        Err(e) => {
            let md = MdBuilder::new("run").error().build_error(&e.to_string());
            println!("{}", md);
            Err(e)
        }
    }
}

/// Result of resolving the next ready subtask
pub enum SubtaskResolution<'a> {
    /// A ready subtask was found
    Ready(&'a Task),
    /// All subtasks are closed — nothing left to do
    AllComplete,
    /// Subtasks exist but none are ready (all blocked or in-progress)
    Blocked(Vec<&'a Task>),
    /// Parent task has no subtasks
    NoSubtasks,
}

/// Resolve the next ready subtask of a parent task.
///
/// Looks at all subtasks of the parent (excluding the synthetic `.0` digest subtask),
/// and returns the first ready (open + unblocked) subtask sorted by priority then
/// creation time.
///
/// Returns:
/// - `Ready(task)` if a ready subtask is found
/// - `AllComplete` if all non-digest subtasks are closed
/// - `Blocked(unclosed)` if subtasks exist but none are ready
/// - `NoSubtasks` if the parent has no subtasks (excluding digest)
pub fn resolve_next_subtask<'a>(
    graph: &'a TaskGraph,
    parent_id: &str,
) -> SubtaskResolution<'a> {
    use crate::tasks::manager::get_subtasks;

    // The DIGEST_SUBTASK_NAME constant is defined in commands/task.rs.
    // Rather than creating a cross-module dependency, we match by the known name.
    const DIGEST_SUBTASK_NAME: &str = "Digest subtasks and start first batch";

    // Get all subtasks, excluding the synthetic .0 digest
    let subtasks: Vec<&Task> = get_subtasks(graph, parent_id)
        .into_iter()
        .filter(|t| t.name != DIGEST_SUBTASK_NAME)
        .collect();

    if subtasks.is_empty() {
        return SubtaskResolution::NoSubtasks;
    }

    // Filter to ready subtasks (open + unblocked)
    let mut ready: Vec<&Task> = subtasks
        .iter()
        .copied()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| !graph.is_blocked(&t.id))
        .collect();

    // Sort by priority (P0 first), then by creation time (oldest first)
    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    if let Some(first) = ready.first() {
        return SubtaskResolution::Ready(first);
    }

    // No ready subtasks — check if all are closed vs some are blocked/in-progress
    let unclosed: Vec<&Task> = subtasks
        .into_iter()
        .filter(|t| t.status != TaskStatus::Closed)
        .collect();

    if unclosed.is_empty() {
        SubtaskResolution::AllComplete
    } else {
        SubtaskResolution::Blocked(unclosed)
    }
}

/// Result of resolving the next session (needs-context aware)
pub enum SessionResolution<'a> {
    /// A single standalone task (no needs-context chain)
    Standalone(&'a Task),
    /// A needs-context chain (ordered task IDs from head to tail)
    Chain(Vec<String>),
    /// All subtasks are closed — nothing left to do
    AllComplete,
    /// Subtasks exist but none are ready (all blocked or in-progress)
    Blocked(Vec<&'a Task>),
    /// Parent task has no subtasks
    NoSubtasks,
}

/// Resolve the next session to run for a parent task.
///
/// Like `resolve_next_subtask`, but needs-context aware: if the next ready
/// subtask is the head of a `needs-context` chain, returns `Chain` with the
/// full ordered list of chain task IDs. For standalone tasks, returns
/// `Standalone`.
pub fn resolve_next_session<'a>(
    graph: &'a TaskGraph,
    parent_id: &str,
) -> SessionResolution<'a> {
    match resolve_next_subtask(graph, parent_id) {
        SubtaskResolution::Ready(task) => {
            if graph.is_needs_context_head(&task.id) {
                let chain = graph.get_needs_context_chain(&task.id);
                SessionResolution::Chain(chain)
            } else {
                SessionResolution::Standalone(task)
            }
        }
        SubtaskResolution::AllComplete => SessionResolution::AllComplete,
        SubtaskResolution::Blocked(unclosed) => SessionResolution::Blocked(unclosed),
        SubtaskResolution::NoSubtasks => SessionResolution::NoSubtasks,
    }
}

/// Resolve the next session to run within a specific lane.
///
/// Like `resolve_next_session`, but restricted to subtasks within the
/// lane identified by `lane_prefix` (head task ID prefix matching).
pub fn resolve_next_session_in_lane<'a>(
    graph: &'a TaskGraph,
    parent_id: &str,
    lane_prefix: &str,
) -> crate::error::Result<SessionResolution<'a>> {
    use crate::tasks::lanes::{derive_lanes, get_lane_task_ids, resolve_lane_prefix};
    use crate::tasks::manager::get_subtasks;
    use crate::tasks::md::short_id;

    let decomp = derive_lanes(graph, parent_id);

    // Resolve the lane prefix to a full lane head ID
    let lane_head = resolve_lane_prefix(&decomp, lane_prefix, short_id(parent_id))
        .map_err(|msg| crate::error::AikiError::InvalidArgument(msg))?;

    // Get task IDs in the lane
    let lane_task_ids = get_lane_task_ids(&decomp, &lane_head)
        .ok_or_else(|| crate::error::AikiError::InvalidArgument(
            format!("Lane '{}' not found", lane_head)
        ))?;

    const DIGEST_SUBTASK_NAME: &str = "Digest subtasks and start first batch";

    // Get subtasks filtered to this lane
    let subtasks: Vec<&Task> = get_subtasks(graph, parent_id)
        .into_iter()
        .filter(|t| t.name != DIGEST_SUBTASK_NAME)
        .filter(|t| lane_task_ids.contains(&t.id))
        .collect();

    if subtasks.is_empty() {
        return Ok(SessionResolution::NoSubtasks);
    }

    // Filter to ready subtasks within the lane (open + unblocked)
    let mut ready: Vec<&Task> = subtasks
        .iter()
        .copied()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| !graph.is_blocked(&t.id))
        .collect();

    ready.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    if let Some(first) = ready.first() {
        if graph.is_needs_context_head(&first.id) {
            let chain = graph.get_needs_context_chain(&first.id);
            return Ok(SessionResolution::Chain(chain));
        } else {
            return Ok(SessionResolution::Standalone(first));
        }
    }

    // No ready subtasks in this lane
    let unclosed: Vec<&Task> = subtasks
        .into_iter()
        .filter(|t| t.status != TaskStatus::Closed)
        .collect();

    if unclosed.is_empty() {
        Ok(SessionResolution::AllComplete)
    } else {
        Ok(SessionResolution::Blocked(unclosed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_run_options_default() {
        let options = TaskRunOptions::default();
        assert!(options.agent_override.is_none());
        assert!(!options.quiet);
    }

    #[test]
    fn test_task_run_options_with_agent() {
        let options = TaskRunOptions::new().with_agent(AgentType::ClaudeCode);
        assert_eq!(options.agent_override, Some(AgentType::ClaudeCode));
    }

    #[test]
    fn test_task_run_options_with_chain() {
        let chain = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let options = TaskRunOptions::new().with_chain(chain.clone());
        assert_eq!(options.chain_task_ids, Some(chain));
    }

    // --- resolve_next_session tests ---
    // These require building a TaskGraph with subtask-of links

    use crate::tasks::graph::materialize_graph;
    use crate::tasks::types::{TaskEvent, TaskPriority};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_created(id: &str, name: &str) -> TaskEvent {
        TaskEvent::Created {
            task_id: id.to_string(),
            name: name.to_string(),
            slug: None,
            task_type: None,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    fn make_link(from: &str, to: &str, kind: &str) -> TaskEvent {
        TaskEvent::LinkAdded {
            from: from.to_string(),
            to: to.to_string(),
            kind: kind.to_string(),
            autorun: None,
            timestamp: Utc::now(),
        }
    }

    fn make_closed(id: &str) -> TaskEvent {
        TaskEvent::Closed {
            session_id: None,
            task_ids: vec![id.to_string()],
            outcome: crate::tasks::types::TaskOutcome::Done,
            summary: None,
            turn_id: None,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_resolve_next_session_standalone() {
        // Parent P with subtask A (no needs-context) → Standalone
        let events = vec![
            make_created("P", "Parent"),
            make_created("A", "Task A"),
            make_link("A", "P", "subtask-of"),
        ];
        let graph = materialize_graph(&events);
        match resolve_next_session(&graph, "P") {
            SessionResolution::Standalone(task) => {
                assert_eq!(task.id, "A");
            }
            other => panic!("Expected Standalone, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn test_resolve_next_session_chain_head() {
        // Parent P with subtasks A→B→C (needs-context chain)
        // A is ready and is the chain head → Chain([A, B, C])
        let events = vec![
            make_created("P", "Parent"),
            make_created("A", "Explore"),
            make_created("B", "Plan"),
            make_created("C", "Implement"),
            make_link("A", "P", "subtask-of"),
            make_link("B", "P", "subtask-of"),
            make_link("C", "P", "subtask-of"),
            make_link("B", "A", "needs-context"),
            make_link("C", "B", "needs-context"),
        ];
        let graph = materialize_graph(&events);
        match resolve_next_session(&graph, "P") {
            SessionResolution::Chain(chain) => {
                assert_eq!(chain, vec!["A", "B", "C"]);
            }
            other => panic!("Expected Chain, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn test_resolve_next_session_all_complete() {
        // Parent P with all subtasks closed → AllComplete
        let events = vec![
            make_created("P", "Parent"),
            make_created("A", "Task A"),
            make_link("A", "P", "subtask-of"),
            make_closed("A"),
        ];
        let graph = materialize_graph(&events);
        assert!(matches!(
            resolve_next_session(&graph, "P"),
            SessionResolution::AllComplete
        ));
    }

    #[test]
    fn test_resolve_next_session_no_subtasks() {
        // Parent P with no subtasks → NoSubtasks
        let events = vec![make_created("P", "Parent")];
        let graph = materialize_graph(&events);
        assert!(matches!(
            resolve_next_session(&graph, "P"),
            SessionResolution::NoSubtasks
        ));
    }

    #[test]
    fn test_resolve_next_session_non_head_chain_member() {
        // Parent P with subtasks: A→B (needs-context chain) + C (standalone)
        // A is done, B is ready but is NOT a chain head → Standalone(B)
        let events = vec![
            make_created("P", "Parent"),
            make_created("A", "Explore"),
            make_created("B", "Plan"),
            make_created("C", "Review"),
            make_link("A", "P", "subtask-of"),
            make_link("B", "P", "subtask-of"),
            make_link("C", "P", "subtask-of"),
            make_link("B", "A", "needs-context"),
            make_closed("A"),
        ];
        let graph = materialize_graph(&events);
        // B is now ready (A is done). B has a predecessor in needs-context, so it's NOT a head.
        // C is also ready (standalone). Both are open+unblocked.
        // resolve_next_subtask picks by priority then creation time.
        // B was created before C, so B is picked first.
        // B is not a chain head (it has a predecessor A), so → Standalone(B)
        match resolve_next_session(&graph, "P") {
            SessionResolution::Standalone(task) => {
                assert_eq!(task.id, "B");
            }
            other => panic!("Expected Standalone(B), got {:?}", std::mem::discriminant(&other)),
        }
    }
}

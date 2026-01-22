//! Task management system for Aiki
//!
//! Provides an AI-first task tracking system with:
//! - Event-sourced storage on `aiki/tasks` branch
//! - XML output format for agent consumption
//! - Ready queue calculation with priority sorting
//! - Task execution via agent runtimes
//! - Template-based task creation

pub mod id;
pub mod manager;
pub mod runner;
pub mod storage;
pub mod templates;
pub mod types;
pub mod xml;

pub use id::{generate_child_id, generate_task_id, get_next_subtask_number, is_task_id};
#[allow(unused_imports)]
pub use manager::{
    all_subtasks_closed, find_task, get_subtasks, get_current_scope_set, get_in_progress,
    get_ready_queue, get_ready_queue_for_agent, get_ready_queue_for_agent_scoped,
    get_ready_queue_for_human, get_ready_queue_for_scope_set, get_scoped_ready_queue,
    get_unclosed_subtasks, has_subtasks, materialize_tasks, ScopeSet,
};
#[allow(unused_imports)]
pub use storage::{ensure_tasks_branch, read_events, write_event};
#[allow(unused_imports)]
pub use types::{Task, TaskComment, TaskEvent, TaskOutcome, TaskPriority, TaskStatus};
pub use xml::XmlBuilder;

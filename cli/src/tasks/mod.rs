//! Task management system for Aiki
//!
//! Provides an AI-first task tracking system with:
//! - Event-sourced storage on `aiki/tasks` branch
//! - XML output format for agent consumption
//! - Ready queue calculation with priority sorting

pub mod id;
pub mod manager;
pub mod storage;
pub mod types;
pub mod xml;

pub use id::{generate_child_id, generate_task_id, get_next_child_number};
pub use manager::{
    all_children_closed, find_task, get_children, get_current_scope_set, get_current_scopes,
    get_in_progress, get_ready_queue, get_scoped_ready_queue, get_unclosed_children, has_children,
    materialize_tasks, ScopeSet,
};
pub use storage::{ensure_tasks_branch, read_events, write_event};
pub use types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus};
pub use xml::XmlBuilder;

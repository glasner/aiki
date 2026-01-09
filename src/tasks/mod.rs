//! Task management system for Aiki
//!
//! Provides an AI-first task tracking system with:
//! - Event-sourced storage on `aiki/tasks` branch
//! - XML output format for agent consumption
//! - Ready queue calculation with priority sorting

pub mod engine;
pub mod id;
pub mod storage;
pub mod types;
pub mod xml;

pub use engine::{get_in_progress, get_ready_queue, materialize_tasks};
pub use id::generate_task_id;
pub use storage::{read_events, write_event};
pub use types::{Task, TaskEvent, TaskOutcome, TaskPriority, TaskStatus};
pub use xml::XmlBuilder;

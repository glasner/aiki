//! Core types for the task system

use chrono::{DateTime, Utc};
use std::fmt;

/// Task status (derived from event stream)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Ready to work on
    Open,
    /// Currently being worked on
    InProgress,
    /// Was in progress, now stopped (has reason)
    Stopped,
    /// Done or won't do
    Closed,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Open => write!(f, "open"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Stopped => write!(f, "stopped"),
            TaskStatus::Closed => write!(f, "closed"),
        }
    }
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// Critical/urgent
    P0,
    /// High priority
    P1,
    /// Normal priority (default)
    P2,
    /// Low priority
    P3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::P2
    }
}

impl fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskPriority::P0 => write!(f, "p0"),
            TaskPriority::P1 => write!(f, "p1"),
            TaskPriority::P2 => write!(f, "p2"),
            TaskPriority::P3 => write!(f, "p3"),
        }
    }
}

impl TaskPriority {
    /// Parse priority from string (e.g., "p0", "p1", "p2", "p3")
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "p0" => Some(TaskPriority::P0),
            "p1" => Some(TaskPriority::P1),
            "p2" => Some(TaskPriority::P2),
            "p3" => Some(TaskPriority::P3),
            _ => None,
        }
    }
}

/// Task closure outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    /// Completed successfully
    Done,
    /// Won't implement
    WontDo,
}

impl fmt::Display for TaskOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskOutcome::Done => write!(f, "done"),
            TaskOutcome::WontDo => write!(f, "wont_do"),
        }
    }
}

impl TaskOutcome {
    /// Parse outcome from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "done" => Some(TaskOutcome::Done),
            "wont_do" | "wontdo" => Some(TaskOutcome::WontDo),
            _ => None,
        }
    }
}

/// Events stored on aiki/tasks branch
#[derive(Debug, Clone)]
pub enum TaskEvent {
    /// Task was created
    Created {
        task_id: String,
        name: String,
        priority: TaskPriority,
        assignee: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Task(s) were started (batch operation)
    Started {
        task_ids: Vec<String>,
        agent_type: String,
        timestamp: DateTime<Utc>,
        /// Task IDs that were auto-stopped when these tasks started
        stopped: Vec<String>,
    },
    /// Task(s) were stopped (batch operation)
    Stopped {
        task_ids: Vec<String>,
        reason: Option<String>,
        /// If set, creates a blocker task assigned to human
        blocked_reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Task(s) were closed (batch operation)
    Closed {
        task_ids: Vec<String>,
        outcome: TaskOutcome,
        timestamp: DateTime<Utc>,
    },
}

/// Materialized task view (computed from events)
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub assignee: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Latest stop reason (if stopped)
    pub stopped_reason: Option<String>,
    /// Closure outcome (if closed)
    pub closed_outcome: Option<TaskOutcome>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(TaskPriority::P0 < TaskPriority::P1);
        assert!(TaskPriority::P1 < TaskPriority::P2);
        assert!(TaskPriority::P2 < TaskPriority::P3);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(TaskPriority::default(), TaskPriority::P2);
    }

    #[test]
    fn test_priority_display() {
        assert_eq!(TaskPriority::P0.to_string(), "p0");
        assert_eq!(TaskPriority::P2.to_string(), "p2");
    }

    #[test]
    fn test_priority_from_str() {
        assert_eq!(TaskPriority::from_str("p0"), Some(TaskPriority::P0));
        assert_eq!(TaskPriority::from_str("P2"), Some(TaskPriority::P2));
        assert_eq!(TaskPriority::from_str("invalid"), None);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(TaskStatus::Open.to_string(), "open");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
    }

    #[test]
    fn test_outcome_display() {
        assert_eq!(TaskOutcome::Done.to_string(), "done");
        assert_eq!(TaskOutcome::WontDo.to_string(), "wont_do");
    }
}

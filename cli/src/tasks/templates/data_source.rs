//! Data source resolution for template iteration
//!
//! When a template has `subtasks: source.comments` in frontmatter, this module
//! handles parsing the source specification and resolving it to actual data
//! that can be iterated over to create subtasks.
//!
//! # Example
//!
//! ```yaml
//! ---
//! version: 1.0.0
//! subtasks: source.comments
//! ---
//! ```
//!
//! The source string "source.comments" is parsed into `DataSource::Comments`,
//! and the task ID is provided via CLI `--source task:<id>` option.

use crate::error::{AikiError, Result};
use crate::tasks::types::{Task, TaskComment};

/// Data source for template iteration
///
/// Specifies where to fetch data for dynamic subtask creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSource {
    /// Comments from a task (task_id comes from --source task:<id> option)
    Comments,
}

/// Parse a data source string from template frontmatter
///
/// # Arguments
/// * `source_str` - The source specification string (e.g., "source.comments")
///
/// # Returns
/// * `Ok(DataSource)` if the string is a valid data source specification
/// * `Err(AikiError::UnknownDataSource)` if the string is not recognized
///
/// # Examples
/// ```
/// use aiki::tasks::templates::data_source::parse_data_source;
///
/// assert!(parse_data_source("source.comments").is_ok());
/// assert!(parse_data_source("invalid").is_err());
/// ```
pub fn parse_data_source(source_str: &str) -> Result<DataSource> {
    match source_str.trim() {
        "source.comments" => Ok(DataSource::Comments),
        // Future: add more data sources here
        // "source.files" => Ok(DataSource::Files),
        // "source.changes" => Ok(DataSource::Changes),
        other => Err(AikiError::UnknownDataSource(other.to_string())),
    }
}

/// Resolve a data source to a Vec of items for iteration
///
/// Each item in the returned Vec represents one subtask to create.
///
/// # Arguments
/// * `source` - The parsed DataSource
/// * `task_id` - The task ID to fetch data from (from --source task:<id>)
/// * `tasks` - The materialized tasks HashMap
///
/// # Returns
/// Vec of `TaskComment` for comments data source (can be refactored to a trait
/// when other data sources are added)
///
/// # Errors
/// Returns `AikiError::TaskNotFound` if the specified task doesn't exist
pub fn resolve_data_source(
    source: &DataSource,
    task_id: &str,
    tasks: &std::collections::HashMap<String, Task>,
) -> Result<Vec<TaskComment>> {
    match source {
        DataSource::Comments => resolve_comments(task_id, tasks),
    }
}

/// Resolve comments from a task
fn resolve_comments(
    task_id: &str,
    tasks: &std::collections::HashMap<String, Task>,
) -> Result<Vec<TaskComment>> {
    let task = tasks
        .get(task_id)
        .ok_or_else(|| AikiError::TaskNotFound(task_id.to_string()))?;

    Ok(task.comments.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskPriority, TaskStatus};
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_task(id: &str, comments: Vec<TaskComment>) -> Task {
        Task {
            id: id.to_string(),
            name: format!("Test task {}", id),
            task_type: None,
            status: TaskStatus::Open,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            comments,
        }
    }

    fn create_test_comment(text: &str) -> TaskComment {
        TaskComment {
            id: None,
            text: text.to_string(),
            timestamp: Utc::now(),
            data: HashMap::new(),
        }
    }

    fn create_test_comment_with_data(text: &str, data: HashMap<String, String>) -> TaskComment {
        TaskComment {
            id: None,
            text: text.to_string(),
            timestamp: Utc::now(),
            data,
        }
    }

    // Tests for parse_data_source

    #[test]
    fn test_parse_data_source_comments() {
        let result = parse_data_source("source.comments");
        assert_eq!(result.unwrap(), DataSource::Comments);
    }

    #[test]
    fn test_parse_data_source_comments_with_whitespace() {
        let result = parse_data_source("  source.comments  ");
        assert_eq!(result.unwrap(), DataSource::Comments);
    }

    #[test]
    fn test_parse_data_source_invalid() {
        assert!(parse_data_source("invalid").is_err());
        assert!(parse_data_source("source.unknown").is_err());
        assert!(parse_data_source("comments").is_err());
        assert!(parse_data_source("").is_err());
    }

    #[test]
    fn test_parse_data_source_error_message() {
        let result = parse_data_source("source.unknown");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AikiError::UnknownDataSource(_)));
        assert!(err.to_string().contains("source.unknown"));
        assert!(err.to_string().contains("source.comments")); // Shows valid options
    }

    // Tests for resolve_data_source

    #[test]
    fn test_resolve_comments_empty() {
        let mut tasks = HashMap::new();
        let task = create_test_task("task1", vec![]);
        tasks.insert("task1".to_string(), task);

        let result = resolve_data_source(&DataSource::Comments, "task1", &tasks).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_comments_single() {
        let mut tasks = HashMap::new();
        let task = create_test_task("task1", vec![create_test_comment("Fix the bug")]);
        tasks.insert("task1".to_string(), task);

        let result = resolve_data_source(&DataSource::Comments, "task1", &tasks).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Fix the bug");
        assert!(result[0].data.is_empty());
    }

    #[test]
    fn test_resolve_comments_multiple() {
        let mut tasks = HashMap::new();
        let task = create_test_task(
            "task1",
            vec![
                create_test_comment("First comment"),
                create_test_comment("Second comment"),
                create_test_comment("Third comment"),
            ],
        );
        tasks.insert("task1".to_string(), task);

        let result = resolve_data_source(&DataSource::Comments, "task1", &tasks).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "First comment");
        assert_eq!(result[1].text, "Second comment");
        assert_eq!(result[2].text, "Third comment");
    }

    #[test]
    fn test_resolve_comments_with_data() {
        let mut comment_data = HashMap::new();
        comment_data.insert("severity".to_string(), "error".to_string());
        comment_data.insert("file".to_string(), "src/main.rs".to_string());
        comment_data.insert("line".to_string(), "42".to_string());

        let mut tasks = HashMap::new();
        let task = create_test_task(
            "task1",
            vec![create_test_comment_with_data("Fix null check", comment_data)],
        );
        tasks.insert("task1".to_string(), task);

        let result = resolve_data_source(&DataSource::Comments, "task1", &tasks).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Fix null check");
        assert_eq!(result[0].data.get("severity"), Some(&"error".to_string()));
        assert_eq!(result[0].data.get("file"), Some(&"src/main.rs".to_string()));
        assert_eq!(result[0].data.get("line"), Some(&"42".to_string()));
    }

    #[test]
    fn test_resolve_comments_task_not_found() {
        let tasks = HashMap::new();

        let result = resolve_data_source(&DataSource::Comments, "nonexistent", &tasks);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, AikiError::TaskNotFound(_)));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn test_resolve_data_source_dispatches_correctly() {
        let mut tasks = HashMap::new();
        let task = create_test_task("task1", vec![create_test_comment("Test")]);
        tasks.insert("task1".to_string(), task);

        let result = resolve_data_source(&DataSource::Comments, "task1", &tasks).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Test");
    }
}

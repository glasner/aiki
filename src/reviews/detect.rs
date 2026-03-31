//! Review target detection — resolve CLI args to a ReviewScope.

use std::path::{Path, PathBuf};

use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::reviews::{ReviewScope, ReviewScopeKind};
use crate::session::find_active_session;
use crate::tasks::looks_like_task_id;
use crate::tasks::md::MdBuilder;
use crate::tasks::{find_task, materialize_graph, read_events, Task, TaskStatus};

fn output_nothing_to_review() -> Result<()> {
    output_utils::emit(|| {
        MdBuilder::new().build("Nothing to review — no closed tasks in session.\n")
    });
    Ok(())
}

/// Detect the review target from the CLI argument and flags.
///
/// Returns a `ReviewScope` and optionally a worker agent string (for task targets).
/// The `cwd` is needed to resolve file paths and load tasks.
pub fn detect_target(
    cwd: &Path,
    arg: Option<&str>,
    code: bool,
) -> Result<(ReviewScope, Option<String>)> {
    match arg {
        None => {
            if code {
                return Err(AikiError::InvalidArgument(
                    "--code flag only applies to file targets".to_string(),
                ));
            }

            // Session scope — collect closed tasks from current session
            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let session = find_active_session(cwd);

            let (session_id, session_agent) = match &session {
                Some(s) => (
                    Some(s.session_id.clone()),
                    Some(s.agent_type.as_str().to_string()),
                ),
                None => (None, None),
            };

            let closed_tasks: Vec<Task> = tasks
                .values()
                .filter(|t| {
                    t.status == TaskStatus::Closed
                        && match (&t.last_session_id, &session_id) {
                            (Some(task_session), Some(current_session)) => {
                                task_session == current_session
                            }
                            (_, None) => true,
                            (None, Some(_)) => false,
                        }
                })
                .cloned()
                .collect();

            if closed_tasks.is_empty() {
                output_nothing_to_review()?;
                return Err(AikiError::NothingToReview);
            }

            let task_ids: Vec<String> = closed_tasks.iter().map(|t| t.id.clone()).collect();
            let fallback_id = {
                let mut ids = task_ids.clone();
                ids.sort();
                let hash_input = ids.join(",");
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, hash_input.as_bytes()).to_string()
            };
            let scope = ReviewScope {
                kind: ReviewScopeKind::Session,
                id: session_id.unwrap_or(fallback_id),
                task_ids,
            };
            Ok((scope, session_agent))
        }

        Some(s) if s.ends_with(".md") && PathBuf::from(s).exists() => {
            let kind = if code {
                ReviewScopeKind::Code
            } else {
                ReviewScopeKind::Plan
            };
            Ok((
                ReviewScope {
                    kind,
                    id: s.to_string(),
                    task_ids: vec![],
                },
                None,
            ))
        }

        Some(s) if s.ends_with(".md") => {
            Err(AikiError::InvalidArgument(format!("File not found: {}", s)))
        }

        Some(s) if looks_like_task_id(s) => {
            if code {
                return Err(AikiError::InvalidArgument(
                    "--code flag only applies to file targets".to_string(),
                ));
            }

            let events = read_events(cwd)?;
            let tasks = materialize_graph(&events).tasks;
            let task = find_task(&tasks, s)?;
            let worker = task.assignee.as_deref().map(|s| s.to_string());
            let scope = ReviewScope {
                kind: ReviewScopeKind::Task,
                id: task.id.clone(),
                task_ids: vec![],
            };
            Ok((scope, worker))
        }

        Some(s) if Path::new(s).exists() => Err(AikiError::InvalidArgument(
            "File review only supports .md files currently".to_string(),
        )),

        Some(s) => Err(AikiError::InvalidArgument(format!(
            "Target not found: {}",
            s
        ))),
    }
}

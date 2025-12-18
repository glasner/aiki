use crate::cache::{debug_log, DEBUG_ENABLED};
use crate::error::Result;

use crate::events::result::HookResult;
use crate::events::{
    self, AikiChangeCompletedPayload, AikiChangePermissionAskedPayload, AikiEvent,
    AikiSessionEndedPayload, ChangeOperation, DeleteOperation, MoveOperation,
};
use crate::session::AikiSession;
use crate::tools::{parse_file_operation_from_shell_command, FileOperation};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Cache for move operation directory detection results.
///
/// When shell.permission_asked fires for an `mv` command, we detect whether the destination
/// is a directory using the pre-move filesystem state. We cache this result so that when
/// shell.completed fires for the same command, we can use the correct detection instead
/// of relying on syntactic-only detection (which misses `mv file existing_dir` without
/// trailing slash).
///
/// Key: (session_id, command_string)
/// Value: dest_is_directory (bool)
static MOVE_DIR_CACHE: std::sync::LazyLock<Mutex<HashMap<(String, String), bool>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Dispatch an event to the appropriate handler
///
/// This is the central routing point for all events in the system.
/// Events are routed based on their type, and handlers return generic
/// HookResult objects that can be translated to editor-specific formats.
pub fn dispatch(event: AikiEvent) -> Result<HookResult> {
    // Handle unsupported events immediately
    if matches!(event, AikiEvent::Unsupported) {
        return Ok(HookResult::success());
    }

    // Log event for debugging (uses cached debug flag)
    if *DEBUG_ENABLED {
        let event_type_name = match &event {
            // Session lifecycle
            AikiEvent::SessionStarted(_) => "session.started",
            AikiEvent::SessionResumed(_) => "session.resumed",
            AikiEvent::SessionEnded(_) => "session.ended",
            // User / agent interaction
            AikiEvent::PromptSubmitted(_) => "prompt.submitted",
            AikiEvent::ResponseReceived(_) => "response.received",
            // Read operations
            AikiEvent::ReadPermissionAsked(_) => "read.permission_asked",
            AikiEvent::ReadCompleted(_) => "read.completed",
            // Change operations (unified mutations: write, delete, move)
            AikiEvent::ChangePermissionAsked(_) => "change.permission_asked",
            AikiEvent::ChangeCompleted(_) => "change.completed",
            // Shell commands
            AikiEvent::ShellPermissionAsked(_) => "shell.permission_asked",
            AikiEvent::ShellCompleted(_) => "shell.completed",
            // Web access
            AikiEvent::WebPermissionAsked(_) => "web.permission_asked",
            AikiEvent::WebCompleted(_) => "web.completed",
            // MCP tools
            AikiEvent::McpPermissionAsked(_) => "mcp.permission_asked",
            AikiEvent::McpCompleted(_) => "mcp.completed",
            // Git integration
            AikiEvent::CommitMessageStarted(_) => "commit.message_started",
            // Fallback
            AikiEvent::Unsupported => "unsupported",
        };
        debug_log(|| {
            format!(
                "Dispatching event: {} from agent: {:?}",
                event_type_name,
                event.agent_type()
            )
        });
    }

    // Route to appropriate handler
    let result = match event {
        // Session lifecycle
        AikiEvent::SessionStarted(e) => events::handle_session_started(e),
        AikiEvent::SessionResumed(e) => events::handle_session_resumed(e),
        AikiEvent::SessionEnded(e) => events::handle_session_ended(e),

        // User / agent interaction
        AikiEvent::PromptSubmitted(e) => events::handle_prompt_submitted(e),
        AikiEvent::ResponseReceived(e) => {
            // Extract fields we'll need for session.ended before consuming the event
            let session = e.session.clone();
            let cwd = e.cwd.clone();

            // Handle response.received and check for autoreply
            let response = events::handle_response_received(e)?;

            // Allow benchmark to force autoreply behavior (skip session.ended)
            // Preserve actual failures/decisions but override context
            if std::env::var("AIKI_BENCHMARK_FORCE_AUTOREPLY").is_ok() {
                return Ok(HookResult {
                    context: Some("benchmark-autoreply".to_string()),
                    decision: response.decision,
                    failures: response.failures,
                });
            }

            // If response.received produced an autoreply, return it (session continues)
            if response.has_context() {
                return Ok(response);
            }

            // No autoreply - session is done, trigger session.ended event
            let session_ended_event = build_session_ended_payload(session, cwd);
            events::handle_session_ended(session_ended_event)
        }

        // Read operations
        AikiEvent::ReadPermissionAsked(e) => events::handle_read_permission_asked(e),
        AikiEvent::ReadCompleted(e) => events::handle_read_completed(e),

        // Change operations (unified mutations: write, delete, move)
        AikiEvent::ChangePermissionAsked(e) => events::handle_change_permission_asked(e),
        AikiEvent::ChangeCompleted(e) => events::handle_change_completed(e),

        // Shell commands - transform rm/rmdir to change.* events with Delete operation,
        // and mv to change.* events with Move operation
        AikiEvent::ShellPermissionAsked(e) => {
            let command = e.command.clone();
            let (file_op, paths) = parse_file_operation_from_shell_command(&command);
            match file_op {
                Some(FileOperation::Delete) => {
                    let change_event = transform_shell_delete_to_change_permission_asked(e, paths);
                    return events::handle_change_permission_asked(change_event);
                }
                Some(FileOperation::Move) => {
                    let change_event =
                        transform_shell_move_to_change_permission_asked(e, paths, &command);
                    return events::handle_change_permission_asked(change_event);
                }
                _ => events::handle_shell_permission_asked(e),
            }
        }
        AikiEvent::ShellCompleted(e) => {
            let command = e.command.clone();
            let (file_op, paths) = parse_file_operation_from_shell_command(&command);
            match file_op {
                Some(FileOperation::Delete) => {
                    let change_event = transform_shell_delete_to_change_completed(e, paths);
                    return events::handle_change_completed(change_event);
                }
                Some(FileOperation::Move) => {
                    let change_event = transform_shell_move_to_change_completed(e, paths, &command);
                    return events::handle_change_completed(change_event);
                }
                _ => events::handle_shell_completed(e),
            }
        }

        // Web access
        AikiEvent::WebPermissionAsked(e) => events::handle_web_permission_asked(e),
        AikiEvent::WebCompleted(e) => events::handle_web_completed(e),

        // MCP tools
        AikiEvent::McpPermissionAsked(e) => events::handle_mcp_permission_asked(e),
        AikiEvent::McpCompleted(e) => events::handle_mcp_completed(e),

        // Git integration
        AikiEvent::CommitMessageStarted(e) => events::handle_commit_message_started(e),

        // Fallback
        AikiEvent::Unsupported => return Ok(HookResult::success()),
    };

    // If handler fails, return a failure response instead of propagating error
    match result {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("Warning: Aiki event handler failed: {}", e);
            Ok(HookResult::failure(format!("Aiki handler failed: {}", e)))
        }
    }
}

/// Build a session.ended event payload
///
/// Called automatically when response.received doesn't generate an autoreply.
fn build_session_ended_payload(session: AikiSession, cwd: PathBuf) -> AikiSessionEndedPayload {
    debug_log(|| "No autoreply generated - ending session automatically");

    AikiSessionEndedPayload {
        session,
        cwd,
        timestamp: chrono::Utc::now(),
    }
}

/// Transform shell.permission_asked to change.permission_asked for rm/rmdir commands
///
/// Called when shell command is detected as a delete operation.
fn transform_shell_delete_to_change_permission_asked(
    shell_event: crate::events::AikiShellPermissionAskedPayload,
    paths: Vec<String>,
) -> AikiChangePermissionAskedPayload {
    debug_log(|| {
        format!(
            "Transforming shell.permission_asked (rm/rmdir) to change.permission_asked: {:?}",
            paths
        )
    });

    AikiChangePermissionAskedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        operation: ChangeOperation::Delete(DeleteOperation { file_paths: paths }),
    }
}

/// Transform shell.completed to change.completed for rm/rmdir commands
///
/// Called when shell command is detected as a delete operation.
fn transform_shell_delete_to_change_completed(
    shell_event: crate::events::AikiShellCompletedPayload,
    paths: Vec<String>,
) -> AikiChangeCompletedPayload {
    debug_log(|| {
        format!(
            "Transforming shell.completed (rm/rmdir) to change.completed: {:?}",
            paths
        )
    });

    AikiChangeCompletedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        success: shell_event.success,
        operation: ChangeOperation::Delete(DeleteOperation { file_paths: paths }),
    }
}

/// Transform shell.permission_asked to change.permission_asked for mv commands
///
/// Called when shell command is detected as a move operation.
/// The paths vector should contain source path(s) followed by destination path.
///
/// Uses filesystem-based directory detection since we're in the pre-move state.
/// Caches the detection result for use by the corresponding completed event.
fn transform_shell_move_to_change_permission_asked(
    shell_event: crate::events::AikiShellPermissionAskedPayload,
    paths: Vec<String>,
    command: &str,
) -> AikiChangePermissionAskedPayload {
    debug_log(|| {
        format!(
            "Transforming shell.permission_asked (mv) to change.permission_asked: {:?}",
            paths
        )
    });

    // Check if destination is a directory using pre-move filesystem state
    let dest_is_directory = if paths.len() >= 2 {
        let dest = &paths[paths.len() - 1];
        let dest_path = if std::path::Path::new(dest).is_absolute() {
            std::path::PathBuf::from(dest)
        } else {
            shell_event.cwd.join(dest)
        };
        dest_path.is_dir()
    } else {
        false
    };

    // Cache the detection result for the completed event
    let cache_key = (
        shell_event.session.external_id().to_string(),
        command.to_string(),
    );
    if let Ok(mut cache) = MOVE_DIR_CACHE.lock() {
        cache.insert(cache_key, dest_is_directory);
        // Limit cache size to prevent unbounded growth (keep last 100 entries)
        if cache.len() > 100 {
            // Remove oldest entries (HashMap doesn't preserve order, so just clear half)
            let keys_to_remove: Vec<_> = cache.keys().take(50).cloned().collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }
    }

    // Use the detection result to build the move operation
    let move_op = MoveOperation::from_move_paths_with_hint(paths, Some(dest_is_directory));

    AikiChangePermissionAskedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        operation: ChangeOperation::Move(move_op),
    }
}

/// Transform shell.completed to change.completed for mv commands
///
/// Called when shell command is detected as a move operation.
/// The paths vector should contain source path(s) followed by destination path.
///
/// Uses cached directory detection from the permission_asked phase if available.
/// Falls back to syntactic-only detection if no cache entry exists (e.g., if
/// permission_asked was never fired for this command).
fn transform_shell_move_to_change_completed(
    shell_event: crate::events::AikiShellCompletedPayload,
    paths: Vec<String>,
    command: &str,
) -> AikiChangeCompletedPayload {
    debug_log(|| {
        format!(
            "Transforming shell.completed (mv) to change.completed: {:?}",
            paths
        )
    });

    // Try to get cached directory detection from permission_asked phase
    let cache_key = (
        shell_event.session.external_id().to_string(),
        command.to_string(),
    );
    let cached_dest_is_dir = MOVE_DIR_CACHE
        .lock()
        .ok()
        .and_then(|mut cache| cache.remove(&cache_key));

    // Use cached detection if available, otherwise fall back to syntactic-only
    let move_op = match cached_dest_is_dir {
        Some(is_dir) => {
            debug_log(|| {
                format!(
                    "Using cached directory detection for mv: dest_is_directory={}",
                    is_dir
                )
            });
            MoveOperation::from_move_paths_with_hint(paths, Some(is_dir))
        }
        None => {
            debug_log(|| {
                "No cached directory detection for mv, using syntactic-only detection".to_string()
            });
            MoveOperation::from_move_paths(paths)
        }
    };

    AikiChangeCompletedPayload {
        session: shell_event.session,
        cwd: shell_event.cwd,
        timestamp: shell_event.timestamp,
        tool_name: "Bash".to_string(),
        success: shell_event.success,
        operation: ChangeOperation::Move(move_op),
    }
}

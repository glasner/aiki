use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiPrepareCommitMessageEvent};
use crate::handlers::HookResponse;
use crate::provenance::AgentType;
use chrono::Utc;
use std::env;
use std::path::PathBuf;

/// Detect which editor is calling the Git hook
#[derive(Debug, Clone, Copy)]
enum EditorContext {
    Claude,
    Cursor,
    Unknown,
}

fn detect_editor_context() -> EditorContext {
    // Detect from environment variables
    if env::var("CLAUDE_SESSION_ID").is_ok() {
        EditorContext::Claude
    } else if env::var("CURSOR_SESSION_ID").is_ok() {
        EditorContext::Cursor
    } else {
        EditorContext::Unknown
    }
}

/// Dispatch a PrepareCommitMessage event through the event bus
///
/// This is called from Git's prepare-commit-msg hook. It runs the flow
/// to modify the commit message (typically adding co-author attributions),
/// translates the response to editor-specific format, and exits.
pub fn run_prepare_commit_message() -> Result<()> {
    let cwd = env::current_dir()?;

    // Get commit message file path from environment (set by Git hook)
    let commit_msg_file = env::var("AIKI_COMMIT_MSG_FILE").ok().map(PathBuf::from);

    let event = AikiPrepareCommitMessageEvent {
        agent_type: AgentType::Claude, // Default agent for git hooks
        cwd,
        timestamp: Utc::now(),
        commit_msg_file,
    };

    // Get generic response from handler
    let response = event_bus::dispatch(AikiEvent::PrepareCommitMessage(event))?;

    // Detect editor context and translate
    let editor = detect_editor_context();
    let (json_output, exit_code) = translate_for_git_hook(response, editor);

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with code
    std::process::exit(exit_code);
}

/// Translate HookResponse for Git hooks based on editor context
///
/// Git hooks may be called from different editors, so we need to detect
/// which editor is active and format the response appropriately.
fn translate_for_git_hook(response: HookResponse, editor: EditorContext) -> (Option<String>, i32) {
    let exit_code = response
        .exit_code
        .unwrap_or(if response.success { 0 } else { 1 });

    match editor {
        EditorContext::Claude => {
            // Delegate to Claude Code's translator
            // Note: We can't call the private function, so we inline the logic
            translate_for_claude_code(response, exit_code)
        }
        EditorContext::Cursor => {
            // Delegate to Cursor's translator
            translate_for_cursor(response, exit_code)
        }
        EditorContext::Unknown => {
            // Generic stderr output for unknown editors
            if let Some(msg) = response.user_message {
                eprintln!("[aiki] {}", msg);
            }
            (None, exit_code)
        }
    }
}

/// Translate for Claude Code (Git hook context)
fn translate_for_claude_code(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    use serde_json::{json, Map};

    match exit_code {
        2 => {
            // Blocking error - for Git hooks, use continue: false
            let mut json = Map::new();
            json.insert("continue".to_string(), json!(false));

            if let Some(msg) = response.user_message {
                json.insert("stopReason".to_string(), json!(msg));
            }

            if let Some(agent_msg) = response.agent_message {
                json.insert("systemMessage".to_string(), json!(agent_msg));
            }

            (Some(serde_json::to_string(&json).unwrap()), 0)
        }
        0 => {
            // Success or non-blocking warnings
            let mut json = Map::new();

            let has_warning = response.user_message.as_ref().map_or(false, |msg| {
                msg.starts_with("⚠️") || msg.contains("warning") || msg.contains("failed")
            });

            if has_warning {
                if let Some(msg) = response.user_message {
                    json.insert("systemMessage".to_string(), json!(msg));
                }
            }

            if !response.metadata.is_empty() {
                let metadata: Vec<Vec<String>> = response
                    .metadata
                    .into_iter()
                    .map(|(k, v)| vec![k, v])
                    .collect();
                json.insert("metadata".to_string(), json!(metadata));
            }

            if json.is_empty() {
                (None, 0)
            } else {
                (Some(serde_json::to_string(&json).unwrap()), 0)
            }
        }
        _ => {
            if let Some(msg) = response.user_message {
                eprintln!("{}", msg);
            }
            (None, exit_code)
        }
    }
}

/// Translate for Cursor (Git hook context)
fn translate_for_cursor(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    use serde_json::{json, Map, Value};

    match exit_code {
        2 => {
            // Blocking error
            let mut json = Map::new();

            if let Some(msg) = response.user_message {
                json.insert("user_message".to_string(), json!(msg));
            }

            if let Some(agent_msg) = response.agent_message {
                json.insert("agent_message".to_string(), json!(agent_msg));
            }

            (Some(serde_json::to_string(&json).unwrap()), 2)
        }
        0 => {
            // Success or non-blocking
            let mut json = Map::new();

            if let Some(msg) = response.user_message {
                json.insert("user_message".to_string(), json!(msg));
            }

            if let Some(agent_msg) = response.agent_message {
                json.insert("agent_message".to_string(), json!(agent_msg));
            }

            if !response.metadata.is_empty() {
                let metadata: Map<String, Value> = response
                    .metadata
                    .into_iter()
                    .map(|(k, v)| (k, json!(v)))
                    .collect();
                json.insert("metadata".to_string(), json!(metadata));
            }

            if json.is_empty() {
                (None, 0)
            } else {
                (Some(serde_json::to_string(&json).unwrap()), 0)
            }
        }
        _ => {
            if let Some(msg) = response.user_message {
                eprintln!("{}", msg);
            }
            (None, exit_code)
        }
    }
}

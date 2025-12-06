use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiPrepareCommitMessageEvent};
use crate::handlers::{Decision, HookResponse, Message};
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
    let exit_code = match response.decision {
        Decision::Allow => 0,
        Decision::Block(_) => 2,
    };

    match editor {
        EditorContext::Claude => {
            // Delegate to Claude Code's translator
            // Note: We can't call the private function, so we inline the logic
            translate_for_claude_code(&response)
        }
        EditorContext::Cursor => {
            // Delegate to Cursor's translator
            translate_for_cursor(&response)
        }
        EditorContext::Unknown => {
            // Generic stderr output for unknown editors
            for msg in &response.messages {
                match msg {
                    Message::Info(s) => eprintln!("[aiki] ℹ️ {}", s),
                    Message::Warning(s) => eprintln!("[aiki] ⚠️ {}", s),
                    Message::Error(s) => eprintln!("[aiki] ❌ {}", s),
                }
            }
            (None, exit_code)
        }
    }
}

/// Translate for Claude Code (Git hook context)
fn translate_for_claude_code(response: &HookResponse) -> (Option<String>, i32) {
    use serde_json::{json, Map};

    let exit_code = match response.decision {
        Decision::Allow => 0,
        Decision::Block(_) => 2,
    };

    match exit_code {
        2 => {
            // Blocking error - for Git hooks, use continue: false
            let mut json = Map::new();
            json.insert("continue".to_string(), json!(false));

            // Extract error messages for stopReason
            let error_msgs: Vec<String> = response
                .messages
                .iter()
                .filter_map(|msg| match msg {
                    Message::Error(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();

            if !error_msgs.is_empty() {
                json.insert("stopReason".to_string(), json!(error_msgs.join("; ")));
            }

            // Extract info messages for systemMessage
            let info_msgs: Vec<String> = response
                .messages
                .iter()
                .filter_map(|msg| match msg {
                    Message::Info(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();

            if !info_msgs.is_empty() {
                json.insert("systemMessage".to_string(), json!(info_msgs.join("\n")));
            }

            (Some(serde_json::to_string(&json).unwrap()), 0)
        }
        0 => {
            // Success or non-blocking warnings
            let mut json = Map::new();

            // Check if there are any warnings or errors
            let has_warning = response
                .messages
                .iter()
                .any(|msg| matches!(msg, Message::Warning(_) | Message::Error(_)));

            if has_warning || !response.messages.is_empty() {
                // Combine all messages for systemMessage
                let all_msgs: Vec<String> = response
                    .messages
                    .iter()
                    .map(|msg| match msg {
                        Message::Info(s) => format!("ℹ️ {}", s),
                        Message::Warning(s) => format!("⚠️ {}", s),
                        Message::Error(s) => format!("❌ {}", s),
                    })
                    .collect();

                if !all_msgs.is_empty() {
                    json.insert("systemMessage".to_string(), json!(all_msgs.join("\n")));
                }
            }

            if json.is_empty() {
                (None, 0)
            } else {
                (Some(serde_json::to_string(&json).unwrap()), 0)
            }
        }
        _ => {
            for msg in &response.messages {
                match msg {
                    Message::Info(s) => eprintln!("ℹ️ {}", s),
                    Message::Warning(s) => eprintln!("⚠️ {}", s),
                    Message::Error(s) => eprintln!("❌ {}", s),
                }
            }
            (None, exit_code)
        }
    }
}

/// Translate for Cursor (Git hook context)
fn translate_for_cursor(response: &HookResponse) -> (Option<String>, i32) {
    use serde_json::{json, Map};

    let exit_code = match response.decision {
        Decision::Allow => 0,
        Decision::Block(_) => 2,
    };

    match exit_code {
        2 => {
            // Blocking error
            let mut json = Map::new();

            // Extract error messages for user_message
            let error_msgs: Vec<String> = response
                .messages
                .iter()
                .filter_map(|msg| match msg {
                    Message::Error(s) | Message::Warning(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();

            if !error_msgs.is_empty() {
                json.insert("user_message".to_string(), json!(error_msgs.join("; ")));
            }

            // Extract info messages for agent_message
            let info_msgs: Vec<String> = response
                .messages
                .iter()
                .filter_map(|msg| match msg {
                    Message::Info(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();

            if !info_msgs.is_empty() {
                json.insert("agent_message".to_string(), json!(info_msgs.join("\n")));
            }

            (Some(serde_json::to_string(&json).unwrap()), 2)
        }
        0 => {
            // Success or non-blocking
            let mut json = Map::new();

            // Combine all messages for user_message
            let all_msgs: Vec<String> = response
                .messages
                .iter()
                .map(|msg| match msg {
                    Message::Info(s) => format!("ℹ️ {}", s),
                    Message::Warning(s) => format!("⚠️ {}", s),
                    Message::Error(s) => format!("❌ {}", s),
                })
                .collect();

            if !all_msgs.is_empty() {
                json.insert("user_message".to_string(), json!(all_msgs.join("\n")));
            }

            if json.is_empty() {
                (None, 0)
            } else {
                (Some(serde_json::to_string(&json).unwrap()), 0)
            }
        }
        _ => {
            for msg in &response.messages {
                match msg {
                    Message::Info(s) => eprintln!("ℹ️ {}", s),
                    Message::Warning(s) => eprintln!("⚠️ {}", s),
                    Message::Error(s) => eprintln!("❌ {}", s),
                }
            }
            (None, exit_code)
        }
    }
}

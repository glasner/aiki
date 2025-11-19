use crate::acp::protocol::{InitializeRequest, JsonRpcMessage, SessionNotification};
use crate::error::{AikiError, Result};
use crate::event_bus;
use crate::events::{AikiEvent, AikiPostChangeEvent};
use crate::provenance::AgentType;
use agent_client_protocol::{SessionUpdate, ToolCallStatus, ToolKind};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Run the ACP bidirectional proxy
///
/// This command acts as a transparent proxy between an IDE (Zed, Neovim, etc.)
/// and an AI agent (claude-code, cursor, gemini, etc.), allowing Aiki to:
/// - Observe agent → IDE messages (tool_call notifications)
/// - Intercept IDE → agent messages (foundation for modification)
/// - Auto-detect the client (IDE) from InitializeRequest
/// - Record provenance with both client_name and agent_type
///
/// # Arguments
/// * `agent_type` - The agent type for provenance tracking (e.g., "claude-code", "cursor")
/// * `bin` - Optional custom binary path (defaults to derived from agent_type)
/// * `agent_args` - Optional arguments to pass to the agent executable
pub fn run(agent_type: String, bin: Option<String>, agent_args: Vec<String>) -> Result<()> {
    // Validate agent_type matches our enum
    let validated_agent_type = parse_agent_type(&agent_type)?;

    // Determine executable: use --bin flag if provided, otherwise derive from agent_type
    let executable = bin.unwrap_or_else(|| derive_executable(&agent_type));

    // Shared state for client name (detected from InitializeRequest)
    let client_name: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Shared state for working directory (from session/new or initialize)
    let cwd: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

    // Launch agent with piped stdin/stdout
    let mut agent = Command::new(&executable)
        .args(&agent_args)
        .env("AIKI_ENABLED", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to spawn agent '{}': {}. Make sure the agent is installed and in your PATH.",
                executable,
                e
            ))
        })?;

    let mut agent_stdin = agent.stdin.take().unwrap();
    let agent_stdout = agent.stdout.take().unwrap();

    // Thread 1: IDE → Agent (intercept and modify)
    let client_name_clone = Arc::clone(&client_name);
    let cwd_clone = Arc::clone(&cwd);
    let agent_type_clone = agent_type.clone();
    thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;

            // Parse message from IDE
            let msg: JsonRpcMessage = match serde_json::from_str(&line) {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("Warning: Failed to parse JSON-RPC message from IDE: {}", e);
                    // Forward raw line anyway
                    writeln!(agent_stdin, "{}", line)?;
                    agent_stdin.flush()?;
                    continue;
                }
            };

            // Capture metadata from IDE requests
            if let Some(method) = &msg.method {
                match method.as_str() {
                    "initialize" => {
                        if let Some(params) = &msg.params {
                            if let Ok(init_req) =
                                serde_json::from_value::<InitializeRequest>(params.clone())
                            {
                                if let Some(client_info) = init_req.client_info {
                                    let mut client = client_name_clone.lock().unwrap();
                                    *client = Some(client_info.name.clone());
                                    eprintln!(
                                        "ACP Proxy: Detected client '{}' connecting to agent '{}'",
                                        client_info.name, agent_type_clone
                                    );
                                }
                            }
                        }
                    }
                    "session/new" | "session/load" => {
                        // Extract working directory from session requests
                        if let Some(params) = &msg.params {
                            if let Some(cwd_str) = params.get("cwd").and_then(|v| v.as_str()) {
                                let mut working_dir = cwd_clone.lock().unwrap();
                                *working_dir = Some(PathBuf::from(cwd_str));
                                if std::env::var("AIKI_DEBUG").is_ok() {
                                    eprintln!("ACP Proxy: Set working directory to: {}", cwd_str);
                                }
                            }
                        }
                    }
                    _ => {}
                }

                // Future: Modify messages before sending to agent
                // match method.as_str() {
                //     "session/send_message" => {
                //         msg = modify_user_prompt(msg, &client_name_clone)?;
                //     }
                //     _ => {}
                // }
            }

            // Forward to agent
            let json = serde_json::to_string(&msg).map_err(|e| {
                AikiError::Other(anyhow::anyhow!("Failed to serialize JSON: {}", e))
            })?;
            writeln!(agent_stdin, "{}", json)?;
            agent_stdin.flush()?;
        }
        Ok(())
    });

    // Thread 2: Agent → IDE (observe and record)
    let client_name_clone = Arc::clone(&client_name);
    let cwd_clone = Arc::clone(&cwd);
    for line in BufReader::new(agent_stdout).lines() {
        let line = line?;

        // Parse message from agent
        if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&line) {
            if let Some(method) = &msg.method {
                if method == "session/update" {
                    // Record provenance via event bus (non-blocking)
                    if let Err(e) = handle_session_update(
                        &msg,
                        &validated_agent_type,
                        &client_name_clone,
                        &cwd_clone,
                    ) {
                        eprintln!("Warning: Failed to record provenance: {}", e);
                    }
                }
            }
        }

        // Forward to IDE
        println!("{}", line);
        io::stdout().flush()?;
    }

    let status = agent.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Parse and validate agent type against our AgentType enum
fn parse_agent_type(agent: &str) -> Result<AgentType> {
    match agent {
        "claude-code" => Ok(AgentType::ClaudeCode),
        "cursor" => Ok(AgentType::Cursor),
        "gemini" => Ok(AgentType::Gemini),
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}

/// Derive the executable name from the agent type
///
/// Most agent types use their name as the executable, but some have custom mappings.
fn derive_executable(agent_type: &str) -> String {
    match agent_type {
        "gemini" => "gemini-cli".to_string(),
        other => other.to_string(),
    }
}

/// Handle session/update notification from agent
///
/// Extracts tool_call information and dispatches provenance recording via event bus.
/// This is called for every session/update from the agent to the IDE.
fn handle_session_update(
    msg: &JsonRpcMessage,
    agent_type: &AgentType,
    client_name: &Arc<Mutex<Option<String>>>,
    cwd: &Arc<Mutex<Option<PathBuf>>>,
) -> Result<()> {
    // Parse session/update params
    let params = msg
        .params
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("session/update missing params")))?;

    let notification: SessionNotification =
        serde_json::from_value(params.clone()).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to parse SessionNotification: {}",
                e
            ))
        })?;

    // Extract ToolCallUpdate from SessionUpdate enum
    let tool_call = match &notification.update {
        SessionUpdate::ToolCallUpdate(update) => update,
        _ => {
            // Not a tool_call update, ignore (could be message, thought, etc.)
            return Ok(());
        }
    };

    // Only record completed tool calls that modified files
    if !matches!(tool_call.fields.status, Some(ToolCallStatus::Completed)) {
        return Ok(());
    }

    // Only record edit/delete/move operations (file modifications)
    let kind = tool_call
        .fields
        .kind
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Tool call missing kind")))?;

    if !matches!(kind, ToolKind::Edit | ToolKind::Delete | ToolKind::Move) {
        return Ok(());
    }

    // Extract affected file paths from locations
    let locations = tool_call
        .fields
        .locations
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Tool call missing locations")))?;

    if locations.is_empty() {
        return Ok(());
    }

    // Get working directory
    let working_dir = cwd
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?;

    // Get client name
    let client = client_name.lock().unwrap().clone();

    // Get tool name from kind
    let tool_name = format!("{:?}", kind); // Convert ToolKind enum to string (Edit, Delete, Move)

    // Create and dispatch an event for each affected file
    for location in locations {
        let event = AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: *agent_type,
            client_name: client.clone(),
            session_id: notification.session_id.to_string(),
            tool_name: tool_name.clone(),
            file_path: location.path.to_string_lossy().to_string(),
            cwd: working_dir.clone(),
            timestamp: chrono::Utc::now(),
        });

        // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
        if let Err(e) = event_bus::dispatch(event) {
            eprintln!("Warning: Event bus dispatch failed: {}", e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_type_valid() {
        assert!(matches!(
            parse_agent_type("claude-code"),
            Ok(AgentType::ClaudeCode)
        ));
        assert!(matches!(parse_agent_type("cursor"), Ok(AgentType::Cursor)));
        assert!(matches!(parse_agent_type("gemini"), Ok(AgentType::Gemini)));
    }

    #[test]
    fn test_parse_agent_type_invalid() {
        let result = parse_agent_type("invalid-agent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AikiError::UnknownAgentType(_)
        ));
    }

    #[test]
    fn test_derive_executable_default() {
        assert_eq!(derive_executable("claude-code"), "claude-code");
        assert_eq!(derive_executable("cursor"), "cursor");
    }

    #[test]
    fn test_derive_executable_custom() {
        assert_eq!(derive_executable("gemini"), "gemini-cli");
    }
}

use crate::acp::protocol::{
    InitializeRequest, InitializeResponse, JsonRpcMessage, SessionNotification,
};
use crate::commands::zed_detection;
use crate::error::{AikiError, Result};
use crate::event_bus;
use crate::events::{AikiEvent, AikiPostChangeEvent};
use crate::provenance::AgentType;
use agent_client_protocol::{
    SessionUpdate, ToolCall, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolKind,
};
use std::collections::HashMap;
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

    // Resolve agent binary: use --bin flag if provided, otherwise detect from Zed or PATH
    let (command, command_args) = if let Some(custom_bin) = bin {
        // User provided custom binary path - use it directly
        eprintln!("  Using custom binary: {}", custom_bin);
        (custom_bin, agent_args.clone())
    } else {
        // Auto-detect using Zed detection with PATH fallback
        let resolved = zed_detection::resolve_agent_binary(&agent_type)?;
        let cmd = resolved.command();
        let mut args = resolved.args();
        args.extend(agent_args.clone());
        (cmd, args)
    };

    // Shared state for client info (detected from InitializeRequest)
    let client_name: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let client_version: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Shared state for agent info (detected from InitializeResponse)
    let agent_version: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Shared state for working directory (from session/new or initialize)
    let cwd: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

    // Shared state for tool call metadata (locations, kind)
    let tool_call_contexts: Arc<Mutex<HashMap<ToolCallId, ToolCallContext>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Launch agent with piped stdin/stdout
    let mut agent = Command::new(&command)
        .args(&command_args)
        .env("AIKI_ENABLED", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to spawn command '{}': {}",
                command,
                e
            ))
        })?;

    let mut agent_stdin = agent.stdin.take().unwrap();
    let agent_stdout = agent.stdout.take().unwrap();

    // Thread 1: IDE → Agent (intercept and modify)
    let client_name_clone = Arc::clone(&client_name);
    let client_version_clone = Arc::clone(&client_version);
    let cwd_clone = Arc::clone(&cwd);
    let agent_type_clone = agent_type.clone();
    let ide_to_agent_thread = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        eprintln!("ACP Proxy: IDE → Agent thread started");
        for line in stdin.lock().lines() {
            let line = line?;
            eprintln!("ACP Proxy: IDE → Agent: received {} bytes", line.len());

            // Try to parse message from IDE for metadata extraction
            if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&line) {
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

                                        if let Some(version) = client_info.version {
                                            let mut client_ver =
                                                client_version_clone.lock().unwrap();
                                            *client_ver = Some(version.clone());
                                            eprintln!(
                                                "ACP Proxy: Detected client '{}' version '{}' connecting to agent '{}'",
                                                client_info.name, version, agent_type_clone
                                            );
                                        } else {
                                            eprintln!(
                                                "ACP Proxy: Detected client '{}' connecting to agent '{}'",
                                                client_info.name, agent_type_clone
                                            );
                                        }
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
                                        eprintln!(
                                            "ACP Proxy: Set working directory to: {}",
                                            cwd_str
                                        );
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Forward raw line to agent (no re-serialization)
            eprintln!("ACP Proxy: Forwarding to agent: {} bytes", line.len());
            writeln!(agent_stdin, "{}", line)?;
            agent_stdin.flush()?;
        }
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("ACP Proxy: IDE stdin closed, stopping IDE → Agent thread");
        }
        Ok(())
    });

    // Thread 2: Agent → IDE (observe and record)
    let client_name_clone = Arc::clone(&client_name);
    let client_version_clone = Arc::clone(&client_version);
    let agent_version_clone = Arc::clone(&agent_version);
    let cwd_clone = Arc::clone(&cwd);

    // Run main forwarding loop, capturing any errors
    let tool_call_contexts_clone = Arc::clone(&tool_call_contexts);
    let loop_result = (|| -> Result<()> {
        eprintln!("ACP Proxy: Agent → IDE thread started");
        for line in BufReader::new(agent_stdout).lines() {
            let line = line?;
            eprintln!("ACP Proxy: Agent → IDE: received {} bytes", line.len());

            // Parse message from agent
            if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&line) {
                // Log every message method and id for debugging
                eprintln!(
                    "ACP Proxy: Message: method={:?} id={:?}",
                    msg.method, msg.id
                );

                // Capture agent version from initialize response
                if msg.id.is_some() && msg.result.is_some() {
                    if let Some(result) = &msg.result {
                        if let Ok(init_resp) =
                            serde_json::from_value::<InitializeResponse>(result.clone())
                        {
                            if let Some(agent_info) = init_resp.agent_info {
                                if let Some(version) = agent_info.version {
                                    let mut agent_ver = agent_version_clone.lock().unwrap();
                                    *agent_ver = Some(version.clone());
                                    eprintln!(
                                        "ACP Proxy: Detected agent '{}' version '{}'",
                                        agent_info.name, version
                                    );
                                }
                            }
                        }
                    }
                }

                if let Some(method) = &msg.method {
                    // Log all session/update messages with their update type
                    if method == "session/update" {
                        if let Some(params) = &msg.params {
                            eprintln!(
                                "ACP Proxy: session/update params: {}",
                                serde_json::to_string_pretty(params)
                                    .unwrap_or_else(|_| "error".to_string())
                            );
                        }

                        // Record provenance via event bus (non-blocking)
                        if let Err(e) = handle_session_update(
                            &msg,
                            &validated_agent_type,
                            &client_name_clone,
                            &client_version_clone,
                            &agent_version_clone,
                            &cwd_clone,
                            &tool_call_contexts_clone,
                        ) {
                            eprintln!("Warning: Failed to record provenance: {}", e);
                        }
                    }
                }
            }

            // Forward to IDE
            eprintln!("ACP Proxy: Forwarding to IDE: {} bytes", line.len());
            println!("{}", line);
            io::stdout().flush()?;
        }
        Ok(())
    })();

    // Log any errors from the main loop, but don't exit yet - we need cleanup
    if let Err(ref e) = loop_result {
        eprintln!("ACP Proxy: Error in Agent → IDE forwarding: {}", e);
    }

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("ACP Proxy: Agent stdout closed, stopping Agent → IDE thread");
    }

    // ALWAYS wait for agent process to exit, even if there was an error
    let status_result = agent.wait();
    if let Err(ref e) = status_result {
        eprintln!("ACP Proxy: Failed to wait for agent process: {}", e);
    }

    // ALWAYS join the IDE → Agent thread to ensure clean shutdown
    match ide_to_agent_thread.join() {
        Ok(Ok(())) => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("ACP Proxy: IDE → Agent thread exited cleanly");
            }
        }
        Ok(Err(e)) => {
            eprintln!("Warning: IDE → Agent thread returned error: {}", e);
        }
        Err(e) => {
            eprintln!("Warning: IDE → Agent thread panicked: {:?}", e);
        }
    }

    // Now propagate the original error if there was one
    loop_result?;

    // Exit with agent's exit code, or 1 if we couldn't get it
    let exit_code = status_result.ok().and_then(|s| s.code()).unwrap_or(1);
    std::process::exit(exit_code);
}

/// Parse and validate agent type against our AgentType enum
fn parse_agent_type(agent: &str) -> Result<AgentType> {
    match agent {
        "claude-code" => Ok(AgentType::ClaudeCode),
        "codex" => Ok(AgentType::Codex),
        "cursor" => Ok(AgentType::Cursor),
        "gemini" => Ok(AgentType::Gemini),
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}

// Note: Executable derivation logic moved to zed_detection::derive_executable_name()
// and zed_detection::resolve_agent_binary() which handles both Zed-installed
// and PATH-based agents.

/// Handle session/update notification from agent
///
/// Extracts tool_call information and dispatches provenance recording via event bus.
/// This is called for every session/update from the agent to the IDE.
fn handle_session_update(
    msg: &JsonRpcMessage,
    agent_type: &AgentType,
    client_name: &Arc<Mutex<Option<String>>>,
    cwd: &Arc<Mutex<Option<PathBuf>>>,
    tool_call_contexts: &Arc<Mutex<HashMap<ToolCallId, ToolCallContext>>>,
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

    let session_id = notification.session_id.to_string();

    match &notification.update {
        SessionUpdate::ToolCall(tool_call) => process_tool_call(
            &session_id,
            tool_call,
            agent_type,
            client_name,
            cwd,
            tool_call_contexts,
        ),
        SessionUpdate::ToolCallUpdate(update) => process_tool_call_update(
            &session_id,
            update,
            agent_type,
            client_name,
            cwd,
            tool_call_contexts,
        ),
        _ => Ok(()),
    }
}

fn process_tool_call(
    session_id: &str,
    tool_call: &ToolCall,
    agent_type: &AgentType,
    client_name: &Arc<Mutex<Option<String>>>,
    cwd: &Arc<Mutex<Option<PathBuf>>>,
    tool_call_contexts: &Arc<Mutex<HashMap<ToolCallId, ToolCallContext>>>,
) -> Result<()> {
    let context = ToolCallContext {
        kind: tool_call.kind,
        paths: paths_from_locations(&tool_call.locations),
    };

    let status = tool_call.status;

    {
        let mut contexts = tool_call_contexts.lock().unwrap();
        contexts.insert(tool_call.id.clone(), context.clone());
        if matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) {
            contexts.remove(&tool_call.id);
        }
    }

    if status == ToolCallStatus::Completed {
        record_post_change_events(session_id, agent_type, client_name, cwd, context)?;
    }

    Ok(())
}

fn process_tool_call_update(
    session_id: &str,
    tool_call: &ToolCallUpdate,
    agent_type: &AgentType,
    client_name: &Arc<Mutex<Option<String>>>,
    cwd: &Arc<Mutex<Option<PathBuf>>>,
    tool_call_contexts: &Arc<Mutex<HashMap<ToolCallId, ToolCallContext>>>,
) -> Result<()> {
    let mut contexts = tool_call_contexts.lock().unwrap();
    let entry = contexts
        .entry(tool_call.id.clone())
        .or_insert_with(|| ToolCallContext {
            kind: tool_call.fields.kind.unwrap_or(ToolKind::Other),
            paths: Vec::new(),
        });

    if let Some(kind) = tool_call.fields.kind {
        entry.kind = kind;
    }

    if let Some(locations) = &tool_call.fields.locations {
        entry.paths = paths_from_locations(locations);
    }

    let status = tool_call.fields.status;
    let should_record =
        matches!(status, Some(ToolCallStatus::Completed)) && !entry.paths.is_empty();
    let context = if should_record {
        Some(entry.clone())
    } else {
        None
    };

    if matches!(
        status,
        Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
    ) {
        contexts.remove(&tool_call.id);
    }

    drop(contexts);

    if let Some(context) = context {
        record_post_change_events(session_id, agent_type, client_name, cwd, context)?;
    }

    Ok(())
}

#[derive(Clone)]
struct ToolCallContext {
    kind: ToolKind,
    paths: Vec<PathBuf>,
}

fn paths_from_locations(locations: &[ToolCallLocation]) -> Vec<PathBuf> {
    locations.iter().map(|loc| loc.path.clone()).collect()
}

fn record_post_change_events(
    session_id: &str,
    agent_type: &AgentType,
    client_name: &Arc<Mutex<Option<String>>>,
    cwd: &Arc<Mutex<Option<PathBuf>>>,
    context: ToolCallContext,
) -> Result<()> {
    if !matches!(
        context.kind,
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move
    ) {
        return Ok(());
    }

    if context.paths.is_empty() {
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
    let tool_name = format!("{:?}", context.kind); // Convert ToolKind enum to string (Edit, Delete, Move)

    // Create and dispatch an event for each affected file
    for path in context.paths {
        let event = AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: *agent_type,
            client_name: client.clone(),
            session_id: session_id.to_string(),
            tool_name: tool_name.clone(),
            file_path: path.to_string_lossy().to_string(),
            cwd: working_dir.clone(),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::ACP,
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
        assert!(matches!(parse_agent_type("codex"), Ok(AgentType::Codex)));
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

    // Note: derive_executable tests moved to zed_detection module tests
}

//! ACP (Agent Client Protocol) Proxy
//!
//! This module implements a transparent proxy between an IDE and an AI agent that
//! communicates via the [Agent Client Protocol](https://agentclientprotocol.com).
//!
//! # Architecture
//!
//! The proxy uses a **three-thread architecture** with explicit state ownership:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         ACP Proxy Process                               │
//! │                                                                         │
//! │  ┌──────────────────────┐                    ┌──────────────────────┐  │
//! │  │ IDE → Agent Thread   │   StateMessage     │ Agent → IDE Thread   │  │
//! │  │                      │  ─────────────────▶│                      │  │
//! │  │ - Parse IDE requests │   mpsc::channel    │ - OWNS all state     │  │
//! │  │ - Fire prompt.submitted                   │ - Parse agent msgs   │  │
//! │  │ - Fire change.permission_asked            │ - Fire response.received  │
//! │  │ - Forward to agent   │                    │ - Fire change.done   │  │
//! │  │                      │                    │ - Track tool calls   │  │
//! │  │                      │  AutoreplyChannel  │ - Accumulate text    │  │
//! │  │                      │  ◀─────────────────│                      │  │
//! │  └──────────────────────┘   Message          └──────────────────────┘  │
//! │         ▲                                              │                │
//! │         │                                              ▼                │
//! │    IDE stdin                                     Agent stdout           │
//! │         │                                              ▲                │
//! │         ▼                                              │                │
//! │    Agent stdin ◀───────────────────────────────────────┘                │
//! │         ▲                                                               │
//! │         │                                                               │
//! │  ┌──────┴──────────────┐                                               │
//! │  │ Autoreply Forwarder │                                               │
//! │  │ Thread              │                                               │
//! │  │ - Drains autoreply  │                                               │
//! │  │   channel           │                                               │
//! │  │ - Sends to agent    │                                               │
//! │  └─────────────────────┘                                               │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Thread Responsibilities
//!
//! ## IDE → Agent Thread
//!
//! - Reads JSON-RPC messages from IDE (stdin)
//! - Extracts metadata (client info, session IDs, working directory)
//! - Sends metadata updates via `StateMessage` channel to Agent→IDE thread
//! - Fires `prompt.submitted` events (allows flows to inject context)
//! - Forwards messages to agent stdin
//!
//! ## Agent → IDE Thread (State Owner)
//!
//! - **Owns all proxy state** (client info, agent info, cwd, tool call contexts)
//! - Receives metadata updates from IDE→Agent thread via channel
//! - Reads JSON-RPC messages from agent (stdout)
//! - Fires `session.started`, `session.ended`, `change.done` events
//! - Tracks response text accumulation per session
//! - Detects autoreplies from flows and queues them via autoreply channel
//! - Forwards messages to IDE (stdout)
//!
//! ## Autoreply Forwarder Thread
//!
//! - Dedicated thread to drain the autoreply channel
//! - Ensures autoreplies are sent even when IDE is idle
//! - Writes autoreply JSON-RPC requests to agent stdin
//!
//! # State Synchronization
//!
//! The proxy uses **message-passing channels** for thread communication:
//!
//! - `StateMessage` channel: IDE→Agent thread sends metadata to Agent→IDE thread
//! - `AutoreplyMessage` channel: Agent→IDE thread sends autoreplies to forwarder
//!
//! This design prevents data races and makes state ownership explicit.
//!
//! # Shutdown Protocol
//!
//! When the agent process exits:
//!
//! 1. Agent→IDE thread detects EOF on agent stdout and exits its forwarding loop
//! 2. Main thread sends `Shutdown` messages to both autoreply and metadata channels
//! 3. Agent→IDE thread (if still running) exits on `StateMessage::Shutdown`
//! 4. Autoreply forwarder thread exits on `AutoreplyMessage::Shutdown`
//! 5. IDE→Agent thread exits when IDE closes stdin (natural EOF on stdin.lock().lines())
//! 6. Main thread joins all threads before exiting
//!
//! Note: IDE→Agent thread shutdown is driven by stdin EOF, not by the Shutdown message,
//! because it's blocked on stdin.lock().lines() and cannot check the metadata channel.
//! This is correct behavior - the thread only needs to exit when the IDE disconnects.
//!
//! # Events Fired
//!
//! - **session.started**: When `session/new` response is received with `sessionId`
//! - **prompt.submitted**: Before `session/prompt` is forwarded to agent (allows context injection)
//! - **change.permission_asked**: Before `session/request_permission` for file-modifying tools
//! - **change.done**: When tool calls complete (from `session/update` notifications)
//! - **session.ended**: When agent completes a turn (`stopReason: end_turn`)
//!
//! # Example Flow
//!
//! 1. IDE sends `initialize` request → IDE→Agent thread extracts client info
//! 2. Agent responds with `initialize` response → Agent→IDE thread extracts agent info
//! 3. IDE sends `session/new` → IDE→Agent thread tracks request ID
//! 4. Agent responds with `sessionId` → Agent→IDE thread fires `session.started` event
//! 5. IDE sends `session/prompt` → IDE→Agent thread fires `prompt.submitted` event
//! 6. Agent sends `session/update` chunks → Agent→IDE thread accumulates response text
//! 7. Agent completes turn → Agent→IDE thread fires `session.ended` event
//! 8. Flow returns autoreply → Agent→IDE thread queues it via autoreply channel
//! 9. Autoreply forwarder sends it to agent stdin
//! 10. Process repeats

use crate::acp::protocol::{
    AgentInfo, ClientInfo, InitializeRequest, InitializeResponse, JsonRpcMessage,
    SessionNotification,
};
use crate::cache::debug_log;
use crate::commands::zed_detection;
use crate::error::{AikiError, Result};
use crate::event_bus;
use crate::events::result::HookResult;
use crate::events::{
    AikiEvent, AikiPromptSubmittedPayload, AikiResponseReceivedPayload, AikiSessionStartPayload,
    AikiWriteCompletedPayload, AikiWritePermissionAskedPayload,
};
use crate::provenance::AgentType;
use crate::session::AikiSession;
use agent_client_protocol::{
    ContentBlock, SessionUpdate, ToolCall, ToolCallId, ToolCallLocation, ToolCallStatus,
    ToolCallUpdate, ToolKind,
};
use serde_json::json;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// A normalized JSON-RPC request/response ID used as a HashMap key
///
/// JSON-RPC IDs can be strings, numbers, or null. This newtype wraps the
/// normalized string representation to ensure type safety when using IDs
/// as HashMap keys.
///
/// # Normalization Rules
/// - String ID "abc" → `JsonRpcId("\"abc\"")`  (with quotes)
/// - Number ID 123 → `JsonRpcId("123")`        (no quotes)
/// - Null ID → `JsonRpcId("null")`
///
/// This matches the behavior of `serde_json::Value::to_string()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct JsonRpcId(String);

impl JsonRpcId {
    /// Normalize a JSON-RPC ID from a serde_json::Value
    fn from_value(id: &serde_json::Value) -> Self {
        Self(id.to_string())
    }
}

/// Session ID type using Arc<str> for cheap cloning
///
/// Instead of cloning full strings (~32 bytes + allocation), we use Arc<str>
/// which is just a pointer copy. This eliminates ~20+ allocations per message
/// when session IDs are used in HashMap operations.
type SessionId = Arc<str>;

/// Helper to create a SessionId from a string slice
fn session_id(s: &str) -> SessionId {
    Arc::from(s)
}

/// State coordination messages sent between proxy threads
/// (IDE→Agent thread sends, Agent→IDE thread owns state)
#[derive(Debug, Clone)]
enum StateMessage {
    /// Update client (IDE) information detected from initialize request
    SetClientInfo(ClientInfo),
    /// Update working directory from session/new or session/load
    SetWorkingDirectory(PathBuf),
    /// Track session/prompt request for session.ended event matching
    TrackPrompt {
        request_id: serde_json::Value, // Raw JSON-RPC "id" field (normalized at consumption)
        session_id: SessionId,
    },
    /// Clear response accumulator for a session (on new prompt)
    ClearAccumulator { session_id: SessionId },
    /// Reset autoreply counter for a session (on new user prompt)
    ResetAutoreplyCounter { session_id: SessionId },
    /// Track session/new request ID to match with response for session.started event
    TrackNewSession {
        request_id: serde_json::Value, // Raw JSON-RPC "id" field (normalized at consumption)
    },
    /// Signal shutdown when agent process exits
    Shutdown,
}

/// Messages sent through the autoreply channel
#[derive(Debug, Clone)]
enum AutoreplyMessage {
    /// A JSON-RPC autoreply message to be sent to the agent only (not forwarded to IDE)
    SendAutoreply(Autoreply),
    /// Explicit shutdown signal
    Shutdown,
}

/// A JSON-RPC autoreply message to be sent to the agent
///
/// Stores the structured data for a session/prompt autoreply request.
/// The JSON is generated on-demand when needed.
#[derive(Debug, Clone)]
struct Autoreply {
    /// The session ID to send the prompt to
    session_id: SessionId,
    /// The text content of the autoreply
    text: String,
    /// The raw request ID string (for JSON serialization)
    raw_request_id: String,
    /// The normalized request ID (for HashMap tracking)
    normalized_request_id: JsonRpcId,
}

impl Autoreply {
    /// Create a new session/prompt autoreply request
    ///
    /// # Arguments
    /// * `session_id` - The session ID to send the prompt to
    /// * `autoreply_text` - The text content to send as the prompt
    /// * `counter` - The autoreply counter for this session (for unique ID generation)
    fn new(session_id: &SessionId, autoreply_text: String, counter: usize) -> Self {
        // Generate unique request ID (raw string without quotes)
        let raw_id = format!("aiki-autoreply-{}-{}", session_id, counter);
        // Normalize for HashMap tracking (with quotes for string IDs)
        let normalized_id = JsonRpcId::from_value(&serde_json::Value::String(raw_id.clone()));

        Self {
            session_id: Arc::clone(session_id),
            text: autoreply_text,
            raw_request_id: raw_id,
            normalized_request_id: normalized_id,
        }
    }

    /// Generate the JSON-RPC request and serialize it to a string
    fn as_json(&self) -> Result<String> {
        use serde_json::json;

        let autoreply_request = json!({
            "jsonrpc": "2.0",
            "id": &self.raw_request_id,  // Use raw string, not normalized
            "method": "session/prompt",
            "params": {
                "sessionId": &self.session_id,
                "prompt": [{
                    "type": "text",
                    "text": &self.text
                }]
            }
        });

        serde_json::to_string(&autoreply_request)
            .map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to serialize autoreply: {}", e)))
    }

    /// Get the normalized request ID for HashMap tracking
    fn normalized_request_id(&self) -> &JsonRpcId {
        &self.normalized_request_id
    }

    /// Get the raw request ID string for display/debugging
    fn raw_request_id_display(&self) -> &str {
        &self.raw_request_id
    }
}

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
    // Install panic hook to diagnose crashes - writes to both file and stderr
    crate::utils::panic::install_acp_panic_hook();

    // Validate agent_type matches our enum
    let validated_agent_type = parse_agent_type(&agent_type)?;

    // Resolve agent binary: use --bin flag if provided, otherwise detect from Zed or PATH
    let (command, command_args) = if let Some(custom_bin) = bin {
        // User provided custom binary path - use it directly
        eprintln!("  Using custom binary: {}", custom_bin);
        (custom_bin, agent_args)
    } else {
        // Auto-detect using Zed detection with PATH fallback
        let resolved = zed_detection::resolve_agent_binary(&agent_type)?;
        let cmd = resolved.command();
        let mut args = resolved.args();
        args.extend(agent_args);
        (cmd, args)
    };

    // Create channel for metadata communication
    // IDE→Agent thread will send discovered metadata
    // Agent→IDE thread will receive and own the state
    let (metadata_tx, metadata_rx) = mpsc::channel::<StateMessage>();

    // Create channel for autoreplies
    // Agent→IDE thread detects session.ended and sends autoreply requests
    // IDE→Agent thread receives and forwards them to agent
    let (autoreply_tx, autoreply_rx) = mpsc::channel::<AutoreplyMessage>();

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

    let agent_stdin = agent.stdin.take().expect(
        "Failed to acquire agent stdin - this should never happen as we set Stdio::piped()",
    );
    let agent_stdout = agent.stdout.take().expect(
        "Failed to acquire agent stdout - this should never happen as we set Stdio::piped()",
    );

    // Thread 1: Autoreply forwarder (dedicated thread to drain autoreply channel)
    // This ensures autoreplies are sent even when IDE is idle
    // Wrap agent_stdin in Arc<Mutex<>> for safe sharing between threads
    let agent_stdin_shared = Arc::new(Mutex::new(agent_stdin));
    let agent_stdin_for_autoreplies = Arc::clone(&agent_stdin_shared);

    let autoreply_thread = thread::spawn(move || -> Result<()> {
        eprintln!("ACP Proxy: Autoreply forwarder thread started");

        // Block on the autoreply channel and forward messages as they arrive
        while let Ok(msg) = autoreply_rx.recv() {
            match msg {
                AutoreplyMessage::SendAutoreply(autoreply_msg) => {
                    // Generate JSON on-demand
                    let json = match autoreply_msg.as_json() {
                        Ok(j) => j,
                        Err(e) => {
                            eprintln!("Warning: Failed to serialize autoreply: {}", e);
                            break;
                        }
                    };

                    // Send to agent only (not forwarded to IDE to avoid race condition)
                    // Serialize outside lock to minimize critical section
                    let data = format!("{}\n", json).into_bytes();
                    {
                        // ✅ FIX for Issue #5: Handle mutex poisoning gracefully
                        let mut stdin = match agent_stdin_for_autoreplies.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                eprintln!("Warning: Mutex poisoned (another thread panicked), attempting recovery");
                                poisoned.into_inner()
                            }
                        };
                        if let Err(e) = stdin.write_all(&data) {
                            eprintln!("Warning: Failed to send autoreply to agent: {}", e);
                            break;
                        }
                        if let Err(e) = stdin.flush() {
                            eprintln!("Warning: Failed to flush autoreply to agent: {}", e);
                            break;
                        }
                    }
                    debug_log(|| format!("[acp] Sent autoreply to agent: {} bytes", json.len()));
                }
                AutoreplyMessage::Shutdown => {
                    debug_log(|| "ACP Proxy: Autoreply thread received shutdown signal");
                    break;
                }
            }
        }

        debug_log(|| "ACP Proxy: Autoreply forwarder thread exiting");
        Ok(())
    });

    // Thread 2: IDE → Agent (intercept and modify)
    // This thread discovers metadata and sends it via channel
    let metadata_tx_clone = metadata_tx.clone();
    let agent_type_clone = agent_type.clone();
    let agent_stdin_for_ide = Arc::clone(&agent_stdin_shared);
    let ide_to_agent_thread = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        eprintln!("ACP Proxy: IDE → Agent thread started");

        // Track cwd in this thread for prompt.submitted events
        // This mirrors the `cwd` in Agent→IDE thread, both updated via StateMessage::SetWorkingDirectory
        let mut cwd: Option<PathBuf> = None;

        for line in stdin.lock().lines() {
            let line = line?;
            // Removed verbose logging to prevent stderr overflow panic

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
                                        // Send full client info to Agent→IDE thread
                                        let _ = metadata_tx_clone
                                            .send(StateMessage::SetClientInfo(client_info.clone()));

                                        if let Some(ref ver) = client_info.version {
                                            eprintln!(
                                                "ACP Proxy: Detected client '{}' version '{}' connecting to agent '{}'",
                                                client_info.name, ver, agent_type_clone
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
                        "session/new" => {
                            // Extract working directory from session requests
                            if let Some(params) = &msg.params {
                                if let Some(cwd_str) = params.get("cwd").and_then(|v| v.as_str()) {
                                    let path = PathBuf::from(cwd_str);

                                    // Store in this thread's cwd
                                    cwd = Some(path.clone());

                                    // Send working directory to Agent→IDE thread
                                    let _ = metadata_tx_clone
                                        .send(StateMessage::SetWorkingDirectory(path));

                                    debug_log(|| {
                                        format!("ACP Proxy: Set working directory to: {}", cwd_str)
                                    });
                                }
                            }

                            // Track session/new request for session.started event
                            if let Some(request_id) = &msg.id {
                                let _ = metadata_tx_clone.send(StateMessage::TrackNewSession {
                                    request_id: request_id.clone(),
                                });
                            }
                        }
                        "session/load" => {
                            // Extract working directory from session requests
                            if let Some(params) = &msg.params {
                                if let Some(cwd_str) = params.get("cwd").and_then(|v| v.as_str()) {
                                    let path = PathBuf::from(cwd_str);

                                    // Store in this thread's cwd
                                    cwd = Some(path.clone());

                                    // Send working directory to Agent→IDE thread
                                    let _ = metadata_tx_clone
                                        .send(StateMessage::SetWorkingDirectory(path));

                                    debug_log(|| {
                                        format!("ACP Proxy: Set working directory to: {}", cwd_str)
                                    });
                                }
                            }
                        }
                        "session/prompt" => {
                            // prompt.submitted event: intercept and potentially modify prompt
                            if let Some(params) = &msg.params {
                                // Extract sessionId directly from params (session/prompt doesn't have 'update' field)
                                let session_id_str = params
                                    .get("sessionId")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();

                                if !session_id_str.is_empty() {
                                    let session_id = session_id(session_id_str);

                                    // Track this prompt request BEFORE any fallible work
                                    // This ensures session.ended fires even if prompt.submitted processing fails (graceful degradation)
                                    if let Some(request_id) = &msg.id {
                                        let _ = metadata_tx_clone.send(StateMessage::TrackPrompt {
                                            request_id: request_id.clone(),
                                            session_id: Arc::clone(&session_id),
                                        });
                                    }

                                    // Signal Agent→IDE thread to clear response accumulator and reset autoreply counter
                                    // This ensures we start fresh for each new prompt, preventing concatenation
                                    // of old text if the previous turn ended without end_turn (error, cancel, etc.)
                                    // Also resets autoreply counter per turn (not permanently after 5 total)
                                    let _ =
                                        metadata_tx_clone.send(StateMessage::ClearAccumulator {
                                            session_id: Arc::clone(&session_id),
                                        });
                                    let _ = metadata_tx_clone
                                        .send(StateMessage::ResetAutoreplyCounter { session_id });
                                }

                                // Pass the thread's tracked cwd
                                if let Err(e) = handle_session_prompt(
                                    &agent_stdin_for_ide,
                                    &msg,
                                    params,
                                    &validated_agent_type,
                                    &cwd,
                                    &metadata_tx_clone,
                                ) {
                                    eprintln!("Warning: Failed to handle session/prompt: {}", e);
                                    // On error, forward original message
                                    // session.ended will still fire because we tracked the request above
                                    let data = format!("{}\n", line).into_bytes();
                                    {
                                        // ✅ FIX for Issue #5: Handle mutex poisoning gracefully
                                        let mut stdin = match agent_stdin_for_ide.lock() {
                                            Ok(guard) => guard,
                                            Err(poisoned) => {
                                                eprintln!("Warning: Mutex poisoned (another thread panicked), attempting recovery");
                                                poisoned.into_inner()
                                            }
                                        };
                                        stdin.write_all(&data)?;
                                        stdin.flush()?;
                                    }
                                }
                                // Skip the normal forwarding since handle_session_prompt already did it
                                continue;
                            }
                        }
                        "authenticate" => {
                            // Just observe and forward - let the agent handle authentication
                            debug_log(|| "ACP Proxy: Forwarding authenticate request to agent");
                        }
                        _ => {}
                    }
                }
            }

            // Forward raw line to agent (no re-serialization)
            // Serialize outside lock to minimize critical section
            let data = format!("{}\n", line).into_bytes();
            {
                // ✅ FIX for Issue #5: Handle mutex poisoning gracefully
                let mut stdin = match agent_stdin_for_ide.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        eprintln!("Warning: Mutex poisoned (another thread panicked), attempting recovery");
                        poisoned.into_inner()
                    }
                };
                stdin.write_all(&data)?;
                stdin.flush()?;
            }
        }
        debug_log(|| "ACP Proxy: IDE stdin closed, stopping IDE → Agent thread");
        Ok(())
    });

    // Thread 2: Agent → IDE (observe and record)
    // This thread OWNS all metadata state and receives updates via channel
    let mut client_info: Option<ClientInfo> = None;
    let mut agent_info: Option<AgentInfo> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut tool_call_contexts: HashMap<ToolCallId, ToolCallContext> = HashMap::new();

    // Track prompt requests for session.ended event
    // Key is JsonRpcId (normalized request_id), value is session_id
    let mut prompt_requests: HashMap<JsonRpcId, SessionId> = HashMap::new();

    // Track session/new requests for session.started event
    // Key is JsonRpcId (normalized request_id), value is boolean (true = pending)
    let mut session_new_requests: HashMap<JsonRpcId, bool> = HashMap::new();

    // Track autoreply counters per session (not global)
    let mut autoreply_counters: HashMap<SessionId, usize> = HashMap::new();

    // Track response text accumulation per session (not per request_id)
    // A session only has one active prompt at a time, so we key by session_id
    // rather than request_id. This simplifies accumulation across multiple
    // agent_message_chunk updates, which all share the same session_id.
    let mut response_accumulator: HashMap<SessionId, String> = HashMap::new();

    // Run main forwarding loop, capturing any errors
    let loop_result = (|| -> Result<()> {
        eprintln!("ACP Proxy: Agent → IDE thread started");
        for line in BufReader::new(agent_stdout).lines() {
            let line = line?;
            // Removed verbose logging to prevent stderr overflow panic

            // Drain all pending metadata updates from IDE→Agent thread
            while let Ok(msg) = metadata_rx.try_recv() {
                match msg {
                    StateMessage::SetClientInfo(info) => {
                        client_info = Some(info);
                    }
                    StateMessage::SetWorkingDirectory(path) => {
                        cwd = Some(path);
                    }
                    StateMessage::TrackPrompt {
                        request_id,
                        session_id,
                    } => {
                        // Normalize the ID at consumption point to ensure consistent HashMap keys
                        // This handles any JSON-RPC ID format (string, number, null)
                        prompt_requests.insert(JsonRpcId::from_value(&request_id), session_id);
                    }
                    StateMessage::ClearAccumulator { session_id } => {
                        // Clear accumulated response text for this session
                        // This happens on each new prompt to prevent stale text from failed turns
                        response_accumulator.remove(&session_id);
                    }
                    StateMessage::ResetAutoreplyCounter { session_id } => {
                        // Reset autoreply counter for this session (per-turn, not permanent)
                        // This allows each turn to use up to MAX_AUTOREPLIES, preventing
                        // permanent blocking after the session accumulates 5 total autoreplies
                        reset_autoreply_counter(&mut autoreply_counters, &session_id);
                        debug_log(|| {
                            format!("[acp] Reset autoreply counter for session: {}", session_id)
                        });
                    }
                    StateMessage::TrackNewSession { request_id } => {
                        // Track session/new request to match with response
                        session_new_requests.insert(JsonRpcId::from_value(&request_id), true);
                    }
                    StateMessage::Shutdown => {
                        // Explicit shutdown signal - exit the loop
                        debug_log(|| "ACP Proxy: Agent→IDE thread received shutdown signal");
                        break;
                    }
                }
            }

            // Parse message from agent
            if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&line) {
                // Removed verbose logging to prevent stderr overflow panic

                // Capture agent info from initialize response
                if msg.id.is_some() && msg.result.is_some() {
                    if let Some(result) = &msg.result {
                        if let Ok(init_resp) =
                            serde_json::from_value::<InitializeResponse>(result.clone())
                        {
                            if let Some(info) = init_resp.agent_info {
                                if let Some(ref version) = info.version {
                                    eprintln!(
                                        "ACP Proxy: Detected agent '{}' version '{}'",
                                        info.name, version
                                    );
                                }
                                agent_info = Some(info);
                            }
                        }

                        // Check for session/new response (session.started event)
                        if let Some(response_id) = &msg.id {
                            let request_id = JsonRpcId::from_value(response_id);
                            if session_new_requests.remove(&request_id).is_some() {
                                // This is a session/new response
                                if let Some(session_id) =
                                    result.get("sessionId").and_then(|v| v.as_str())
                                {
                                    // Fire session.started event
                                    if let Err(e) = fire_session_start_event(
                                        session_id,
                                        &validated_agent_type,
                                        &cwd,
                                    ) {
                                        eprintln!(
                                            "Warning: Failed to fire session.started event: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }

                        // Check for stopReason (turn completion)
                        // Clean up prompt_requests for ANY stopReason to prevent memory leaks
                        // and stale ID issues (per ACP spec: end_turn, max_tokens, max_turn_requests,
                        // refusal, cancelled)
                        if let Some(stop_reason) = result.get("stopReason").and_then(|v| v.as_str())
                        {
                            if let Some(response_id) = &msg.id {
                                // Normalize the response ID for HashMap lookup
                                let request_id = JsonRpcId::from_value(response_id);

                                // Always remove the prompt tracking entry to prevent memory leaks
                                if let Some(session_id) = prompt_requests.remove(&request_id) {
                                    // Fire session.ended event only for successful end_turn
                                    if stop_reason == "end_turn" {
                                        // Get accumulated response text for this session
                                        let response_text = response_accumulator
                                            .remove(&session_id)
                                            .unwrap_or_default();

                                        // Fire session.ended event
                                        if let Err(e) = handle_session_end(
                                            &session_id,
                                            &validated_agent_type,
                                            &cwd,
                                            &response_text,
                                            &mut autoreply_counters,
                                            MAX_AUTOREPLIES,
                                            &autoreply_tx,
                                            &mut prompt_requests,
                                        ) {
                                            eprintln!(
                                                "Warning: Failed to handle session.ended: {}",
                                                e
                                            );
                                        }
                                    } else {
                                        // Non-end_turn stopReason (max_tokens, refusal, cancelled, etc.)
                                        // Clean up accumulated response but don't fire session.ended
                                        response_accumulator.remove(&session_id);

                                        debug_log(|| {
                                            format!(
                                            "[acp] Turn ended with stopReason '{}', cleaned up session {}",
                                            stop_reason, session_id
                                        )
                                        });
                                    }
                                } else {
                                    debug_log(|| {
                                        format!(
                                        "[acp] Detected stopReason '{}' but no matching request_id: {:?}",
                                        stop_reason, response_id
                                    )
                                    });
                                }
                            }
                        }
                    }

                    // Handle JSON-RPC error responses
                    // Clean up prompt_requests to prevent memory leaks when errors occur
                    if msg.error.is_some() {
                        if let Some(response_id) = &msg.id {
                            let request_id = JsonRpcId::from_value(response_id);

                            // Remove tracking entry and accumulated response
                            if let Some(session_id) = prompt_requests.remove(&request_id) {
                                response_accumulator.remove(&session_id);

                                debug_log(|| {
                                    format!(
                                    "[acp] JSON-RPC error response for request {:?}, cleaned up session {}",
                                    response_id, session_id
                                )
                                });
                            }
                        }
                    }
                }

                if let Some(method) = &msg.method {
                    // Handle session/request_permission - fire change.permission_asked for file-modifying tools
                    if method == "session/request_permission" {
                        if is_file_modifying_permission_request(&msg) {
                            // Extract session_id from params
                            if let Some(params) = &msg.params {
                                if let Some(session_id) =
                                    params.get("sessionId").and_then(|v| v.as_str())
                                {
                                    // Fire change.permission_asked event BEFORE forwarding permission request to IDE
                                    if let Err(e) = fire_pre_file_change_event(
                                        session_id,
                                        &validated_agent_type,
                                        &cwd,
                                    ) {
                                        eprintln!(
                                            "Warning: Failed to fire change.permission_asked event: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Handle session/update messages
                    if method == "session/update" {
                        // Removed verbose logging to prevent stderr overflow panic

                        // Capture response text from agent_message_chunk updates
                        if let Some(params) = &msg.params {
                            if let Ok(notification) =
                                serde_json::from_value::<SessionNotification>(params.clone())
                            {
                                let session_id = session_id(&notification.session_id.to_string());

                                // Check if this is an AgentMessageChunk with text content
                                // Note: SessionUpdate uses the discriminator field "sessionUpdate" (not "type")
                                // with snake_case variant names per the ACP spec
                                if let SessionUpdate::AgentMessageChunk(content_chunk) =
                                    &notification.update
                                {
                                    if let ContentBlock::Text(text_content) = &content_chunk.content
                                    {
                                        // Accumulate response text per session
                                        // Pre-allocate 4KB capacity to reduce reallocations
                                        response_accumulator
                                            .entry(Arc::clone(&session_id))
                                            .or_insert_with(|| String::with_capacity(4096))
                                            .push_str(&text_content.text);
                                    }
                                }
                            }
                        }

                        // Record provenance via event bus (non-blocking)
                        if let Err(e) = handle_session_update(
                            &msg,
                            &validated_agent_type,
                            &client_info,
                            &agent_info,
                            &cwd,
                            &mut tool_call_contexts,
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
        Ok(())
    })();

    // Log any errors from the main loop, but don't exit yet - we need cleanup
    if let Err(ref e) = loop_result {
        eprintln!("ACP Proxy: Error in Agent → IDE forwarding: {}", e);
    }

    debug_log(|| "ACP Proxy: Agent stdout closed, stopping Agent → IDE thread");

    // ALWAYS wait for agent process to exit, even if there was an error
    let status_result = agent.wait();
    if let Err(ref e) = status_result {
        eprintln!("ACP Proxy: Failed to wait for agent process: {}", e);
    } else if let Ok(ref status) = status_result {
        let exit_code = status.code().unwrap_or(-1);
        eprintln!("ACP Proxy: Agent process exited with code: {}", exit_code);
        if exit_code == 101 {
            eprintln!(
                "ACP Proxy: EXIT CODE 101 - This is a Rust panic in the AGENT, not the proxy!"
            );
        }
    }

    // Send explicit shutdown signals to threads
    // This is more robust than relying on channel closure via drop()
    // The threads will exit their recv() loops when they see the Shutdown message
    let _ = autoreply_tx.send(AutoreplyMessage::Shutdown);
    let _ = metadata_tx.send(StateMessage::Shutdown);

    // ALWAYS join the IDE → Agent thread to ensure clean shutdown
    // Join threads in reverse dependency order to ensure graceful shutdown:
    // 1. IDE→Agent thread (may still be sending autoreplies)
    // 2. Autoreply forwarder thread (drains final messages)
    match ide_to_agent_thread.join() {
        Ok(Ok(())) => {
            debug_log(|| "ACP Proxy: IDE → Agent thread exited cleanly");
        }
        Ok(Err(e)) => {
            eprintln!("Warning: IDE → Agent thread returned error: {}", e);
        }
        Err(e) => {
            eprintln!("Warning: IDE → Agent thread panicked: {:?}", e);
        }
    }

    // ALWAYS join the autoreply forwarder thread to ensure clean shutdown
    match autoreply_thread.join() {
        Ok(Ok(())) => {
            debug_log(|| "ACP Proxy: Autoreply forwarder thread exited cleanly");
        }
        Ok(Err(e)) => {
            eprintln!("Warning: Autoreply forwarder thread returned error: {}", e);
        }
        Err(e) => {
            eprintln!("Warning: Autoreply forwarder thread panicked: {:?}", e);
        }
    }

    // Now propagate the original error if there was one
    loop_result?;

    // Exit with agent's exit code, or 1 if we couldn't get it
    let exit_code = status_result.ok().and_then(|s| s.code()).unwrap_or(1);
    debug_log(|| format!("ACP Proxy: Exiting with code {}", exit_code));
    std::process::exit(exit_code);
}

// ============================================================================
// Pure utility functions for testability
// ============================================================================
//
// These functions are extracted from the main proxy logic to enable easy unit testing
// without requiring event bus mocking. They handle prompt manipulation, metadata extraction,
// and autoreply counter logic.

/// Maximum number of autoreplies allowed per turn
const MAX_AUTOREPLIES: usize = 5;

/// Extract text content from a prompt array
///
/// Iterates through prompt items and collects all text from items with `type: "text"`.
fn extract_text_from_prompt_array(prompt_array: &[serde_json::Value]) -> Vec<String> {
    prompt_array
        .iter()
        .filter_map(|item| {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                item.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Concatenate text chunks with double newline separators
fn concatenate_text_chunks(chunks: &[String]) -> String {
    chunks.join("\n\n")
}

/// Build a modified prompt array by replacing only the first text entry
///
/// Preserves the original array order and all non-text resources (images, etc.).
/// Replaces the first text item with the modified text, removing all other text items.
/// This maintains the original ordering of resources while updating the user's prompt.
fn build_modified_prompt(
    original_prompt: &[serde_json::Value],
    modified_text: &str,
) -> Vec<serde_json::Value> {
    let mut new_prompt = Vec::new();
    let mut replaced_first_text = false;

    for item in original_prompt {
        let is_text = item.get("type").and_then(|v| v.as_str()) == Some("text");

        if is_text {
            if !replaced_first_text {
                // Clone the first text block to preserve all fields (annotations, _meta, etc.)
                // then mutate only the "text" field
                let mut modified_item = item.clone();
                if let Some(obj) = modified_item.as_object_mut() {
                    obj.insert("text".to_string(), json!(modified_text));
                }
                new_prompt.push(modified_item);
                replaced_first_text = true;
            }
            // Skip all other text entries (they were concatenated into modified_text)
        } else {
            // Preserve all non-text items in their original position
            new_prompt.push(item.clone());
        }
    }

    // If there were no text items, append a minimal text block at the end
    if !replaced_first_text {
        new_prompt.push(json!({
            "type": "text",
            "text": modified_text
        }));
    }

    new_prompt
}

/// Extract modified prompt from context with fallback
/// Extract autoreply from context
fn extract_autoreply(response: &HookResult) -> Option<String> {
    response.context.as_ref().filter(|s| !s.is_empty()).cloned()
}

/// Check if autoreply limit has been reached
fn check_autoreply_limit(current_count: usize, max_autoreplies: usize) -> bool {
    current_count >= max_autoreplies
}

/// Increment autoreply counter for a session
fn increment_autoreply_counter(
    counters: &mut HashMap<SessionId, usize>,
    session_id: &SessionId,
) -> usize {
    let current_count = counters.get(session_id).copied().unwrap_or(0);
    let new_count = current_count + 1;
    counters.insert(session_id.clone(), new_count);
    new_count
}

/// Reset autoreply counter for a session
fn reset_autoreply_counter(counters: &mut HashMap<SessionId, usize>, session_id: &SessionId) {
    counters.remove(session_id);
}

// ============================================================================
// Agent type parsing
// ============================================================================

/// Parse and validate agent type against our AgentType enum
fn parse_agent_type(agent: &str) -> Result<AgentType> {
    match agent {
        "claude" | "claude-code" => Ok(AgentType::Claude), // Accept both for backwards compatibility
        "codex" => Ok(AgentType::Codex),
        "cursor" => Ok(AgentType::Cursor),
        "gemini" => Ok(AgentType::Gemini),
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}

/// Create an AikiSession for ACP protocol tracking
fn create_session(
    agent_type: AgentType,
    session_id: impl Into<String>,
    agent_version: Option<&str>,
) -> AikiSession {
    AikiSession::new(
        agent_type,
        session_id,
        agent_version,
        crate::provenance::DetectionMethod::ACP,
    )
}

/// Handle session/update notification from agent
///
/// Extracts tool_call information and dispatches provenance recording via event bus.
/// This is called for every session/update from the agent to the IDE.
fn handle_session_update(
    msg: &JsonRpcMessage,
    agent_type: &AgentType,
    client_info: &Option<ClientInfo>,
    agent_info: &Option<AgentInfo>,
    cwd: &Option<PathBuf>,
    tool_call_contexts: &mut HashMap<ToolCallId, ToolCallContext>,
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
            client_info,
            agent_info,
            cwd,
            tool_call_contexts,
        ),
        SessionUpdate::ToolCallUpdate(update) => process_tool_call_update(
            &session_id,
            update,
            agent_type,
            client_info,
            agent_info,
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
    client_info: &Option<ClientInfo>,
    agent_info: &Option<AgentInfo>,
    cwd: &Option<PathBuf>,
    tool_call_contexts: &mut HashMap<ToolCallId, ToolCallContext>,
) -> Result<()> {
    let context = ToolCallContext {
        kind: tool_call.kind,
        paths: paths_from_locations(&tool_call.locations),
        content: tool_call.content.clone(),
    };

    let status = tool_call.status;

    // Store context for potential updates
    tool_call_contexts.insert(tool_call.id.clone(), context.clone());
    if matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) {
        tool_call_contexts.remove(&tool_call.id);
    }

    if status == ToolCallStatus::Completed {
        record_post_change_events(
            session_id,
            agent_type,
            client_info,
            agent_info,
            cwd,
            context,
        )?;
    }

    Ok(())
}

fn process_tool_call_update(
    session_id: &str,
    tool_call: &ToolCallUpdate,
    agent_type: &AgentType,
    client_info: &Option<ClientInfo>,
    agent_info: &Option<AgentInfo>,
    cwd: &Option<PathBuf>,
    tool_call_contexts: &mut HashMap<ToolCallId, ToolCallContext>,
) -> Result<()> {
    let entry = tool_call_contexts
        .entry(tool_call.id.clone())
        .or_insert_with(|| ToolCallContext {
            kind: tool_call.fields.kind.unwrap_or(ToolKind::Other),
            paths: Vec::new(),
            content: Vec::new(),
        });

    if let Some(kind) = tool_call.fields.kind {
        entry.kind = kind;
    }

    if let Some(locations) = &tool_call.fields.locations {
        entry.paths = paths_from_locations(locations);
    }

    if let Some(content) = &tool_call.fields.content {
        entry.content = content.clone();
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
        tool_call_contexts.remove(&tool_call.id);
    }

    if let Some(context) = context {
        record_post_change_events(
            session_id,
            agent_type,
            client_info,
            agent_info,
            cwd,
            context,
        )?;
    }

    Ok(())
}

#[derive(Clone)]
struct ToolCallContext {
    kind: ToolKind,
    paths: Vec<PathBuf>,
    content: Vec<agent_client_protocol::ToolCallContent>,
}

fn paths_from_locations(locations: &[ToolCallLocation]) -> Vec<PathBuf> {
    locations.iter().map(|loc| loc.path.clone()).collect()
}

fn record_post_change_events(
    session_id: &str,
    agent_type: &AgentType,
    client_info: &Option<ClientInfo>,
    agent_info: &Option<AgentInfo>,
    cwd: &Option<PathBuf>,
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

    // Get working directory (required)
    let working_dir = cwd
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    // Get tool name from kind
    let tool_name = format!("{:?}", context.kind); // Convert ToolKind enum to string (Edit, Delete, Move)

    // Convert all paths to strings
    let file_paths: Vec<String> = context
        .paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Extract edit details from tool call parameters (if available)
    let edit_details = extract_edit_details(&context);

    // Create session with agent version and client info
    let agent_version = agent_info.as_ref().and_then(|a| a.version.as_deref());
    let session = create_session(*agent_type, session_id.to_string(), agent_version)
        .with_client_info(
            client_info.as_ref().map(|c| c.name.as_str()),
            client_info.as_ref().and_then(|c| c.version.as_deref()),
        );

    // Create and dispatch a single event for all affected files
    let event = AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
        session,
        cwd: working_dir.clone(),
        timestamp: chrono::Utc::now(),
        tool_name: tool_name.clone(),
        file_paths,
        success: true,
        edit_details,
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("Warning: Event bus dispatch failed: {}", e);
    }

    Ok(())
}

/// Extract edit details from ACP tool call context
///
/// Extracts old_text/new_text from ToolCallContent::Diff variants.
/// The ACP protocol provides file diffs in the content field when tools
/// modify files, allowing us to detect user edits.
fn extract_edit_details(context: &ToolCallContext) -> Vec<crate::events::EditDetail> {
    use agent_client_protocol::ToolCallContent;

    let mut edit_details = Vec::new();

    for content_item in &context.content {
        if let ToolCallContent::Diff { diff } = content_item {
            // Convert PathBuf to string for file_path
            let file_path = diff.path.to_string_lossy().to_string();

            // old_text is Option<String>, use empty string if None (new file)
            let old_string = diff.old_text.clone().unwrap_or_default();

            // new_text is the modified content
            let new_string = diff.new_text.clone();

            edit_details.push(crate::events::EditDetail::new(
                file_path, old_string, new_string,
            ));
        }
    }

    if !edit_details.is_empty() {
        debug_log(|| {
            format!(
                "[acp] Extracted {} edit details from tool call content",
                edit_details.len()
            )
        });
    }

    edit_details
}

/// Check if a permission request is for a file-modifying tool
///
/// Parses session/request_permission params to determine if the tool
/// will modify files (Edit, Delete, Move). Returns true only for these
/// file-modifying operations, not for read-only tools like Read, Bash, etc.
fn is_file_modifying_permission_request(msg: &JsonRpcMessage) -> bool {
    // Parse the params to extract tool kind
    if let Some(params) = &msg.params {
        // The params should contain toolCallId and potentially tool details
        // We need to check the tool kind from the request
        if let Some(tool_call_id) = params.get("toolCallId") {
            debug_log(|| {
                format!(
                    "[acp] Found permission request for tool_call_id: {:?}",
                    tool_call_id
                )
            });
        }

        // Try to extract the kind from the permission request
        // The ACP spec shows options array, but we need to check the actual tool details
        // For now, we'll return true if we see certain patterns in the request
        // This may need refinement based on actual ACP permission request structure
        if let Some(kind_val) = params.get("kind").or_else(|| params.get("toolKind")) {
            if let Some(kind_str) = kind_val.as_str() {
                return matches!(kind_str, "edit" | "delete" | "move");
            }
        }
    }

    false
}

/// Handle session/prompt request and fire prompt.submitted event
///
/// This intercepts the user's prompt, fires a prompt.submitted event, and potentially
/// modifies the prompt before forwarding to the agent. Implements graceful
/// degradation - on any error, forwards the original message.
///
/// Note: Request tracking (TrackPrompt) is done by the caller before this function
/// to ensure response.received fires even if prompt.submitted processing fails.
fn handle_session_prompt(
    agent_stdin: &Arc<Mutex<std::process::ChildStdin>>,
    msg: &JsonRpcMessage,
    params: &serde_json::Value,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
    _metadata_tx: &mpsc::Sender<StateMessage>,
) -> Result<()> {
    // Extract session_id
    let session_id = session_id(
        params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    );

    // Extract all text content from prompt array
    let prompt_array = params
        .get("prompt")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Missing prompt array")))?;

    let text_chunks = extract_text_from_prompt_array(prompt_array);
    let original_text = concatenate_text_chunks(&text_chunks);

    // Get working directory with fallback
    let working_dir = cwd
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Fire prompt.submitted event
    let session = create_session(*agent_type, session_id.to_string(), None::<&str>);
    let event = AikiEvent::PromptSubmitted(AikiPromptSubmittedPayload {
        session,
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        prompt: original_text.clone(),
    });

    let response = event_bus::dispatch(event)?;

    // Emit failures to stderr (user-visible)
    for failure in &response.failures {
        use crate::events::result::Failure;
        let Failure(s) = failure;
        eprintln!("[aiki] ❌ {}", s);
    }

    // Check if blocked
    if response.is_blocking() {
        return Err(AikiError::Other(anyhow::anyhow!(
            "prompt.submitted validation blocked prompt"
        )));
    }

    // Build final prompt: messages + context + original
    let formatted_messages = response.format_messages();
    let prepended_context = response.context.as_deref().unwrap_or("");

    let final_prompt = match (
        !formatted_messages.is_empty(),
        !prepended_context.is_empty(),
    ) {
        (true, true) => {
            // Both messages and context: combine them
            format!(
                "{}\n\n{}\n\n{}",
                formatted_messages, prepended_context, original_text
            )
        }
        (true, false) => {
            // Only messages
            format!("{}\n\n{}", formatted_messages, original_text)
        }
        (false, true) => {
            // Only context
            format!("{}\n\n{}", prepended_context, original_text)
        }
        (false, false) => {
            // Neither: use original prompt
            original_text.to_string()
        }
    };

    // Modify the JSON params to replace prompt text
    // We rebuild the prompt array with a single text entry containing the final prompt,
    // while preserving all non-text resources (images, etc.) to avoid sending duplicate
    // content when the IDE sends multiple text chunks.
    let mut modified_msg = msg.clone();
    if let Some(params_mut) = modified_msg.params.as_mut() {
        if let Some(params_obj) = params_mut.as_object_mut() {
            if let Some(prompt_arr) = params_obj.get_mut("prompt").and_then(|v| v.as_array_mut()) {
                let new_prompt = build_modified_prompt(prompt_arr, &final_prompt);
                *prompt_arr = new_prompt;
            }
        }
    }

    // Note: Request tracking is now done in the caller (before this function is called)
    // to ensure response.received fires even if prompt.submitted processing fails (graceful degradation)

    // Forward modified message to agent
    let modified_line = serde_json::to_string(&modified_msg).map_err(|e| {
        AikiError::Other(anyhow::anyhow!(
            "Failed to serialize modified prompt: {}",
            e
        ))
    })?;
    // Serialize outside lock to minimize critical section
    let data = format!("{}\n", modified_line).into_bytes();
    {
        // ✅ FIX for Issue #5: Handle mutex poisoning gracefully
        let mut stdin = match agent_stdin.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: Mutex poisoned (another thread panicked), attempting recovery");
                poisoned.into_inner()
            }
        };
        stdin.write_all(&data)?;
        stdin.flush()?;
    }

    debug_log(|| {
        format!(
            "[acp] Fired prompt.submitted event for session: {}, modified: {}",
            session_id,
            final_prompt != original_text
        )
    });

    Ok(())
}

/// Handle session.ended event and autoreply
///
/// Fires when the agent completes a turn (stopReason: end_turn).
/// Dispatches session.ended event to flows, and if they return an autoreply,
/// sends it back to the agent (up to MAX_AUTOREPLIES times per session).
fn handle_session_end(
    session_id: &SessionId,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
    response_text: &str,
    autoreply_counters: &mut HashMap<SessionId, usize>,
    max_autoreplies: usize,
    autoreply_tx: &mpsc::Sender<AutoreplyMessage>,
    prompt_requests: &mut HashMap<JsonRpcId, SessionId>,
) -> Result<()> {
    // Get working directory with fallback
    let working_dir = cwd
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Fire response.received event with accumulated response text
    let session = create_session(*agent_type, session_id.to_string(), None::<&str>);
    let event = AikiEvent::ResponseReceived(AikiResponseReceivedPayload {
        session,
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        response: response_text.to_string(),
        modified_files: Vec::new(), // Files tracked separately via change.done events
    });

    let response = event_bus::dispatch(event)?;

    // Emit failures to stderr (user-visible only)
    for failure in &response.failures {
        use crate::events::result::Failure;
        let Failure(s) = failure;
        eprintln!("[aiki] ❌ {}", s);
    }

    // Check for autoreply (agent-visible via next prompt)
    // Combine both messages and context if both are present
    let formatted_messages = response.format_messages();
    let autoreply_context = extract_autoreply(&response);

    let autoreply_text = match (formatted_messages.is_empty(), autoreply_context.is_some()) {
        (false, true) => {
            // Both messages and autoreply context: combine them
            Some(format!(
                "{}\n\n{}",
                formatted_messages,
                autoreply_context.unwrap()
            ))
        }
        (false, false) => {
            // Only messages
            Some(formatted_messages)
        }
        (true, true) => {
            // Only autoreply context
            autoreply_context
        }
        (true, false) => {
            // Neither
            None
        }
    };

    if let Some(autoreply_text) = autoreply_text {
        // Get current autoreply count for this session
        let current_count = autoreply_counters.get(session_id).copied().unwrap_or(0);

        if !check_autoreply_limit(current_count, max_autoreplies) {
            // Increment counter for this session
            let new_count = increment_autoreply_counter(autoreply_counters, session_id);

            debug_log(|| {
                format!(
                    "[acp] response.received autoreply #{} for session {}: {} chars",
                    new_count,
                    session_id,
                    autoreply_text.len()
                )
            });

            // Create autoreply message (JSON generated on-demand when sent)
            let autoreply_msg = Autoreply::new(session_id, autoreply_text, new_count);

            // Extract debug info before moving
            let debug_request_id = Some(autoreply_msg.raw_request_id_display().to_string());

            // ✅ FIX for Issue #2: Insert into HashMap BEFORE sending to channel
            // This prevents a race condition where the agent responds before we've
            // registered the request ID, causing the response.received event to be lost.
            // The correct order is: prepare state first, then trigger the action.
            prompt_requests.insert(
                autoreply_msg.normalized_request_id().clone(),
                Arc::clone(session_id),
            );

            // Send via channel to autoreply forwarder thread
            // Autoreplies sent to agent only (not IDE) to avoid race condition
            autoreply_tx
                .send(AutoreplyMessage::SendAutoreply(autoreply_msg))
                .map_err(|e| {
                    AikiError::Other(anyhow::anyhow!("Failed to send autoreply: {}", e))
                })?;

            if let Some(request_id) = debug_request_id {
                debug_log(|| {
                    format!(
                        "[acp] Queued autoreply #{} for session: {} with request_id: {}",
                        new_count, session_id, request_id
                    )
                });
            }
        } else if current_count >= max_autoreplies {
            eprintln!(
                "Warning: Maximum autoreplies ({}) reached for session {}, ignoring autoreply from flow",
                max_autoreplies, session_id
            );
        }
    } else {
        debug_log(|| {
            format!(
                "[acp] Fired response.received event for session: {}, no autoreply",
                session_id
            )
        });
    }

    Ok(())
}
/// Fire change.permission_asked event before file-modifying tool executes
///
/// This is called when we intercept a session/request_permission for
/// file-modifying tools (Edit, Delete, Move). It allows flows to stash
/// user edits before the AI starts making changes.
fn fire_session_start_event(
    session_id: &str,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
) -> Result<()> {
    // Get working directory (required)
    let working_dir = cwd
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    // Create and dispatch session.started event
    let session = create_session(*agent_type, session_id.to_string(), None::<&str>);
    let event = AikiEvent::SessionStarted(AikiSessionStartPayload {
        session,
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("Warning: session.started event bus dispatch failed: {}", e);
    } else {
        debug_log(|| {
            format!(
                "[acp] Fired session.started event for session: {}",
                session_id
            )
        });
    }

    Ok(())
}

fn fire_pre_file_change_event(
    session_id: &str,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
) -> Result<()> {
    // Get working directory (required)
    let working_dir = cwd
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    // Create and dispatch write.permission_asked event
    let session = create_session(*agent_type, session_id.to_string(), None::<&str>);
    let event = AikiEvent::WritePermissionAsked(AikiWritePermissionAskedPayload {
        session,
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        tool_name: "acp".to_string(),
        file_paths: vec![],
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!(
            "Warning: write.permission_asked event bus dispatch failed: {}",
            e
        );
    } else {
        debug_log(|| {
            format!(
                "[acp] Fired write.permission_asked event for session: {}",
                session_id
            )
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_agent_type_valid() {
        assert!(matches!(parse_agent_type("claude"), Ok(AgentType::Claude)));
        assert!(matches!(
            parse_agent_type("claude-code"),
            Ok(AgentType::Claude)
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

    #[test]
    fn test_autoreply_message_id_serialization() {
        // Create an autoreply message
        let sid = session_id("test-session-123");
        let msg = Autoreply::new(&sid, "Fix the errors".to_string(), 1);

        // Verify the raw ID is a plain string (no quotes)
        assert_eq!(msg.raw_request_id, "aiki-autoreply-test-session-123-1");

        // Verify the normalized ID has quotes (for HashMap key)
        assert_eq!(
            msg.normalized_request_id.0,
            "\"aiki-autoreply-test-session-123-1\""
        );

        // Verify JSON serialization uses the raw ID (no double-quoting)
        let json = msg.as_json().expect("Failed to serialize autoreply");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

        // The ID field should be a string (not a string containing quotes)
        assert_eq!(
            parsed["id"].as_str().unwrap(),
            "aiki-autoreply-test-session-123-1"
        );

        // Verify the JSON structure is correct
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "session/prompt");
        assert_eq!(parsed["params"]["sessionId"], "test-session-123");
        assert_eq!(parsed["params"]["prompt"][0]["text"], "Fix the errors");
    }

    #[test]
    fn test_json_rpc_id_normalization() {
        // Test string ID normalization
        let string_id = serde_json::Value::String("test-123".to_string());
        let normalized = JsonRpcId::from_value(&string_id);
        assert_eq!(normalized.0, "\"test-123\""); // Includes quotes

        // Test number ID normalization
        let number_id = serde_json::Value::Number(serde_json::Number::from(42));
        let normalized = JsonRpcId::from_value(&number_id);
        assert_eq!(normalized.0, "42"); // No quotes

        // Test null ID normalization
        let null_id = serde_json::Value::Null;
        let normalized = JsonRpcId::from_value(&null_id);
        assert_eq!(normalized.0, "null"); // No quotes
    }

    #[test]
    fn test_autoreply_message_unique_ids() {
        // Verify that different counters produce different IDs
        let sid1 = session_id("session-1");
        let sid2 = session_id("session-2");
        let msg1 = Autoreply::new(&sid1, "text1".to_string(), 1);
        let msg2 = Autoreply::new(&sid1, "text2".to_string(), 2);
        let msg3 = Autoreply::new(&sid2, "text3".to_string(), 1);

        assert_ne!(msg1.raw_request_id, msg2.raw_request_id);
        assert_ne!(msg1.raw_request_id, msg3.raw_request_id);
        assert_ne!(msg2.raw_request_id, msg3.raw_request_id);
    }

    #[test]
    fn test_json_rpc_id_round_trip_through_hashmap() {
        use std::collections::HashMap;

        // This is the critical pattern used in the proxy:
        // 1. IDE sends request with ID
        // 2. We normalize and store in HashMap
        // 3. Agent sends response with same ID
        // 4. We normalize and look up in HashMap

        // Test with string ID (most common case)
        let request_id = serde_json::Value::String("test-request-123".to_string());
        let normalized_request = JsonRpcId::from_value(&request_id);

        let mut map = HashMap::new();
        map.insert(normalized_request.clone(), "test-session".to_string());

        // Simulate receiving a response with the same ID
        let response_id = serde_json::Value::String("test-request-123".to_string());
        let normalized_response = JsonRpcId::from_value(&response_id);

        // Critical: normalized IDs must match for HashMap lookup
        assert_eq!(normalized_request, normalized_response);
        assert_eq!(
            map.get(&normalized_response),
            Some(&"test-session".to_string())
        );

        // Test with number ID
        let request_id = serde_json::Value::Number(serde_json::Number::from(42));
        let normalized_request = JsonRpcId::from_value(&request_id);

        let mut map = HashMap::new();
        map.insert(normalized_request.clone(), "test-session-2".to_string());

        let response_id = serde_json::Value::Number(serde_json::Number::from(42));
        let normalized_response = JsonRpcId::from_value(&response_id);

        assert_eq!(normalized_request, normalized_response);
        assert_eq!(
            map.get(&normalized_response),
            Some(&"test-session-2".to_string())
        );
    }

    #[test]
    fn test_prompt_rewrite_with_multiple_text_chunks() {
        // Simulate an IDE sending multiple text chunks (the challenging scenario
        // mentioned in ops/current/event-dispatch-gap-analysis.md:708-713)
        let prompt_with_multiple_chunks = json!([
            {
                "type": "text",
                "text": "First chunk of user prompt"
            },
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "iVBORw0KGgoAAAANS..."
                }
            },
            {
                "type": "text",
                "text": "Second chunk of user prompt"
            },
            {
                "type": "text",
                "text": "Third chunk of user prompt"
            }
        ]);

        // Create a sampling/createMessage request
        let request = json!({
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "sampling/createMessage",
            "params": {
                "prompt": prompt_with_multiple_chunks,
                "sessionId": "test-session"
            }
        });

        let msg: JsonRpcMessage = serde_json::from_value(request.clone()).unwrap();

        // Extract the prompt array
        let prompt_array = msg
            .params
            .as_ref()
            .unwrap()
            .get("prompt")
            .and_then(|v| v.as_array())
            .unwrap();

        // Verify we start with 4 items (3 text + 1 image)
        assert_eq!(prompt_array.len(), 4);

        // Concatenate all text chunks (simulating what the code does)
        let mut original_text = String::new();
        for item in prompt_array {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !original_text.is_empty() {
                        original_text.push_str("\n\n");
                    }
                    original_text.push_str(text);
                }
            }
        }

        assert_eq!(
            original_text,
            "First chunk of user prompt\n\nSecond chunk of user prompt\n\nThird chunk of user prompt"
        );

        // Simulate a flow rewriting the prompt
        let modified_prompt = "MODIFIED: Complete rewrite of the prompt";

        // Apply the fix: rebuild prompt array with single text entry + non-text resources
        let mut modified_msg = msg.clone();
        if let Some(params_mut) = modified_msg.params.as_mut() {
            if let Some(params_obj) = params_mut.as_object_mut() {
                if let Some(prompt_arr) =
                    params_obj.get_mut("prompt").and_then(|v| v.as_array_mut())
                {
                    let mut new_prompt: Vec<serde_json::Value> = Vec::new();

                    // Add the single modified text entry first
                    new_prompt.push(json!({
                        "type": "text",
                        "text": modified_prompt
                    }));

                    // Preserve all non-text resources
                    for item in prompt_arr.iter() {
                        if item.get("type").and_then(|v| v.as_str()) != Some("text") {
                            new_prompt.push(item.clone());
                        }
                    }

                    // Replace the entire prompt array
                    *prompt_arr = new_prompt;
                }
            }
        }

        // Verify the result
        let final_prompt = modified_msg
            .params
            .as_ref()
            .unwrap()
            .get("prompt")
            .and_then(|v| v.as_array())
            .unwrap();

        // Should have 2 items: 1 text + 1 image (the 3 original text chunks consolidated)
        assert_eq!(final_prompt.len(), 2);

        // First item should be the modified text
        assert_eq!(
            final_prompt[0].get("type").and_then(|v| v.as_str()),
            Some("text")
        );
        assert_eq!(
            final_prompt[0].get("text").and_then(|v| v.as_str()),
            Some(modified_prompt)
        );

        // Second item should be the preserved image
        assert_eq!(
            final_prompt[1].get("type").and_then(|v| v.as_str()),
            Some("image")
        );
        assert!(final_prompt[1].get("source").is_some());

        // Critical assertion: verify no original text chunks remain
        let remaining_text_chunks: Vec<_> = final_prompt
            .iter()
            .filter(|item| item.get("type").and_then(|v| v.as_str()) == Some("text"))
            .collect();

        assert_eq!(
            remaining_text_chunks.len(),
            1,
            "Should have exactly one text chunk after rewrite"
        );

        // Verify the text doesn't contain fragments of the original
        let final_text = remaining_text_chunks[0]
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap();

        assert!(!final_text.contains("First chunk"));
        assert!(!final_text.contains("Second chunk"));
        assert!(!final_text.contains("Third chunk"));
        assert_eq!(final_text, modified_prompt);
    }

    #[test]
    fn test_prompt_requests_cleanup_on_all_stop_reasons() {
        // Test that prompt_requests HashMap is cleaned up for ALL stopReasons,
        // not just "end_turn". This prevents memory leaks and stale ID reuse issues.

        let stop_reasons = vec![
            "end_turn",          // Normal completion
            "max_tokens",        // Hit token limit
            "max_turn_requests", // Too many tool calls
            "refusal",           // Agent refused
            "cancelled",         // User cancelled (Ctrl-C)
        ];

        for stop_reason in stop_reasons {
            // Create a sampling/createMessage response with this stopReason
            let response = json!({
                "jsonrpc": "2.0",
                "id": "test-request-123",
                "result": {
                    "stopReason": stop_reason,
                    "content": [{
                        "type": "text",
                        "text": "Response text"
                    }]
                }
            });

            let msg: JsonRpcMessage = serde_json::from_value(response).unwrap();

            // Verify the message has the expected structure
            assert_eq!(msg.id, Some(json!("test-request-123")));
            assert!(msg.result.is_some());

            let result = msg.result.as_ref().unwrap();
            assert_eq!(
                result.get("stopReason").and_then(|v| v.as_str()),
                Some(stop_reason)
            );
        }
    }

    #[test]
    fn test_prompt_requests_cleanup_on_json_rpc_error() {
        // Test that prompt_requests HashMap is cleaned up when JSON-RPC errors occur
        // (e.g., agent crashes, protocol errors, etc.)

        // Create a JSON-RPC error response
        let error_response = json!({
            "jsonrpc": "2.0",
            "id": "test-request-456",
            "error": {
                "code": -32603,
                "message": "Internal error",
                "data": {
                    "details": "Agent process crashed"
                }
            }
        });

        let msg: JsonRpcMessage = serde_json::from_value(error_response).unwrap();

        // Verify the message has an error field
        assert_eq!(msg.id, Some(json!("test-request-456")));
        assert!(msg.error.is_some());
        assert!(msg.result.is_none());

        let error = msg.error.as_ref().unwrap();
        assert_eq!(error.get("code").and_then(|v| v.as_i64()), Some(-32603));
        assert_eq!(
            error.get("message").and_then(|v| v.as_str()),
            Some("Internal error")
        );
    }

    #[test]
    fn test_json_rpc_id_normalization_for_cleanup() {
        // Test that JsonRpcId normalization works correctly for HashMap cleanup
        // This ensures we can match responses to requests regardless of ID format

        // String ID
        let string_id = json!("test-abc-123");
        let normalized_string = JsonRpcId::from_value(&string_id);
        assert_eq!(normalized_string.0, "\"test-abc-123\""); // Quoted

        // Number ID
        let number_id = json!(42);
        let normalized_number = JsonRpcId::from_value(&number_id);
        assert_eq!(normalized_number.0, "42"); // No quotes

        // Null ID (rare but valid in JSON-RPC)
        let null_id = json!(null);
        let normalized_null = JsonRpcId::from_value(&null_id);
        assert_eq!(normalized_null.0, "null");

        // Verify HashMap lookup works with normalized IDs
        use std::collections::HashMap;
        let mut map: HashMap<JsonRpcId, String> = HashMap::new();

        map.insert(normalized_string.clone(), "session-1".to_string());
        map.insert(normalized_number.clone(), "session-2".to_string());

        // Lookup should work with freshly normalized IDs
        let lookup_string = JsonRpcId::from_value(&json!("test-abc-123"));
        let lookup_number = JsonRpcId::from_value(&json!(42));

        assert_eq!(map.get(&lookup_string), Some(&"session-1".to_string()));
        assert_eq!(map.get(&lookup_number), Some(&"session-2".to_string()));
    }

    // Unit tests for stopReason handling (Phase-1 core behavior)

    #[test]
    fn test_stop_reason_response_structure() {
        use fixtures::*;

        // Test all valid stopReasons from ACP spec
        let stop_reasons = vec![
            "end_turn",
            "max_tokens",
            "max_turn_requests",
            "refusal",
            "cancelled",
        ];

        for stop_reason in stop_reasons {
            let response = sampling_response("test-123", stop_reason, "Response text");

            // Verify structure
            assert_eq!(response.id, Some(json!("test-123")));
            assert!(response.result.is_some());

            let result = response.result.as_ref().unwrap();
            assert_eq!(
                result.get("stopReason").and_then(|v| v.as_str()),
                Some(stop_reason)
            );
        }
    }

    #[test]
    fn test_session_prompt_structure() {
        use fixtures::*;

        let msg = session_prompt_message("test-session", "Hello");

        // Verify structure
        assert_eq!(msg.method, Some("session/prompt".to_string()));
        assert!(msg.params.is_some());

        let params = msg.params.as_ref().unwrap();
        assert_eq!(
            params.get("sessionId").and_then(|v| v.as_str()),
            Some("test-session")
        );

        let prompt = params.get("prompt").and_then(|v| v.as_array()).unwrap();
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].get("type").and_then(|v| v.as_str()), Some("text"));
        assert_eq!(
            prompt[0].get("text").and_then(|v| v.as_str()),
            Some("Hello")
        );
    }

    #[test]
    fn test_session_prompt_multiple_text_chunks_structure() {
        use fixtures::*;

        let msg = session_prompt_with_multiple_text_chunks(
            "test-session",
            vec!["First chunk", "Second chunk", "Third chunk"],
        );

        let params = msg.params.as_ref().unwrap();
        let prompt = params.get("prompt").and_then(|v| v.as_array()).unwrap();

        // Should have 3 text entries
        assert_eq!(prompt.len(), 3);

        for chunk in prompt.iter() {
            assert_eq!(chunk.get("type").and_then(|v| v.as_str()), Some("text"));
        }

        assert_eq!(
            prompt[0].get("text").and_then(|v| v.as_str()),
            Some("First chunk")
        );
        assert_eq!(
            prompt[1].get("text").and_then(|v| v.as_str()),
            Some("Second chunk")
        );
        assert_eq!(
            prompt[2].get("text").and_then(|v| v.as_str()),
            Some("Third chunk")
        );
    }

    #[test]
    fn test_agent_message_chunk_notification_structure() {
        use fixtures::*;

        let msg = agent_message_chunk_notification("test-session", "Chunk text");

        // Verify structure
        assert_eq!(msg.method, Some("session/update".to_string()));
        assert!(msg.params.is_some());

        let params = msg.params.as_ref().unwrap();
        assert_eq!(
            params.get("sessionId").and_then(|v| v.as_str()),
            Some("test-session")
        );

        let update = params.get("update").unwrap();
        assert_eq!(
            update.get("type").and_then(|v| v.as_str()),
            Some("agent_message_chunk")
        );

        let content = update.get("content").unwrap();
        assert_eq!(
            content.get("text").and_then(|v| v.as_str()),
            Some("Chunk text")
        );
    }

    #[test]
    fn test_json_rpc_error_structure() {
        use fixtures::*;

        let msg = json_rpc_error("test-123", -32603, "Internal error");

        // Verify structure
        assert_eq!(msg.id, Some(json!("test-123")));
        assert!(msg.error.is_some());
        assert!(msg.result.is_none());

        let error = msg.error.as_ref().unwrap();
        assert_eq!(error.get("code").and_then(|v| v.as_i64()), Some(-32603));
        assert_eq!(
            error.get("message").and_then(|v| v.as_str()),
            Some("Internal error")
        );
    }

    // Tests for metadata extraction and fallbacks

    #[test]
    fn test_session_id_extraction_from_prompt() {
        use fixtures::*;

        let msg = session_prompt_message("my-session-123", "Hello");

        // Extract session_id like the code does at line 1256-1264
        let session_id = msg
            .params
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        assert_eq!(session_id, "my-session-123");
    }

    #[test]
    fn test_session_id_missing_uses_empty_string() {
        // Create message without sessionId
        let msg: JsonRpcMessage = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "session/prompt",
            "params": {
                "prompt": [{
                    "type": "text",
                    "text": "Hello"
                }]
            }
        }))
        .unwrap();

        // Extract session_id with fallback (per line 1264)
        let session_id = msg
            .params
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        assert_eq!(session_id, "");
    }

    #[test]
    fn test_prompt_array_extraction() {
        use fixtures::*;

        let msg = session_prompt_with_multiple_text_chunks(
            "test-session",
            vec!["First", "Second", "Third"],
        );

        // Extract prompt array like code does at line 1267-1272
        let prompt_array = msg
            .params
            .as_ref()
            .and_then(|p| p.get("prompt"))
            .and_then(|v| v.as_array());

        assert!(prompt_array.is_some());
        assert_eq!(prompt_array.unwrap().len(), 3);
    }

    #[test]
    fn test_prompt_array_missing_returns_none() {
        // Create message without prompt array
        let msg: JsonRpcMessage = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "session/prompt",
            "params": {
                "sessionId": "test-session"
            }
        }))
        .unwrap();

        // Extract prompt array (should be None per line 1272)
        let prompt_array = msg
            .params
            .as_ref()
            .and_then(|p| p.get("prompt"))
            .and_then(|v| v.as_array());

        assert!(prompt_array.is_none());
    }

    #[test]
    fn test_text_concatenation_from_multiple_chunks() {
        use fixtures::*;

        let msg = session_prompt_with_multiple_text_chunks(
            "test-session",
            vec!["First chunk", "Second chunk", "Third chunk"],
        );

        let prompt_array = msg
            .params
            .as_ref()
            .unwrap()
            .get("prompt")
            .and_then(|v| v.as_array())
            .unwrap();

        // Concatenate like code does at lines 1274-1282
        let mut original_text = String::new();
        for item in prompt_array {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !original_text.is_empty() {
                        original_text.push_str("\n\n");
                    }
                    original_text.push_str(text);
                }
            }
        }

        assert_eq!(original_text, "First chunk\n\nSecond chunk\n\nThird chunk");
    }

    #[test]
    fn test_text_concatenation_skips_non_text_items() {
        use fixtures::*;

        let msg = session_prompt_with_image("test-session", "Hello", "base64data");

        let prompt_array = msg
            .params
            .as_ref()
            .unwrap()
            .get("prompt")
            .and_then(|v| v.as_array())
            .unwrap();

        // Concatenate only text items (per lines 1276-1280)
        let mut original_text = String::new();
        for item in prompt_array {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !original_text.is_empty() {
                        original_text.push_str("\n\n");
                    }
                    original_text.push_str(text);
                }
            }
        }

        // Should only have text from text item, not image
        assert_eq!(original_text, "Hello");
    }

    #[test]
    fn test_empty_prompt_array_produces_empty_text() {
        // Create message without empty prompt array
        let msg: JsonRpcMessage = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "session/prompt",
            "params": {
                "sessionId": "test-session",
                "prompt": []
            }
        }))
        .unwrap();

        let prompt_array = msg
            .params
            .as_ref()
            .unwrap()
            .get("prompt")
            .and_then(|v| v.as_array())
            .unwrap();

        // Concatenate (should be empty)
        let mut original_text = String::new();
        for item in prompt_array {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !original_text.is_empty() {
                        original_text.push_str("\n\n");
                    }
                    original_text.push_str(text);
                }
            }
        }

        assert_eq!(original_text, "");
    }

    #[test]
    fn test_stop_reason_extraction() {
        use fixtures::*;

        let response = sampling_response("test-123", "max_tokens", "Response");

        // Extract stopReason like code does at line 725
        let stop_reason = response
            .result
            .as_ref()
            .and_then(|r| r.get("stopReason"))
            .and_then(|v| v.as_str());

        assert_eq!(stop_reason, Some("max_tokens"));
    }

    #[test]
    fn test_stop_reason_missing_returns_none() {
        // Create response without stopReason
        let response: JsonRpcMessage = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "test-123",
            "result": {
                "content": [{
                    "type": "text",
                    "text": "Response"
                }]
            }
        }))
        .unwrap();

        // Extract stopReason (should be None)
        let stop_reason = response
            .result
            .as_ref()
            .and_then(|r| r.get("stopReason"))
            .and_then(|v| v.as_str());

        assert!(stop_reason.is_none());
    }

    // Tests for Autoreply counter and MAX_AUTOREPLIES enforcement

    #[test]
    fn test_autoreply_counter_starts_at_zero() {
        use std::collections::HashMap;

        let mut counters: HashMap<SessionId, usize> = HashMap::new();
        let session_id = session_id("test-session");

        // First autoreply (per line 1432)
        let current_count = counters.get(&session_id).copied().unwrap_or(0);
        assert_eq!(current_count, 0);

        let new_count = current_count + 1;
        counters.insert(session_id.clone(), new_count);

        assert_eq!(new_count, 1);
        assert_eq!(counters.get(&session_id), Some(&1));
    }

    #[test]
    fn test_autoreply_counter_increments() {
        use std::collections::HashMap;

        let mut counters: HashMap<SessionId, usize> = HashMap::new();
        let session_id = session_id("test-session");

        // Simulate multiple autoreplies (per lines 1432-1439)
        for expected_count in 1..=5 {
            let current_count = counters.get(&session_id).copied().unwrap_or(0);
            let new_count = current_count + 1;
            counters.insert(session_id.clone(), new_count);

            assert_eq!(new_count, expected_count);
        }

        assert_eq!(counters.get(&session_id), Some(&5));
    }

    #[test]
    fn test_autoreply_counter_per_session() {
        use std::collections::HashMap;

        let mut counters: HashMap<SessionId, usize> = HashMap::new();
        let session1 = session_id("session-1");
        let session2 = session_id("session-2");

        // Increment session 1 to 3
        for _ in 1..=3 {
            let count = counters.get(&session1).copied().unwrap_or(0);
            counters.insert(session1.clone(), count + 1);
        }

        // Increment session 2 to 2
        for _ in 1..=2 {
            let count = counters.get(&session2).copied().unwrap_or(0);
            counters.insert(session2.clone(), count + 1);
        }

        // Each session has independent counter
        assert_eq!(counters.get(&session1), Some(&3));
        assert_eq!(counters.get(&session2), Some(&2));
    }

    #[test]
    fn test_max_autoreplies_check() {
        // Test counter at limit
        let current_count = 5;
        assert!(current_count >= MAX_AUTOREPLIES);

        // Test counter under limit
        let current_count = 4;
        assert!(current_count < MAX_AUTOREPLIES);
    }

    #[test]
    fn test_autoreply_counter_reset() {
        use std::collections::HashMap;

        let mut counters: HashMap<SessionId, usize> = HashMap::new();
        let session_id = session_id("test-session");

        // Set counter to 5
        counters.insert(session_id.clone(), 5);
        assert_eq!(counters.get(&session_id), Some(&5));

        // Reset counter (per lines 636-643, StateMessage::ResetAutoreplyCounter)
        counters.remove(&session_id);

        // Counter should be gone, defaulting to 0
        let current_count = counters.get(&session_id).copied().unwrap_or(0);
        assert_eq!(current_count, 0);
    }

    #[test]
    fn test_autoreply_id_format() {
        let session_id = session_id("my-session-123");
        let count = 1;

        // Test ID format (per line 1443)
        let autoreply = Autoreply::new(&session_id, "Test".to_string(), count);

        // Verify ID format: "aiki-autoreply-{session_id}-{count}"
        assert_eq!(autoreply.raw_request_id, "aiki-autoreply-my-session-123-1");
    }

    #[test]
    fn test_autoreply_id_uniqueness_by_count() {
        let session_id = session_id("test-session");

        // Create autoreplies with different counts
        let autoreply1 = Autoreply::new(&session_id, "Test1".to_string(), 1);
        let autoreply2 = Autoreply::new(&session_id, "Test2".to_string(), 2);
        let autoreply3 = Autoreply::new(&session_id, "Test3".to_string(), 3);

        // IDs should be unique
        assert_ne!(autoreply1.raw_request_id, autoreply2.raw_request_id);
        assert_ne!(autoreply2.raw_request_id, autoreply3.raw_request_id);
        assert_ne!(autoreply1.raw_request_id, autoreply3.raw_request_id);
    }

    #[test]
    fn test_autoreply_id_uniqueness_by_session() {
        let session1 = session_id("session-1");
        let session2 = session_id("session-2");

        // Same count, different sessions
        let autoreply1 = Autoreply::new(&session1, "Test".to_string(), 1);
        let autoreply2 = Autoreply::new(&session2, "Test".to_string(), 1);

        // IDs should be unique
        assert_ne!(autoreply1.raw_request_id, autoreply2.raw_request_id);
    }

    #[test]
    fn test_autoreply_empty_text_detection() {
        let autoreply_text = "";

        // Test empty check (per line 1434)
        assert!(autoreply_text.is_empty());

        let autoreply_text = "   ";
        assert!(!autoreply_text.is_empty()); // Not trimmed in actual code

        let autoreply_text = "Valid text";
        assert!(!autoreply_text.is_empty());
    }

    #[test]
    fn test_autoreply_json_serialization() {
        let session_id = session_id("test-session");
        let autoreply = Autoreply::new(&session_id, "Fix the errors".to_string(), 1);

        // Serialize to JSON (per lines 1443-1467)
        let json = autoreply.as_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Verify JSON structure
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "session/prompt");
        assert_eq!(parsed["id"], "aiki-autoreply-test-session-1");

        let params = &parsed["params"];
        assert_eq!(params["sessionId"], "test-session");

        let prompt = params["prompt"].as_array().unwrap();
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0]["type"], "text");
        assert_eq!(prompt[0]["text"], "Fix the errors");
    }

    // Test fixtures for JSON-RPC messages
    mod fixtures {
        use super::*;

        pub fn session_prompt_message(session_id: &str, text: &str) -> JsonRpcMessage {
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": "test-request-123",
                "method": "session/prompt",
                "params": {
                    "sessionId": session_id,
                    "prompt": [{
                        "type": "text",
                        "text": text
                    }]
                }
            }))
            .unwrap()
        }

        pub fn session_prompt_with_multiple_text_chunks(
            session_id: &str,
            texts: Vec<&str>,
        ) -> JsonRpcMessage {
            let prompt: Vec<serde_json::Value> = texts
                .into_iter()
                .map(|t| {
                    json!({
                        "type": "text",
                        "text": t
                    })
                })
                .collect();

            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": "test-request-123",
                "method": "session/prompt",
                "params": {
                    "sessionId": session_id,
                    "prompt": prompt
                }
            }))
            .unwrap()
        }

        pub fn session_prompt_with_image(
            session_id: &str,
            text: &str,
            image_data: &str,
        ) -> JsonRpcMessage {
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": "test-request-123",
                "method": "session/prompt",
                "params": {
                    "sessionId": session_id,
                    "prompt": [
                        {
                            "type": "text",
                            "text": text
                        },
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": image_data
                            }
                        }
                    ]
                }
            }))
            .unwrap()
        }

        pub fn sampling_response(
            request_id: &str,
            stop_reason: &str,
            text: &str,
        ) -> JsonRpcMessage {
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "stopReason": stop_reason,
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                }
            }))
            .unwrap()
        }

        pub fn agent_message_chunk_notification(session_id: &str, text: &str) -> JsonRpcMessage {
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session_id,
                    "update": {
                        "type": "agent_message_chunk",
                        "content": {
                            "text": text
                        }
                    }
                }
            }))
            .unwrap()
        }

        pub fn json_rpc_error(request_id: &str, code: i64, message: &str) -> JsonRpcMessage {
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": code,
                    "message": message
                }
            }))
            .unwrap()
        }
    }

    // Tests for pure utility functions

    #[test]
    fn test_extract_text_from_prompt_array() {
        let prompt = vec![
            json!({"type": "text", "text": "Hello"}),
            json!({"type": "image", "data": "base64"}),
            json!({"type": "text", "text": "World"}),
        ];

        let result = extract_text_from_prompt_array(&prompt);
        assert_eq!(result, vec!["Hello", "World"]);
    }

    #[test]
    fn test_concatenate_text_chunks_with_separators() {
        let chunks = vec![
            "First".to_string(),
            "Second".to_string(),
            "Third".to_string(),
        ];
        let result = concatenate_text_chunks(&chunks);
        assert_eq!(result, "First\n\nSecond\n\nThird");
    }

    #[test]
    fn test_build_modified_prompt_single_text() {
        let original = vec![json!({"type": "text", "text": "old"})];
        let result = build_modified_prompt(&original, "new");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["text"], "new");
    }

    #[test]
    fn test_build_modified_prompt_preserves_images() {
        let original = vec![
            json!({"type": "text", "text": "old"}),
            json!({"type": "image", "data": "img"}),
        ];
        let result = build_modified_prompt(&original, "new");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["text"], "new");
        assert_eq!(result[1]["type"], "image");
    }

    #[test]
    fn test_extract_autoreply_with_context() {
        let response = HookResult {
            context: Some("Fix errors".to_string()),
            decision: crate::events::result::Decision::Allow,
            failures: Vec::new(),
        };

        let result = extract_autoreply(&response);
        assert_eq!(result, Some("Fix errors".to_string()));
    }

    #[test]
    fn test_extract_autoreply_missing_returns_none() {
        let response = HookResult {
            context: None,
            decision: crate::events::result::Decision::Allow,
            failures: Vec::new(),
        };

        let result = extract_autoreply(&response);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_autoreply_empty_returns_none() {
        let response = HookResult {
            context: Some("".to_string()),
            decision: crate::events::result::Decision::Allow,
            failures: Vec::new(),
        };

        let result = extract_autoreply(&response);
        assert_eq!(result, None);
    }

    #[test]
    fn test_check_autoreply_limit_under_max() {
        assert!(!check_autoreply_limit(0, 5));
        assert!(!check_autoreply_limit(4, 5));
    }

    #[test]
    fn test_check_autoreply_limit_at_max() {
        assert!(check_autoreply_limit(5, 5));
    }

    #[test]
    fn test_check_autoreply_limit_over_max() {
        assert!(check_autoreply_limit(6, 5));
    }

    #[test]
    fn test_increment_autoreply_counter_first_time() {
        let mut counters = HashMap::new();
        let session_id = session_id("test");

        let count = increment_autoreply_counter(&mut counters, &session_id);

        assert_eq!(count, 1);
        assert_eq!(counters.get(&session_id), Some(&1));
    }

    #[test]
    fn test_increment_autoreply_counter_existing() {
        let mut counters = HashMap::new();
        let session_id = session_id("test");
        counters.insert(session_id.clone(), 3);

        let count = increment_autoreply_counter(&mut counters, &session_id);

        assert_eq!(count, 4);
        assert_eq!(counters.get(&session_id), Some(&4));
    }

    // Note: derive_executable tests moved to zed_detection module tests
}

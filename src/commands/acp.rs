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
//! │  │ - Fire PrePrompt     │                    │ - Parse agent msgs   │  │
//! │  │ - Fire PreFileChange │                    │ - Fire PostResponse  │  │
//! │  │ - Forward to agent   │                    │ - Fire PostFileChange│  │
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
//! - Fires `PrePrompt` events (allows flows to inject context)
//! - Forwards messages to agent stdin
//!
//! ## Agent → IDE Thread (State Owner)
//!
//! - **Owns all proxy state** (client info, agent info, cwd, tool call contexts)
//! - Receives metadata updates from IDE→Agent thread via channel
//! - Reads JSON-RPC messages from agent (stdout)
//! - Fires `SessionStart`, `PostResponse`, `PostFileChange` events
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
//! - `AutoreplyChannelMessage` channel: Agent→IDE thread sends autoreplies to forwarder
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
//! 4. Autoreply forwarder thread exits on `AutoreplyChannelMessage::Shutdown`
//! 5. IDE→Agent thread exits when IDE closes stdin (natural EOF on stdin.lock().lines())
//! 6. Main thread joins all threads before exiting
//!
//! Note: IDE→Agent thread shutdown is driven by stdin EOF, not by the Shutdown message,
//! because it's blocked on stdin.lock().lines() and cannot check the metadata channel.
//! This is correct behavior - the thread only needs to exit when the IDE disconnects.
//!
//! # Events Fired
//!
//! - **SessionStart**: When `session/new` response is received with `sessionId`
//! - **PrePrompt**: Before `session/prompt` is forwarded to agent (allows context injection)
//! - **PreFileChange**: Before `session/request_permission` for file-modifying tools
//! - **PostFileChange**: When tool calls complete (from `session/update` notifications)
//! - **PostResponse**: When agent completes a turn (`stopReason: end_turn`)
//!
//! # Example Flow
//!
//! 1. IDE sends `initialize` request → IDE→Agent thread extracts client info
//! 2. Agent responds with `initialize` response → Agent→IDE thread extracts agent info
//! 3. IDE sends `session/new` → IDE→Agent thread tracks request ID
//! 4. Agent responds with `sessionId` → Agent→IDE thread fires `SessionStart` event
//! 5. IDE sends `session/prompt` → IDE→Agent thread fires `PrePrompt` event
//! 6. Agent sends `session/update` chunks → Agent→IDE thread accumulates response text
//! 7. Agent completes turn → Agent→IDE thread fires `PostResponse` event
//! 8. Flow returns autoreply → Agent→IDE thread queues it via autoreply channel
//! 9. Autoreply forwarder sends it to agent stdin
//! 10. Process repeats

use crate::acp::protocol::{
    AgentInfo, ClientInfo, InitializeRequest, InitializeResponse, JsonRpcMessage,
    SessionNotification,
};
use crate::commands::zed_detection;
use crate::error::{AikiError, Result};
use crate::event_bus;
use crate::events::{
    AikiEvent, AikiPostFileChangeEvent, AikiPostResponseEvent, AikiPrePromptEvent, AikiStartEvent,
};
use crate::provenance::AgentType;
use agent_client_protocol::{
    SessionUpdate, ToolCall, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolKind,
};
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
    /// Track session/prompt request for PostResponse event matching
    TrackPrompt {
        request_id: serde_json::Value, // Raw JSON-RPC "id" field (normalized at consumption)
        session_id: SessionId,
    },
    /// Clear response accumulator for a session (on new prompt)
    ClearAccumulator { session_id: SessionId },
    /// Reset autoreply counter for a session (on new user prompt)
    ResetAutoreplyCounter { session_id: SessionId },
    /// Track session/new request ID to match with response for SessionStart event
    TrackNewSession {
        request_id: serde_json::Value, // Raw JSON-RPC "id" field (normalized at consumption)
    },
    /// Signal shutdown when agent process exits
    Shutdown,
}

/// Messages sent through the autoreply channel
#[derive(Debug, Clone)]
enum AutoreplyChannelMessage {
    /// A JSON-RPC autoreply message to be sent to the agent (and optionally to IDE for visibility)
    Autoreply {
        message: AutoreplyMessage,
        /// Whether to also forward this autoreply to the IDE for visibility
        /// (allows user to see the autoreply prompt in the IDE chat history)
        forward_to_ide: bool,
    },
    /// Explicit shutdown signal
    Shutdown,
}

/// A JSON-RPC autoreply message to be sent to the agent
///
/// Stores the structured data for a session/prompt autoreply request.
/// The JSON is generated on-demand when needed.
#[derive(Debug, Clone)]
struct AutoreplyMessage {
    /// The session ID to send the prompt to
    session_id: SessionId,
    /// The text content of the autoreply
    text: String,
    /// The raw request ID string (for JSON serialization)
    raw_request_id: String,
    /// The normalized request ID (for HashMap tracking)
    normalized_request_id: JsonRpcId,
}

impl AutoreplyMessage {
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
    // Agent→IDE thread detects PostResponse and sends autoreply requests
    // IDE→Agent thread receives and forwards them to agent
    let (autoreply_tx, autoreply_rx) = mpsc::channel::<AutoreplyChannelMessage>();

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
                AutoreplyChannelMessage::Autoreply {
                    message: autoreply_msg,
                    forward_to_ide,
                } => {
                    // Generate JSON on-demand
                    let json = match autoreply_msg.as_json() {
                        Ok(j) => j,
                        Err(e) => {
                            eprintln!("Warning: Failed to serialize autoreply: {}", e);
                            break;
                        }
                    };

                    // Forward to IDE first (if requested) so it sees the prompt before the response
                    if forward_to_ide {
                        if let Err(e) = writeln!(io::stdout(), "{}", json) {
                            eprintln!("Warning: Failed to forward autoreply to IDE: {}", e);
                        } else if let Err(e) = io::stdout().flush() {
                            eprintln!("Warning: Failed to flush autoreply to IDE: {}", e);
                        } else if std::env::var("AIKI_DEBUG").is_ok() {
                            eprintln!("[acp] Forwarded autoreply to IDE for visibility");
                        }
                    }

                    // Always send to agent
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
                    if std::env::var("AIKI_DEBUG").is_ok() {
                        eprintln!("[acp] Sent autoreply to agent: {} bytes", json.len());
                    }
                }
                AutoreplyChannelMessage::Shutdown => {
                    if std::env::var("AIKI_DEBUG").is_ok() {
                        eprintln!("ACP Proxy: Autoreply thread received shutdown signal");
                    }
                    break;
                }
            }
        }

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("ACP Proxy: Autoreply forwarder thread exiting");
        }
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

        // Track cwd in this thread for PrePrompt events
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

                                    if std::env::var("AIKI_DEBUG").is_ok() {
                                        eprintln!(
                                            "ACP Proxy: Set working directory to: {}",
                                            cwd_str
                                        );
                                    }
                                }
                            }

                            // Track session/new request for SessionStart event
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

                                    if std::env::var("AIKI_DEBUG").is_ok() {
                                        eprintln!(
                                            "ACP Proxy: Set working directory to: {}",
                                            cwd_str
                                        );
                                    }
                                }
                            }
                        }
                        "session/prompt" => {
                            // PrePrompt event: intercept and potentially modify prompt
                            if let Some(params) = &msg.params {
                                // Signal Agent→IDE thread to clear response accumulator and reset autoreply counter
                                // This ensures we start fresh for each new prompt, preventing concatenation
                                // of old text if the previous turn ended without end_turn (error, cancel, etc.)
                                // Also resets autoreply counter per turn (not permanently after 5 total)
                                // Extract sessionId directly from params (session/prompt doesn't have 'update' field)
                                let session_id_str = params
                                    .get("sessionId")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();

                                if !session_id_str.is_empty() {
                                    let session_id = session_id(session_id_str);
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
                            if std::env::var("AIKI_DEBUG").is_ok() {
                                eprintln!("ACP Proxy: Forwarding authenticate request to agent");
                            }
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
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("ACP Proxy: IDE stdin closed, stopping IDE → Agent thread");
        }
        Ok(())
    });

    // Thread 2: Agent → IDE (observe and record)
    // This thread OWNS all metadata state and receives updates via channel
    let mut client_info: Option<ClientInfo> = None;
    let mut agent_info: Option<AgentInfo> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut tool_call_contexts: HashMap<ToolCallId, ToolCallContext> = HashMap::new();

    // Track prompt requests for PostResponse event
    // Key is JsonRpcId (normalized request_id), value is session_id
    let mut prompt_requests: HashMap<JsonRpcId, SessionId> = HashMap::new();

    // Track session/new requests for SessionStart event
    // Key is JsonRpcId (normalized request_id), value is boolean (true = pending)
    let mut session_new_requests: HashMap<JsonRpcId, bool> = HashMap::new();

    // Track autoreply counters per session (not global)
    let mut autoreply_counters: HashMap<SessionId, usize> = HashMap::new();
    const MAX_AUTOREPLIES: usize = 5;

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
                        autoreply_counters.remove(&session_id);
                        if std::env::var("AIKI_DEBUG").is_ok() {
                            eprintln!("[acp] Reset autoreply counter for session: {}", session_id);
                        }
                    }
                    StateMessage::TrackNewSession { request_id } => {
                        // Track session/new request to match with response
                        session_new_requests.insert(JsonRpcId::from_value(&request_id), true);
                    }
                    StateMessage::Shutdown => {
                        // Explicit shutdown signal - exit the loop
                        if std::env::var("AIKI_DEBUG").is_ok() {
                            eprintln!("ACP Proxy: Agent→IDE thread received shutdown signal");
                        }
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

                        // Check for session/new response (SessionStart event)
                        if let Some(response_id) = &msg.id {
                            let request_id = JsonRpcId::from_value(response_id);
                            if session_new_requests.remove(&request_id).is_some() {
                                // This is a session/new response
                                if let Some(session_id) =
                                    result.get("sessionId").and_then(|v| v.as_str())
                                {
                                    // Fire SessionStart event
                                    if let Err(e) = fire_session_start_event(
                                        session_id,
                                        &validated_agent_type,
                                        &cwd,
                                    ) {
                                        eprintln!(
                                            "Warning: Failed to fire SessionStart event: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }

                        // Check for stopReason (turn completion)
                        if let Some(stop_reason) = result.get("stopReason").and_then(|v| v.as_str())
                        {
                            // PostResponse event: agent finished responding
                            if stop_reason == "end_turn" {
                                if let Some(response_id) = &msg.id {
                                    // Normalize the response ID for HashMap lookup
                                    let request_id = JsonRpcId::from_value(response_id);

                                    // Look up session_id from the original request
                                    if let Some(session_id) = prompt_requests.remove(&request_id) {
                                        // Get accumulated response text for this session
                                        let response_text = response_accumulator
                                            .remove(&session_id)
                                            .unwrap_or_default();

                                        // Fire PostResponse event
                                        if let Err(e) = handle_post_response(
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
                                                "Warning: Failed to handle PostResponse: {}",
                                                e
                                            );
                                        }
                                    } else if std::env::var("AIKI_DEBUG").is_ok() {
                                        eprintln!(
                                            "[acp] Detected stopReason but no matching request_id: {:?}",
                                            response_id
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(method) = &msg.method {
                    // Handle session/request_permission - fire PreFileChange for file-modifying tools
                    if method == "session/request_permission" {
                        if is_file_modifying_permission_request(&msg) {
                            // Extract session_id from params
                            if let Some(params) = &msg.params {
                                if let Some(session_id) =
                                    params.get("sessionId").and_then(|v| v.as_str())
                                {
                                    // Fire PreFileChange event BEFORE forwarding permission request to IDE
                                    if let Err(e) = fire_pre_file_change_event(
                                        session_id,
                                        &validated_agent_type,
                                        &cwd,
                                    ) {
                                        eprintln!(
                                            "Warning: Failed to fire PreFileChange event: {}",
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

                                // Check if this is an agent_message_chunk with text content
                                if let Some(update_obj) =
                                    params.get("update").and_then(|v| v.as_object())
                                {
                                    if update_obj.get("type").and_then(|v| v.as_str())
                                        == Some("agent_message_chunk")
                                    {
                                        if let Some(content) =
                                            update_obj.get("content").and_then(|v| v.as_object())
                                        {
                                            if let Some(text) =
                                                content.get("text").and_then(|v| v.as_str())
                                            {
                                                // Accumulate response text per session
                                                // Pre-allocate 4KB capacity to reduce reallocations
                                                response_accumulator
                                                    .entry(Arc::clone(&session_id))
                                                    .or_insert_with(|| String::with_capacity(4096))
                                                    .push_str(text);
                                            }
                                        }
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

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("ACP Proxy: Agent stdout closed, stopping Agent → IDE thread");
    }

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
    let _ = autoreply_tx.send(AutoreplyChannelMessage::Shutdown);
    let _ = metadata_tx.send(StateMessage::Shutdown);

    // ALWAYS join the IDE → Agent thread to ensure clean shutdown
    // Join threads in reverse dependency order to ensure graceful shutdown:
    // 1. IDE→Agent thread (may still be sending autoreplies)
    // 2. Autoreply forwarder thread (drains final messages)
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

    // ALWAYS join the autoreply forwarder thread to ensure clean shutdown
    match autoreply_thread.join() {
        Ok(Ok(())) => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("ACP Proxy: Autoreply forwarder thread exited cleanly");
            }
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
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("ACP Proxy: Exiting with code {}", exit_code);
    }
    std::process::exit(exit_code);
}

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

    // Extract client info fields (optional)
    let client_name = client_info.as_ref().map(|c| c.name.clone());
    let client_version = client_info.as_ref().and_then(|c| c.version.clone());

    // Extract agent version (optional)
    let agent_version = agent_info.as_ref().and_then(|a| a.version.clone());

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

    // Create and dispatch a single event for all affected files
    let event = AikiEvent::PostFileChange(AikiPostFileChangeEvent {
        agent_type: *agent_type,
        client_name: client_name.clone(),
        client_version: client_version.clone(),
        agent_version: agent_version.clone(),
        session_id: session_id.to_string(),
        tool_name: tool_name.clone(),
        file_paths,
        cwd: working_dir.clone(),
        timestamp: chrono::Utc::now(),
        detection_method: crate::provenance::DetectionMethod::ACP,
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

    if std::env::var("AIKI_DEBUG").is_ok() && !edit_details.is_empty() {
        eprintln!(
            "[acp] Extracted {} edit details from tool call content",
            edit_details.len()
        );
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
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!(
                    "[acp] Found permission request for tool_call_id: {:?}",
                    tool_call_id
                );
            }
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

/// Handle session/prompt request and fire PrePrompt event
///
/// This intercepts the user's prompt, fires a PrePrompt event, and potentially
/// modifies the prompt before forwarding to the agent. Implements graceful
/// degradation - on any error, forwards the original message.
fn handle_session_prompt(
    agent_stdin: &Arc<Mutex<std::process::ChildStdin>>,
    msg: &JsonRpcMessage,
    params: &serde_json::Value,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
    metadata_tx: &mpsc::Sender<StateMessage>,
) -> Result<()> {
    use serde_json::json;

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

    // Get working directory with fallback
    let working_dir = cwd
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Fire PrePrompt event
    let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        original_prompt: original_text.clone(),
    });

    let response = event_bus::dispatch(event)?;

    // Extract modified_prompt from metadata (use original if not found)
    let modified_prompt = response
        .metadata
        .iter()
        .find(|(k, _)| k == "modified_prompt")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| original_text.clone());

    // Modify the JSON params to replace prompt text
    let mut modified_msg = msg.clone();
    if let Some(params_mut) = modified_msg.params.as_mut() {
        if let Some(params_obj) = params_mut.as_object_mut() {
            if let Some(prompt_arr) = params_obj.get_mut("prompt").and_then(|v| v.as_array_mut()) {
                // Find first text item and replace it
                for item in prompt_arr.iter_mut() {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        if let Some(item_obj) = item.as_object_mut() {
                            item_obj.insert("text".to_string(), json!(modified_prompt));
                            break;
                        }
                    }
                }
            }
        }
    }

    // Send metadata about this prompt request for PostResponse tracking
    if let Some(request_id) = msg.id.clone() {
        let _ = metadata_tx.send(StateMessage::TrackPrompt {
            request_id, // Pass raw Value; normalization happens at consumption
            session_id: Arc::clone(&session_id),
        });
    }

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

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[acp] Fired PrePrompt event for session: {}, modified: {}",
            session_id,
            modified_prompt != original_text
        );
    }

    Ok(())
}

/// Handle PostResponse event and autoreply
///
/// Fires when the agent completes a turn (stopReason: end_turn).
/// Dispatches PostResponse event to flows, and if they return an autoreply,
/// sends it back to the agent (up to MAX_AUTOREPLIES times per session).
fn handle_post_response(
    session_id: &SessionId,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
    response_text: &str,
    autoreply_counters: &mut HashMap<SessionId, usize>,
    max_autoreplies: usize,
    autoreply_tx: &mpsc::Sender<AutoreplyChannelMessage>,
    prompt_requests: &mut HashMap<JsonRpcId, SessionId>,
) -> Result<()> {
    // Get working directory with fallback
    let working_dir = cwd
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Fire PostResponse event with accumulated response text
    let event = AikiEvent::PostResponse(AikiPostResponseEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        response: response_text.to_string(),
        modified_files: Vec::new(), // Files tracked separately via PostFileChange events
    });

    let response = event_bus::dispatch(event)?;

    // Check for autoreply in metadata
    let autoreply = response
        .metadata
        .iter()
        .find(|(k, _)| k == "autoreply")
        .map(|(_, v)| v.clone());

    if let Some(autoreply_text) = autoreply {
        // Get current autoreply count for this session
        let current_count = autoreply_counters.get(session_id).copied().unwrap_or(0);

        if !autoreply_text.is_empty() && current_count < max_autoreplies {
            // Increment counter for this session
            let new_count = current_count + 1;
            autoreply_counters.insert(Arc::clone(session_id), new_count);

            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!(
                    "[acp] PostResponse autoreply #{} for session {}: {} chars",
                    new_count,
                    session_id,
                    autoreply_text.len()
                );
            }

            // Create autoreply message (JSON generated on-demand when sent)
            let autoreply_msg = AutoreplyMessage::new(session_id, autoreply_text, new_count);

            // Extract debug info before moving
            let debug_request_id = if std::env::var("AIKI_DEBUG").is_ok() {
                Some(autoreply_msg.raw_request_id_display().to_string())
            } else {
                None
            };

            // ✅ FIX for Issue #2: Insert into HashMap BEFORE sending to channel
            // This prevents a race condition where the agent responds before we've
            // registered the request ID, causing the PostResponse event to be lost.
            // The correct order is: prepare state first, then trigger the action.
            prompt_requests.insert(
                autoreply_msg.normalized_request_id().clone(),
                Arc::clone(session_id),
            );

            // Send via channel to autoreply forwarder thread
            // forward_to_ide=true ensures the IDE sees the autoreply prompt in chat history
            autoreply_tx
                .send(AutoreplyChannelMessage::Autoreply {
                    message: autoreply_msg,
                    forward_to_ide: true, // Make autoreplies visible to user
                })
                .map_err(|e| {
                    AikiError::Other(anyhow::anyhow!("Failed to send autoreply: {}", e))
                })?;

            if let Some(request_id) = debug_request_id {
                eprintln!(
                    "[acp] Queued autoreply #{} for session: {} with request_id: {}",
                    new_count, session_id, request_id
                );
            }
        } else if current_count >= max_autoreplies {
            eprintln!(
                "Warning: Maximum autoreplies ({}) reached for session {}, ignoring autoreply from flow",
                max_autoreplies, session_id
            );
        }
    } else if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[acp] Fired PostResponse event for session: {}, no autoreply",
            session_id
        );
    }

    Ok(())
}

/// Fire PreFileChange event before file-modifying tool executes
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

    // Create and dispatch SessionStart event
    let event = AikiEvent::SessionStart(AikiStartEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("Warning: SessionStart event bus dispatch failed: {}", e);
    } else if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[acp] Fired SessionStart event for session: {}", session_id);
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

    // Create and dispatch PreFileChange event
    let event = AikiEvent::PreFileChange(crate::events::AikiPreFileChangeEvent {
        agent_type: *agent_type,
        session_id: session_id.to_string(),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("Warning: PreFileChange event bus dispatch failed: {}", e);
    } else if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[acp] Fired PreFileChange event for session: {}",
            session_id
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let msg = AutoreplyMessage::new(&sid, "Fix the errors".to_string(), 1);

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
        let msg1 = AutoreplyMessage::new(&sid1, "text1".to_string(), 1);
        let msg2 = AutoreplyMessage::new(&sid1, "text2".to_string(), 2);
        let msg3 = AutoreplyMessage::new(&sid2, "text3".to_string(), 1);

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

    // Note: derive_executable tests moved to zed_detection module tests
}

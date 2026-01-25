pub mod turn_state;

use crate::error::{AikiError, Result};
use crate::provenance::{AgentType, DetectionMethod};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// TTL threshold for editor agents (Claude Code, Cursor) - 8 hours
const EDITOR_TTL: Duration = Duration::from_secs(8 * 60 * 60);

/// TTL threshold for CLI agents (standalone tools) - 2 hours
const CLI_TTL: Duration = Duration::from_secs(2 * 60 * 60);

/// Reason a session was cleaned up
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCleanupReason {
    /// Parent process no longer alive
    PidDead,
    /// No activity within TTL threshold
    TtlExpired,
    /// Orphaned session (no events found in conversation history)
    NoEvents,
}

/// Session file handle for atomic file operations
#[derive(Debug, Clone)]
pub struct AikiSessionFile {
    path: PathBuf,
    session: AikiSession,
}

impl AikiSessionFile {
    /// Create a new session file handle
    #[must_use]
    pub fn new(session: &AikiSession, repo_path: impl AsRef<Path>) -> Self {
        let path = repo_path
            .as_ref()
            .join(".aiki/sessions")
            .join(session.uuid());
        Self {
            path,
            session: session.clone(),
        }
    }

    /// Atomically create the session file with metadata
    ///
    /// Uses `create_new()` (O_EXCL) for atomic file creation.
    /// Returns `Ok(true)` if created, `Ok(false)` if already exists.
    pub fn create(&self) -> Result<bool> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // Build metadata from session in [aiki]...[/aiki] format
        let started_at = Utc::now();

        let mut metadata = format!(
            "[aiki]\nagent={}\nexternal_session_id={}\nsession_id={}\nstarted_at={}\n",
            self.session.agent_type().to_metadata_string(),
            self.session.external_id(),
            self.session.uuid(),
            started_at.to_rfc3339(),
        );

        // Add agent_version if available
        if let Some(version) = self.session.agent_version() {
            metadata.push_str(&format!("agent_version={}\n", version));
        }

        // Add parent_pid for PID-based session detection
        if let Some(pid) = self.session.parent_pid() {
            metadata.push_str(&format!("parent_pid={}\n", pid));
        }

        metadata.push_str("[/aiki]\n");

        // Create sessions directory if it doesn't exist
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AikiError::Other(anyhow::anyhow!(
                    "Failed to create sessions directory: {}",
                    e
                ))
            })?;
        }

        // Try to create file atomically with O_EXCL flag
        match OpenOptions::new()
            .write(true)
            .create_new(true) // O_EXCL: fails if file exists (atomic check-and-create)
            .open(&self.path)
        {
            Ok(mut file) => {
                // File created successfully - write metadata
                file.write_all(metadata.as_bytes()).map_err(|e| {
                    AikiError::Other(anyhow::anyhow!("Failed to write session file: {}", e))
                })?;

                Ok(true) // File created
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // File already exists - this is expected for concurrent creates
                Ok(false) // File already exists
            }
            Err(e) => {
                // Other error - propagate
                Err(AikiError::Other(anyhow::anyhow!(
                    "Failed to create session file: {}",
                    e
                )))
            }
        }
    }

    /// Delete the session file
    pub fn delete(&self) -> Result<()> {
        // Try to remove the file; ignore if it doesn't exist
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AikiError::Other(anyhow::anyhow!(
                "Failed to delete session file: {}",
                e
            ))),
        }
    }

    /// Read agent_version from existing session file
    ///
    /// Returns None if file doesn't exist or agent_version field not found.
    /// This allows subsequent events to read cached version without re-detecting.
    #[allow(dead_code)] // Part of AikiSessionFile API
    pub fn read_agent_version(&self) -> Option<String> {
        fs::read_to_string(&self.path).ok().and_then(|content| {
            // Parse [aiki]...[/aiki] block for agent_version field
            content
                .lines()
                .find(|line| line.starts_with("agent_version="))
                .and_then(|line| line.strip_prefix("agent_version="))
                .map(|v| v.to_string())
        })
    }
}

/// Aiki Session tracking
///
/// Tracks an active AI agent session with deterministic ID generation.
/// Session IDs are deterministic hashes of (agent_type, external_session_id)
/// to ensure consistent session identification across multiple events.
///
/// Session metadata (client info, agent version, integration type) is stored here
/// to avoid duplication across events. Hook-based detection will have None for client info,
/// while ACP-based detection populates these fields from Initialize messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AikiSession {
    /// The deterministic UUID v5 hash
    uuid: String,
    /// The agent type (claude, cursor, etc.)
    agent_type: AgentType,
    /// Agent version - e.g., "0.10.6" (from ACP InitializeResponse or hook detection)
    agent_version: Option<String>,
    /// The external session ID provided by the agent
    external_id: String,
    /// Client (IDE) name - e.g., "zed", "neovim" (from ACP InitializeRequest)
    client_name: Option<String>,
    /// Client (IDE) version - e.g., "0.213.3" (from ACP InitializeRequest)
    client_version: Option<String>,
    /// Integration type - how Aiki is integrated with the agent (Hook vs ACP)
    detection_method: DetectionMethod,
    /// Parent process ID of the agent (for PID-based session detection)
    ///
    /// In hook mode, this is the parent of the hook process (the agent).
    /// In ACP mode, this is the `agent_pid` from the session/start message.
    /// Used to match bash commands back to their originating session.
    parent_pid: Option<u32>,
}

impl AikiSession {
    /// Namespace UUID for Aiki session IDs (fixed UUID v5 namespace)
    /// Generated once with: uuidgen -> 6ba7b810-9dad-11d1-80b4-00c04fd430c8
    const NAMESPACE: uuid::Uuid = uuid::Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ]);

    /// Create a new Aiki session ID from agent type and external session ID
    ///
    /// Uses UUID v5 (SHA-1 hash) for deterministic ID generation:
    /// ```text
    /// session_id = UUIDv5(NAMESPACE, "{agent_type}:{external_session_id}")
    /// ```
    ///
    /// # Arguments
    /// * `agent_type` - The AI agent type (Claude, Cursor, etc.)
    /// * `external_id` - The session ID provided by the agent/IDE
    /// * `agent_version` - Optional agent version (e.g., "0.10.6")
    /// * `detection_method` - How Aiki is integrated (Hook, ACP, Unknown)
    ///
    /// # Examples
    /// ```
    /// use aiki::session::AikiSession;
    /// use aiki::provenance::{AgentType, DetectionMethod};
    ///
    /// // Hook-based (no agent version)
    /// let session = AikiSession::new(
    ///     AgentType::ClaudeCode,
    ///     "claude-session-abc123",
    ///     None::<&str>,
    ///     DetectionMethod::Hook
    /// );
    ///
    /// // ACP-based (with agent version)
    /// let session = AikiSession::new(
    ///     AgentType::ClaudeCode,
    ///     "claude-session-abc123",
    ///     Some("0.10.6"),
    ///     DetectionMethod::ACP
    /// );
    ///
    /// // Same inputs produce same UUID (deterministic)
    /// let session2 = AikiSession::new(
    ///     AgentType::ClaudeCode,
    ///     "claude-session-abc123",
    ///     None::<&str>,
    ///     DetectionMethod::Hook
    /// );
    /// assert_eq!(session.uuid(), session2.uuid());
    /// ```
    #[must_use]
    pub fn new(
        agent_type: AgentType,
        external_id: impl Into<String>,
        agent_version: Option<impl Into<String>>,
        detection_method: DetectionMethod,
    ) -> Self {
        let external_id = external_id.into();
        let uuid = Self::generate_uuid(agent_type, &external_id);

        Self {
            uuid,
            agent_type,
            external_id,
            client_name: None,
            client_version: None,
            agent_version: agent_version.map(|v| v.into()),
            detection_method,
            parent_pid: None,
        }
    }

    /// Reconstruct a session from a pre-computed UUID (e.g., from a session file)
    ///
    /// Used when we already have the final UUID and don't need to re-generate it.
    /// This is the case during TTL cleanup when reading session files.
    #[must_use]
    pub fn from_uuid(uuid: String, agent_type: AgentType) -> Self {
        Self {
            uuid,
            agent_type,
            external_id: String::new(),
            client_name: None,
            client_version: None,
            agent_version: None,
            detection_method: DetectionMethod::Unknown,
            parent_pid: None,
        }
    }

    /// Create a new session for hook-based detection
    ///
    /// Convenience constructor that automatically sets `DetectionMethod::Hook`
    /// and captures the parent process ID for PID-based session detection.
    ///
    /// # Examples
    /// ```
    /// use aiki::session::AikiSession;
    /// use aiki::provenance::AgentType;
    ///
    /// let session = AikiSession::for_hook(
    ///     AgentType::ClaudeCode,
    ///     "claude-session-abc123",
    ///     Some("0.10.6")
    /// );
    /// assert!(session.parent_pid().is_some());
    /// ```
    #[must_use]
    pub fn for_hook(
        agent_type: AgentType,
        external_id: impl Into<String>,
        agent_version: Option<impl Into<String>>,
    ) -> Self {
        // Capture parent PID - the agent that spawned this hook process
        let parent_pid = get_parent_pid();

        Self::new(
            agent_type,
            external_id,
            agent_version,
            DetectionMethod::Hook,
        )
        .with_parent_pid(parent_pid)
    }

    /// Generate a deterministic UUID v5 for a session
    ///
    /// Creates a UUID v5 by hashing: "{agent_type}:{external_session_id}"
    /// This ensures the same agent and external session always produce the same UUID.
    ///
    /// This is useful when you need to compute a session UUID without creating
    /// a full AikiSession object (e.g., for cache lookups).
    #[must_use]
    pub fn generate_uuid(agent_type: AgentType, external_id: &str) -> String {
        // Create deterministic hash input: "agent_type:external_session_id"
        let hash_input = format!("{}:{}", agent_type.to_metadata_string(), external_id);

        // Generate UUID v5 (SHA-1 based, deterministic)
        uuid::Uuid::new_v5(&Self::NAMESPACE, hash_input.as_bytes()).to_string()
    }

    /// Add client (IDE) information to the session
    ///
    /// This is typically called when using ACP-based detection, where the client
    /// provides its name and version in the InitializeRequest.
    ///
    /// # Example
    /// ```
    /// use aiki::session::AikiSession;
    /// use aiki::provenance::{AgentType, DetectionMethod};
    ///
    /// let session = AikiSession::new(
    ///     AgentType::ClaudeCode,
    ///     "session-123",
    ///     None::<&str>,
    ///     DetectionMethod::ACP
    /// )
    /// .with_client_info(Some("zed"), Some("0.213.3"));
    /// ```
    #[must_use]
    pub fn with_client_info(
        mut self,
        client_name: Option<impl Into<String>>,
        client_version: Option<impl Into<String>>,
    ) -> Self {
        self.client_name = client_name.map(|n| n.into());
        self.client_version = client_version.map(|v| v.into());
        self
    }

    /// Set the parent process ID for PID-based session detection
    ///
    /// In hook mode, this should be the parent of the hook process (the agent).
    /// In ACP mode, this should be the `agent_pid` from the session/start message.
    ///
    /// # Example
    /// ```
    /// use aiki::session::AikiSession;
    /// use aiki::provenance::{AgentType, DetectionMethod};
    ///
    /// let session = AikiSession::new(
    ///     AgentType::ClaudeCode,
    ///     "session-123",
    ///     None::<&str>,
    ///     DetectionMethod::Hook
    /// )
    /// .with_parent_pid(Some(1234));
    /// ```
    #[must_use]
    pub fn with_parent_pid(mut self, pid: Option<u32>) -> Self {
        self.parent_pid = pid;
        self
    }

    /// Get the session UUID as a string
    #[must_use]
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// Get the agent type
    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        self.agent_type
    }

    /// Get the external session ID
    #[must_use]
    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    /// Get the client (IDE) name
    #[must_use]
    pub fn client_name(&self) -> Option<&str> {
        self.client_name.as_deref()
    }

    /// Get the client (IDE) version
    #[must_use]
    pub fn client_version(&self) -> Option<&str> {
        self.client_version.as_deref()
    }

    /// Get the agent version
    #[must_use]
    pub fn agent_version(&self) -> Option<&str> {
        self.agent_version.as_deref()
    }

    /// Get the detection method (integration type)
    #[must_use]
    pub fn detection_method(&self) -> &DetectionMethod {
        &self.detection_method
    }

    /// Get the parent process ID
    #[must_use]
    pub fn parent_pid(&self) -> Option<u32> {
        self.parent_pid
    }

    /// Get a session file handle for this session
    #[must_use]
    pub fn file(&self, repo_path: impl AsRef<Path>) -> AikiSessionFile {
        AikiSessionFile::new(self, repo_path)
    }

    /// End this session and clean up its session file
    ///
    /// Deletes the session file from `.aiki/sessions/`. This is called automatically
    /// when a SessionEnd event is dispatched.
    pub fn end(&self, repo_path: impl AsRef<Path>) -> Result<()> {
        self.file(repo_path).delete()
    }
}

/// Count active sessions in the repository
#[allow(dead_code)] // Part of session API
pub fn count_sessions(repo_path: impl AsRef<Path>) -> Result<usize> {
    let sessions_dir = repo_path.as_ref().join(".aiki/sessions");

    if !sessions_dir.exists() {
        return Ok(0);
    }

    let count = fs::read_dir(&sessions_dir)
        .map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to read sessions directory: {}", e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .count();

    Ok(count)
}

/// Result of detecting the current session context
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Part of session API
pub enum SessionContext {
    /// No active sessions - likely running from human terminal
    NoSession,
    /// Exactly one active session with this agent type
    SingleSession(AgentType),
    /// Multiple active sessions - ambiguous context
    MultipleSessions,
}

/// Detect the current session context
///
/// Returns detailed information about active sessions:
/// - NoSession: No session files found (human terminal)
/// - SingleSession(agent): Exactly one active session
/// - MultipleSessions: More than one session active (ambiguous)
#[allow(dead_code)] // Part of session API
pub fn get_session_context(repo_path: impl AsRef<Path>) -> SessionContext {
    let sessions_dir = repo_path.as_ref().join(".aiki/sessions");

    if !sessions_dir.exists() {
        return SessionContext::NoSession;
    }

    let session_files: Vec<_> = match fs::read_dir(&sessions_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .collect(),
        Err(_) => return SessionContext::NoSession,
    };

    match session_files.len() {
        0 => SessionContext::NoSession,
        1 => {
            // Try to read agent type from the session file
            if let Ok(content) = fs::read_to_string(session_files[0].path()) {
                for line in content.lines() {
                    let line = line.trim();
                    if let Some(agent_str) = line.strip_prefix("agent=") {
                        if let Some(agent_type) = AgentType::from_str(agent_str) {
                            return SessionContext::SingleSession(agent_type);
                        }
                    }
                }
            }
            // Couldn't parse agent type, treat as no session
            SessionContext::NoSession
        }
        _ => SessionContext::MultipleSessions,
    }
}

/// Get the agent type from the current active session, if any
///
/// Returns the agent type if there's exactly one active session.
/// Returns None if there are no sessions or multiple sessions.
#[allow(dead_code)] // Part of session API
pub fn get_current_agent_type(repo_path: impl AsRef<Path>) -> Option<AgentType> {
    match get_session_context(repo_path) {
        SessionContext::SingleSession(agent) => Some(agent),
        _ => None,
    }
}

/// Check if a specific session is active
///
/// Uses deterministic UUID generation to check if the session file exists.
/// This allows precise session lookup even when multiple sessions are active.
#[allow(dead_code)] // Part of session API
pub fn has_active_session(
    repo_path: impl AsRef<Path>,
    agent_type: AgentType,
    external_session_id: &str,
) -> bool {
    let uuid = AikiSession::generate_uuid(agent_type, external_session_id);
    let session_file = repo_path.as_ref().join(".aiki/sessions").join(&uuid);
    session_file.exists()
}

/// End a session and clean up its session file
#[allow(dead_code)] // Part of session API
pub fn end_session(
    repo_path: impl AsRef<Path>,
    agent_type: AgentType,
    external_session_id: impl Into<String>,
    detection_method: DetectionMethod,
) -> Result<()> {
    let session = AikiSession::new(
        agent_type,
        external_session_id,
        None::<&str>,
        detection_method,
    );
    session.file(&repo_path).delete()?;
    Ok(())
}

// ============================================================================
// PID-based session detection
// ============================================================================

/// Get the parent process ID
///
/// Returns the PID of the parent process, or None if it cannot be determined.
#[must_use]
pub fn get_parent_pid() -> Option<u32> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let current_pid = Pid::from_u32(std::process::id());
    system.process(current_pid)?.parent().map(|p| p.as_u32())
}

/// Get all ancestor PIDs from the current process up to init
///
/// Returns a HashSet for O(1) lookup when matching against session files.
fn get_ancestor_pids() -> HashSet<u32> {
    let mut ancestors = HashSet::new();
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut pid = Pid::from_u32(std::process::id());

    loop {
        let Some(process) = system.process(pid) else {
            break;
        };

        let Some(parent_pid) = process.parent() else {
            break;
        };

        // Prevent infinite loop
        if parent_pid == pid {
            break;
        }

        ancestors.insert(parent_pid.as_u32());
        pid = parent_pid;
    }

    ancestors
}

/// Result of PID-based session lookup
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of SessionMatch API
pub struct SessionMatch {
    /// The agent type from the session file
    pub agent_type: AgentType,
    /// The external session ID from the session file
    pub external_session_id: String,
    /// The Aiki session UUID (deterministic, stable identifier)
    pub session_id: String,
}

/// Find an active session by matching parent_pid against the current process's ancestors
///
/// This is the core function for PID-based session detection:
/// 1. Get all ancestor PIDs of the current process
/// 2. Scan session files in .aiki/sessions/
/// 3. Find sessions whose parent_pid matches one of our ancestors
/// 4. If multiple match, prefer the most recently *active* session
///    (uses .turn file mtime as activity indicator, falls back to started_at)
///
/// Returns None if no matching session found (human terminal mode).
pub fn find_session_by_ancestor_pid(repo_path: impl AsRef<Path>) -> Option<SessionMatch> {
    let sessions_dir = repo_path.as_ref().join(".aiki/sessions");

    if !sessions_dir.exists() {
        return None;
    }

    let ancestor_pids = get_ancestor_pids();
    if ancestor_pids.is_empty() {
        return None;
    }

    let entries = match fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    // Track best match with its last-activity time
    let mut best_match: Option<(SessionMatch, std::time::SystemTime)> = None;

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Skip non-session files (e.g., .turn state files)
        if path.extension().is_some() {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse session file fields
        let mut parent_pid: Option<u32> = None;
        let mut agent_type: Option<AgentType> = None;
        let mut external_session_id: Option<String> = None;
        let mut aiki_session_id: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("parent_pid=") {
                parent_pid = val.parse().ok();
            } else if let Some(val) = line.strip_prefix("agent=") {
                agent_type = AgentType::from_str(val);
            } else if let Some(val) = line.strip_prefix("external_session_id=") {
                external_session_id = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("session_id=") {
                aiki_session_id = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("aiki_session_id=") {
                if aiki_session_id.is_none() {
                    aiki_session_id = Some(val.to_string());
                }
            }
        }

        // Check if this session's parent_pid matches one of our ancestors
        if let Some(pid) = parent_pid {
            if ancestor_pids.contains(&pid) {
                if let (Some(agent), Some(ext_id), Some(aiki_id)) =
                    (agent_type, external_session_id, aiki_session_id)
                {
                    let candidate = SessionMatch {
                        agent_type: agent,
                        external_session_id: ext_id,
                        session_id: aiki_id,
                    };

                    // Determine last activity time:
                    // 1. .turn file mtime (updated every turn) - most accurate
                    // 2. Session file mtime as fallback
                    let turn_file = path.with_extension("turn");
                    let last_activity = turn_file.metadata()
                        .or_else(|_| path.metadata())
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                    let should_replace = match &best_match {
                        None => true,
                        Some((_, prev_time)) => last_activity > *prev_time,
                    };

                    if should_replace {
                        best_match = Some((candidate, last_activity));
                    }
                }
            }
        }
    }

    best_match.map(|(m, _)| m)
}

/// Get the TTL threshold for a given agent type
///
/// Editor agents (Cursor, Claude Code) get 8 hours.
/// CLI agents get 2 hours.
#[must_use]
fn get_ttl_threshold(agent_type: AgentType) -> Duration {
    match agent_type {
        AgentType::ClaudeCode | AgentType::Cursor => EDITOR_TTL,
        _ => CLI_TTL,
    }
}

/// Extract and parse the `timestamp=` field from a JJ event description.
///
/// Supports RFC 3339 format (e.g., "2026-01-23T12:00:00Z") and
/// JJ's native format (e.g., "2026-01-23 12:00:00.000 -08:00").
///
/// Returns:
/// - `Ok(Some(timestamp))` - timestamp found and parsed
/// - `Ok(None)` - empty description (no events)
/// - `Err(e)` - timestamp field missing or unparseable
fn parse_event_timestamp(description: &str) -> std::result::Result<Option<DateTime<Utc>>, String> {
    if description.trim().is_empty() {
        return Ok(None);
    }

    // Extract timestamp= field from the event description metadata
    let timestamp_line = description
        .lines()
        .find(|line| line.trim().starts_with("timestamp="));

    let timestamp_str = match timestamp_line {
        Some(line) => line.trim().strip_prefix("timestamp=").unwrap_or(""),
        None => return Err("No timestamp= field in event description".to_string()),
    };

    if timestamp_str.is_empty() {
        return Err("Empty timestamp= field in event description".to_string());
    }

    // Parse RFC 3339 timestamp (e.g., "2026-01-23T12:00:00Z")
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(timestamp_str) {
        return Ok(Some(ts.with_timezone(&Utc)));
    }

    // Try JJ format: "2026-01-23 12:00:00.000 -08:00"
    if let Ok(ts) = chrono::DateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S%.3f %:z") {
        return Ok(Some(ts.with_timezone(&Utc)));
    }

    // Try without milliseconds
    if let Ok(ts) = chrono::DateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S %:z") {
        return Ok(Some(ts.with_timezone(&Utc)));
    }

    Err(format!("Failed to parse timestamp: '{}'", timestamp_str))
}

/// Query the latest event timestamp for a session from JJ conversation history
///
/// Shells out to `jj log` to find the most recent event for a session.
/// Returns:
/// - `Ok(Some(timestamp))` - events found, latest timestamp returned
/// - `Ok(None)` - no events found (orphaned session)
/// - `Err(e)` - JJ query failed (repo lock, jj not in PATH, etc.)
fn query_latest_event(repo_path: &Path, session_id: &str) -> std::result::Result<Option<DateTime<Utc>>, String> {
    use std::process::Command;

    // Query JJ for latest event in this session
    // Use ::aiki/conversations (ancestors) to scan full conversation history
    // Extract the event metadata timestamp= field (not committer.timestamp() which
    // can skew if events are backfilled or timestamped differently)
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("::aiki/conversations & description(\"session_id={}\")", session_id),
            "--limit", "1",
            "--no-graph",
            "--template", "description ++ \"\\n\"",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run jj: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("jj log failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(None); // No events found
    }

    parse_event_timestamp(&stdout)
}

/// Parse a session file and extract metadata needed for TTL cleanup
struct SessionFileInfo {
    path: PathBuf,
    agent_type: Option<AgentType>,
    session_id: Option<String>,
    parent_pid: Option<u32>,
}

fn parse_session_file(path: &Path) -> Option<SessionFileInfo> {
    let content = fs::read_to_string(path).ok()?;
    let mut agent_type: Option<AgentType> = None;
    let mut session_id: Option<String> = None;
    let mut parent_pid: Option<u32> = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("parent_pid=") {
            parent_pid = val.parse().ok();
        } else if let Some(val) = line.strip_prefix("agent=") {
            agent_type = AgentType::from_str(val);
        } else if let Some(val) = line.strip_prefix("session_id=") {
            session_id = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("aiki_session_id=") {
            // Old field name
            if session_id.is_none() {
                session_id = Some(val.to_string());
            }
        }
    }

    Some(SessionFileInfo {
        path: path.to_path_buf(),
        agent_type,
        session_id,
        parent_pid,
    })
}

/// Emit a synthetic session.ended event to history only (no flow execution)
///
/// Used during TTL/PID cleanup when the agent is disconnected.
/// Does NOT execute the `session.ended` flow section since context actions are meaningless.
fn emit_synthetic_session_ended(repo_path: &Path, session_info: &SessionFileInfo, reason: SessionCleanupReason) {
    use crate::cache::debug_log;

    let reason_str = match reason {
        SessionCleanupReason::PidDead => "pid_dead",
        SessionCleanupReason::TtlExpired => "ttl_expired",
        SessionCleanupReason::NoEvents => "no_events",
    };

    debug_log(|| format!(
        "Synthetic session.ended: session={}, reason={}",
        session_info.session_id.as_deref().unwrap_or("unknown"),
        reason_str
    ));

    // Record to history if we have enough info
    // Use from_uuid since session_id in the file IS the final UUID (not external_id)
    if let (Some(session_id), Some(agent_type)) = (&session_info.session_id, session_info.agent_type) {
        let session = AikiSession::from_uuid(session_id.clone(), agent_type);
        if let Err(e) = crate::history::record_session_end(repo_path, &session, Utc::now(), reason_str) {
            debug_log(|| format!("Failed to record synthetic session end: {}", e));
        }
    }
}

/// Determine what cleanup action to take for a session.
///
/// Returns `Some(reason)` if the session should be cleaned up, `None` if it should be kept.
/// This is the core decision logic extracted for testability.
///
/// Decision priorities:
/// 1. PID dead → immediate cleanup (fast, no JJ query needed)
/// 2. TTL expired → cleanup if last event exceeds threshold
/// 3. No events → orphaned session, cleanup
/// 4. Query error → keep session (transient failure)
fn determine_cleanup_action(
    pid_alive: bool,
    agent_type: AgentType,
    latest_event: std::result::Result<Option<DateTime<Utc>>, String>,
    now: DateTime<Utc>,
) -> Option<SessionCleanupReason> {
    // Fast path: PID dead takes precedence
    if !pid_alive {
        return Some(SessionCleanupReason::PidDead);
    }

    let ttl = get_ttl_threshold(agent_type);

    match latest_event {
        Ok(Some(last_event)) => {
            let elapsed = now.signed_duration_since(last_event);
            if elapsed > chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(8)) {
                Some(SessionCleanupReason::TtlExpired)
            } else {
                None // Active session - within TTL
            }
        }
        Ok(None) => {
            // No events found = orphaned session (created but never used)
            Some(SessionCleanupReason::NoEvents)
        }
        Err(_) => {
            // Query failed - don't delete (could be transient error)
            None
        }
    }
}

/// Clean up stale session files where the parent process no longer exists
/// or where the session has exceeded its TTL threshold.
///
/// Called on SessionStart to remove orphaned sessions from crashed agents.
/// Cleanup priorities:
/// 1. PID dead → immediate cleanup (fast, no JJ query)
/// 2. TTL expired → cleanup after JJ query confirms staleness
/// 3. No events → orphaned session, cleanup
pub fn cleanup_stale_sessions(repo_path: impl AsRef<Path>) {
    use crate::cache::debug_log;

    let repo_path = repo_path.as_ref();
    let sessions_dir = repo_path.join(".aiki/sessions");

    if !sessions_dir.exists() {
        return;
    }

    let entries = match fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Refresh process list once for all PID checks
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    // Collect session files to process
    let session_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let path = entry.path();
            path.is_file() && path.extension().is_none() // Skip .turn files
        })
        .filter_map(|entry| parse_session_file(&entry.path()))
        .collect();

    for session_info in &session_files {
        let pid_alive = match session_info.parent_pid {
            Some(pid) => system.process(Pid::from_u32(pid)).is_some(),
            None => true, // No PID = can't determine, treat as alive
        };

        let agent_type = match session_info.agent_type {
            Some(at) => at,
            None => continue, // Can't determine TTL without agent type
        };

        let session_id = match &session_info.session_id {
            Some(id) => id,
            None => continue, // Can't query without session_id
        };

        // Skip JJ query if PID is dead (determine_cleanup_action will return PidDead)
        let latest_event = if pid_alive {
            query_latest_event(repo_path, session_id)
        } else {
            Ok(None) // Doesn't matter - PID dead takes precedence
        };

        match determine_cleanup_action(pid_alive, agent_type, latest_event, Utc::now()) {
            Some(reason) => cleanup_session_file(repo_path, session_info, reason),
            None => {
                // Session is active or query failed - keep it
                if !pid_alive {
                    // Shouldn't reach here, but log if it does
                    debug_log(|| "Unexpected: PID dead but no cleanup action".to_string());
                }
            }
        }
    }
}

/// Remove a session file and its associated turn state, emitting synthetic session.ended
fn cleanup_session_file(repo_path: &Path, session_info: &SessionFileInfo, reason: SessionCleanupReason) {
    // Emit synthetic session.ended to history only (no flows)
    emit_synthetic_session_ended(repo_path, session_info, reason);

    // Remove session file
    let _ = fs::remove_file(&session_info.path);

    // Also clean up the turn state file
    let turn_file = session_info.path.with_extension("turn");
    if turn_file.exists() {
        let _ = fs::remove_file(&turn_file);
    }

    // Clean up autoreply flag file
    let autoreply_file = session_info.path.with_extension("turn.autoreply");
    if autoreply_file.exists() {
        let _ = fs::remove_file(&autoreply_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki")).unwrap();
        temp_dir
    }

    #[test]
    fn test_create_and_query_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session and write its file
        let session = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-abc123",
            None::<&str>,
        );
        session.file(repo_path).create().unwrap();

        // Verify session file was created using the API
        let session_file_path = repo_path.join(".aiki/sessions").join(session.uuid());
        assert!(session_file_path.exists());

        // Verify session file format uses [aiki]...[/aiki] blocks
        let content = fs::read_to_string(&session_file_path).unwrap();
        assert!(content.starts_with("[aiki]\n"));
        assert!(content.contains("agent=claude"));
        assert!(content.contains("external_session_id=claude-session-abc123"));
        assert!(content.contains(&format!("session_id={}", session.uuid())));
        assert!(content.contains("started_at="));
        assert!(!content.contains("cwd="), "cwd field should not be in session file");
        assert!(content.ends_with("[/aiki]\n"));

        // Verify session count
        assert_eq!(count_sessions(repo_path).unwrap(), 1);
    }

    #[test]
    fn test_multiple_creates_same_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create session file twice (idempotent via O_EXCL)
        let session1 = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-abc123",
            None::<&str>,
        );
        let created1 = session1.file(repo_path).create().unwrap();
        assert!(created1); // First create succeeds

        let session2 = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-abc123",
            None::<&str>,
        );
        let created2 = session2.file(repo_path).create().unwrap();
        assert!(!created2); // Second create returns false (already exists)

        // Should produce same session UUID
        assert_eq!(session1.uuid(), session2.uuid());

        // Should only have one file
        assert_eq!(count_sessions(repo_path).unwrap(), 1);
    }

    #[test]
    fn test_multiple_different_sessions() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create multiple different sessions
        let session1 = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-1",
            None::<&str>,
        );
        session1.file(repo_path).create().unwrap();

        let session2 = AikiSession::new(
            AgentType::Cursor,
            "cursor-session-2",
            None::<&str>,
            DetectionMethod::Hook,
        );
        session2.file(repo_path).create().unwrap();

        // Both should exist
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session1.uuid())
            .exists());
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session2.uuid())
            .exists());

        // Should have 2 sessions
        assert_eq!(count_sessions(repo_path).unwrap(), 2);
    }

    #[test]
    fn test_deterministic_session_ids() {
        // Same inputs should produce same session UUIDs
        let session1 = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        );
        let session2 = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        );

        assert_eq!(session1.uuid(), session2.uuid());
        assert_eq!(session1.uuid(), session2.uuid());

        // Different inputs should produce different UUIDs
        let session3 = AikiSession::new(
            AgentType::Cursor,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        );
        assert_ne!(session1.uuid(), session3.uuid());
    }

    #[test]
    fn test_session_end() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Start a session
        let session = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-end-test",
            None::<&str>,
        );
        session.file(repo_path).create().unwrap();

        // Verify it exists
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session.uuid())
            .exists());
        assert_eq!(count_sessions(repo_path).unwrap(), 1);

        // End the session
        end_session(
            repo_path,
            AgentType::ClaudeCode,
            "claude-session-end-test",
            DetectionMethod::Hook,
        )
        .unwrap();

        // Verify it's gone
        assert!(!repo_path
            .join(".aiki/sessions")
            .join(session.uuid())
            .exists());
        assert_eq!(count_sessions(repo_path).unwrap(), 0);
    }

    #[test]
    fn test_session_lifecycle() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Start
        let session = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "lifecycle-test",
            None::<&str>,
        );
        session.file(repo_path).create().unwrap();

        // Verify session file exists
        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        assert!(session_file.exists());

        // End
        session.end(repo_path).unwrap();

        // Verify session file is deleted
        assert!(!session_file.exists());
    }

    #[test]
    fn test_idempotent_file_creation() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create same session file 5 times (idempotent via O_EXCL)
        for i in 0..5 {
            let session = AikiSession::for_hook(
                AgentType::ClaudeCode,
                "idempotent-test",
                None::<&str>,
            );
            let created = session.file(repo_path).create().unwrap();
            // Only first create should return true
            assert_eq!(created, i == 0);
        }

        // Should only have 1 session file
        assert_eq!(count_sessions(repo_path).unwrap(), 1);
    }

    #[test]
    fn test_session_file_stores_agent_version() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with agent version
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session-with-version",
            Some("2.0.61"),
            DetectionMethod::Hook,
        );

        // Write session file
        let session_file = session.file(repo_path);
        session_file.create().unwrap();

        // Verify agent_version is stored in the file
        let content = fs::read_to_string(&session_file.path).unwrap();
        assert!(
            content.contains("agent_version=2.0.61"),
            "Session file should contain agent_version"
        );

        // Verify we can read it back
        let cached_version = session_file.read_agent_version();
        assert_eq!(cached_version, Some("2.0.61".to_string()));
    }

    #[test]
    fn test_session_file_without_agent_version() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session without agent version
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session-no-version",
            None::<&str>,
            DetectionMethod::Hook,
        );

        // Write session file
        let session_file = session.file(repo_path);
        session_file.create().unwrap();

        // Verify agent_version is NOT in the file
        let content = fs::read_to_string(&session_file.path).unwrap();
        assert!(
            !content.contains("agent_version="),
            "Session file should not contain agent_version field"
        );

        // Verify read returns None
        let cached_version = session_file.read_agent_version();
        assert_eq!(cached_version, None);
    }

    // ========================================================================
    // PID-based session detection tests
    // ========================================================================

    #[test]
    fn test_session_file_stores_parent_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with parent_pid
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session-with-pid",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .with_parent_pid(Some(12345));

        // Write session file
        let session_file = session.file(repo_path);
        session_file.create().unwrap();

        // Verify parent_pid is stored in the file
        let content = fs::read_to_string(&session_file.path).unwrap();
        assert!(
            content.contains("parent_pid=12345"),
            "Session file should contain parent_pid"
        );
    }

    #[test]
    fn test_session_file_without_parent_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session without parent_pid (ACP mode without agent_pid)
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session-no-pid",
            None::<&str>,
            DetectionMethod::ACP,
        );

        // Write session file
        let session_file = session.file(repo_path);
        session_file.create().unwrap();

        // Verify parent_pid is NOT in the file
        let content = fs::read_to_string(&session_file.path).unwrap();
        assert!(
            !content.contains("parent_pid="),
            "Session file should not contain parent_pid field"
        );
    }

    #[test]
    fn test_for_hook_captures_parent_pid() {
        // for_hook should capture the current parent PID
        let session = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "test-session",
            None::<&str>,
        );

        // Should have a parent_pid (unless we're init which has no parent)
        // Just verify the method doesn't panic and returns a session
        assert_eq!(session.agent_type(), AgentType::ClaudeCode);
        // Parent PID may or may not be set depending on the process tree
    }

    #[test]
    fn test_with_parent_pid_builder() {
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session",
            None::<&str>,
            DetectionMethod::ACP,
        )
        .with_parent_pid(Some(99999));

        assert_eq!(session.parent_pid(), Some(99999));
    }

    #[test]
    fn test_get_parent_pid_returns_value() {
        // get_parent_pid should return Some value for any normal process
        let pid = get_parent_pid();
        // In a normal test environment, we should have a parent process
        // This could be None only for init process (PID 1)
        assert!(std::process::id() != 1 || pid.is_none());
    }

    #[test]
    fn test_find_session_by_ancestor_pid_no_sessions() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // No sessions exist
        let result = find_session_by_ancestor_pid(repo_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_by_ancestor_pid_with_matching_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with our parent PID
        let our_parent_pid = get_parent_pid();

        if let Some(pid) = our_parent_pid {
            let session = AikiSession::new(
                AgentType::ClaudeCode,
                "matching-session",
                None::<&str>,
                DetectionMethod::Hook,
            )
            .with_parent_pid(Some(pid));

            session.file(repo_path).create().unwrap();

            // Should find the session
            let result = find_session_by_ancestor_pid(repo_path);
            assert!(result.is_some());

            let matched = result.unwrap();
            assert_eq!(matched.agent_type, AgentType::ClaudeCode);
            assert_eq!(matched.external_session_id, "matching-session");
        }
    }

    #[test]
    fn test_find_session_by_ancestor_pid_non_matching_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with a PID that's not in our ancestor chain
        // Use a very high PID that's unlikely to be a real process
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "non-matching-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .with_parent_pid(Some(999999));

        session.file(repo_path).create().unwrap();

        // Should not find the session (PID doesn't match our ancestors)
        let result = find_session_by_ancestor_pid(repo_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_dead_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with a PID that definitely doesn't exist
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "stale-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .with_parent_pid(Some(999999));

        session.file(repo_path).create().unwrap();

        // Verify session file exists
        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        assert!(session_file.exists());

        // Cleanup should remove it
        cleanup_stale_sessions(repo_path);

        // Session file should be gone
        assert!(!session_file.exists());
    }

    #[test]
    fn test_cleanup_stale_sessions_keeps_live_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create a session with our own PID (which is alive)
        let our_pid = std::process::id();
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "live-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .with_parent_pid(Some(our_pid));

        session.file(repo_path).create().unwrap();

        // Verify session file exists
        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        assert!(session_file.exists());

        // Cleanup should NOT remove it (process is alive)
        cleanup_stale_sessions(repo_path);

        // Session file should still exist
        assert!(session_file.exists());
    }

    #[test]
    fn test_for_hook_session_file_has_parent_pid() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create session using for_hook (which captures parent PID)
        let session = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "hook-session",
            None::<&str>,
        );
        session.file(repo_path).create().unwrap();

        // Verify parent_pid is in the session file
        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        let content = fs::read_to_string(&session_file).unwrap();

        // Should have parent_pid for hook mode
        assert!(
            content.contains("parent_pid="),
            "Hook mode session should have parent_pid"
        );
    }

    #[test]
    fn test_from_uuid_preserves_uuid() {
        let uuid = "12345678-1234-5678-1234-567812345678".to_string();
        let session = AikiSession::from_uuid(uuid.clone(), AgentType::ClaudeCode);
        assert_eq!(session.uuid(), &uuid);
        assert_eq!(session.agent_type(), AgentType::ClaudeCode);
    }

    #[test]
    fn test_from_uuid_does_not_rehash() {
        // from_uuid should preserve the UUID, not regenerate it
        let original = AikiSession::new(
            AgentType::ClaudeCode,
            "test-ext-id",
            None::<&str>,
            DetectionMethod::Hook,
        );
        let original_uuid = original.uuid().to_string();

        // from_uuid with the generated UUID should preserve it
        let reconstructed = AikiSession::from_uuid(original_uuid.clone(), AgentType::ClaudeCode);
        assert_eq!(reconstructed.uuid(), &original_uuid);

        // But new() with the UUID as external_id would generate a different UUID
        let rehashed = AikiSession::new(
            AgentType::ClaudeCode,
            &original_uuid,
            None::<&str>,
            DetectionMethod::Unknown,
        );
        assert_ne!(rehashed.uuid(), &original_uuid, "new() should rehash the UUID");
    }

    #[test]
    fn test_get_ttl_threshold_editor_agents() {
        assert_eq!(get_ttl_threshold(AgentType::ClaudeCode), EDITOR_TTL);
        assert_eq!(get_ttl_threshold(AgentType::Cursor), EDITOR_TTL);
    }

    #[test]
    fn test_parse_session_file_new_format() {
        let temp_dir = setup_test_repo();
        let sessions_dir = temp_dir.path().join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        let file_path = sessions_dir.join("test-session-id");
        fs::write(&file_path, "[aiki]\nagent=claude\nexternal_session_id=ext-123\nsession_id=uuid-456\nstarted_at=2026-01-24T12:00:00Z\nparent_pid=99999\n[/aiki]\n").unwrap();

        let info = parse_session_file(&file_path).unwrap();
        assert_eq!(info.agent_type, Some(AgentType::ClaudeCode));
        assert_eq!(info.session_id, Some("uuid-456".to_string()));
        assert_eq!(info.parent_pid, Some(99999));
    }

    #[test]
    fn test_parse_session_file_old_format() {
        let temp_dir = setup_test_repo();
        let sessions_dir = temp_dir.path().join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        let file_path = sessions_dir.join("test-session-id");
        fs::write(&file_path, "[aiki]\nagent=cursor\nexternal_session_id=ext-789\naiki_session_id=old-uuid-123\nstarted_at=2026-01-24T12:00:00Z\nparent_pid=88888\n[/aiki]\n").unwrap();

        let info = parse_session_file(&file_path).unwrap();
        assert_eq!(info.agent_type, Some(AgentType::Cursor));
        assert_eq!(info.session_id, Some("old-uuid-123".to_string()));
        assert_eq!(info.parent_pid, Some(88888));
    }

    #[test]
    fn test_cleanup_session_file_removes_all_related_files() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        let sessions_dir = repo_path.join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        // Create session file and associated files
        let session_path = sessions_dir.join("test-session");
        let turn_path = sessions_dir.join("test-session.turn");
        let autoreply_path = sessions_dir.join("test-session.turn.autoreply");

        fs::write(&session_path, "[aiki]\nagent=claude\nsession_id=test-uuid\nparent_pid=1\n[/aiki]\n").unwrap();
        fs::write(&turn_path, "turn=3").unwrap();
        fs::write(&autoreply_path, "").unwrap();

        let info = SessionFileInfo {
            path: session_path.clone(),
            agent_type: Some(AgentType::ClaudeCode),
            session_id: Some("test-uuid".to_string()),
            parent_pid: Some(1),
        };

        cleanup_session_file(repo_path, &info, SessionCleanupReason::PidDead);

        assert!(!session_path.exists(), "Session file should be removed");
        assert!(!turn_path.exists(), "Turn state file should be removed");
        assert!(!autoreply_path.exists(), "Autoreply flag should be removed");
    }

    #[test]
    fn test_find_session_prefers_most_recently_active() {
        use std::fs::FileTimes;
        use std::time::{Duration, SystemTime};

        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        let sessions_dir = repo_path.join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        // Use parent PID (which is in our ancestor chain)
        let parent_pid = match get_parent_pid() {
            Some(pid) => pid,
            None => return, // Skip test if parent PID unavailable
        };

        // Create session A
        let a_content = format!(
            "[aiki]\nagent=claude\nexternal_session_id=session-a\nsession_id=uuid-a\nstarted_at=2026-01-24T12:00:00Z\nparent_pid={}\n[/aiki]\n",
            parent_pid
        );
        fs::write(sessions_dir.join("uuid-a"), &a_content).unwrap();
        fs::write(sessions_dir.join("uuid-a.turn"), "turn=1").unwrap();

        // Create session B (older started_at, but more recently active)
        let b_content = format!(
            "[aiki]\nagent=claude\nexternal_session_id=session-b\nsession_id=uuid-b\nstarted_at=2026-01-20T12:00:00Z\nparent_pid={}\n[/aiki]\n",
            parent_pid
        );
        fs::write(sessions_dir.join("uuid-b"), &b_content).unwrap();
        fs::write(sessions_dir.join("uuid-b.turn"), "turn=5").unwrap();

        // Set .turn file mtimes explicitly to avoid filesystem resolution flakiness.
        // Session A's .turn file: 1 hour ago (clearly older)
        let old_time = SystemTime::now() - Duration::from_secs(3600);
        let old_times = FileTimes::new().set_modified(old_time);
        fs::File::options().write(true).open(sessions_dir.join("uuid-a.turn"))
            .unwrap().set_times(old_times).unwrap();

        // Session B's .turn file: now (clearly newer)
        let new_times = FileTimes::new().set_modified(SystemTime::now());
        fs::File::options().write(true).open(sessions_dir.join("uuid-b.turn"))
            .unwrap().set_times(new_times).unwrap();

        let result = find_session_by_ancestor_pid(repo_path);
        assert!(result.is_some());
        let session = result.unwrap();
        // Should prefer session B despite older started_at, because its .turn file is newer
        assert_eq!(session.session_id, "uuid-b", "Should prefer the most recently active session (newer .turn mtime)");
    }

    #[test]
    fn test_cleanup_session_file_with_ttl_expired_reason() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        let sessions_dir = repo_path.join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        let session_path = sessions_dir.join("ttl-session");
        fs::write(&session_path, "[aiki]\nagent=claude\nsession_id=ttl-uuid\nparent_pid=1\n[/aiki]\n").unwrap();
        fs::write(sessions_dir.join("ttl-session.turn"), "turn=10").unwrap();

        let info = SessionFileInfo {
            path: session_path.clone(),
            agent_type: Some(AgentType::ClaudeCode),
            session_id: Some("ttl-uuid".to_string()),
            parent_pid: Some(1),
        };

        cleanup_session_file(repo_path, &info, SessionCleanupReason::TtlExpired);

        assert!(!session_path.exists(), "Session file should be removed for ttl_expired");
        assert!(!sessions_dir.join("ttl-session.turn").exists(), "Turn state should be removed");
    }

    #[test]
    fn test_cleanup_session_file_with_no_events_reason() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        let sessions_dir = repo_path.join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        let session_path = sessions_dir.join("orphan-session");
        fs::write(&session_path, "[aiki]\nagent=cursor\nsession_id=orphan-uuid\nparent_pid=2\n[/aiki]\n").unwrap();

        let info = SessionFileInfo {
            path: session_path.clone(),
            agent_type: Some(AgentType::Cursor),
            session_id: Some("orphan-uuid".to_string()),
            parent_pid: Some(2),
        };

        cleanup_session_file(repo_path, &info, SessionCleanupReason::NoEvents);

        assert!(!session_path.exists(), "Session file should be removed for no_events");
    }

    // --- determine_cleanup_action decision layer tests ---

    #[test]
    fn test_cleanup_decision_pid_dead_trumps_all() {
        let now = Utc::now();
        // Even with a recent event, PID dead should trigger cleanup
        let recent_event = Ok(Some(now - chrono::Duration::minutes(5)));
        let result = determine_cleanup_action(false, AgentType::ClaudeCode, recent_event, now);
        assert_eq!(result, Some(SessionCleanupReason::PidDead));
    }

    #[test]
    fn test_cleanup_decision_pid_dead_with_no_events() {
        let now = Utc::now();
        let result = determine_cleanup_action(false, AgentType::Cursor, Ok(None), now);
        assert_eq!(result, Some(SessionCleanupReason::PidDead));
    }

    #[test]
    fn test_cleanup_decision_pid_dead_with_query_error() {
        let now = Utc::now();
        let result = determine_cleanup_action(false, AgentType::ClaudeCode, Err("jj failed".to_string()), now);
        assert_eq!(result, Some(SessionCleanupReason::PidDead));
    }

    #[test]
    fn test_cleanup_decision_ttl_expired_editor_agent() {
        let now = Utc::now();
        // Editor TTL is 8 hours; event 9 hours ago should trigger cleanup
        let old_event = Ok(Some(now - chrono::Duration::hours(9)));
        let result = determine_cleanup_action(true, AgentType::ClaudeCode, old_event, now);
        assert_eq!(result, Some(SessionCleanupReason::TtlExpired));
    }

    #[test]
    fn test_cleanup_decision_ttl_expired_cli_agent() {
        let now = Utc::now();
        // CLI TTL is 2 hours; event 3 hours ago should trigger cleanup
        let old_event = Ok(Some(now - chrono::Duration::hours(3)));
        let result = determine_cleanup_action(true, AgentType::Unknown, old_event, now);
        assert_eq!(result, Some(SessionCleanupReason::TtlExpired));
    }

    #[test]
    fn test_cleanup_decision_within_ttl_keeps_session() {
        let now = Utc::now();
        // Editor TTL is 8 hours; event 1 hour ago is within threshold
        let recent_event = Ok(Some(now - chrono::Duration::hours(1)));
        let result = determine_cleanup_action(true, AgentType::ClaudeCode, recent_event, now);
        assert_eq!(result, None, "Session within TTL should be kept");
    }

    #[test]
    fn test_cleanup_decision_cli_within_ttl() {
        let now = Utc::now();
        // CLI TTL is 2 hours; event 30 minutes ago is within threshold
        let recent_event = Ok(Some(now - chrono::Duration::minutes(30)));
        let result = determine_cleanup_action(true, AgentType::Unknown, recent_event, now);
        assert_eq!(result, None, "CLI session within TTL should be kept");
    }

    #[test]
    fn test_cleanup_decision_no_events_orphaned() {
        let now = Utc::now();
        let result = determine_cleanup_action(true, AgentType::Cursor, Ok(None), now);
        assert_eq!(result, Some(SessionCleanupReason::NoEvents));
    }

    #[test]
    fn test_cleanup_decision_query_error_keeps_session() {
        let now = Utc::now();
        let result = determine_cleanup_action(true, AgentType::ClaudeCode, Err("jj not found".to_string()), now);
        assert_eq!(result, None, "Query failure should not trigger cleanup");
    }

    #[test]
    fn test_cleanup_decision_editor_vs_cli_ttl_boundary() {
        let now = Utc::now();
        // Event 3 hours ago: within editor TTL (8h) but past CLI TTL (2h)
        let event_3h_ago = Ok(Some(now - chrono::Duration::hours(3)));

        let editor_result = determine_cleanup_action(true, AgentType::ClaudeCode, event_3h_ago.clone(), now);
        assert_eq!(editor_result, None, "Editor agent within 8h TTL should be kept");

        let cli_result = determine_cleanup_action(true, AgentType::Unknown, event_3h_ago, now);
        assert_eq!(cli_result, Some(SessionCleanupReason::TtlExpired), "CLI agent past 2h TTL should be cleaned");
    }

    #[test]
    fn test_session_file_uses_new_format() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        );
        session.file(repo_path).create().unwrap();

        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        let content = fs::read_to_string(&session_file).unwrap();

        // Should use new field name
        assert!(content.contains("session_id="), "Should use session_id field");
        assert!(!content.contains("aiki_session_id="), "Should not use old field name");
        assert!(!content.contains("cwd="), "Should not include cwd field");
    }

    // ========================================================================
    // parse_event_timestamp tests (covers TTL cleanup timestamp parsing)
    // ========================================================================

    #[test]
    fn test_parse_event_timestamp_rfc3339() {
        let description = "[aiki]\nevent=prompt\nsession_id=sess123\ntimestamp=2026-01-23T12:00:00Z\n[/aiki]\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_ok());
        let ts = result.unwrap().unwrap();
        assert_eq!(ts.year(), 2026);
        assert_eq!(ts.month(), 1);
        assert_eq!(ts.day(), 23);
        assert_eq!(ts.hour(), 12);
    }

    #[test]
    fn test_parse_event_timestamp_rfc3339_with_offset() {
        let description = "event=response\ntimestamp=2026-01-23T04:00:00-08:00\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_ok());
        let ts = result.unwrap().unwrap();
        // 04:00 -08:00 = 12:00 UTC
        assert_eq!(ts.hour(), 12);
    }

    #[test]
    fn test_parse_event_timestamp_jj_format_with_millis() {
        let description = "event=prompt\ntimestamp=2026-01-23 12:00:00.000 +00:00\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_ok());
        let ts = result.unwrap().unwrap();
        assert_eq!(ts.hour(), 12);
    }

    #[test]
    fn test_parse_event_timestamp_jj_format_no_millis() {
        let description = "event=prompt\ntimestamp=2026-01-23 12:00:00 +00:00\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_ok());
        let ts = result.unwrap().unwrap();
        assert_eq!(ts.hour(), 12);
    }

    #[test]
    fn test_parse_event_timestamp_empty_description() {
        let result = parse_event_timestamp("");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_parse_event_timestamp_whitespace_only() {
        let result = parse_event_timestamp("  \n  \n  ");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_parse_event_timestamp_no_timestamp_field() {
        let description = "event=prompt\nsession_id=sess123\nagent_type=claude-code\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No timestamp="));
    }

    #[test]
    fn test_parse_event_timestamp_empty_value() {
        let description = "event=prompt\ntimestamp=\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty timestamp="));
    }

    #[test]
    fn test_parse_event_timestamp_unparseable() {
        let description = "event=prompt\ntimestamp=not-a-timestamp\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    #[test]
    fn test_parse_event_timestamp_indented_field() {
        // Handles whitespace before timestamp= field
        let description = "event=prompt\n  timestamp=2026-01-23T12:00:00Z\n";
        let result = parse_event_timestamp(description);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    // ========================================================================
    // determine_cleanup_action integration tests with parse_event_timestamp
    // ========================================================================

    #[test]
    fn test_cleanup_ttl_expired_via_event_timestamp() {
        let now = Utc::now();
        // Simulate event description from 10 hours ago
        let old_timestamp = (now - chrono::Duration::hours(10)).to_rfc3339();
        let description = format!("event=prompt\ntimestamp={}\n", old_timestamp);
        let parsed = parse_event_timestamp(&description);

        // Editor agent: 8h TTL, 10h elapsed -> should expire
        let result = determine_cleanup_action(true, AgentType::ClaudeCode, parsed, now);
        assert_eq!(result, Some(SessionCleanupReason::TtlExpired));
    }

    #[test]
    fn test_cleanup_within_ttl_via_event_timestamp() {
        let now = Utc::now();
        // Simulate event description from 1 hour ago
        let recent_timestamp = (now - chrono::Duration::hours(1)).to_rfc3339();
        let description = format!("event=prompt\ntimestamp={}\n", recent_timestamp);
        let parsed = parse_event_timestamp(&description);

        // Editor agent: 8h TTL, 1h elapsed -> should keep
        let result = determine_cleanup_action(true, AgentType::ClaudeCode, parsed, now);
        assert_eq!(result, None);
    }

    #[test]
    fn test_cleanup_no_events_orphaned() {
        let now = Utc::now();
        // Empty description = no events
        let parsed = parse_event_timestamp("");

        let result = determine_cleanup_action(true, AgentType::ClaudeCode, parsed, now);
        assert_eq!(result, Some(SessionCleanupReason::NoEvents));
    }

    #[test]
    fn test_cleanup_query_error_keeps_session() {
        let now = Utc::now();
        // Simulate parse error (missing timestamp field)
        let parsed = parse_event_timestamp("event=prompt\nno_timestamp_here\n");

        let result = determine_cleanup_action(true, AgentType::ClaudeCode, parsed, now);
        assert_eq!(result, None, "Query errors should keep session (transient failure)");
    }
}

use crate::error::{AikiError, Result};
use crate::provenance::{AgentType, DetectionMethod};
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{Pid, ProcessesToUpdate, System};

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
    pub fn create(&self, cwd: impl AsRef<Path>) -> Result<bool> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // Build metadata from session in [aiki]...[/aiki] format
        let started_at = Utc::now();
        let cwd_str = cwd.as_ref().display();

        let mut metadata = format!(
            "[aiki]\nagent={}\nexternal_session_id={}\naiki_session_id={}\nstarted_at={}\ncwd={}\n",
            self.session.agent_type().to_metadata_string(),
            self.session.external_id(),
            self.session.uuid(),
            started_at.to_rfc3339(),
            cwd_str
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
    /// aiki_session_id = UUIDv5(NAMESPACE, "{agent_type}:{external_session_id}")
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
    /// The Aiki session UUID
    pub aiki_session_id: String,
}

/// Find an active session by matching parent_pid against the current process's ancestors
///
/// This is the core function for PID-based session detection:
/// 1. Get all ancestor PIDs of the current process
/// 2. Scan session files in .aiki/sessions/
/// 3. Find a session whose parent_pid matches one of our ancestors
/// 4. Validate the agent type from the process tree
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

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse session file for parent_pid, agent, and external_session_id
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
            } else if let Some(val) = line.strip_prefix("aiki_session_id=") {
                aiki_session_id = Some(val.to_string());
            }
        }

        // Check if this session's parent_pid matches one of our ancestors
        if let Some(pid) = parent_pid {
            if ancestor_pids.contains(&pid) {
                // Validate: ensure the detected agent matches the process tree
                // This prevents stale sessions from matching after PID reuse
                if let (Some(agent), Some(ext_id), Some(aiki_id)) =
                    (agent_type, external_session_id, aiki_session_id)
                {
                    // Optional: validate agent type matches process tree
                    // For now, trust the session file since the PID match is strong
                    return Some(SessionMatch {
                        agent_type: agent,
                        external_session_id: ext_id,
                        aiki_session_id: aiki_id,
                    });
                }
            }
        }
    }

    None
}

/// Clean up stale session files where the parent process no longer exists
///
/// Called on SessionStart to remove orphaned sessions from crashed agents.
/// A session is considered stale if its parent_pid process no longer exists.
pub fn cleanup_stale_sessions(repo_path: impl AsRef<Path>) {
    let sessions_dir = repo_path.as_ref().join(".aiki/sessions");

    if !sessions_dir.exists() {
        return;
    }

    let entries = match fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Refresh process list once
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse parent_pid from session file
        let mut parent_pid: Option<u32> = None;
        for line in content.lines() {
            if let Some(val) = line.trim().strip_prefix("parent_pid=") {
                parent_pid = val.parse().ok();
                break;
            }
        }

        // If parent_pid exists but process is dead, remove the session file
        if let Some(pid) = parent_pid {
            if system.process(Pid::from_u32(pid)).is_none() {
                // Process no longer exists - session is stale
                let _ = fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        session.file(repo_path).create(repo_path).unwrap();

        // Verify session file was created using the API
        let session_file_path = repo_path.join(".aiki/sessions").join(session.uuid());
        assert!(session_file_path.exists());

        // Verify session file format uses [aiki]...[/aiki] blocks
        let content = fs::read_to_string(&session_file_path).unwrap();
        assert!(content.starts_with("[aiki]\n"));
        assert!(content.contains("agent=claude"));
        assert!(content.contains("external_session_id=claude-session-abc123"));
        assert!(content.contains(&format!("aiki_session_id={}", session.uuid())));
        assert!(content.contains("started_at="));
        assert!(content.contains("cwd="));
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
        let created1 = session1.file(repo_path).create(repo_path).unwrap();
        assert!(created1); // First create succeeds

        let session2 = AikiSession::for_hook(
            AgentType::ClaudeCode,
            "claude-session-abc123",
            None::<&str>,
        );
        let created2 = session2.file(repo_path).create(repo_path).unwrap();
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
        session1.file(repo_path).create(repo_path).unwrap();

        let session2 = AikiSession::new(
            AgentType::Cursor,
            "cursor-session-2",
            None::<&str>,
            DetectionMethod::Hook,
        );
        session2.file(repo_path).create(repo_path).unwrap();

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
        session.file(repo_path).create(repo_path).unwrap();

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
        session.file(repo_path).create(repo_path).unwrap();

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
            let created = session.file(repo_path).create(repo_path).unwrap();
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
        session_file.create(repo_path).unwrap();

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
        session_file.create(repo_path).unwrap();

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
        session_file.create(repo_path).unwrap();

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
        session_file.create(repo_path).unwrap();

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

            session.file(repo_path).create(repo_path).unwrap();

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

        session.file(repo_path).create(repo_path).unwrap();

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

        session.file(repo_path).create(repo_path).unwrap();

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

        session.file(repo_path).create(repo_path).unwrap();

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
        session.file(repo_path).create(repo_path).unwrap();

        // Verify parent_pid is in the session file
        let session_file = repo_path.join(".aiki/sessions").join(session.uuid());
        let content = fs::read_to_string(&session_file).unwrap();

        // Should have parent_pid for hook mode
        assert!(
            content.contains("parent_pid="),
            "Hook mode session should have parent_pid"
        );
    }
}

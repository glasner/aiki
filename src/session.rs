use crate::error::{AikiError, Result};
use crate::provenance::{AgentType, DetectionMethod};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

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
    ///     AgentType::Claude,
    ///     "claude-session-abc123",
    ///     None::<&str>,
    ///     DetectionMethod::Hook
    /// ).unwrap();
    ///
    /// // ACP-based (with agent version)
    /// let session = AikiSession::new(
    ///     AgentType::Claude,
    ///     "claude-session-abc123",
    ///     Some("0.10.6"),
    ///     DetectionMethod::ACP
    /// ).unwrap();
    ///
    /// // Same inputs produce same UUID (deterministic)
    /// let session2 = AikiSession::new(
    ///     AgentType::Claude,
    ///     "claude-session-abc123",
    ///     None::<&str>,
    ///     DetectionMethod::Hook
    /// ).unwrap();
    /// assert_eq!(session.uuid(), session2.uuid());
    /// ```
    pub fn new(
        agent_type: AgentType,
        external_id: impl Into<String>,
        agent_version: Option<impl Into<String>>,
        detection_method: DetectionMethod,
    ) -> Result<Self> {
        let external_id = external_id.into();
        let uuid = Self::generate_uuid(agent_type, &external_id);

        Ok(Self {
            uuid,
            agent_type,
            external_id,
            client_name: None,
            client_version: None,
            agent_version: agent_version.map(|v| v.into()),
            detection_method,
        })
    }

    /// Generate a deterministic UUID v5 for a session
    ///
    /// Creates a UUID v5 by hashing: "{agent_type}:{external_session_id}"
    /// This ensures the same agent and external session always produce the same UUID.
    ///
    /// This is useful when you need to compute a session UUID without creating
    /// a full AikiSession object (e.g., for cache lookups).
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
    ///     AgentType::Claude,
    ///     "session-123",
    ///     None::<&str>,
    ///     DetectionMethod::ACP
    /// )
    /// .unwrap()
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

/// Record a session start event
///
/// Creates a session file in .aiki/sessions/ to track that a session has started.
/// This is idempotent - calling multiple times for the same session is safe.
pub fn record_session_start(
    repo_path: impl AsRef<Path>,
    agent_type: AgentType,
    external_session_id: impl Into<String>,
    cwd: impl AsRef<Path>,
    detection_method: DetectionMethod,
) -> Result<AikiSession> {
    let external_session_id = external_session_id.into();
    let session = AikiSession::new(
        agent_type,
        &external_session_id,
        None::<&str>,
        detection_method,
    )?;

    // Try to create session file atomically (O_EXCL handles race conditions)
    // Returns true if created, false if already exists - both cases are success
    let _ = session.file(&repo_path).create(&cwd)?;

    Ok(session)
}

/// Count active sessions in the repository
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

/// End a session and clean up its session file
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
    )?;
    session.file(&repo_path).delete()?;
    Ok(())
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
    fn test_record_and_query_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Record a new session
        let session_id = record_session_start(
            repo_path,
            AgentType::Claude,
            "claude-session-abc123",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        // Verify session file was created using the API
        let session_file_path = repo_path.join(".aiki/sessions").join(session_id.uuid());
        assert!(session_file_path.exists());

        // Verify session file format uses [aiki]...[/aiki] blocks
        let content = fs::read_to_string(&session_file_path).unwrap();
        assert!(content.starts_with("[aiki]\n"));
        assert!(content.contains("agent=claude"));
        assert!(content.contains("external_session_id=claude-session-abc123"));
        assert!(content.contains(&format!("aiki_session_id={}", session_id.uuid())));
        assert!(content.contains("started_at="));
        assert!(content.contains("cwd="));
        assert!(content.ends_with("[/aiki]\n"));

        // Verify session count
        assert_eq!(count_sessions(repo_path).unwrap(), 1);
    }

    #[test]
    fn test_multiple_records_same_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Record session twice (idempotent)
        let session_id1 = record_session_start(
            repo_path,
            AgentType::Claude,
            "claude-session-abc123",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        let session_id2 = record_session_start(
            repo_path,
            AgentType::Claude,
            "claude-session-abc123",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        // Should produce same session UUID
        assert_eq!(session_id1.uuid(), session_id2.uuid());

        // Should only create one file
        assert_eq!(count_sessions(repo_path).unwrap(), 1);
    }

    #[test]
    fn test_concurrent_session_creation() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create multiple different sessions
        let _session1 = record_session_start(
            repo_path,
            AgentType::Claude,
            "claude-session-1",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        let _session2 = record_session_start(
            repo_path,
            AgentType::Cursor,
            "cursor-session-2",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        // Both should exist
        let session = AikiSession::new(
            AgentType::Claude,
            "claude-session-1",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session.uuid())
            .exists());
        let session = AikiSession::new(
            AgentType::Cursor,
            "cursor-session-2",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session.uuid())
            .exists());

        // Should have 2 sessions
        assert_eq!(count_sessions(repo_path).unwrap(), 2);
    }

    #[test]
    fn test_deterministic_session_ids() {
        // Same inputs should produce same session UUIDs
        let session1 = AikiSession::new(
            AgentType::Claude,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        let session2 = AikiSession::new(
            AgentType::Claude,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();

        assert_eq!(session1.uuid(), session2.uuid());
        assert_eq!(session1.uuid(), session2.uuid());

        // Different inputs should produce different UUIDs
        let session3 = AikiSession::new(
            AgentType::Cursor,
            "test-session",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        assert_ne!(session1.uuid(), session3.uuid());
    }

    #[test]
    fn test_session_end() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Start a session
        record_session_start(
            repo_path,
            AgentType::Claude,
            "claude-session-end-test",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        // Verify it exists
        let session = AikiSession::new(
            AgentType::Claude,
            "claude-session-end-test",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();
        assert!(repo_path
            .join(".aiki/sessions")
            .join(session.uuid())
            .exists());
        assert_eq!(count_sessions(repo_path).unwrap(), 1);

        // End the session
        end_session(
            repo_path,
            AgentType::Claude,
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
        let session_id = record_session_start(
            repo_path,
            AgentType::Claude,
            "lifecycle-test",
            repo_path,
            DetectionMethod::Hook,
        )
        .unwrap();

        // Verify session file exists
        let session_file = repo_path.join(".aiki/sessions").join(session_id.uuid());
        assert!(session_file.exists());

        // End
        end_session(
            repo_path,
            AgentType::Claude,
            "lifecycle-test",
            DetectionMethod::Hook,
        )
        .unwrap();

        // Verify session file is deleted
        assert!(!session_file.exists());
    }

    #[test]
    fn test_idempotent_recording() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Record same session 5 times
        for _ in 0..5 {
            record_session_start(
                repo_path,
                AgentType::Claude,
                "idempotent-test",
                repo_path,
                DetectionMethod::Hook,
            )
            .unwrap();
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
            AgentType::Claude,
            "test-session-with-version",
            Some("2.0.61"),
            DetectionMethod::Hook,
        )
        .unwrap();

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
            AgentType::Claude,
            "test-session-no-version",
            None::<&str>,
            DetectionMethod::Hook,
        )
        .unwrap();

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
}

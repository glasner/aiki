use crate::cache::debug_log;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Codex session state shared between OTel receiver and notify handler.
///
/// This state file enables correlation between the two complementary channels:
/// - OTel provides session lifecycle, turn starts, and tool tracking
/// - Notify provides turn completion with response text
///
/// Both share the same session identity via `conversation.id` / `thread-id`.
///
/// Stored at `~/.aiki/codex-sessions/{external_id}.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSessionState {
    /// The conversation/thread ID from Codex (shared between OTel and notify)
    pub external_id: String,
    /// Agent identifier
    pub agent: String,
    /// Agent version (from OTel resource attributes `service.version`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    /// Agent PID (from OTel resource attributes `process.pid` or hook parent PID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_pid: Option<u32>,
    /// Current turn number (incremented on each `codex.user_prompt` OTel event)
    pub current_turn: u32,
    /// Last turn number for which turn.started was emitted
    #[serde(default)]
    pub last_turn_started: u32,
    /// Whether session.started was emitted
    #[serde(default)]
    pub session_started: bool,
    /// Files modified during the current turn (accumulated from OTel `tool_result` events)
    /// Cleared on next `turn.started` (not on `turn.completed`)
    pub modified_files: BTreeSet<String>,
    /// Working directory for this session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Timestamp of the last event received (for TTL cleanup)
    pub last_event_at: DateTime<Utc>,
}

impl CodexSessionState {
    /// Create a new session state
    #[must_use]
    pub fn new(external_id: impl Into<String>) -> Self {
        Self {
            external_id: external_id.into(),
            agent: "codex".to_string(),
            agent_version: None,
            agent_pid: None,
            current_turn: 0,
            last_turn_started: 0,
            session_started: false,
            modified_files: BTreeSet::new(),
            cwd: None,
            last_event_at: Utc::now(),
        }
    }

    /// Start a new turn: increment counter and clear modified_files from previous turn
    pub fn start_turn(&mut self) {
        self.current_turn += 1;
        self.modified_files.clear();
        self.last_event_at = Utc::now();
    }

    /// Add a modified file path (deduplicated via BTreeSet)
    pub fn add_modified_file(&mut self, path: impl Into<String>) {
        self.modified_files.insert(path.into());
        self.last_event_at = Utc::now();
    }

    /// Set the agent PID if available
    pub fn set_agent_pid(&mut self, pid: u32) {
        self.agent_pid = Some(pid);
        self.last_event_at = Utc::now();
    }

    /// Mark that session.started has been emitted
    pub fn mark_session_started(&mut self) {
        self.session_started = true;
        self.last_event_at = Utc::now();
    }

    /// Track the last turn for which turn.started was emitted
    pub fn mark_turn_started(&mut self, turn: u32) {
        self.last_turn_started = turn;
        self.last_event_at = Utc::now();
    }

    /// Update last_event_at timestamp
    pub fn touch(&mut self) {
        self.last_event_at = Utc::now();
    }

    /// Set cwd and normalize any relative paths in modified_files.
    ///
    /// When tool_result events arrive before cwd is known, file paths
    /// are stored as-is (relative). Once cwd is established (from notify
    /// or conversation_starts), this method resolves those relative paths.
    pub fn set_cwd(&mut self, cwd: PathBuf) {
        if self.cwd.as_ref() == Some(&cwd) {
            return;
        }
        // Resolve relative paths against the new cwd
        let resolved: BTreeSet<String> = self
            .modified_files
            .iter()
            .map(|p| {
                let path = Path::new(p);
                if path.is_relative() {
                    cwd.join(path).to_string_lossy().to_string()
                } else {
                    p.clone()
                }
            })
            .collect();
        self.modified_files = resolved;
        self.cwd = Some(cwd);
    }

    /// Generate a deterministic turn_id for the current turn
    ///
    /// Format: `{external_id}:{current_turn}`
    #[must_use]
    pub fn turn_id(&self) -> String {
        format!("{}:{}", self.external_id, self.current_turn)
    }
}

/// Default directory where Codex session state files are stored
fn default_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".aiki/codex-sessions")
}

/// Get the path for a session state file within a base directory
fn state_file_path_in(base_dir: &Path, external_id: &str) -> PathBuf {
    base_dir.join(format!("{}.json", sanitize_filename(external_id)))
}

/// Get the path for a session lock file within a base directory
fn lock_file_path_in(base_dir: &Path, external_id: &str) -> PathBuf {
    base_dir.join(format!("{}.json.lock", sanitize_filename(external_id)))
}

/// Sanitize external_id for use as a filename
/// Replace non-alphanumeric chars (except - and _) with _
fn sanitize_filename(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Maximum time to wait for lock acquisition
const LOCK_TIMEOUT: Duration = Duration::from_millis(100);

/// Lock guard that releases the lock file on drop
struct LockGuard {
    #[cfg(unix)]
    _file: fs::File,
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire an exclusive lock on the session state file
#[cfg(unix)]
fn acquire_lock_in(base_dir: &Path, external_id: &str) -> Option<LockGuard> {
    use std::os::unix::io::AsRawFd;

    let lock_path = lock_file_path_in(base_dir, external_id);

    // Ensure directory exists
    if let Some(parent) = lock_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            debug_log(|| format!("Failed to create codex sessions dir: {}", e));
            return None;
        }
    }

    // Open/create the lock file
    let file = match fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            debug_log(|| format!("Failed to open lock file: {}", e));
            return None;
        }
    };

    // Try non-blocking flock first
    let fd = file.as_raw_fd();
    let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if result == 0 {
        return Some(LockGuard {
            _file: file,
            path: lock_path,
        });
    }

    // Lock is held by another process - wait with timeout
    let start = std::time::Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(5));
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 {
            return Some(LockGuard {
                _file: file,
                path: lock_path,
            });
        }
        if start.elapsed() >= LOCK_TIMEOUT {
            debug_log(|| "Lock acquisition timed out for codex session state".to_string());
            return None;
        }
    }
}

#[cfg(not(unix))]
fn acquire_lock_in(_base_dir: &Path, _external_id: &str) -> Option<LockGuard> {
    None
}

/// Read the session state from disk.
///
/// Returns None if the file doesn't exist or is corrupt.
pub fn read_state(external_id: &str) -> Option<CodexSessionState> {
    read_state_in(&default_sessions_dir(), external_id)
}

/// Read the session state from a specific directory.
fn read_state_in(base_dir: &Path, external_id: &str) -> Option<CodexSessionState> {
    let path = state_file_path_in(base_dir, external_id);
    let content = fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&content) {
        Ok(state) => Some(state),
        Err(e) => {
            debug_log(|| format!("Corrupt codex session state, deleting: {}", e));
            let _ = fs::remove_file(&path);
            None
        }
    }
}

/// Write the session state to disk atomically (write tmp → rename).
fn write_state_in(base_dir: &Path, state: &CodexSessionState) -> io::Result<()> {
    let path = state_file_path_in(base_dir, &state.external_id);

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write to temp file, then rename (atomic on same filesystem)
    let tmp_path = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;

    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Perform an atomic read-modify-write on the session state.
///
/// Acquires a lock, reads the current state (or creates new if absent),
/// applies the update function, and writes back atomically.
///
/// If the lock cannot be acquired within 100ms, logs a warning and skips
/// the update (non-fatal).
///
/// Returns the updated state, or None if the update was skipped.
pub fn update_state<F>(external_id: &str, updater: F) -> Option<CodexSessionState>
where
    F: FnOnce(&mut CodexSessionState),
{
    update_state_in(&default_sessions_dir(), external_id, updater)
}

/// Perform an atomic read-modify-write in a specific directory (test-accessible).
#[allow(dead_code)]
pub fn update_state_with_dir<F>(
    base_dir: &Path,
    external_id: &str,
    updater: F,
) -> Option<CodexSessionState>
where
    F: FnOnce(&mut CodexSessionState),
{
    update_state_in(base_dir, external_id, updater)
}

/// Perform an atomic read-modify-write in a specific directory.
fn update_state_in<F>(base_dir: &Path, external_id: &str, updater: F) -> Option<CodexSessionState>
where
    F: FnOnce(&mut CodexSessionState),
{
    let _lock = match acquire_lock_in(base_dir, external_id) {
        Some(guard) => guard,
        None => {
            debug_log(|| {
                format!(
                    "Skipping codex session state update for {}: lock unavailable",
                    external_id
                )
            });
            return None;
        }
    };

    // Read existing state or create new
    let mut state =
        read_state_in(base_dir, external_id).unwrap_or_else(|| CodexSessionState::new(external_id));

    // Apply update
    updater(&mut state);

    // Write back atomically
    if let Err(e) = write_state_in(base_dir, &state) {
        debug_log(|| format!("Failed to write codex session state: {}", e));
        return None;
    }

    Some(state)
}

/// Delete the session state file and its lock file
pub fn delete_state(external_id: &str) {
    delete_state_in(&default_sessions_dir(), external_id);
}

/// Delete state in a specific directory.
fn delete_state_in(base_dir: &Path, external_id: &str) {
    let state_path = state_file_path_in(base_dir, external_id);
    let lock_path = lock_file_path_in(base_dir, external_id);
    let _ = fs::remove_file(&state_path);
    let _ = fs::remove_file(&lock_path);
}

/// List all session state files with their last_event_at timestamps.
///
/// Used for TTL-based cleanup of stale Codex sessions.
pub fn list_sessions() -> Vec<CodexSessionState> {
    list_sessions_in(&default_sessions_dir())
}

/// List sessions in a specific directory.
fn list_sessions_in(base_dir: &Path) -> Vec<CodexSessionState> {
    if !base_dir.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(base_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                return None;
            }
            let content = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect()
}

/// TTL for Codex CLI sessions (2 hours, matching CLI_TTL in session/mod.rs)
const CODEX_SESSION_TTL: Duration = Duration::from_secs(2 * 60 * 60);

/// Information about an expired Codex session (for event dispatch)
#[derive(Debug, Clone)]
pub struct ExpiredSession {
    pub external_id: String,
    pub agent_version: Option<String>,
    pub current_turn: u32,
    pub modified_files: Vec<String>,
    pub cwd: Option<PathBuf>,
}

/// Clean up stale Codex sessions that have exceeded the TTL.
///
/// Returns expired sessions so the caller can dispatch final events
/// (turn.completed with remaining modified_files, then session.ended).
/// State files are deleted for expired sessions.
pub fn cleanup_stale_sessions() -> Vec<ExpiredSession> {
    cleanup_stale_sessions_in(&default_sessions_dir())
}

/// Clean up stale sessions in a specific directory.
fn cleanup_stale_sessions_in(base_dir: &Path) -> Vec<ExpiredSession> {
    let sessions = list_sessions_in(base_dir);
    let now = Utc::now();
    let mut expired = Vec::new();

    for session in sessions {
        let age = now
            .signed_duration_since(session.last_event_at)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        if age >= CODEX_SESSION_TTL {
            debug_log(|| {
                format!(
                    "Codex session {} expired (age: {:?})",
                    session.external_id, age
                )
            });

            expired.push(ExpiredSession {
                external_id: session.external_id.clone(),
                agent_version: session.agent_version.clone(),
                current_turn: session.current_turn,
                modified_files: session.modified_files.into_iter().collect(),
                cwd: session.cwd.clone(),
            });

            delete_state_in(base_dir, &session.external_id);
        }
    }

    expired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_state() {
        let state = CodexSessionState::new("conv_abc123");
        assert_eq!(state.external_id, "conv_abc123");
        assert_eq!(state.agent, "codex");
        assert_eq!(state.current_turn, 0);
        assert_eq!(state.last_turn_started, 0);
        assert!(!state.session_started);
        assert!(state.modified_files.is_empty());
    }

    #[test]
    fn test_start_turn() {
        let mut state = CodexSessionState::new("conv_abc123");
        state.add_modified_file("src/foo.rs");

        state.start_turn();
        assert_eq!(state.current_turn, 1);
        assert!(
            state.modified_files.is_empty(),
            "modified_files should be cleared"
        );
    }

    #[test]
    fn test_add_modified_file_deduplicates() {
        let mut state = CodexSessionState::new("conv_abc123");
        state.add_modified_file("src/foo.rs");
        state.add_modified_file("src/bar.rs");
        state.add_modified_file("src/foo.rs"); // duplicate

        assert_eq!(state.modified_files.len(), 2);
        assert!(state.modified_files.contains("src/foo.rs"));
        assert!(state.modified_files.contains("src/bar.rs"));
    }

    #[test]
    fn test_turn_id_format() {
        let mut state = CodexSessionState::new("conv_abc123");
        state.start_turn();
        assert_eq!(state.turn_id(), "conv_abc123:1");

        state.start_turn();
        assert_eq!(state.turn_id(), "conv_abc123:2");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("conv_abc123"), "conv_abc123");
        assert_eq!(sanitize_filename("conv/abc.123"), "conv_abc_123");
        assert_eq!(sanitize_filename("my-session-id"), "my-session-id");
    }

    #[test]
    fn test_update_state_creates_and_persists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let result = update_state_in(dir, "test-conv-1", |state| {
            state.start_turn();
            state.add_modified_file("src/main.rs");
        });

        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(state.current_turn, 1);
        assert!(state.modified_files.contains("src/main.rs"));

        // Read back
        let loaded = read_state_in(dir, "test-conv-1");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().current_turn, 1);
    }

    #[test]
    fn test_update_state_modifies_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        // First update: create state with turn 1
        update_state_in(dir, "test-conv-2", |state| {
            state.start_turn();
        });

        // Second update: increment to turn 2
        let result = update_state_in(dir, "test-conv-2", |state| {
            state.start_turn();
            state.add_modified_file("new-file.rs");
        });

        let state = result.unwrap();
        assert_eq!(state.current_turn, 2);
        assert!(state.modified_files.contains("new-file.rs"));
    }

    #[test]
    fn test_delete_state() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        update_state_in(dir, "test-conv-3", |state| {
            state.start_turn();
        });

        assert!(read_state_in(dir, "test-conv-3").is_some());

        delete_state_in(dir, "test-conv-3");

        assert!(read_state_in(dir, "test-conv-3").is_none());
    }

    #[test]
    fn test_list_sessions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        update_state_in(dir, "session-a", |s| s.start_turn());
        update_state_in(dir, "session-b", |s| {
            s.start_turn();
            s.start_turn();
        });

        let sessions = list_sessions_in(dir);
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_corrupt_state_file_is_deleted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        // Write corrupt JSON
        let path = dir.join("corrupt-session.json");
        fs::write(&path, "not valid json{{{").unwrap();

        // read_state_in should return None and delete the file
        let result = read_state_in(dir, "corrupt-session");
        assert!(result.is_none());
        assert!(!path.exists());
    }
}

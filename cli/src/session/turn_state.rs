use crate::cache::debug_log;
use crate::events::TurnSource;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Persistent turn state for a session
///
/// Tracks the current turn number across hook invocations.
/// Stored in `.aiki/sessions/<uuid>.turn` as a simple integer.
///
/// Turn IDs are deterministic: `turn_id = uuid_v5(session_uuid, turn.to_string())`
#[derive(Debug, Clone)]
pub struct TurnState {
    /// Path to the turn state file
    state_path: PathBuf,
    /// The session UUID (used as namespace for turn_id generation)
    session_uuid: String,
    /// Current turn number (starts at 0, incremented on each turn.started)
    pub current_turn: u32,
    /// Current turn ID (deterministic UUID v5)
    pub current_turn_id: String,
    /// Source of the current turn
    pub current_turn_source: TurnSource,
}

impl TurnState {
    /// Load turn state from disk, or create fresh state if no file exists.
    ///
    /// The state file is at `.aiki/sessions/<uuid>.turn` and contains
    /// just the current turn number as a decimal integer.
    ///
    /// If the `.turn` file is missing (e.g., deleted or lost), falls back to
    /// querying JJ history for the max turn number in this session's changes.
    /// This prevents turn_id collisions on session resume after file loss.
    #[must_use]
    pub fn load(session_uuid: &str, repo_path: &Path) -> Self {
        let state_path = repo_path
            .join(".aiki/sessions")
            .join(format!("{}.turn", session_uuid));

        let (current_turn, current_turn_source) = if state_path.exists() {
            parse_turn_file(&state_path)
        } else {
            // Fallback: try to restore turn counter from JJ history
            (restore_turn_from_jj(session_uuid, repo_path).unwrap_or(0), TurnSource::User)
        };

        let current_turn_id = generate_turn_id(session_uuid, current_turn);

        Self {
            state_path,
            session_uuid: session_uuid.to_string(),
            current_turn,
            current_turn_id,
            current_turn_source,
        }
    }

    /// Start a new turn: increment counter, generate turn_id, persist to disk
    ///
    /// Returns the new turn number.
    pub fn start_turn(&mut self, source: TurnSource) -> u32 {
        self.current_turn += 1;
        self.current_turn_id = generate_turn_id(&self.session_uuid, self.current_turn);
        self.current_turn_source = source;

        // Persist to disk (best-effort, don't fail the hook)
        if let Err(e) = self.save() {
            debug_log(|| format!("Failed to save turn state: {}", e));
        }

        self.current_turn
    }

    /// Save current turn number and source to disk
    fn save(&self) -> std::io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let source_str = match self.current_turn_source {
            TurnSource::User => "user",
            TurnSource::Autoreply => "autoreply",
        };
        fs::write(&self.state_path, format!("{} {}", self.current_turn, source_str))
    }

    /// Delete the turn state file (called on session end)
    pub fn delete(&self) {
        if self.state_path.exists() {
            if let Err(e) = fs::remove_file(&self.state_path) {
                debug_log(|| format!("Failed to delete turn state file: {}", e));
            }
        }
        // Also clean up any pending autoreply flag
        self.clear_pending_autoreply();
    }

    /// Mark that the next turn.started should be treated as an autoreply.
    ///
    /// Called from turn.completed when a flow produces autoreply context.
    /// The presence of `<uuid>.turn.autoreply` signals this.
    pub fn set_pending_autoreply(&self) {
        let flag_path = self.autoreply_flag_path();
        if let Some(parent) = flag_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&flag_path, "") {
            debug_log(|| format!("Failed to set pending autoreply flag: {}", e));
        }
    }

    /// Check and consume the pending autoreply flag.
    ///
    /// Returns `true` if the flag was set (and clears it).
    /// Called from turn.started to detect autoreply-initiated turns.
    pub fn take_pending_autoreply(&self) -> bool {
        let flag_path = self.autoreply_flag_path();
        if flag_path.exists() {
            let _ = fs::remove_file(&flag_path);
            true
        } else {
            false
        }
    }

    /// Clear pending autoreply flag without checking it
    fn clear_pending_autoreply(&self) {
        let flag_path = self.autoreply_flag_path();
        if flag_path.exists() {
            let _ = fs::remove_file(&flag_path);
        }
    }

    /// Path to the autoreply flag file
    fn autoreply_flag_path(&self) -> PathBuf {
        self.state_path.with_extension("turn.autoreply")
    }
}

/// Parse the turn state file, supporting both old format (just number) and
/// new format (number + source).
fn parse_turn_file(path: &Path) -> (u32, TurnSource) {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return (0, TurnSource::User),
    };
    let trimmed = content.trim();
    // New format: "<turn> <source>"
    if let Some((turn_str, source_str)) = trimmed.split_once(' ') {
        let turn = turn_str.parse::<u32>().unwrap_or(0);
        let source = match source_str {
            "autoreply" => TurnSource::Autoreply,
            _ => TurnSource::User,
        };
        (turn, source)
    } else {
        // Old format: just the turn number
        (trimmed.parse::<u32>().unwrap_or(0), TurnSource::User)
    }
}

/// Restore turn counter from JJ history by finding the max turn value
/// in changes associated with this session.
///
/// Queries JJ for all changes with `session_id=<uuid>` in their description,
/// parses `turn=N` values, and returns the maximum.
///
/// Returns `None` if JJ is unavailable or no turns are found.
fn restore_turn_from_jj(session_uuid: &str, repo_path: &Path) -> Option<u32> {
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("description(\"session_id={}\")", session_uuid),
            "--template",
            "description ++ \"\\n---\\n\"",
            "--no-graph",
        ])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let max_turn = stdout
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("turn=") {
                trimmed.strip_prefix("turn=")?.parse::<u32>().ok()
            } else {
                None
            }
        })
        .max();

    if let Some(turn) = max_turn {
        debug_log(|| {
            format!(
                "Restored turn counter from JJ history: session={}, turn={}",
                session_uuid, turn
            )
        });
    }

    max_turn
}

/// Generate a deterministic turn ID using UUID v5
///
/// `turn_id = uuid_v5(session_uuid_as_namespace, turn.to_string())`
///
/// This ensures the same session + turn number always produces the same turn_id.
fn generate_turn_id(session_uuid: &str, turn: u32) -> String {
    // Parse the session UUID to use as namespace
    let namespace = uuid::Uuid::parse_str(session_uuid).unwrap_or(uuid::Uuid::nil());
    let turn_id = uuid::Uuid::new_v5(&namespace, turn.to_string().as_bytes());
    turn_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_fresh_state() {
        let tmp = TempDir::new().unwrap();
        let state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn, 0);
    }

    #[test]
    fn test_start_turn_increments() {
        let tmp = TempDir::new().unwrap();
        // Create sessions directory
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        let mut state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn, 0);

        let turn = state.start_turn(TurnSource::User);
        assert_eq!(turn, 1);
        assert_eq!(state.current_turn, 1);

        let turn = state.start_turn(TurnSource::User);
        assert_eq!(turn, 2);
        assert_eq!(state.current_turn, 2);
    }

    #[test]
    fn test_state_persists_across_loads() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        // Start some turns
        {
            let mut state = TurnState::load("test-session-uuid", tmp.path());
            state.start_turn(TurnSource::User);
            state.start_turn(TurnSource::User);
            state.start_turn(TurnSource::Autoreply);
        }

        // Load again - should have turn 3 and source Autoreply (last start_turn)
        let state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn, 3);
        assert_eq!(state.current_turn_source, TurnSource::Autoreply);
    }

    #[test]
    fn test_source_persists_user() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        {
            let mut state = TurnState::load("test-session-uuid", tmp.path());
            state.start_turn(TurnSource::User);
        }

        let state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn_source, TurnSource::User);
    }

    #[test]
    fn test_source_persists_autoreply() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        {
            let mut state = TurnState::load("test-session-uuid", tmp.path());
            state.start_turn(TurnSource::Autoreply);
        }

        let state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn_source, TurnSource::Autoreply);
    }

    #[test]
    fn test_old_format_backwards_compatible() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp.path().join(".aiki/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        // Simulate old format: just the turn number
        let state_path = sessions_dir.join("test-session-uuid.turn");
        fs::write(&state_path, "5").unwrap();

        let state = TurnState::load("test-session-uuid", tmp.path());
        assert_eq!(state.current_turn, 5);
        assert_eq!(state.current_turn_source, TurnSource::User); // defaults to User
    }

    #[test]
    fn test_turn_id_is_deterministic() {
        let id1 = generate_turn_id("550e8400-e29b-41d4-a716-446655440000", 1);
        let id2 = generate_turn_id("550e8400-e29b-41d4-a716-446655440000", 1);
        assert_eq!(id1, id2);

        // Different turn numbers produce different IDs
        let id3 = generate_turn_id("550e8400-e29b-41d4-a716-446655440000", 2);
        assert_ne!(id1, id3);

        // Different sessions produce different IDs
        let id4 = generate_turn_id("660e8400-e29b-41d4-a716-446655440000", 1);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_turn_source_tracking() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        let mut state = TurnState::load("test-session-uuid", tmp.path());
        state.start_turn(TurnSource::User);
        assert_eq!(state.current_turn_source, TurnSource::User);

        state.start_turn(TurnSource::Autoreply);
        assert_eq!(state.current_turn_source, TurnSource::Autoreply);
    }

    #[test]
    fn test_delete_removes_file() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        let mut state = TurnState::load("test-session-uuid", tmp.path());
        state.start_turn(TurnSource::User);

        // File should exist
        assert!(state.state_path.exists());

        // Delete should remove it
        state.delete();
        assert!(!state.state_path.exists());
    }

    #[test]
    fn test_pending_autoreply_flag() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        let state = TurnState::load("test-session-uuid", tmp.path());

        // Initially no pending autoreply
        assert!(!state.take_pending_autoreply());

        // Set the flag
        state.set_pending_autoreply();

        // Take should return true and clear the flag
        assert!(state.take_pending_autoreply());

        // Second take should return false (consumed)
        assert!(!state.take_pending_autoreply());
    }

    #[test]
    fn test_pending_autoreply_persists_across_loads() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        // Set flag in one instance
        {
            let mut state = TurnState::load("test-session-uuid", tmp.path());
            state.start_turn(TurnSource::User);
            state.set_pending_autoreply();
        }

        // Load fresh instance - flag should persist
        let state = TurnState::load("test-session-uuid", tmp.path());
        assert!(state.take_pending_autoreply());
    }

    #[test]
    fn test_delete_clears_pending_autoreply() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".aiki/sessions")).unwrap();

        let mut state = TurnState::load("test-session-uuid", tmp.path());
        state.start_turn(TurnSource::User);
        state.set_pending_autoreply();

        // Delete should remove both files
        state.delete();
        assert!(!state.take_pending_autoreply());
    }
}

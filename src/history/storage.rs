//! Event storage on aiki/conversations branch
//!
//! Conversation events are stored as fileless JJ changes on the `aiki/conversations` branch.
//! Each event is a JJ change with metadata in the description.

use crate::cache::debug_log;
use crate::error::{AikiError, Result};
use crate::jj::jj_cmd;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::Duration;

use super::types::{AgentType, ConversationEvent, ConversationSummary, TurnSource, CONVERSATIONS_BRANCH, METADATA_END, METADATA_START};

/// Configuration for JJ write retry logic
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 50;
const BACKOFF_MULTIPLIER: u64 = 2;

/// Ensure the aiki/conversations branch exists
pub fn ensure_conversations_branch(cwd: &Path) -> Result<()> {
    // Check if branch exists by listing bookmarks
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);

    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Create the branch as an orphan (no parent) starting from root()
        let result = jj_cmd()
            .current_dir(cwd)
            .args(["bookmark", "create", CONVERSATIONS_BRANCH, "-r", "root()", "--ignore-working-copy"])
            .output()
            .map_err(|e| {
                AikiError::ConversationsBranchInitFailed(format!("Failed to create bookmark: {}", e))
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(AikiError::ConversationsBranchInitFailed(stderr.to_string()));
        }
    }
    Ok(())
}

/// Write a conversation event to the aiki/conversations branch
///
/// Uses `jj new --no-edit` to create the event change without affecting the working copy.
/// Includes retry logic with exponential backoff to handle concurrent writes from multiple agents.
pub fn write_event(cwd: &Path, event: &ConversationEvent) -> Result<()> {
    ensure_conversations_branch(cwd)?;

    let metadata = event_to_metadata_block(event);

    // Retry loop for concurrent write handling
    let mut attempt = 0;
    let mut last_error = None;

    while attempt < MAX_RETRIES {
        match write_event_inner(cwd, &metadata) {
            Ok(()) => return Ok(()),
            Err(e) => {
                attempt += 1;
                last_error = Some(e);

                if attempt < MAX_RETRIES {
                    // Exponential backoff: 50ms, 100ms, 200ms, ...
                    let backoff_ms = INITIAL_BACKOFF_MS * BACKOFF_MULTIPLIER.pow(attempt - 1);
                    debug_log(|| {
                        format!(
                            "JJ write failed (attempt {}/{}), retrying in {}ms",
                            attempt, MAX_RETRIES, backoff_ms
                        )
                    });
                    thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }

    // All retries exhausted - return the last error
    Err(last_error.unwrap_or_else(|| {
        AikiError::JjCommandFailed("Write failed with unknown error".to_string())
    }))
}

/// Inner write operation (without retry logic)
fn write_event_inner(cwd: &Path, metadata: &str) -> Result<()> {
    // Create a new change as child of aiki/conversations WITHOUT switching working copy
    let result = jj_cmd()
        .current_dir(cwd)
        .args(["new", CONVERSATIONS_BRANCH, "--no-edit", "--ignore-working-copy", "-m", metadata])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to create conversation event: {}", e))
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to write conversation event: {}",
            stderr
        )));
    }

    // Move the bookmark forward to point at the newly created change
    // Filter to only the conversation change (has [aiki-conversation] in description), not the working copy
    let result = jj_cmd()
        .current_dir(cwd)
        .args([
            "bookmark",
            "set",
            CONVERSATIONS_BRANCH,
            "-r",
            &format!(
                "children({}) & description(substring:\"{}\")",
                CONVERSATIONS_BRANCH, METADATA_START
            ),
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to update bookmark: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to update conversations bookmark: {}",
            stderr
        )));
    }

    Ok(())
}

/// Get the turn number and source from the latest Prompt event for a session
///
/// Queries the `aiki/conversations` branch for the most recent Prompt event
/// for this session and extracts the `turn` and `source` fields.
///
/// Returns `(0, TurnSource::User)` if no prompt events are found (new session).
pub fn get_current_turn_info(cwd: &Path, session_id: &str) -> Result<(u32, TurnSource)> {
    // Check if branch exists first
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to list bookmarks: {}",
            stderr
        )));
    }

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Branch doesn't exist yet - new session
        return Ok((0, TurnSource::User));
    }

    // Query for the latest prompt event from this session
    // We use a revset to find prompt events, ordered by newest first
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!(
                "ancestors({}) & description(substring:'{}') & description(substring:'event=prompt') & description(substring:'session={}')",
                CONVERSATIONS_BRANCH, METADATA_START, session_id
            ),
            "--no-graph",
            "-T",
            "description ++ \"\\n---EVENT-SEPARATOR---\\n\"",
            "--limit",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to query prompt events: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to query prompt events: {}",
            stderr
        )));
    }

    let description = String::from_utf8_lossy(&output.stdout);

    // Parse the turn and source from the description
    let mut turn: u32 = 0;
    let mut source = TurnSource::User;

    for line in description.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("turn=") {
            if let Ok(t) = value.parse() {
                turn = t;
            }
        } else if let Some(value) = line.strip_prefix("source=") {
            source = match value {
                "autoreply" => TurnSource::Autoreply,
                _ => TurnSource::User,
            };
        }
    }

    Ok((turn, source))
}

/// Check if there's a pending autoreply for a session
///
/// Returns true if the latest event for this session is an Autoreply event
/// (not a Prompt), indicating the next turn should be treated as an autoreply.
///
/// Logic: Find the latest event for this session. If it's an Autoreply event,
/// then we're in autoreply mode (the autoreply was generated but the prompt
/// for it hasn't been recorded yet).
pub fn has_pending_autoreply(cwd: &Path, session_id: &str) -> Result<bool> {
    // Check if branch exists first
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to list bookmarks: {}",
            stderr
        )));
    }

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Branch doesn't exist yet - no autoreply pending
        return Ok(false);
    }

    // Query for the latest event from this session (any event type)
    // We want the most recent event to see if it's an autoreply
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!(
                "ancestors({}) & description(substring:'{}') & description(substring:'session={}')",
                CONVERSATIONS_BRANCH, METADATA_START, session_id
            ),
            "--no-graph",
            "-T",
            "description ++ \"\\n---EVENT-SEPARATOR---\\n\"",
            "--limit",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to query conversation events: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to query conversation events: {}",
            stderr
        )));
    }

    let description = String::from_utf8_lossy(&output.stdout);

    // Check if the latest event is an autoreply
    for line in description.lines() {
        let line = line.trim();
        if line == "event=autoreply" {
            return Ok(true);
        }
        // If we see any other event type first, it's not an autoreply
        if line.starts_with("event=") {
            return Ok(false);
        }
    }

    // No events found
    Ok(false)
}

/// Get the change_id of the latest prompt event for a given session
///
/// Used by `--source prompt` to automatically resolve to the triggering prompt.
/// Returns None if no prompt events found for the session.
pub fn get_latest_prompt_change_id(cwd: &Path, session_id: &str) -> Result<Option<String>> {
    // Check if branch exists first
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to list bookmarks: {}",
            stderr
        )));
    }

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Branch doesn't exist yet
        return Ok(None);
    }

    // Query for the latest prompt event from this session
    // We use a revset to find prompt events, ordered by newest first
    // Match on metadata markers to avoid false positives from prompt content:
    // - METADATA_START ensures we're looking at aiki metadata
    // - event=prompt ensures it's a prompt event (not response, session_start, etc.)
    // - session=<id> ensures it's from the right session
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!(
                "ancestors({}) & description(substring:'{}') & description(substring:'event=prompt') & description(substring:'session={}')",
                CONVERSATIONS_BRANCH, METADATA_START, session_id
            ),
            "--no-graph",
            "-T",
            "change_id ++ \"\\n\"",
            "--limit",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to query prompt events: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to query prompt events: {}",
            stderr
        )));
    }

    let change_id = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(change_id)
}

/// Get the current turn number for a session (from most recent prompt event)
///
/// Returns 0 if no prompt events found (new session).
pub fn get_current_turn_number(cwd: &Path, session_id: &str) -> Result<u32> {
    let (turn, _source) = get_current_turn_info(cwd, session_id)?;
    Ok(turn)
}

/// Check if a session.started event exists for a session
///
/// Returns true if a session_start event exists in the conversation history.
pub fn has_session_started_event(cwd: &Path, session_id: &str) -> Result<bool> {
    // Check if branch exists first
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    if !output.status.success() {
        return Ok(false);
    }

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        return Ok(false);
    }

    // Query for session_start event
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!(
                "ancestors({}) & description(substring:'{}') & description(substring:'event=session_start') & description(substring:'session={}')",
                CONVERSATIONS_BRANCH, METADATA_START, session_id
            ),
            "--no-graph",
            "-T",
            "change_id",
            "--limit",
            "1",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to query session events: {}", e)))?;

    if !output.status.success() {
        return Ok(false);
    }

    let result = String::from_utf8_lossy(&output.stdout);
    Ok(!result.trim().is_empty())
}

/// Get the turn number of the most recent prompt event for a session
///
/// Used to check if turn.started has been emitted for a given turn.
/// Returns None if no prompt events found.
pub fn get_last_prompt_turn(cwd: &Path, session_id: &str) -> Result<Option<u32>> {
    let (turn, _source) = get_current_turn_info(cwd, session_id)?;
    if turn == 0 {
        Ok(None)
    } else {
        Ok(Some(turn))
    }
}

/// Read all conversation events from the aiki/conversations branch
#[allow(dead_code)] // Part of history API
pub fn read_events(cwd: &Path) -> Result<Vec<ConversationEvent>> {
    // Check if branch exists first
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["bookmark", "list", "--all", "--ignore-working-copy"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Branch doesn't exist yet, return empty list
        return Ok(Vec::new());
    }

    // Read all changes on the branch, oldest first
    let output = jj_cmd()
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!("root()..{}", CONVERSATIONS_BRANCH),
            "--no-graph",
            "-T",
            "description ++ \"\\n---EVENT-SEPARATOR---\\n\"",
            "--reversed",
            "--ignore-working-copy",
        ])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to read conversation events: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to read conversation events: {}",
            stderr
        )));
    }

    let descriptions = String::from_utf8_lossy(&output.stdout);
    let mut events = Vec::new();

    // Split by our separator and parse each description
    for desc in descriptions.split("---EVENT-SEPARATOR---") {
        let desc = desc.trim();
        if desc.is_empty() {
            continue;
        }

        // Look for metadata block
        if let Some(start_idx) = desc.find(METADATA_START) {
            if let Some(end_idx) = desc.find(METADATA_END) {
                let block = &desc[start_idx + METADATA_START.len()..end_idx];
                if let Some(event) = parse_metadata_block(block) {
                    events.push(event);
                }
            }
        }
    }

    Ok(events)
}

/// List conversations with summary information
///
/// Returns a list of conversation summaries, sorted by most recent activity first.
/// Only sessions that have a `SessionStart` event are included.
/// Defaults to returning at most 10 results if `limit` is `None`.
#[allow(dead_code)] // Part of history API
pub fn list_conversations(cwd: &Path, limit: Option<usize>) -> Result<Vec<ConversationSummary>> {
    let events = read_events(cwd)?;

    // Group events by session_id
    let mut sessions: HashMap<String, Vec<&ConversationEvent>> = HashMap::new();
    for event in &events {
        let session_id = match event {
            ConversationEvent::Prompt { session_id, .. }
            | ConversationEvent::Response { session_id, .. }
            | ConversationEvent::SessionStart { session_id, .. }
            | ConversationEvent::SessionEnd { session_id, .. }
            | ConversationEvent::Autoreply { session_id, .. } => session_id,
        };
        sessions.entry(session_id.clone()).or_default().push(event);
    }

    let mut summaries: Vec<ConversationSummary> = Vec::new();

    for (_session_id, session_events) in &sessions {
        // Find the SessionStart event
        let session_start = session_events.iter().find(|e| {
            matches!(e, ConversationEvent::SessionStart { .. })
        });

        let session_start = match session_start {
            Some(s) => s,
            None => continue, // Skip sessions without a SessionStart event
        };

        let (session_id, agent_type, started_at, repo_id) = match session_start {
            ConversationEvent::SessionStart {
                session_id,
                agent_type,
                timestamp,
                repo_id,
                ..
            } => (session_id.clone(), agent_type.clone(), *timestamp, repo_id.clone()),
            _ => unreachable!(),
        };

        // Count Prompt events as turn_count
        let turn_count = session_events
            .iter()
            .filter(|e| matches!(e, ConversationEvent::Prompt { .. }))
            .count() as u32;

        // Find the latest event timestamp
        let last_activity = session_events
            .iter()
            .map(|e| match e {
                ConversationEvent::Prompt { timestamp, .. }
                | ConversationEvent::Response { timestamp, .. }
                | ConversationEvent::SessionStart { timestamp, .. }
                | ConversationEvent::SessionEnd { timestamp, .. }
                | ConversationEvent::Autoreply { timestamp, .. } => *timestamp,
            })
            .max()
            .unwrap_or(started_at);

        summaries.push(ConversationSummary {
            session_id,
            agent_type,
            started_at,
            turn_count,
            last_activity,
            repo_id,
        });
    }

    // Sort by last_activity descending (most recent first)
    summaries.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    // Apply limit (default to 10)
    let limit = limit.unwrap_or(10);
    summaries.truncate(limit);

    Ok(summaries)
}

/// Escape a string value for metadata storage
/// Encodes characters that would break key=value parsing: %, =, \n, \r
fn escape_metadata_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => result.push_str("%25"),
            '=' => result.push_str("%3D"),
            '\n' => result.push_str("%0A"),
            '\r' => result.push_str("%0D"),
            _ => result.push(c),
        }
    }
    result
}

/// Unescape a metadata value
#[allow(dead_code)] // Used by parse_metadata_block
fn unescape_metadata_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            // Read two hex characters
            let hex: String = chars.by_ref().take(2).collect();
            match hex.as_str() {
                "25" => result.push('%'),
                "3D" | "3d" => result.push('='),
                "0A" | "0a" => result.push('\n'),
                "0D" | "0d" => result.push('\r'),
                _ => {
                    // Unknown escape, keep as-is
                    result.push('%');
                    result.push_str(&hex);
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Helper to add metadata field (for safe values)
fn add_metadata(key: &str, value: impl std::fmt::Display, lines: &mut Vec<String>) {
    lines.push(format!("{}={}", key, value));
}

/// Helper to add metadata field with escaping (for user-provided text)
fn add_metadata_escaped(key: &str, value: &str, lines: &mut Vec<String>) {
    lines.push(format!("{}={}", key, escape_metadata_value(value)));
}

/// Helper to add timestamp metadata field
fn add_metadata_timestamp(timestamp: &DateTime<Utc>, lines: &mut Vec<String>) {
    add_metadata("timestamp", timestamp.to_rfc3339(), lines);
}

/// Helper to add list as multiple metadata lines
fn add_metadata_list(key: &str, values: &[String], lines: &mut Vec<String>) {
    for value in values {
        add_metadata_escaped(key, value, lines);
    }
}

/// Add optional location metadata (repo_id and cwd) to metadata lines
fn add_location_metadata(repo_id: &Option<String>, cwd: &Option<String>, lines: &mut Vec<String>) {
    if let Some(repo) = repo_id {
        add_metadata("repo", repo, lines);
    }
    if let Some(c) = cwd {
        add_metadata_escaped("cwd", c, lines);
    }
}

/// Convert a ConversationEvent to a metadata block string
fn event_to_metadata_block(event: &ConversationEvent) -> String {
    let mut lines = vec![METADATA_START.to_string()];

    match event {
        ConversationEvent::Prompt {
            session_id,
            agent_type,
            turn,
            source,
            content,
            injected_refs,
            timestamp,
            repo_id,
            cwd,
        } => {
            add_metadata("event", "prompt", &mut lines);
            add_metadata("session", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata("turn", turn, &mut lines);
            add_metadata("source", source, &mut lines);
            add_metadata_escaped("content", content, &mut lines);
            add_metadata_list("injected_ref", injected_refs, &mut lines);
            add_location_metadata(repo_id, cwd, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::Response {
            session_id,
            agent_type,
            turn,
            files_written,
            summary,
            timestamp,
            repo_id,
            cwd,
        } => {
            add_metadata("event", "response", &mut lines);
            add_metadata("session", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata("turn", turn, &mut lines);
            add_metadata_list("files_written", files_written, &mut lines);
            if let Some(s) = summary {
                add_metadata_escaped("summary", s, &mut lines);
            }
            add_location_metadata(repo_id, cwd, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionStart {
            session_id,
            agent_type,
            timestamp,
            repo_id,
            cwd,
        } => {
            add_metadata("event", "session_start", &mut lines);
            add_metadata("session", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_location_metadata(repo_id, cwd, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionEnd {
            session_id,
            timestamp,
            reason,
            repo_id,
            cwd,
        } => {
            add_metadata("event", "session_end", &mut lines);
            add_metadata("session", session_id, &mut lines);
            if !reason.is_empty() {
                add_metadata("reason", reason, &mut lines);
            }
            add_location_metadata(repo_id, cwd, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::Autoreply {
            session_id,
            agent_type,
            turn,
            content,
            timestamp,
            repo_id,
            cwd,
        } => {
            add_metadata("event", "autoreply", &mut lines);
            add_metadata("session", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata("turn", turn, &mut lines);
            add_metadata_escaped("content", content, &mut lines);
            add_location_metadata(repo_id, cwd, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
    }

    lines.push(METADATA_END.to_string());
    lines.join("\n")
}

/// Parse list values from metadata fields
#[allow(dead_code)] // Used by parse_metadata_block
fn parse_list_field(fields: &HashMap<&str, Vec<&str>>, key: &str) -> Vec<String> {
    fields
        .get(key)
        .map(|v| v.iter().map(|s| unescape_metadata_value(s)).collect())
        .unwrap_or_default()
}

/// Parse location metadata (repo_id and cwd) from fields
fn parse_location_metadata(fields: &HashMap<&str, Vec<&str>>) -> (Option<String>, Option<String>) {
    let repo_id = fields.get("repo").and_then(|v| v.first()).map(|s| s.to_string());
    let cwd = fields.get("cwd").and_then(|v| v.first()).map(|s| unescape_metadata_value(s));
    (repo_id, cwd)
}

/// Parse a metadata block into a ConversationEvent
#[allow(dead_code)] // Used by read_events
fn parse_metadata_block(block: &str) -> Option<ConversationEvent> {
    let mut fields: HashMap<&str, Vec<&str>> = HashMap::new();

    // Collect all values for each key (to handle multiple lines for lists)
    for line in block.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            fields
                .entry(key.trim())
                .or_insert_with(Vec::new)
                .push(value.trim());
        }
    }

    let event_type = fields.get("event")?.first()?;
    let timestamp = fields
        .get("timestamp")
        .and_then(|v| v.first())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let (repo_id, cwd) = parse_location_metadata(&fields);

    match *event_type {
        "prompt" => {
            let session_id = fields.get("session")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);
            let turn = fields
                .get("turn")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let source = fields
                .get("source")
                .and_then(|v| v.first())
                .map(|s| match *s {
                    "autoreply" => TurnSource::Autoreply,
                    _ => TurnSource::User,
                })
                .unwrap_or(TurnSource::User);
            let content = fields
                .get("content")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s))
                .unwrap_or_default();
            let injected_refs = parse_list_field(&fields, "injected_ref");

            Some(ConversationEvent::Prompt {
                session_id,
                agent_type,
                turn,
                source,
                content,
                injected_refs,
                timestamp,
                repo_id,
                cwd,
            })
        }
        "response" => {
            let session_id = fields.get("session")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);
            let turn = fields
                .get("turn")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let files_written = parse_list_field(&fields, "files_written");
            let summary = fields
                .get("summary")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));

            Some(ConversationEvent::Response {
                session_id,
                agent_type,
                turn,
                files_written,
                summary,
                timestamp,
                repo_id,
                cwd,
            })
        }
        "session_start" => {
            let session_id = fields.get("session")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);

            Some(ConversationEvent::SessionStart {
                session_id,
                agent_type,
                timestamp,
                repo_id,
                cwd,
            })
        }
        "session_end" => {
            let session_id = fields.get("session")?.first()?.to_string();
            let reason = fields.get("reason")
                .and_then(|v| v.first())
                .map(|s| s.to_string())
                .unwrap_or_default();

            Some(ConversationEvent::SessionEnd {
                session_id,
                timestamp,
                reason,
                repo_id,
                cwd,
            })
        }
        "autoreply" => {
            let session_id = fields.get("session")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);
            let turn = fields
                .get("turn")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let content = fields
                .get("content")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s))
                .unwrap_or_default();

            Some(ConversationEvent::Autoreply {
                session_id,
                agent_type,
                turn,
                content,
                timestamp,
                repo_id,
                cwd,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_unescape_roundtrip() {
        let test_cases = [
            "simple text",
            "with=equals",
            "with\nnewline",
            "with\r\nwindows newline",
            "with%percent",
            "complex=value\nwith%all=special\rchars",
            "",
            "===",
            "\n\n\n",
            "100% done = success\nNext line",
        ];

        for original in &test_cases {
            let escaped = escape_metadata_value(original);
            let unescaped = unescape_metadata_value(&escaped);
            assert_eq!(
                original, &unescaped,
                "Roundtrip failed for: {:?}",
                original
            );
        }
    }

    #[test]
    fn test_event_to_metadata_block_prompt() {
        let event = ConversationEvent::Prompt {
            session_id: "sess123".to_string(),
            agent_type: AgentType::ClaudeCode,
            turn: 1,
            source: TurnSource::User,
            content: "Fix the bug".to_string(),
            injected_refs: vec!["file1.rs".to_string()],
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            repo_id: None,
            cwd: None,
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-conversation]"));
        assert!(block.contains("event=prompt"));
        assert!(block.contains("session=sess123"));
        assert!(block.contains("agent_type=claude-code"));
        assert!(block.contains("turn=1"));
        assert!(block.contains("source=user"));
        assert!(block.contains("content=Fix the bug"));
        assert!(block.contains("[/aiki-conversation]"));
    }

    #[test]
    fn test_event_to_metadata_block_response() {
        let event = ConversationEvent::Response {
            session_id: "sess123".to_string(),
            agent_type: AgentType::ClaudeCode,
            turn: 2,
            files_written: vec!["auth.rs".to_string(), "tests.rs".to_string()],
            summary: Some("Updated auth module".to_string()),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            repo_id: None,
            cwd: None,
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("event=response"));
        assert!(block.contains("turn=2"));
        assert!(block.contains("files_written=auth.rs"));
        assert!(block.contains("summary=Updated auth module"));
    }

    #[test]
    fn test_parse_metadata_block_prompt() {
        let block = r#"
event=prompt
session=sess123
agent_type=claude-code
turn=3
source=user
content=Fix the bug
injected_ref=file1.rs
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Prompt {
                session_id,
                agent_type,
                turn,
                source,
                content,
                injected_refs,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(agent_type, AgentType::ClaudeCode);
                assert_eq!(turn, 3);
                assert_eq!(source, TurnSource::User);
                assert_eq!(content, "Fix the bug");
                assert_eq!(injected_refs, vec!["file1.rs"]);
            }
            _ => panic!("Expected Prompt event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_prompt_with_autoreply_source() {
        let block = r#"
event=prompt
session=sess456
agent_type=claude-code
turn=5
source=autoreply
content=Continue
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Prompt {
                session_id,
                turn,
                source,
                ..
            } => {
                assert_eq!(session_id, "sess456");
                assert_eq!(turn, 5);
                assert_eq!(source, TurnSource::Autoreply);
            }
            _ => panic!("Expected Prompt event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_prompt_defaults() {
        // Test that missing turn/source fields get sensible defaults
        let block = r#"
event=prompt
session=sess789
agent_type=claude-code
content=Old prompt
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Prompt {
                session_id,
                turn,
                source,
                ..
            } => {
                assert_eq!(session_id, "sess789");
                assert_eq!(turn, 0); // Default when missing
                assert_eq!(source, TurnSource::User); // Default when missing
            }
            _ => panic!("Expected Prompt event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_response() {
        let block = r#"
event=response
session=sess123
agent_type=claude-code
turn=3
files_written=auth.rs
files_written=tests.rs
summary=Updated auth
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Response {
                session_id,
                turn,
                files_written,
                summary,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(turn, 3);
                assert_eq!(files_written, vec!["auth.rs", "tests.rs"]);
                assert_eq!(summary, Some("Updated auth".to_string()));
            }
            _ => panic!("Expected Response event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_response_defaults() {
        // Test that missing turn field gets sensible default
        let block = r#"
event=response
session=sess123
agent_type=claude-code
files_written=auth.rs
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Response {
                turn,
                ..
            } => {
                assert_eq!(turn, 0); // Default when missing
            }
            _ => panic!("Expected Response event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_session_start() {
        let block = r#"
event=session_start
session=sess123
agent_type=cursor
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::SessionStart {
                session_id,
                agent_type,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(agent_type, AgentType::Cursor);
            }
            _ => panic!("Expected SessionStart event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_session_end() {
        let block = r#"
event=session_end
session=sess123
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::SessionEnd { session_id, .. } => {
                assert_eq!(session_id, "sess123");
            }
            _ => panic!("Expected SessionEnd event"),
        }
    }

    #[test]
    fn test_roundtrip_prompt() {
        let original = ConversationEvent::Prompt {
            session_id: "test".to_string(),
            agent_type: AgentType::Gemini,
            turn: 7,
            source: TurnSource::Autoreply,
            content: "Test prompt with = special\nchars".to_string(),
            injected_refs: vec!["ref1.rs".to_string(), "ref2.rs".to_string()],
            timestamp: Utc::now(),
            repo_id: Some("testrepo123".to_string()),
            cwd: Some("/test/path".to_string()),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find(METADATA_START).unwrap() + METADATA_START.len();
        let end = block.find(METADATA_END).unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                ConversationEvent::Prompt {
                    session_id: id1,
                    turn: turn1,
                    source: source1,
                    content: c1,
                    ..
                },
                ConversationEvent::Prompt {
                    session_id: id2,
                    turn: turn2,
                    source: source2,
                    content: c2,
                    ..
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(turn1, turn2);
                assert_eq!(source1, source2);
                assert_eq!(c1, c2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_response() {
        let original = ConversationEvent::Response {
            session_id: "test".to_string(),
            agent_type: AgentType::ClaudeCode,
            turn: 4,
            files_written: vec!["b.rs".to_string()],
            summary: Some("Summary text".to_string()),
            timestamp: Utc::now(),
            repo_id: Some("abc123".to_string()),
            cwd: Some("/path/to/project".to_string()),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find(METADATA_START).unwrap() + METADATA_START.len();
        let end = block.find(METADATA_END).unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                ConversationEvent::Response {
                    turn: turn1,
                    files_written: fw1,
                    summary: s1,
                    ..
                },
                ConversationEvent::Response {
                    turn: turn2,
                    files_written: fw2,
                    summary: s2,
                    ..
                },
            ) => {
                assert_eq!(turn1, turn2);
                assert_eq!(fw1, fw2);
                assert_eq!(s1, s2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_event_to_metadata_block_autoreply() {
        let event = ConversationEvent::Autoreply {
            session_id: "sess456".to_string(),
            agent_type: AgentType::ClaudeCode,
            turn: 3,
            content: "Continue with the implementation".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            repo_id: None,
            cwd: None,
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-conversation]"));
        assert!(block.contains("event=autoreply"));
        assert!(block.contains("session=sess456"));
        assert!(block.contains("agent_type=claude-code"));
        assert!(block.contains("turn=3"));
        assert!(block.contains("content=Continue with the implementation"));
        assert!(block.contains("[/aiki-conversation]"));
    }

    #[test]
    fn test_parse_metadata_block_autoreply() {
        let block = r#"
event=autoreply
session=sess789
agent_type=claude-code
turn=5
content=Continue with the task
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Autoreply {
                session_id,
                agent_type,
                turn,
                content,
                ..
            } => {
                assert_eq!(session_id, "sess789");
                assert_eq!(agent_type, AgentType::ClaudeCode);
                assert_eq!(turn, 5);
                assert_eq!(content, "Continue with the task");
            }
            _ => panic!("Expected Autoreply event"),
        }
    }

    #[test]
    fn test_roundtrip_autoreply() {
        let original = ConversationEvent::Autoreply {
            session_id: "test".to_string(),
            agent_type: AgentType::ClaudeCode,
            turn: 2,
            content: "Autoreply content with = special\nchars".to_string(),
            timestamp: Utc::now(),
            repo_id: Some("def456".to_string()),
            cwd: Some("/some/path".to_string()),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find(METADATA_START).unwrap() + METADATA_START.len();
        let end = block.find(METADATA_END).unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                ConversationEvent::Autoreply {
                    session_id: id1,
                    agent_type: at1,
                    turn: turn1,
                    content: c1,
                    ..
                },
                ConversationEvent::Autoreply {
                    session_id: id2,
                    agent_type: at2,
                    turn: turn2,
                    content: c2,
                    ..
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(at1, at2);
                assert_eq!(turn1, turn2);
                assert_eq!(c1, c2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_get_current_turn_info_no_jj_repo() {
        // Without a JJ repo, get_current_turn_info should return defaults
        let tmp = tempfile::TempDir::new().unwrap();
        let result = super::get_current_turn_info(tmp.path(), "test-session");

        // Should return default values when no JJ repo exists
        // The function handles errors gracefully by returning Err, but the caller
        // in turn_completed.rs uses unwrap_or((0, TurnSource::User))
        match result {
            Ok((turn, source)) => {
                // If somehow JJ exists, check we got sensible defaults
                assert_eq!(turn, 0);
                assert_eq!(source, TurnSource::User);
            }
            Err(_) => {
                // Expected - no JJ repo means command fails
            }
        }
    }

    #[test]
    fn test_has_pending_autoreply_no_jj_repo() {
        // Without a JJ repo, has_pending_autoreply should return false or error
        let tmp = tempfile::TempDir::new().unwrap();
        let result = super::has_pending_autoreply(tmp.path(), "test-session");

        // Should handle gracefully when no JJ repo exists
        match result {
            Ok(has_pending) => {
                // If no error, should be false (no branch = no pending autoreply)
                assert!(!has_pending);
            }
            Err(_) => {
                // Expected - no JJ repo means command fails
            }
        }
    }
}

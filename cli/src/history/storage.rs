//! Event storage on aiki/conversations branch
//!
//! Conversation events are stored as fileless JJ changes on the `aiki/conversations` branch.
//! Each event is a JJ change with metadata in the description.

use crate::error::{AikiError, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use super::types::{AgentType, ConversationEvent, CONVERSATIONS_BRANCH, METADATA_END, METADATA_START};

/// Ensure the aiki/conversations branch exists
pub fn ensure_conversations_branch(cwd: &Path) -> Result<()> {
    // Check if branch exists by listing bookmarks
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "list", "--all"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);

    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Create the branch as an orphan (no parent) starting from root()
        let result = Command::new("jj")
            .current_dir(cwd)
            .args(["bookmark", "create", CONVERSATIONS_BRANCH, "-r", "root()"])
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
pub fn write_event(cwd: &Path, event: &ConversationEvent) -> Result<()> {
    ensure_conversations_branch(cwd)?;

    let metadata = event_to_metadata_block(event);

    // Create a new change as child of aiki/conversations WITHOUT switching working copy
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["new", CONVERSATIONS_BRANCH, "--no-edit", "-m", &metadata])
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
    let result = Command::new("jj")
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

/// Get the change_id of the latest prompt event for a given session
///
/// Used by `--source prompt` to automatically resolve to the triggering prompt.
/// Returns None if no prompt events found for the session.
pub fn get_latest_prompt_change_id(cwd: &Path, session_id: &str) -> Result<Option<String>> {
    // Check if branch exists first
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "list", "--all"])
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
    // - session_id=<id> ensures it's from the right session
    let output = Command::new("jj")
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!(
                "ancestors({}) & description(substring:'{}') & description(substring:'event=prompt') & description(substring:'session_id={}')",
                CONVERSATIONS_BRANCH, METADATA_START, session_id
            ),
            "--no-graph",
            "-T",
            "change_id ++ \"\\n\"",
            "--limit",
            "1",
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

/// Read all conversation events from the aiki/conversations branch
#[allow(dead_code)] // Part of history API
pub fn read_events(cwd: &Path) -> Result<Vec<ConversationEvent>> {
    // Check if branch exists first
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "list", "--all"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to list bookmarks: {}", e)))?;

    let bookmarks = String::from_utf8_lossy(&output.stdout);
    if !bookmarks.contains(CONVERSATIONS_BRANCH) {
        // Branch doesn't exist yet, return empty list
        return Ok(Vec::new());
    }

    // Read all changes on the branch, oldest first
    let output = Command::new("jj")
        .current_dir(cwd)
        .args([
            "log",
            "-r",
            &format!("root()..{}", CONVERSATIONS_BRANCH),
            "--no-graph",
            "-T",
            "description ++ \"\\n---EVENT-SEPARATOR---\\n\"",
            "--reversed",
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

/// Convert a ConversationEvent to a metadata block string
fn event_to_metadata_block(event: &ConversationEvent) -> String {
    let mut lines = vec![METADATA_START.to_string()];

    match event {
        ConversationEvent::Prompt {
            session_id,
            agent_type,
            content,
            injected_refs,
            timestamp,
        } => {
            add_metadata("event", "prompt", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata_escaped("content", content, &mut lines);
            add_metadata_list("injected_ref", injected_refs, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::Response {
            session_id,
            agent_type,
            files_written,
            summary,
            timestamp,
        } => {
            add_metadata("event", "response", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata_list("files_written", files_written, &mut lines);
            if let Some(s) = summary {
                add_metadata_escaped("summary", s, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionStart {
            session_id,
            agent_type,
            timestamp,
        } => {
            add_metadata("event", "session_start", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionEnd {
            session_id,
            timestamp,
        } => {
            add_metadata("event", "session_end", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
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

    match *event_type {
        "prompt" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);
            let content = fields
                .get("content")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s))
                .unwrap_or_default();
            let injected_refs = parse_list_field(&fields, "injected_ref");

            Some(ConversationEvent::Prompt {
                session_id,
                agent_type,
                content,
                injected_refs,
                timestamp,
            })
        }
        "response" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);
            let files_written = parse_list_field(&fields, "files_written");
            let summary = fields
                .get("summary")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));

            Some(ConversationEvent::Response {
                session_id,
                agent_type,
                files_written,
                summary,
                timestamp,
            })
        }
        "session_start" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .and_then(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Unknown);

            Some(ConversationEvent::SessionStart {
                session_id,
                agent_type,
                timestamp,
            })
        }
        "session_end" => {
            let session_id = fields.get("session_id")?.first()?.to_string();

            Some(ConversationEvent::SessionEnd {
                session_id,
                timestamp,
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
            content: "Fix the bug".to_string(),
            injected_refs: vec!["file1.rs".to_string()],
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("[aiki-conversation]"));
        assert!(block.contains("event=prompt"));
        assert!(block.contains("session_id=sess123"));
        assert!(block.contains("agent_type=claude-code"));
        assert!(block.contains("content=Fix the bug"));
        assert!(block.contains("[/aiki-conversation]"));
    }

    #[test]
    fn test_event_to_metadata_block_response() {
        let event = ConversationEvent::Response {
            session_id: "sess123".to_string(),
            agent_type: AgentType::ClaudeCode,
            files_written: vec!["auth.rs".to_string(), "tests.rs".to_string()],
            summary: Some("Updated auth module".to_string()),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("event=response"));
        assert!(block.contains("files_written=auth.rs"));
        assert!(block.contains("summary=Updated auth module"));
    }

    #[test]
    fn test_parse_metadata_block_prompt() {
        let block = r#"
event=prompt
session_id=sess123
agent_type=claude-code
content=Fix the bug
injected_ref=file1.rs
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Prompt {
                session_id,
                agent_type,
                content,
                injected_refs,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(agent_type, AgentType::ClaudeCode);
                assert_eq!(content, "Fix the bug");
                assert_eq!(injected_refs, vec!["file1.rs"]);
            }
            _ => panic!("Expected Prompt event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_response() {
        let block = r#"
event=response
session_id=sess123
agent_type=claude-code
files_written=auth.rs
files_written=tests.rs
summary=Updated auth
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Response {
                session_id,
                files_written,
                summary,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(files_written, vec!["auth.rs", "tests.rs"]);
                assert_eq!(summary, Some("Updated auth".to_string()));
            }
            _ => panic!("Expected Response event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_session_start() {
        let block = r#"
event=session_start
session_id=sess123
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
session_id=sess123
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
            content: "Test prompt with = special\nchars".to_string(),
            injected_refs: vec!["ref1.rs".to_string(), "ref2.rs".to_string()],
            timestamp: Utc::now(),
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
                    content: c1,
                    ..
                },
                ConversationEvent::Prompt {
                    session_id: id2,
                    content: c2,
                    ..
                },
            ) => {
                assert_eq!(id1, id2);
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
            files_written: vec!["b.rs".to_string()],
            summary: Some("Summary text".to_string()),
            timestamp: Utc::now(),
        };

        let block = event_to_metadata_block(&original);
        let start = block.find(METADATA_START).unwrap() + METADATA_START.len();
        let end = block.find(METADATA_END).unwrap();
        let content = &block[start..end];

        let parsed = parse_metadata_block(content).expect("Should parse");

        match (original, parsed) {
            (
                ConversationEvent::Response {
                    files_written: fw1,
                    summary: s1,
                    ..
                },
                ConversationEvent::Response {
                    files_written: fw2,
                    summary: s2,
                    ..
                },
            ) => {
                assert_eq!(fw1, fw2);
                assert_eq!(s1, s2);
            }
            _ => panic!("Event type mismatch"),
        }
    }
}

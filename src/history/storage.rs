//! Event storage on aiki/conversations branch
//!
//! Conversation events are stored as fileless JJ changes on the `aiki/conversations` branch.
//! Each event is a JJ change with metadata in the description.

use crate::error::{AikiError, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use super::types::{
    AgentType, ConversationEvent, IntentSource, CONVERSATIONS_BRANCH, METADATA_END, METADATA_START,
};

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
pub fn write_event(cwd: &Path, event: &ConversationEvent) -> Result<()> {
    ensure_conversations_branch(cwd)?;

    let metadata = event_to_metadata_block(event);

    // Save current working copy change ID so we can restore it later
    let current_wc = Command::new("jj")
        .current_dir(cwd)
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to get working copy: {}", e)))?;

    if !current_wc.status.success() {
        let stderr = String::from_utf8_lossy(&current_wc.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to get working copy: {}",
            stderr
        )));
    }
    let saved_change_id = String::from_utf8_lossy(&current_wc.stdout).trim().to_string();

    // Create a new change as child of aiki/conversations bookmark and move working copy there
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["new", CONVERSATIONS_BRANCH, "-m", &metadata])
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

    // Move the bookmark forward to point at the newly created change (now @)
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["bookmark", "set", CONVERSATIONS_BRANCH, "-r", "@"])
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to update bookmark: {}", e)))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to update conversations bookmark: {}",
            stderr
        )));
    }

    // Restore the original working copy
    let result = Command::new("jj")
        .current_dir(cwd)
        .args(["edit", &saved_change_id])
        .output()
        .map_err(|e| {
            AikiError::JjCommandFailed(format!("Failed to restore working copy: {}", e))
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "Failed to restore working copy: {}",
            stderr
        )));
    }

    Ok(())
}

/// Read all conversation events from the aiki/conversations branch
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
            turn,
            agent_type,
            content,
            injected_refs,
            timestamp,
        } => {
            add_metadata("event", "prompt", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("turn", turn, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            add_metadata_escaped("content", content, &mut lines);
            add_metadata_list("injected_ref", injected_refs, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::Response {
            session_id,
            turn,
            agent_type,
            first_change_id,
            last_change_id,
            intent,
            intent_source,
            duration_ms,
            files_read,
            files_written,
            tools_used,
            summary,
            timestamp,
        } => {
            add_metadata("event", "response", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("turn", turn, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            if let Some(id) = first_change_id {
                add_metadata("first_change_id", id, &mut lines);
            }
            if let Some(id) = last_change_id {
                add_metadata("last_change_id", id, &mut lines);
            }
            if let Some(i) = intent {
                add_metadata_escaped("intent", i, &mut lines);
            }
            if let Some(src) = intent_source {
                add_metadata("intent_source", src, &mut lines);
            }
            if let Some(ms) = duration_ms {
                add_metadata("duration_ms", ms, &mut lines);
            }
            add_metadata_list("files_read", files_read, &mut lines);
            add_metadata_list("files_written", files_written, &mut lines);
            add_metadata_list("tools_used", tools_used, &mut lines);
            if let Some(s) = summary {
                add_metadata_escaped("summary", s, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionStart {
            session_id,
            agent_type,
            resume_from,
            timestamp,
        } => {
            add_metadata("event", "session_start", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("agent_type", agent_type, &mut lines);
            if let Some(rf) = resume_from {
                add_metadata("resume_from", rf, &mut lines);
            }
            add_metadata_timestamp(timestamp, &mut lines);
        }
        ConversationEvent::SessionEnd {
            session_id,
            total_turns,
            timestamp,
        } => {
            add_metadata("event", "session_end", &mut lines);
            add_metadata("session_id", session_id, &mut lines);
            add_metadata("total_turns", total_turns, &mut lines);
            add_metadata_timestamp(timestamp, &mut lines);
        }
    }

    lines.push(METADATA_END.to_string());
    lines.join("\n")
}

/// Parse list values from metadata fields
fn parse_list_field(fields: &HashMap<&str, Vec<&str>>, key: &str) -> Vec<String> {
    fields
        .get(key)
        .map(|v| v.iter().map(|s| unescape_metadata_value(s)).collect())
        .unwrap_or_default()
}

/// Parse a metadata block into a ConversationEvent
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
            let turn = fields
                .get("turn")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .map(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Other("unknown".to_string()));
            let content = fields
                .get("content")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s))
                .unwrap_or_default();
            let injected_refs = parse_list_field(&fields, "injected_ref");

            Some(ConversationEvent::Prompt {
                session_id,
                turn,
                agent_type,
                content,
                injected_refs,
                timestamp,
            })
        }
        "response" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let turn = fields
                .get("turn")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .map(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Other("unknown".to_string()));
            let first_change_id = fields
                .get("first_change_id")
                .and_then(|v| v.first())
                .map(|s| s.to_string());
            let last_change_id = fields
                .get("last_change_id")
                .and_then(|v| v.first())
                .map(|s| s.to_string());
            let intent = fields
                .get("intent")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));
            let intent_source = fields
                .get("intent_source")
                .and_then(|v| v.first())
                .and_then(|s| IntentSource::from_str(s));
            let duration_ms = fields
                .get("duration_ms")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok());
            let files_read = parse_list_field(&fields, "files_read");
            let files_written = parse_list_field(&fields, "files_written");
            let tools_used = parse_list_field(&fields, "tools_used");
            let summary = fields
                .get("summary")
                .and_then(|v| v.first())
                .map(|s| unescape_metadata_value(s));

            Some(ConversationEvent::Response {
                session_id,
                turn,
                agent_type,
                first_change_id,
                last_change_id,
                intent,
                intent_source,
                duration_ms,
                files_read,
                files_written,
                tools_used,
                summary,
                timestamp,
            })
        }
        "session_start" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let agent_type = fields
                .get("agent_type")
                .and_then(|v| v.first())
                .map(|s| AgentType::from_str(s))
                .unwrap_or(AgentType::Other("unknown".to_string()));
            let resume_from = fields
                .get("resume_from")
                .and_then(|v| v.first())
                .map(|s| s.to_string());

            Some(ConversationEvent::SessionStart {
                session_id,
                agent_type,
                resume_from,
                timestamp,
            })
        }
        "session_end" => {
            let session_id = fields.get("session_id")?.first()?.to_string();
            let total_turns = fields
                .get("total_turns")
                .and_then(|v| v.first())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            Some(ConversationEvent::SessionEnd {
                session_id,
                total_turns,
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
            turn: 1,
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
        assert!(block.contains("turn=1"));
        assert!(block.contains("agent_type=claude-code"));
        assert!(block.contains("content=Fix the bug"));
        assert!(block.contains("[/aiki-conversation]"));
    }

    #[test]
    fn test_event_to_metadata_block_response() {
        let event = ConversationEvent::Response {
            session_id: "sess123".to_string(),
            turn: 1,
            agent_type: AgentType::ClaudeCode,
            first_change_id: Some("abc123".to_string()),
            last_change_id: Some("def456".to_string()),
            intent: Some("Fixed authentication bug".to_string()),
            intent_source: Some(IntentSource::AgentSummary),
            duration_ms: Some(5000),
            files_read: vec!["auth.rs".to_string()],
            files_written: vec!["auth.rs".to_string(), "tests.rs".to_string()],
            tools_used: vec!["Edit".to_string()],
            summary: Some("Updated auth module".to_string()),
            timestamp: DateTime::parse_from_rfc3339("2026-01-09T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let block = event_to_metadata_block(&event);
        assert!(block.contains("event=response"));
        assert!(block.contains("first_change_id=abc123"));
        assert!(block.contains("intent=Fixed authentication bug"));
        assert!(block.contains("intent_source=agent_summary"));
    }

    #[test]
    fn test_parse_metadata_block_prompt() {
        let block = r#"
event=prompt
session_id=sess123
turn=1
agent_type=claude-code
content=Fix the bug
injected_ref=file1.rs
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Prompt {
                session_id,
                turn,
                agent_type,
                content,
                injected_refs,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(turn, 1);
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
turn=1
agent_type=claude-code
first_change_id=abc123
last_change_id=def456
intent=Fixed bug
intent_source=agent_summary
duration_ms=5000
files_read=auth.rs
files_written=auth.rs
files_written=tests.rs
tools_used=Edit
summary=Updated auth
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::Response {
                session_id,
                turn,
                first_change_id,
                intent,
                intent_source,
                duration_ms,
                files_written,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(turn, 1);
                assert_eq!(first_change_id, Some("abc123".to_string()));
                assert_eq!(intent, Some("Fixed bug".to_string()));
                assert_eq!(intent_source, Some(IntentSource::AgentSummary));
                assert_eq!(duration_ms, Some(5000));
                assert_eq!(files_written, vec!["auth.rs", "tests.rs"]);
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
                resume_from,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(agent_type, AgentType::Cursor);
                assert!(resume_from.is_none());
            }
            _ => panic!("Expected SessionStart event"),
        }
    }

    #[test]
    fn test_parse_metadata_block_session_end() {
        let block = r#"
event=session_end
session_id=sess123
total_turns=5
timestamp=2026-01-09T10:30:00Z
"#;

        let event = parse_metadata_block(block).expect("Should parse");
        match event {
            ConversationEvent::SessionEnd {
                session_id,
                total_turns,
                ..
            } => {
                assert_eq!(session_id, "sess123");
                assert_eq!(total_turns, 5);
            }
            _ => panic!("Expected SessionEnd event"),
        }
    }

    #[test]
    fn test_roundtrip_prompt() {
        let original = ConversationEvent::Prompt {
            session_id: "test".to_string(),
            turn: 3,
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
                    turn: t1,
                    content: c1,
                    ..
                },
                ConversationEvent::Prompt {
                    session_id: id2,
                    turn: t2,
                    content: c2,
                    ..
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(t1, t2);
                assert_eq!(c1, c2);
            }
            _ => panic!("Event type mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_response() {
        let original = ConversationEvent::Response {
            session_id: "test".to_string(),
            turn: 2,
            agent_type: AgentType::ClaudeCode,
            first_change_id: Some("abc".to_string()),
            last_change_id: Some("def".to_string()),
            intent: Some("Did something = important\nwith newlines".to_string()),
            intent_source: Some(IntentSource::ExplicitTag),
            duration_ms: Some(1234),
            files_read: vec!["a.rs".to_string()],
            files_written: vec!["b.rs".to_string()],
            tools_used: vec!["Edit".to_string(), "Read".to_string()],
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
                    intent: i1,
                    files_written: fw1,
                    ..
                },
                ConversationEvent::Response {
                    intent: i2,
                    files_written: fw2,
                    ..
                },
            ) => {
                assert_eq!(i1, i2);
                assert_eq!(fw1, fw2);
            }
            _ => panic!("Event type mismatch"),
        }
    }
}

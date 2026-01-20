//! History recording with size handling
//!
//! Provides functions to record conversation events with:
//! - Content truncation for large prompts/responses

use chrono::{DateTime, Utc};
use std::path::Path;

use super::storage::write_event;
use super::types::ConversationEvent;
use crate::error::Result;
use crate::session::AikiSession;

// Size limits per design doc
const MAX_PROMPT_SIZE: usize = 64 * 1024; // 64KB
const MAX_SUMMARY_SIZE: usize = 4 * 1024; // 4KB
const MAX_FILES_LIST: usize = 100;

/// Truncate content with marker if too long (UTF-8 safe)
fn truncate_with_marker(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // "...[truncated]" is 14 bytes
        let target = max.saturating_sub(14);
        // Find a valid UTF-8 boundary at or before target bytes
        let truncate_at = s
            .char_indices()
            .map(|(i, c)| i + c.len_utf8())
            .take_while(|&end| end <= target)
            .last()
            .unwrap_or(0);
        format!("{}...[truncated]", &s[..truncate_at])
    }
}

/// Truncate a file list to max entries
fn truncate_file_list(files: Vec<String>) -> Vec<String> {
    if files.len() <= MAX_FILES_LIST {
        files
    } else {
        let mut truncated: Vec<String> = files.into_iter().take(MAX_FILES_LIST - 1).collect();
        truncated.push(format!("...and more (truncated at {})", MAX_FILES_LIST));
        truncated
    }
}

/// Record a session start event
pub fn record_session_start(cwd: &Path, session: &AikiSession, timestamp: DateTime<Utc>) -> Result<()> {
    let event = ConversationEvent::SessionStart {
        session_id: session.uuid().to_string(),
        agent_type: session.agent_type(),
        timestamp,
    };

    write_event(cwd, &event)?;
    Ok(())
}

/// Record a session end event
pub fn record_session_end(
    cwd: &Path,
    session: &AikiSession,
    timestamp: DateTime<Utc>,
) -> Result<()> {
    let event = ConversationEvent::SessionEnd {
        session_id: session.uuid().to_string(),
        timestamp,
    };

    write_event(cwd, &event)?;
    Ok(())
}

/// Record a prompt event
pub fn record_prompt(
    cwd: &Path,
    session: &AikiSession,
    content: &str,
    injected_refs: Vec<String>,
    timestamp: DateTime<Utc>,
) -> Result<()> {
    let event = ConversationEvent::Prompt {
        session_id: session.uuid().to_string(),
        agent_type: session.agent_type(),
        content: truncate_with_marker(content, MAX_PROMPT_SIZE),
        injected_refs: truncate_file_list(injected_refs),
        timestamp,
    };

    write_event(cwd, &event)
}

/// Record a response event
pub fn record_response(
    cwd: &Path,
    session: &AikiSession,
    response_text: &str,
    files_written: Vec<String>,
    timestamp: DateTime<Utc>,
) -> Result<()> {
    // Create summary (first paragraph, truncated)
    let summary = response_text
        .split("\n\n")
        .next()
        .map(|p| truncate_with_marker(p.trim(), MAX_SUMMARY_SIZE));

    let event = ConversationEvent::Response {
        session_id: session.uuid().to_string(),
        agent_type: session.agent_type(),
        files_written: truncate_file_list(files_written),
        summary,
        timestamp,
    };

    write_event(cwd, &event)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_with_marker() {
        // Short content unchanged
        assert_eq!(truncate_with_marker("hello", 100), "hello");

        // Long content truncated
        let long = "x".repeat(100);
        let truncated = truncate_with_marker(&long, 50);
        assert_eq!(truncated.len(), 50);
        assert!(truncated.ends_with("...[truncated]"));
    }

    #[test]
    fn test_truncate_with_marker_utf8_safe() {
        // Multi-byte UTF-8 characters (emoji is 4 bytes, 日 is 3 bytes)
        let utf8_str = "Hello 日本語 🎉 world";

        // Should not panic on any truncation point
        for max in 14..utf8_str.len() + 5 {
            let result = truncate_with_marker(utf8_str, max);
            // Result should be valid UTF-8 (would panic if not)
            assert!(result.is_ascii() || result.chars().count() > 0);
        }

        // Test with longer string to ensure truncation triggers
        // "aaaaaaaaaaaaaaaaaaa日本語" = 28 bytes (19 ASCII + 9 for 3 Japanese chars)
        let input = "aaaaaaaaaaaaaaaaaaa日本語";
        assert_eq!(input.len(), 28);
        // max=18 < 28 triggers truncation. target = 18 - 14 = 4 bytes
        let truncated = truncate_with_marker(input, 18);
        assert_eq!(truncated, "aaaa...[truncated]");

        // Edge case: truncate exactly at multi-byte boundary
        // 日 starts at byte 19. With max=27 (target=13), we get 13 'a's
        let truncated = truncate_with_marker(input, 27); // target=13
        assert_eq!(truncated, "aaaaaaaaaaaaa...[truncated]"); // 13 a's

        // max=20 (target=6) lands before 日, gets 6 'a's
        let truncated = truncate_with_marker(input, 20);
        assert_eq!(truncated, "aaaaaa...[truncated]"); // 6 a's

        // Test that truncation in middle of multi-byte char doesn't panic
        // Use a string where truncation point would be inside a multi-byte char
        let input2 = "a日本語aaaaaaaaaaaaaaaaaaaa"; // 1 + 9 + 20 = 30 bytes
        assert_eq!(input2.len(), 30);
        // max=17 (target=3) - 日 is at bytes 1-3, should truncate to just "a"
        let truncated = truncate_with_marker(input2, 17);
        assert_eq!(truncated, "a...[truncated]");
    }

    #[test]
    fn test_truncate_file_list() {
        // Short list unchanged
        let short: Vec<String> = (0..10).map(|i| format!("file{}.rs", i)).collect();
        assert_eq!(truncate_file_list(short.clone()).len(), 10);

        // Long list truncated
        let long: Vec<String> = (0..150).map(|i| format!("file{}.rs", i)).collect();
        let truncated = truncate_file_list(long);
        assert_eq!(truncated.len(), MAX_FILES_LIST);
        assert!(truncated.last().unwrap().contains("truncated"));
    }
}

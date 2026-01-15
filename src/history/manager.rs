//! Materialization and filtering for conversation history
//!
//! Provides functions to transform raw events into views (sessions, log entries)
//! and filter them based on various criteria.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use super::types::{AgentType, ConversationEvent, LogEntry, Session};

/// Materialize sessions from a list of events
pub fn materialize_sessions(events: &[ConversationEvent]) -> HashMap<String, Session> {
    let mut sessions: HashMap<String, Session> = HashMap::new();

    for event in events {
        match event {
            ConversationEvent::SessionStart {
                session_id,
                agent_type,
                timestamp,
                ..
            } => {
                sessions.insert(
                    session_id.clone(),
                    Session {
                        id: session_id.clone(),
                        agent_type: agent_type.clone(),
                        started_at: *timestamp,
                        ended_at: None,
                        turn_count: 0,
                        summary: None,
                    },
                );
            }
            ConversationEvent::SessionEnd {
                session_id,
                total_turns,
                timestamp,
            } => {
                if let Some(session) = sessions.get_mut(session_id) {
                    session.ended_at = Some(*timestamp);
                    session.turn_count = *total_turns;
                }
            }
            ConversationEvent::Prompt {
                session_id,
                turn,
                agent_type,
                content,
                timestamp,
                ..
            } => {
                // If we see a prompt without a session start, create an implicit session
                let session = sessions.entry(session_id.clone()).or_insert_with(|| Session {
                    id: session_id.clone(),
                    agent_type: agent_type.clone(),
                    started_at: *timestamp,
                    ended_at: None,
                    turn_count: 0,
                    summary: None,
                });

                // Update turn count if this is higher
                if *turn > session.turn_count {
                    session.turn_count = *turn;
                }

                // First prompt becomes the session summary
                if session.summary.is_none() && *turn == 1 {
                    // Take first line, truncated to 80 chars
                    let first_line = content.lines().next().unwrap_or("");
                    let summary = if first_line.len() > 80 {
                        format!("{}...", &first_line[..77])
                    } else {
                        first_line.to_string()
                    };
                    session.summary = Some(summary);
                }
            }
            ConversationEvent::Response {
                session_id,
                turn,
                agent_type,
                timestamp,
                ..
            } => {
                // If we see a response without a session, create an implicit session
                let session = sessions.entry(session_id.clone()).or_insert_with(|| Session {
                    id: session_id.clone(),
                    agent_type: agent_type.clone(),
                    started_at: *timestamp,
                    ended_at: None,
                    turn_count: 0,
                    summary: None,
                });

                // Update turn count if this is higher
                if *turn > session.turn_count {
                    session.turn_count = *turn;
                }
            }
        }
    }

    sessions
}

/// Materialize log entries from response events
pub fn materialize_log_entries(events: &[ConversationEvent]) -> Vec<LogEntry> {
    events
        .iter()
        .filter_map(|event| {
            if let ConversationEvent::Response {
                session_id,
                turn,
                agent_type,
                intent,
                files_written,
                first_change_id,
                timestamp,
                ..
            } = event
            {
                Some(LogEntry {
                    session_id: session_id.clone(),
                    turn: *turn,
                    agent_type: agent_type.clone(),
                    intent: intent.clone(),
                    files_written: files_written.clone(),
                    first_change_id: first_change_id.clone(),
                    timestamp: *timestamp,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Filter log entries based on various criteria
pub fn filter_log_entries(
    entries: Vec<LogEntry>,
    session: Option<&str>,
    agent: Option<&str>,
    files: Option<&str>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    query: Option<&str>,
) -> Vec<LogEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            // Filter by session
            if let Some(s) = session {
                if !entry.session_id.starts_with(s) {
                    return false;
                }
            }

            // Filter by agent type
            if let Some(a) = agent {
                let agent_type = AgentType::from_str(a);
                if entry.agent_type != agent_type {
                    return false;
                }
            }

            // Filter by files touched
            if let Some(f) = files {
                let has_file = entry.files_written.iter().any(|fw| fw.contains(f));
                if !has_file {
                    return false;
                }
            }

            // Filter by time range
            if let Some(s) = since {
                if entry.timestamp < s {
                    return false;
                }
            }
            if let Some(u) = until {
                if entry.timestamp > u {
                    return false;
                }
            }

            // Filter by query (search in intent)
            if let Some(q) = query {
                let q_lower = q.to_lowercase();
                let matches = entry
                    .intent
                    .as_ref()
                    .map(|i| i.to_lowercase().contains(&q_lower))
                    .unwrap_or(false);
                if !matches {
                    return false;
                }
            }

            true
        })
        .collect()
}

/// Get sessions filtered by agent type
pub fn get_sessions_by_agent(
    sessions: HashMap<String, Session>,
    agent: Option<&str>,
) -> Vec<Session> {
    let mut result: Vec<Session> = sessions
        .into_values()
        .filter(|s| {
            if let Some(a) = agent {
                let agent_type = AgentType::from_str(a);
                s.agent_type == agent_type
            } else {
                true
            }
        })
        .collect();

    // Sort by start time, most recent first
    result.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::IntentSource;

    fn make_session_start(
        session_id: &str,
        agent: AgentType,
        timestamp: DateTime<Utc>,
    ) -> ConversationEvent {
        ConversationEvent::SessionStart {
            session_id: session_id.to_string(),
            agent_type: agent,
            resume_from: None,
            timestamp,
        }
    }

    fn make_prompt(
        session_id: &str,
        turn: u32,
        agent: AgentType,
        content: &str,
        timestamp: DateTime<Utc>,
    ) -> ConversationEvent {
        ConversationEvent::Prompt {
            session_id: session_id.to_string(),
            turn,
            agent_type: agent,
            content: content.to_string(),
            injected_refs: vec![],
            timestamp,
        }
    }

    fn make_response(
        session_id: &str,
        turn: u32,
        agent: AgentType,
        intent: Option<&str>,
        files_written: Vec<&str>,
        timestamp: DateTime<Utc>,
    ) -> ConversationEvent {
        ConversationEvent::Response {
            session_id: session_id.to_string(),
            turn,
            agent_type: agent,
            first_change_id: Some(format!("change_{}", turn)),
            last_change_id: None,
            intent: intent.map(|s| s.to_string()),
            intent_source: Some(IntentSource::AgentSummary),
            duration_ms: Some(1000),
            files_read: vec![],
            files_written: files_written.iter().map(|s| s.to_string()).collect(),
            tools_used: vec!["Edit".to_string()],
            summary: None,
            timestamp,
        }
    }

    #[test]
    fn test_materialize_sessions_basic() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let events = vec![
            make_session_start("sess1", AgentType::ClaudeCode, t1),
            make_prompt("sess1", 1, AgentType::ClaudeCode, "Fix the bug", t1),
        ];

        let sessions = materialize_sessions(&events);
        assert_eq!(sessions.len(), 1);

        let session = sessions.get("sess1").unwrap();
        assert_eq!(session.id, "sess1");
        assert_eq!(session.agent_type, AgentType::ClaudeCode);
        assert_eq!(session.summary, Some("Fix the bug".to_string()));
    }

    #[test]
    fn test_materialize_sessions_implicit() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // No session start, just prompt and response
        let events = vec![
            make_prompt("sess1", 1, AgentType::Cursor, "Hello", t1),
            make_response("sess1", 1, AgentType::Cursor, Some("Greeted"), vec![], t1),
        ];

        let sessions = materialize_sessions(&events);
        assert_eq!(sessions.len(), 1);

        let session = sessions.get("sess1").unwrap();
        assert_eq!(session.turn_count, 1);
    }

    #[test]
    fn test_materialize_log_entries() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let events = vec![
            make_prompt("sess1", 1, AgentType::ClaudeCode, "Fix bug", t1),
            make_response(
                "sess1",
                1,
                AgentType::ClaudeCode,
                Some("Fixed auth"),
                vec!["auth.rs"],
                t1,
            ),
            make_prompt("sess1", 2, AgentType::ClaudeCode, "Add tests", t1),
            make_response(
                "sess1",
                2,
                AgentType::ClaudeCode,
                Some("Added tests"),
                vec!["test.rs"],
                t1,
            ),
        ];

        let entries = materialize_log_entries(&events);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].intent, Some("Fixed auth".to_string()));
        assert_eq!(entries[1].intent, Some("Added tests".to_string()));
    }

    #[test]
    fn test_filter_by_session() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let entries = vec![
            LogEntry {
                session_id: "sess1".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("First".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
            LogEntry {
                session_id: "sess2".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Second".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
        ];

        let filtered = filter_log_entries(entries, Some("sess1"), None, None, None, None, None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].session_id, "sess1");
    }

    #[test]
    fn test_filter_by_agent() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let entries = vec![
            LogEntry {
                session_id: "sess1".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Claude".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
            LogEntry {
                session_id: "sess2".to_string(),
                turn: 1,
                agent_type: AgentType::Cursor,
                intent: Some("Cursor".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
        ];

        let filtered =
            filter_log_entries(entries, None, Some("claude-code"), None, None, None, None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].agent_type, AgentType::ClaudeCode);
    }

    #[test]
    fn test_filter_by_files() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let entries = vec![
            LogEntry {
                session_id: "sess1".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Auth work".to_string()),
                files_written: vec!["src/auth.rs".to_string()],
                first_change_id: None,
                timestamp: t1,
            },
            LogEntry {
                session_id: "sess2".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Config work".to_string()),
                files_written: vec!["src/config.rs".to_string()],
                first_change_id: None,
                timestamp: t1,
            },
        ];

        let filtered = filter_log_entries(entries, None, None, Some("auth"), None, None, None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].intent, Some("Auth work".to_string()));
    }

    #[test]
    fn test_filter_by_query() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let entries = vec![
            LogEntry {
                session_id: "sess1".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Fixed authentication bug".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
            LogEntry {
                session_id: "sess2".to_string(),
                turn: 1,
                agent_type: AgentType::ClaudeCode,
                intent: Some("Added new feature".to_string()),
                files_written: vec![],
                first_change_id: None,
                timestamp: t1,
            },
        ];

        let filtered = filter_log_entries(entries, None, None, None, None, None, Some("auth"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].intent,
            Some("Fixed authentication bug".to_string())
        );
    }

    #[test]
    fn test_get_sessions_by_agent() {
        let t1 = DateTime::parse_from_rfc3339("2026-01-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-01-09T11:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let mut sessions = HashMap::new();
        sessions.insert(
            "sess1".to_string(),
            Session {
                id: "sess1".to_string(),
                agent_type: AgentType::ClaudeCode,
                started_at: t1,
                ended_at: None,
                turn_count: 1,
                summary: None,
            },
        );
        sessions.insert(
            "sess2".to_string(),
            Session {
                id: "sess2".to_string(),
                agent_type: AgentType::Cursor,
                started_at: t2,
                ended_at: None,
                turn_count: 2,
                summary: None,
            },
        );

        let filtered = get_sessions_by_agent(sessions, Some("cursor"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "sess2");
    }
}

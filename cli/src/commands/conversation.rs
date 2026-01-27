use crate::error::Result;
use crate::global;
use crate::history;
use crate::history::types::ConversationEvent;
use crate::session;
use clap::Subcommand;
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum ConversationCommands {
    /// List conversations
    List {
        /// Show only active conversations (with running agent process)
        #[arg(long)]
        active: bool,
        /// Maximum number of conversations to show (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Show turns for a specific conversation
    Show {
        /// Conversation/session ID (or unique prefix)
        id: String,
    },
}

pub fn run(command: ConversationCommands) -> Result<()> {
    match command {
        ConversationCommands::List { active, limit } => run_list(active, limit),
        ConversationCommands::Show { id } => run_show(&id),
    }
}

fn format_timestamp(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn run_list(active: bool, limit: usize) -> Result<()> {
    session::prune_dead_pid_sessions();
    let active_sessions = session::list_all_sessions()?;

    if active {
        // --active: show only active sessions, enriched with history data
        if active_sessions.is_empty() {
            println!("No conversations found");
            return Ok(());
        }

        // Load conversation history to get turn counts and timestamps
        let aiki_dir = global::global_aiki_dir();
        let conversations = history::storage::list_conversations(&aiki_dir, None)?;
        let conv_map: HashMap<&str, &history::types::ConversationSummary> = conversations
            .iter()
            .map(|c| (c.session_id.as_str(), c))
            .collect();

        println!(
            "{:<38}  {:<20}  {:<7}  {:<20}  {:<20}  {}",
            "CONVERSATION", "AGENT", "TURNS", "STARTED", "LAST ACTIVITY", "STATUS"
        );

        for s in active_sessions.iter().take(limit) {
            let agent_display = match s.parent_pid {
                Some(pid) => format!("{} ({})", s.agent, pid),
                None => s.agent.clone(),
            };

            let conv = conv_map.get(s.session_id.as_str());

            let turns = conv
                .map(|c| c.turn_count.to_string())
                .unwrap_or_else(|| "-".to_string());

            let started = if let Some(c) = conv {
                format_timestamp(&c.started_at)
            } else if let Some(dot_pos) = s.started_at.find('.') {
                s.started_at[..dot_pos].to_string()
            } else if let Some(plus_pos) = s.started_at.find('+') {
                s.started_at[..plus_pos].to_string()
            } else {
                s.started_at.clone()
            };

            let last_activity = conv
                .map(|c| format_timestamp(&c.last_activity))
                .unwrap_or_else(|| "-".to_string());

            println!(
                "{:<38}  {:<20}  {:<7}  {:<20}  {:<20}  {}",
                s.session_id, agent_display, turns, started, last_activity, "active"
            );
        }

        let count = active_sessions.len();
        println!(
            "\n{} conversation{}",
            count,
            if count == 1 { "" } else { "s" }
        );
    } else {
        // Default: show all conversations from JJ history
        let aiki_dir = global::global_aiki_dir();
        let conversations = history::storage::list_conversations(&aiki_dir, Some(limit))?;

        if conversations.is_empty() {
            println!("No conversations found");
            return Ok(());
        }

        // Build a lookup of active sessions by session_id
        let active_map: HashMap<String, &session::SessionInfo> = active_sessions
            .iter()
            .map(|s| (s.session_id.clone(), s))
            .collect();

        println!(
            "{:<38}  {:<20}  {:<7}  {:<20}  {:<20}  {}",
            "CONVERSATION", "AGENT", "TURNS", "STARTED", "LAST ACTIVITY", "STATUS"
        );

        for conv in &conversations {
            let active_session = active_map.get(&conv.session_id);

            let agent_display = if let Some(session_info) = active_session {
                match session_info.parent_pid {
                    Some(pid) => format!("{} ({})", conv.agent_type, pid),
                    None => conv.agent_type.to_string(),
                }
            } else {
                conv.agent_type.to_string()
            };

            let status = if active_session.is_some() {
                "active"
            } else {
                "ended"
            };

            println!(
                "{:<38}  {:<20}  {:<7}  {:<20}  {:<20}  {}",
                conv.session_id,
                agent_display,
                conv.turn_count,
                format_timestamp(&conv.started_at),
                format_timestamp(&conv.last_activity),
                status
            );
        }

        let count = conversations.len();
        println!(
            "\n{} conversation{}",
            count,
            if count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn event_session_id(event: &ConversationEvent) -> &str {
    match event {
        ConversationEvent::Prompt { session_id, .. }
        | ConversationEvent::Response { session_id, .. }
        | ConversationEvent::SessionStart { session_id, .. }
        | ConversationEvent::SessionEnd { session_id, .. }
        | ConversationEvent::Autoreply { session_id, .. } => session_id,
    }
}

fn run_show(id: &str) -> Result<()> {
    let aiki_dir = global::global_aiki_dir();
    let events = history::storage::read_events(&aiki_dir)?;

    // Filter events whose session_id starts with the given prefix
    let matching: Vec<&ConversationEvent> = events
        .iter()
        .filter(|e| event_session_id(e).starts_with(id))
        .collect();

    if matching.is_empty() {
        println!("No conversation found with ID: {}", id);
        return Ok(());
    }

    // Check for ambiguous prefix (multiple distinct session IDs)
    let mut session_ids: Vec<&str> = matching.iter().map(|e| event_session_id(e)).collect();
    session_ids.sort();
    session_ids.dedup();

    if session_ids.len() > 1 {
        println!("Ambiguous ID '{}', matches:", id);
        for sid in &session_ids {
            println!("  {}", sid);
        }
        return Ok(());
    }

    // Display events chronologically
    for event in &matching {
        match event {
            ConversationEvent::SessionStart {
                timestamp,
                agent_type,
                ..
            } => {
                println!("[{}] Session started ({})", format_timestamp(timestamp), agent_type);
            }
            ConversationEvent::Prompt {
                timestamp,
                turn,
                content,
                ..
            } => {
                println!(
                    "[{}] Turn {} (prompt):\n{}",
                    format_timestamp(timestamp),
                    turn,
                    content
                );
            }
            ConversationEvent::Response {
                timestamp,
                turn,
                summary,
                files_written,
                ..
            } => {
                println!(
                    "[{}] Turn {} (response):",
                    format_timestamp(timestamp),
                    turn,
                );
                if let Some(s) = summary {
                    println!("{}", s);
                }
                if !files_written.is_empty() {
                    println!("Files: {}", files_written.join(", "));
                }
            }
            ConversationEvent::SessionEnd {
                timestamp, reason, ..
            } => {
                println!("[{}] Session ended ({})", format_timestamp(timestamp), reason);
            }
            ConversationEvent::Autoreply {
                timestamp,
                turn,
                content,
                ..
            } => {
                println!(
                    "[{}] Turn {} (autoreply):\n{}",
                    format_timestamp(timestamp),
                    turn,
                    content
                );
            }
        }
    }

    Ok(())
}

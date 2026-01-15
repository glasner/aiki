//! `aiki sessions` command - Manage and list sessions

use crate::error::Result;
use crate::history::{get_sessions_by_agent, materialize_sessions, read_events};
use clap::Subcommand;
use std::env;

#[derive(Subcommand)]
pub enum SessionsCommands {
    /// List all sessions
    List {
        /// Filter by agent type (e.g., claude-code, cursor)
        #[arg(long)]
        agent: Option<String>,

        /// Output format: plain (default), json
        #[arg(long, default_value = "plain")]
        format: String,

        /// Maximum number of sessions to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

pub fn run(command: Option<SessionsCommands>) -> Result<()> {
    match command {
        Some(SessionsCommands::List {
            agent,
            format,
            limit,
        }) => run_list(agent, format, limit),
        None => run_list(None, "plain".to_string(), 20),
    }
}

fn run_list(agent: Option<String>, format: String, limit: usize) -> Result<()> {
    let cwd = env::current_dir()?;

    // Read all events
    let events = read_events(&cwd)?;

    // Materialize sessions
    let sessions_map = materialize_sessions(&events);

    // Filter by agent and sort
    let mut sessions = get_sessions_by_agent(sessions_map, agent.as_deref());

    // Apply limit
    sessions.truncate(limit);

    // Output
    match format.as_str() {
        "json" => output_json(&sessions),
        _ => output_plain(&sessions),
    }

    Ok(())
}

fn output_plain(sessions: &[crate::history::Session]) {
    if sessions.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!(
        "{:<12} {:<12} {:>6} {:<20} {}",
        "SESSION", "AGENT", "TURNS", "STARTED", "SUMMARY"
    );
    println!("{}", "-".repeat(80));

    for session in sessions {
        let id_short = if session.id.len() > 10 {
            format!("{}...", &session.id[..8])
        } else {
            session.id.clone()
        };

        let started = session.started_at.format("%Y-%m-%d %H:%M");
        let summary = session
            .summary
            .as_ref()
            .map(|s| {
                if s.len() > 30 {
                    format!("{}...", &s[..27])
                } else {
                    s.clone()
                }
            })
            .unwrap_or_else(|| "(no summary)".to_string());

        println!(
            "{:<12} {:<12} {:>6} {:<20} {}",
            id_short, session.agent_type, session.turn_count, started, summary
        );
    }
}

fn output_json(sessions: &[crate::history::Session]) {
    println!("[");
    for (i, session) in sessions.iter().enumerate() {
        let summary = session
            .summary
            .as_ref()
            .map(|s| format!("\"{}\"", escape_json(s)))
            .unwrap_or_else(|| "null".to_string());

        let ended_at = session
            .ended_at
            .map(|t| format!("\"{}\"", t.to_rfc3339()))
            .unwrap_or_else(|| "null".to_string());

        println!(
            "  {{\"id\": \"{}\", \"agent_type\": \"{}\", \"turn_count\": {}, \"started_at\": \"{}\", \"ended_at\": {}, \"summary\": {}}}{}",
            session.id,
            session.agent_type,
            session.turn_count,
            session.started_at.to_rfc3339(),
            ended_at,
            summary,
            if i < sessions.len() - 1 { "," } else { "" }
        );
    }
    println!("]");
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

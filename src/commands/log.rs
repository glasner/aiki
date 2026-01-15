//! `aiki log` command - View AI change log

use crate::error::Result;
use crate::history::{filter_log_entries, materialize_log_entries, read_events};
use chrono::{Duration, Utc};
use clap::Args;
use std::env;

#[derive(Args)]
pub struct LogArgs {
    /// Search query (filters by intent)
    #[arg()]
    pub query: Option<String>,

    /// Filter to changes that touched this file
    #[arg(long)]
    pub files: Option<String>,

    /// Filter to a specific session (prefix match)
    #[arg(long)]
    pub session: Option<String>,

    /// Filter by agent type (e.g., claude-code, cursor)
    #[arg(long)]
    pub agent: Option<String>,

    /// Show changes since (e.g., "1 week", "2 days", "3 hours")
    #[arg(long)]
    pub since: Option<String>,

    /// Show changes until (e.g., "1 week", "2 days", "3 hours")
    #[arg(long)]
    pub until: Option<String>,

    /// Maximum number of entries to show
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Output format: plain (default), json
    #[arg(long, default_value = "plain")]
    pub format: String,

    /// Show oldest first (default is newest first)
    #[arg(long)]
    pub reverse: bool,

    /// Show file statistics
    #[arg(long)]
    pub stat: bool,
}

pub fn run(args: LogArgs) -> Result<()> {
    let cwd = env::current_dir()?;

    // Read all events
    let events = read_events(&cwd)?;

    // Materialize log entries from response events
    let mut entries = materialize_log_entries(&events);

    // Parse time filters
    let since = args.since.as_ref().and_then(|s| parse_duration_ago(s));
    let until = args.until.as_ref().and_then(|s| parse_duration_ago(s));

    // Apply filters
    entries = filter_log_entries(
        entries,
        args.session.as_deref(),
        args.agent.as_deref(),
        args.files.as_deref(),
        since,
        until,
        args.query.as_deref(),
    );

    // Sort by timestamp (newest first by default)
    if args.reverse {
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    } else {
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    }

    // Apply limit
    entries.truncate(args.limit);

    // Output
    match args.format.as_str() {
        "json" => output_json(&entries, args.stat),
        _ => output_plain(&entries, args.stat),
    }

    Ok(())
}

fn parse_duration_ago(s: &str) -> Option<chrono::DateTime<Utc>> {
    // Parse strings like "1 week", "2 days", "3 hours"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }

    let amount: i64 = parts[0].parse().ok()?;
    let unit = parts[1].to_lowercase();

    let duration = match unit.as_str() {
        "week" | "weeks" => Duration::weeks(amount),
        "day" | "days" => Duration::days(amount),
        "hour" | "hours" => Duration::hours(amount),
        "minute" | "minutes" => Duration::minutes(amount),
        _ => return None,
    };

    Some(Utc::now() - duration)
}

fn output_plain(entries: &[crate::history::LogEntry], show_stat: bool) {
    if entries.is_empty() {
        println!("No AI changes found.");
        return;
    }

    for entry in entries {
        let change_id = entry
            .first_change_id
            .as_ref()
            .map(|s| &s[..8.min(s.len())])
            .unwrap_or("--------");

        let intent = entry.intent.as_deref().unwrap_or("(no intent)");
        let time = entry.timestamp.format("%Y-%m-%d %H:%M");

        println!(
            "{} {} [{}] {}",
            change_id, time, entry.agent_type, intent
        );

        if show_stat && !entry.files_written.is_empty() {
            for file in &entry.files_written {
                println!("    M {}", file);
            }
        }
    }
}

fn output_json(entries: &[crate::history::LogEntry], show_stat: bool) {
    // Simple JSON output without serde
    println!("[");
    for (i, entry) in entries.iter().enumerate() {
        let change_id = entry
            .first_change_id
            .as_ref()
            .map(|s| format!("\"{}\"", s))
            .unwrap_or_else(|| "null".to_string());

        let intent = entry
            .intent
            .as_ref()
            .map(|s| format!("\"{}\"", escape_json(s)))
            .unwrap_or_else(|| "null".to_string());

        let files_json = if show_stat {
            let files: Vec<String> = entry
                .files_written
                .iter()
                .map(|f| format!("\"{}\"", escape_json(f)))
                .collect();
            format!("[{}]", files.join(", "))
        } else {
            "[]".to_string()
        };

        println!(
            "  {{\"change_id\": {}, \"session_id\": \"{}\", \"turn\": {}, \"agent_type\": \"{}\", \"intent\": {}, \"timestamp\": \"{}\", \"files_written\": {}}}{}",
            change_id,
            entry.session_id,
            entry.turn,
            entry.agent_type,
            intent,
            entry.timestamp.to_rfc3339(),
            files_json,
            if i < entries.len() - 1 { "," } else { "" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_ago_weeks() {
        let result = parse_duration_ago("2 weeks");
        assert!(result.is_some());
        let expected = Utc::now() - Duration::weeks(2);
        let diff = (result.unwrap() - expected).num_seconds().abs();
        assert!(diff < 2);
    }

    #[test]
    fn test_parse_duration_ago_days() {
        let result = parse_duration_ago("3 days");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_duration_ago_hours() {
        let result = parse_duration_ago("5 hours");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_duration_ago_invalid() {
        assert!(parse_duration_ago("invalid").is_none());
        assert!(parse_duration_ago("2").is_none());
        assert!(parse_duration_ago("weeks").is_none());
        assert!(parse_duration_ago("2 months").is_none());
    }

    #[test]
    fn test_escape_json() {
        assert_eq!(escape_json("hello"), "hello");
        assert_eq!(escape_json("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_json("say \"hi\""), "say \\\"hi\\\"");
    }
}

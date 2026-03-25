use crate::error::{AikiError, Result};
use crate::global;
use crate::history;
use crate::history::types::ConversationEvent;
use crate::provenance::record::AgentType;
use crate::session::{self, SessionMode};
use clap::Subcommand;
use std::collections::HashMap;

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum SessionCommands {
    /// List sessions
    List {
        /// Show only active sessions (with running agent process)
        #[arg(long)]
        active: bool,
        /// Show only background sessions (created by `aiki run`)
        #[arg(long, conflicts_with = "interactive")]
        background: bool,
        /// Show only interactive sessions (user-driven)
        #[arg(long, conflicts_with = "background")]
        interactive: bool,
        /// Maximum number of sessions to show (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show turns for a specific session
    Show {
        /// Session ID (or unique prefix)
        id: String,
    },
    /// Wait for session(s) to complete
    Wait {
        /// Session IDs to wait for
        ids: Vec<String>,
        /// Return when any session completes (instead of waiting for all)
        #[arg(long)]
        any: bool,
        /// Output format (e.g., `id` for bare session IDs on stdout)
        #[arg(long, short = 'o')]
        output: Option<super::OutputFormat>,
    },
}

pub fn run(command: SessionCommands) -> Result<()> {
    match command {
        SessionCommands::List {
            active,
            background,
            interactive,
            limit,
        } => run_list(active, background, interactive, limit),
        SessionCommands::Show { id } => run_show(&id),
        SessionCommands::Wait { ids, any, output } => run_wait(ids, any, output),
    }
}

fn format_timestamp(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn format_time_only(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%H:%M").to_string()
}

fn format_duration_between(
    start: &chrono::DateTime<chrono::Utc>,
    end: &chrono::DateTime<chrono::Utc>,
) -> String {
    let secs = (*end - *start).num_seconds().max(0);
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h{}m", h, m)
        }
    }
}

const SEPARATOR_WIDTH: usize = 50;

struct SessionRow {
    session_id: String,
    agent_type: AgentType,
    mode: SessionMode,
    pid: Option<u32>,
    turns: String,
    started: String,
    last_activity: String,
    status: &'static str,
}

fn print_session_table(rows: &[SessionRow]) {
    println!(
        "{:<38}  {:<20}  {:<12}  {:<7}  {:<20}  {:<20}  {}",
        "SESSION", "AGENT", "TYPE", "TURNS", "STARTED", "LAST ACTIVITY", "STATUS"
    );

    for row in rows {
        let agent_display = match row.pid {
            Some(pid) => format!("{} ({})", row.agent_type, pid),
            None => row.agent_type.to_string(),
        };

        println!(
            "{:<38}  {:<20}  {:<12}  {:<7}  {:<20}  {:<20}  {}",
            row.session_id,
            agent_display,
            row.mode.to_string(),
            row.turns,
            row.started,
            row.last_activity,
            row.status
        );
    }

    let count = rows.len();
    println!("\n{} session{}", count, if count == 1 { "" } else { "s" });
}

fn run_list(active: bool, background: bool, interactive: bool, limit: Option<usize>) -> Result<()> {
    session::prune_dead_pid_sessions();
    let mut active_sessions = session::list_all_sessions()?;

    // Apply mode filter if specified
    if background {
        active_sessions.retain(|s| s.mode == SessionMode::Background);
    } else if interactive {
        active_sessions.retain(|s| s.mode == SessionMode::Interactive);
    }

    if active {
        // --active: show only active sessions, enriched with history data
        if active_sessions.is_empty() {
            println!("No sessions found");
            return Ok(());
        }

        // Load conversation history to get turn counts and timestamps
        let aiki_dir = global::global_aiki_dir();
        let conversations = history::storage::list_conversations(&aiki_dir, None)?;
        let conv_map: HashMap<&str, &history::types::ConversationSummary> = conversations
            .iter()
            .map(|c| (c.session_id.as_str(), c))
            .collect();

        let iter = active_sessions.iter();
        let rows: Vec<SessionRow> = match limit {
            Some(n) => iter.take(n).collect::<Vec<_>>(),
            None => iter.collect(),
        }
        .into_iter()
        .map(|s| {
            let agent_type = AgentType::from_str(&s.agent).unwrap_or(AgentType::Unknown);
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

            SessionRow {
                session_id: s.session_id.clone(),
                agent_type,
                mode: s.mode,
                pid: s.parent_pid,
                turns,
                started,
                last_activity,
                status: "active",
            }
        })
        .collect();

        print_session_table(&rows);
    } else {
        // Default: show all sessions from JJ history
        let aiki_dir = global::global_aiki_dir();
        let conversations = history::storage::list_conversations(&aiki_dir, None)?;

        if conversations.is_empty() {
            println!("No sessions found");
            return Ok(());
        }

        // Build a lookup of active sessions by session_id
        let active_map: HashMap<String, &session::SessionInfo> = active_sessions
            .iter()
            .map(|s| (s.session_id.clone(), s))
            .collect();

        let mut rows: Vec<SessionRow> = conversations
            .iter()
            .map(|conv| {
                let active_session = active_map.get(&conv.session_id);

                // Use active session mode if available, otherwise use mode from history,
                // falling back to Interactive for legacy events without session_mode
                let (pid, mode, status) = if let Some(session_info) = active_session {
                    (session_info.parent_pid, session_info.mode, "active")
                } else {
                    (
                        None,
                        conv.session_mode.unwrap_or(SessionMode::Interactive),
                        "ended",
                    )
                };

                SessionRow {
                    session_id: conv.session_id.clone(),
                    agent_type: conv.agent_type,
                    mode,
                    pid,
                    turns: conv.turn_count.to_string(),
                    started: format_timestamp(&conv.started_at),
                    last_activity: format_timestamp(&conv.last_activity),
                    status,
                }
            })
            .collect();

        // Apply mode filter if specified (to both active and ended sessions)
        if background {
            rows.retain(|r| r.mode == SessionMode::Background);
        } else if interactive {
            rows.retain(|r| r.mode == SessionMode::Interactive);
        }

        // Apply limit after filtering
        if let Some(n) = limit {
            rows.truncate(n);
        }

        print_session_table(&rows);
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
        println!("No session found with ID: {}", id);
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

    // Extract session metadata for the header
    let session_start = matching
        .iter()
        .find(|e| matches!(e, ConversationEvent::SessionStart { .. }));
    let session_end = matching
        .iter()
        .find(|e| matches!(e, ConversationEvent::SessionEnd { .. }));

    // Print header line: "Session: <agent> · <date> <start>–<end> (<duration>)"
    if let Some(ConversationEvent::SessionStart {
        timestamp: start_ts,
        agent_type,
        ..
    }) = session_start
    {
        let date = start_ts.format("%Y-%m-%d").to_string();
        let start_time = format_time_only(start_ts);

        let end_part = if let Some(ConversationEvent::SessionEnd {
            timestamp: end_ts, ..
        }) = session_end
        {
            let duration = format_duration_between(start_ts, end_ts);
            format!("\u{2013}{} ({})", format_time_only(end_ts), duration)
        } else {
            String::from(" (active)")
        };

        println!(
            "Session: {} \u{00b7} {} {}{}",
            agent_type, date, start_time, end_part
        );
    }

    // Group events by turn number for display
    let mut last_turn: Option<u32> = None;
    let mut last_prompt_ts: Option<chrono::DateTime<chrono::Utc>> = None;

    for event in &matching {
        match event {
            ConversationEvent::SessionStart { .. } | ConversationEvent::SessionEnd { .. } => {
                // Already handled in header; skip
            }
            ConversationEvent::Prompt {
                timestamp,
                turn,
                content,
                ..
            } => {
                // Print turn separator when turn number changes
                if last_turn != Some(*turn) {
                    println!(
                        "\n\u{2500}\u{2500} Turn {} {}",
                        turn,
                        "\u{2500}".repeat(SEPARATOR_WIDTH - 9 - turn.to_string().len())
                    );
                    last_turn = Some(*turn);
                }
                last_prompt_ts = Some(*timestamp);

                println!(
                    "\n  \u{1f9d1}\u{200d}\u{1f4bb} prompt  ({})",
                    format_time_only(timestamp)
                );
                for line in content.lines() {
                    println!("  {}", line);
                }
            }
            ConversationEvent::Response {
                timestamp,
                content,
                files_written,
                ..
            } => {
                let elapsed = last_prompt_ts
                    .map(|pt| format!(", {}", format_duration_between(&pt, timestamp)))
                    .unwrap_or_default();

                println!(
                    "\n  \u{1f916} response  ({}{})",
                    format_time_only(timestamp),
                    elapsed
                );
                if let Some(s) = content {
                    for line in s.lines() {
                        println!("  {}", line);
                    }
                }
                if !files_written.is_empty() {
                    println!("  Files: {}", files_written.join(", "));
                }
            }
            ConversationEvent::Autoreply {
                timestamp,
                turn,
                content,
                ..
            } => {
                if last_turn != Some(*turn) {
                    println!(
                        "\n\u{2500}\u{2500} Turn {} {}",
                        turn,
                        "\u{2500}".repeat(SEPARATOR_WIDTH - 9 - turn.to_string().len())
                    );
                    last_turn = Some(*turn);
                }

                println!("\n  \u{1f504} autoreply  ({})", format_time_only(timestamp));
                for line in content.lines() {
                    println!("  {}", line);
                }
            }
        }
    }

    // Footer separator
    println!("\n{}", "\u{2500}".repeat(SEPARATOR_WIDTH));

    Ok(())
}

const WAIT_INITIAL_DELAY_MS: u64 = 500;
const WAIT_MAX_DELAY_MS: u64 = 5000;
const WAIT_BACKOFF_MULTIPLIER: u64 = 2;
const WAIT_ABSORPTION_TIMEOUT_SECS: u64 = 60;

/// Resolve a session ID prefix to the full session ID from conversation events.
/// Returns an error if the prefix is ambiguous or not found.
fn resolve_session_id(events: &[ConversationEvent], prefix: &str) -> Result<String> {
    use std::collections::HashSet;

    let matching: HashSet<&str> = events
        .iter()
        .map(|e| event_session_id(e))
        .filter(|sid| sid.starts_with(prefix))
        .collect();

    match matching.len() {
        0 => Err(AikiError::InvalidArgument(format!(
            "No session found matching '{}'",
            prefix
        ))),
        1 => Ok(matching.into_iter().next().unwrap().to_string()),
        _ => {
            let mut ids: Vec<&str> = matching.into_iter().collect();
            ids.sort();
            Err(AikiError::InvalidArgument(format!(
                "Ambiguous session ID '{}', matches: {}",
                prefix,
                ids.join(", ")
            )))
        }
    }
}

/// Check if a session has ended by looking for a SessionEnd event.
fn has_session_ended(events: &[ConversationEvent], session_id: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, ConversationEvent::SessionEnd { session_id: sid, .. } if sid == session_id)
    })
}

fn run_wait(
    ids: Vec<String>,
    any: bool,
    output_format: Option<super::OutputFormat>,
) -> Result<()> {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    let aiki_dir = global::global_aiki_dir();

    if ids.is_empty() {
        return Err(AikiError::InvalidArgument(
            "At least one session ID is required".to_string(),
        ));
    }

    // Resolve all session IDs up front (prefix -> full)
    let events = history::storage::read_events(&aiki_dir)?;
    let mut resolved_ids = Vec::new();
    for id in &ids {
        resolved_ids.push(resolve_session_id(&events, id)?);
    }
    let ids = resolved_ids;

    let mut delay_ms = WAIT_INITIAL_DELAY_MS;

    // Poll until condition is met
    loop {
        let events = history::storage::read_events(&aiki_dir)?;

        let done = if any {
            ids.iter().any(|id| has_session_ended(&events, id))
        } else {
            ids.iter().all(|id| has_session_ended(&events, id))
        };

        if done {
            // Collect which sessions completed
            let completed: HashSet<&str> = ids
                .iter()
                .filter(|id| has_session_ended(&events, id))
                .map(|s| s.as_str())
                .collect();

            // Wait for workspace absorption
            {
                let repo_root = crate::jj::get_repo_root(&std::env::current_dir().map_err(
                    |e| AikiError::Other(anyhow::anyhow!("Failed to get cwd: {}", e)),
                )?)?;

                let absorption_start = Instant::now();
                let mut absorption_delay_ms = WAIT_INITIAL_DELAY_MS;

                let repo_id = crate::repos::ensure_repo_id(&repo_root)?;

                for session_id in &completed {
                    let workspace_name = format!("aiki-{}", session_id);

                    loop {
                        // Check if workspace still exists
                        let ws_exists = crate::session::isolation::find_workspace_change_id(
                            &repo_root,
                            &workspace_name,
                        )?
                        .is_some();

                        if !ws_exists {
                            break;
                        }

                        // Try to absorb the workspace
                        let workspace = crate::session::isolation::IsolatedWorkspace {
                            name: workspace_name.clone(),
                            path: crate::session::isolation::workspaces_dir()
                                .join(&repo_id)
                                .join(session_id),
                        };
                        match crate::session::isolation::absorb_workspace(
                            &repo_root,
                            &workspace,
                            None,
                        ) {
                            Ok(_) => break,
                            Err(_) => {
                                if absorption_start.elapsed()
                                    > Duration::from_secs(WAIT_ABSORPTION_TIMEOUT_SECS)
                                {
                                    eprintln!(
                                        "Warning: Workspace '{}' not absorbed after {}s. Run `jj workspace list` to check.",
                                        workspace_name, WAIT_ABSORPTION_TIMEOUT_SECS
                                    );
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(absorption_delay_ms));
                                absorption_delay_ms = (absorption_delay_ms
                                    * WAIT_BACKOFF_MULTIPLIER)
                                    .min(WAIT_MAX_DELAY_MS);
                            }
                        }
                    }
                }
            }

            // Output
            let output_id = matches!(output_format, Some(super::OutputFormat::Id));

            if output_id {
                for id in &ids {
                    if any && !has_session_ended(&events, id) {
                        continue;
                    }
                    println!("{}", id);
                }
            } else {
                // Re-read events for accurate output after absorption
                let events = history::storage::read_events(&aiki_dir)?;
                let conversations =
                    history::storage::list_conversations(&aiki_dir, None)?;
                let conv_map: HashMap<&str, &history::types::ConversationSummary> = conversations
                    .iter()
                    .map(|c| (c.session_id.as_str(), c))
                    .collect();

                let rows: Vec<SessionRow> = ids
                    .iter()
                    .filter(|id| !any || has_session_ended(&events, id))
                    .map(|id| {
                        let conv = conv_map.get(id.as_str());
                        SessionRow {
                            session_id: id.clone(),
                            agent_type: conv
                                .map(|c| c.agent_type)
                                .unwrap_or(AgentType::Unknown),
                            mode: conv
                                .and_then(|c| c.session_mode)
                                .unwrap_or(SessionMode::Interactive),
                            pid: None,
                            turns: conv
                                .map(|c| c.turn_count.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            started: conv
                                .map(|c| format_timestamp(&c.started_at))
                                .unwrap_or_else(|| "-".to_string()),
                            last_activity: conv
                                .map(|c| format_timestamp(&c.last_activity))
                                .unwrap_or_else(|| "-".to_string()),
                            status: "ended",
                        }
                    })
                    .collect();

                print_session_table(&rows);
            }

            return Ok(());
        }

        std::thread::sleep(Duration::from_millis(delay_ms));
        delay_ms = (delay_ms * WAIT_BACKOFF_MULTIPLIER).min(WAIT_MAX_DELAY_MS);
    }
}

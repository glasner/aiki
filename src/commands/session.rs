use crate::error::Result;
use crate::session;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List active sessions
    List,
}

pub fn run(command: SessionCommands) -> Result<()> {
    match command {
        SessionCommands::List => run_list(),
    }
}

fn run_list() -> Result<()> {
    session::prune_dead_pid_sessions();
    let sessions = session::list_all_sessions()?;

    if sessions.is_empty() {
        println!("No active sessions");
        return Ok(());
    }

    // Print table header
    println!(
        "{:<38}  {:<12}  {:<7}  {:<20}  {}",
        "SESSION", "AGENT", "PID", "STARTED", "REPOS"
    );

    for s in &sessions {
        let pid_str = match s.parent_pid {
            Some(pid) => pid.to_string(),
            None => "-".to_string(),
        };

        let repos_str = s.repos.len().to_string();

        // Truncate started_at to remove sub-second precision for readability
        let started = if let Some(dot_pos) = s.started_at.find('.') {
            // RFC3339 with fractional seconds: trim to seconds
            &s.started_at[..dot_pos]
        } else if let Some(plus_pos) = s.started_at.find('+') {
            &s.started_at[..plus_pos]
        } else {
            &s.started_at
        };

        println!(
            "{:<38}  {:<12}  {:<7}  {:<20}  {}",
            s.session_id, s.agent, pid_str, started, repos_str
        );
    }

    println!("\n{} session{}", sessions.len(), if sessions.len() == 1 { "" } else { "s" });

    Ok(())
}

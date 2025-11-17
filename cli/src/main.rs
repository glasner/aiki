mod authors;
mod blame;
mod commands;
mod config;
mod error;
mod event_bus;
mod events;
mod flows;
mod handlers;
mod jj;
mod provenance;
mod record_change;
mod repo;
mod sign_setup_wizard;
mod signing;
mod vendors;
mod verify;

use clap::{Parser, Subcommand};
use error::Result;

#[derive(Parser)]
#[command(name = "aiki")]
#[command(version)]
#[command(about = "AI code review engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Aiki in the current repository
    Init {
        /// Only print error and warning messages (suppress normal output)
        #[arg(short, long)]
        quiet: bool,
    },
    /// Diagnose and fix configuration issues
    Doctor {
        /// Automatically fix detected issues
        #[arg(long)]
        fix: bool,
    },
    /// Manage Aiki hooks
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },
    /// Record an AI-generated change (called by AI editor hooks)
    #[command(name = "record-change")]
    RecordChange {
        /// Record change from Claude Code
        #[arg(long)]
        claude_code: bool,
        /// Record change from Cursor
        #[arg(long)]
        cursor: bool,
        /// Run synchronously (for testing - waits for background thread)
        #[arg(long)]
        sync: bool,
    },
    /// Show line-by-line AI attribution for a file
    Blame {
        /// File to show blame for
        file: std::path::PathBuf,
        /// Filter by agent type (e.g., claude-code, cursor)
        #[arg(long)]
        agent: Option<String>,
        /// Verify cryptographic signatures on changes
        #[arg(long)]
        verify: bool,
    },
    /// Show authors who contributed to changes
    Authors {
        /// Scope changes: "staged" for Git staging area, default is working copy (@)
        #[arg(long)]
        changes: Option<String>,

        /// Output format: plain (default), git, json
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Verify cryptographic signature on a change
    Verify {
        /// Change ID or revision (defaults to @)
        #[arg(default_value = "@")]
        revision: String,
    },
}

#[derive(Subcommand)]
enum HooksCommands {
    /// Install global hooks for AI editors
    Install,
    /// Handle vendor event (called by all hooks)
    #[command(hide = true)]
    Handle {
        /// Agent type (e.g., claude-code, cursor)
        #[arg(long)]
        agent: String,
        /// Vendor event name (e.g., SessionStart, PostToolUse, beforeSubmitPrompt, afterFileEdit)
        #[arg(long)]
        event: String,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { quiet } => commands::init::run(quiet),
        Commands::Doctor { fix } => commands::doctor::run(fix),
        Commands::Hooks { command } => match command {
            HooksCommands::Install => commands::hooks::run_install(),
            HooksCommands::Handle { agent, event } => commands::hooks::run_handle(agent, event),
        },
        Commands::RecordChange {
            claude_code,
            cursor,
            sync,
        } => commands::record_change::run(claude_code, cursor, sync),
        Commands::Blame {
            file,
            agent,
            verify,
        } => commands::blame::run(file, agent, verify),
        Commands::Authors { changes, format } => commands::authors::run(changes, format),
        Commands::Verify { revision } => commands::verify::run(revision),
    }
}

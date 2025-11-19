mod acp;
mod authors;
mod blame;
mod commands;
mod config;
mod error;
mod event_bus;
mod events;
mod flows;
mod handlers;
mod ide_config;
mod jj;
mod provenance;
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
    /// ACP proxy for IDE-agent communication
    Acp {
        /// Agent type for provenance (e.g., "claude-code", "cursor", "gemini")
        agent_type: String,

        /// Optional custom binary path (defaults to derived from agent_type)
        #[arg(short, long)]
        bin: Option<String>,

        /// Optional arguments to pass to the agent executable
        #[arg(last = true)]
        agent_args: Vec<String>,
    },
    /// Run end-to-end performance benchmark
    Benchmark {
        /// Number of edits to simulate (default: 10)
        #[arg(short, long, default_value = "10")]
        edits: usize,
    },
    /// Dispatch Aiki events (internal use)
    #[command(hide = true)]
    Event {
        #[command(subcommand)]
        command: EventCommands,
    },
}

#[derive(Subcommand)]
enum EventCommands {
    /// Trigger PrepareCommitMessage event (for Git's prepare-commit-msg hook)
    #[command(name = "prepare-commit-msg")]
    PrepareCommitMessage,
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
        Commands::Blame {
            file,
            agent,
            verify,
        } => commands::blame::run(file, agent, verify),
        Commands::Authors { changes, format } => commands::authors::run(changes, format),
        Commands::Verify { revision } => commands::verify::run(revision),
        Commands::Acp {
            agent_type,
            bin,
            agent_args,
        } => commands::acp::run(agent_type, bin, agent_args),
        Commands::Benchmark { edits } => commands::benchmark::run(edits),
        Commands::Event { command } => match command {
            EventCommands::PrepareCommitMessage => commands::event::run_prepare_commit_message(),
        },
    }
}

mod agents;
mod authors;
mod blame;
mod cache;
mod commands;
mod config;
mod editors;
mod error;
mod event_bus;
mod events;
mod flows;
mod global;
mod history;
mod jj;
mod provenance;
mod repo;
mod repo_id;
mod session;
mod signing;
mod tasks;
mod tools;
mod utils;
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
    #[command(hide = true)]
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
    /// Run end-to-end performance benchmark
    Benchmark {
        /// Number of edits to simulate (default: 50)
        #[arg(short, long, default_value = "50")]
        edits: usize,
    },
    /// Manage sessions
    Session {
        #[command(subcommand)]
        command: commands::session::SessionCommands,
    },
    /// Manage tasks
    Task {
        #[command(subcommand)]
        command: Option<commands::task::TaskCommands>,
    },
    /// Dispatch Aiki events (internal use)
    #[command(hide = true)]
    Event {
        #[command(subcommand)]
        command: EventCommands,
    },
    /// Wait for a task to reach terminal state
    Wait {
        /// Task ID to wait for (reads from stdin if not provided)
        task_id: Option<String>,
    },
    /// Create and run followup tasks from review comments
    Fix {
        /// Task ID to read comments from (reads from stdin if not provided)
        task_id: Option<String>,
        /// Run followup task asynchronously
        #[arg(long = "async")]
        run_async: bool,
        /// Start task and return control to calling agent
        #[arg(long)]
        start: bool,
        /// Task template to use (default: aiki/fix)
        #[arg(long)]
        template: Option<String>,
        /// Agent for task assignment (default: claude-code)
        #[arg(long)]
        agent: Option<String>,
    },
    /// Create and run code review tasks
    Review {
        #[command(subcommand)]
        command: Option<commands::review::ReviewCommands>,
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
    /// Stdin integration point (Claude Code, Cursor - reads JSON from stdin)
    #[command(hide = true)]
    Stdin {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        event: String,
        #[arg(trailing_var_arg = true)]
        payload: Vec<String>,
    },
    /// ACP integration point (proxy for ACP protocol agents)
    #[command(hide = true)]
    Acp {
        #[arg(long)]
        agent: String,
        #[arg(short, long)]
        bin: Option<String>,
        #[arg(last = true)]
        agent_args: Vec<String>,
    },
    /// OTel integration point (Codex - reads HTTP/OTLP from stdin)
    #[command(hide = true)]
    Otel {
        #[arg(long, default_value = "codex")]
        agent: String,
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
            HooksCommands::Stdin { agent, event, payload } => {
                let payload_str = if payload.is_empty() { None } else { Some(payload.join(" ")) };
                commands::hooks::run_stdin(agent, event, payload_str)
            }
            HooksCommands::Acp { agent, bin, agent_args } => {
                commands::acp::run(agent, bin, agent_args)
            }
            HooksCommands::Otel { agent } => {
                commands::otel_receive::run(agent)
            }
        },
        Commands::Blame {
            file,
            agent,
            verify,
        } => commands::blame::run(file, agent, verify),
        Commands::Authors { changes, format } => commands::authors::run(changes, format),
        Commands::Verify { revision } => commands::verify::run(revision),
        Commands::Benchmark { edits } => commands::benchmark::run("aiki/core".to_string(), edits),
        Commands::Session { command } => commands::session::run(command),
        Commands::Task { command } => commands::task::run(command),
        Commands::Event { command } => match command {
            EventCommands::PrepareCommitMessage => commands::event::run_prepare_commit_message(),
        },
        Commands::Wait { task_id } => commands::wait::run(task_id),
        Commands::Fix {
            task_id,
            run_async,
            start,
            template,
            agent,
        } => commands::fix::run(task_id, run_async, start, template, agent),
        Commands::Review { command } => commands::review::run(command),
    }
}

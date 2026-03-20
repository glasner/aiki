mod agents;


mod cache;
mod commands;
mod config;
mod editors;
mod error;
mod event_bus;
mod expressions;
mod events;
mod flows;
mod parsing;
mod global;
mod history;
mod instructions;
mod jj;
mod output_utils;
mod plugins;
mod prerequisites;
mod provenance;
mod repos;

mod session;
mod plans;
mod tasks;
mod tools;
mod utils;
mod validation;
mod tui;

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
        /// Specify which instruction file to use (AGENTS.md or CLAUDE.md)
        #[arg(long, value_name = "FILE")]
        instructions_file: Option<String>,
    },
    /// Diagnose and fix configuration issues
    Doctor {
        /// Automatically fix detected issues
        #[arg(long)]
        fix: bool,
    },
    /// Manage plugins (install, update, list, remove)
    Plugin {
        #[command(subcommand)]
        command: commands::plugin::PluginCommands,
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
    /// Show status dashboard
    Status {
        /// Target to show status for
        target: Option<String>,
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
    /// Create and run followup tasks from review comments
    Fix {
        /// Task ID to read comments from (reads from stdin if not provided)
        task_id: Option<String>,
        /// Run followup task asynchronously
        #[arg(long = "async")]
        run_async: bool,
        /// Internal: continue an async fix from a previously created fix-parent
        #[arg(long = "_continue-async", hide = true)]
        continue_async: Option<String>,
        /// Custom plan template (default: fix)
        #[arg(long)]
        template: Option<String>,
        /// Custom decompose template (default: decompose)
        #[arg(long = "decompose-template")]
        decompose_template: Option<String>,
        /// Custom loop template (default: loop)
        #[arg(long = "loop-template")]
        loop_template: Option<String>,
        /// Enable quality loop review step
        #[arg(long, short = 'r')]
        review: bool,
        /// Quality loop review with custom template (implies --review)
        #[arg(long = "review-template")]
        review_template: Option<String>,
        /// Agent for task assignment (default: claude-code)
        #[arg(long)]
        agent: Option<String>,
        /// Enable autorun (auto-start this fix task when its target closes)
        #[arg(long)]
        autorun: bool,
        /// Disable post-fix review loop (single pass only)
        #[arg(long)]
        once: bool,
        /// Output format (e.g., `id` for bare task ID on stdout)
        #[arg(long, short = 'o', value_name = "FORMAT")]
        output: Option<commands::OutputFormat>,
    },
    /// Explore a scope (plan, code, task, or session)
    Explore(commands::explore::ExploreArgs),
    /// Create and run code review tasks
    Review(commands::review::ReviewArgs),
    /// Resolve JJ merge conflicts
    Resolve(commands::resolve::ResolveArgs),
    /// Manage epics (create from plan files, show status, list)
    Epic {
        #[command(subcommand)]
        command: commands::epic::EpicCommands,
    },
    /// Decompose a plan into subtasks under a target task
    Decompose(commands::decompose::DecomposeArgs),
    /// Build from a plan file (decompose and execute all subtasks)
    Build(commands::build::BuildArgs),
    /// Orchestrate a parent task's subtasks via lanes
    Loop(commands::loop_cmd::LoopArgs),
    /// Interactive plan authoring with AI agent.
    /// Subcommands: epic (default), fix.
    /// Examples: `aiki plan feature.md`, `aiki plan epic Add auth`,
    /// `aiki plan fix <review-id>`
    Plan {
        /// Subcommand and arguments. First arg can be 'epic' or 'fix',
        /// otherwise defaults to epic behavior.
        args: Vec<String>,
        /// Plan template to use (default: plan)
        #[arg(long)]
        template: Option<String>,
        /// Agent for plan session (default: claude-code)
        #[arg(long)]
        agent: Option<String>,
        /// Output format (e.g., `id` for bare task ID on stdout)
        #[arg(long, short = 'o', value_name = "FORMAT")]
        output: Option<commands::OutputFormat>,
    },
    /// (deprecated alias for 'aiki plan')
    #[command(hide = true)]
    Spec {
        args: Vec<String>,
        #[arg(long)]
        template: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
enum EventCommands {
    /// Trigger PrepareCommitMessage event (for Git's prepare-commit-msg hook)
    #[command(name = "prepare-commit-msg")]
    PrepareCommitMessage,
}

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
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
        Commands::Init { quiet, instructions_file } => commands::init::run(quiet, instructions_file),
        Commands::Doctor { fix } => commands::doctor::run(fix),
        Commands::Plugin { command } => commands::plugin::run(command),
        Commands::Hooks { command } => match command {
            HooksCommands::Stdin {
                agent,
                event,
                payload,
            } => {
                let payload_str = if payload.is_empty() {
                    None
                } else {
                    Some(payload.join(" "))
                };
                commands::hooks::run_stdin(agent, event, payload_str)
            }
            HooksCommands::Acp {
                agent,
                bin,
                agent_args,
            } => commands::acp::run(agent, bin, agent_args),
            HooksCommands::Otel { agent } => commands::otel_receive::run(agent),
        },
        Commands::Blame {
            file,
            agent,
        } => commands::blame::run(file, agent),
        Commands::Authors { changes, format } => commands::authors::run(changes, format),
        Commands::Benchmark { edits } => commands::benchmark::run("aiki/core".to_string(), edits),
        Commands::Session { command } => commands::session::run(command),
        Commands::Status { target } => Ok(commands::status::run(target)?),
        Commands::Task { command } => commands::task::run(command),
        Commands::Event { command } => match command {
            EventCommands::PrepareCommitMessage => commands::event::run_prepare_commit_message(),
        },
        Commands::Fix {
            task_id,
            run_async,
            continue_async,
            template,
            decompose_template,
            loop_template,
            review,
            review_template,
            agent,
            autorun,
            once,
            output,
        } => {
            // Pass through explicit --review-template only; create_review picks scope-specific default
            // --review flag is a no-op for fix (fix always runs reviews), kept for CLI symmetry with build
            let _ = review;
            commands::fix::run(task_id, run_async, continue_async, template, decompose_template, loop_template, review_template, agent, autorun, once, output)
        }
        Commands::Explore(args) => commands::explore::run(args),
        Commands::Review(args) => commands::review::run(args),
        Commands::Resolve(args) => commands::resolve::run(args),
        Commands::Epic { command } => commands::epic::run(command),
        Commands::Decompose(args) => commands::decompose::run(args),
        Commands::Build(args) => commands::build::run(args),
        Commands::Loop(args) => commands::loop_cmd::run(args),
        Commands::Plan {
            args,
            template,
            agent,
            output,
        } => commands::plan::run(args, template, agent, output),
        Commands::Spec {
            args,
            template,
            agent,
        } => {
            eprintln!("Warning: 'aiki spec' is deprecated, use 'aiki plan' instead.");
            commands::plan::run(args, template, agent, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_parses_agent_after_freeform_args() {
        let cli = Cli::try_parse_from([
            "aiki",
            "plan",
            "ops/now/feature.md",
            "add",
            "auth",
            "--agent",
            "codex",
        ])
        .unwrap();

        match cli.command {
            Commands::Plan { args, agent, .. } => {
                assert_eq!(args, vec!["ops/now/feature.md", "add", "auth"]);
                assert_eq!(agent.as_deref(), Some("codex"));
            }
            _ => panic!("expected plan command"),
        }
    }

    #[test]
    fn spec_parses_agent_after_freeform_args() {
        let cli = Cli::try_parse_from([
            "aiki",
            "spec",
            "ops/now/feature.md",
            "add",
            "auth",
            "--agent",
            "codex",
        ])
        .unwrap();

        match cli.command {
            Commands::Spec { args, agent, .. } => {
                assert_eq!(args, vec!["ops/now/feature.md", "add", "auth"]);
                assert_eq!(agent.as_deref(), Some("codex"));
            }
            _ => panic!("expected spec command"),
        }
    }
}

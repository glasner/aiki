mod config;
mod db;
mod jj;
mod provenance;
mod record_change;
mod repo;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use repo::RepoDetector;
use std::env;

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
    Init,
    /// Record an AI-generated change (called by AI editor hooks)
    #[command(name = "record-change")]
    RecordChange {
        /// Record change from Claude Code
        #[arg(long)]
        claude_code: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init_command(),
        Commands::RecordChange { claude_code } => {
            if claude_code {
                record_change::record_change(provenance::AgentType::ClaudeCode)
            } else {
                eprintln!("Error: Agent type flag required (e.g., --claude-code)");
                std::process::exit(1);
            }
        }
    }
}

fn init_command() -> Result<()> {
    // Get current directory
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Detect repository
    let detector = RepoDetector::new(&current_dir);

    // Find the Git repository root
    let repo_root = detector.find_repo_root()?;

    println!("Initializing Aiki in: {}", repo_root.display());

    // Check if .aiki directory already exists
    let aiki_dir = repo_root.join(".aiki");
    if aiki_dir.exists() {
        println!("\nAiki is already initialized in this repository.");
        return Ok(());
    }

    // Check if JJ is already initialized
    if RepoDetector::has_jj(&repo_root) {
        println!("✓ Found existing JJ repository");
    } else {
        println!("Initializing JJ repository...");
        // Create JJ workspace manager for the repository root
        let workspace = jj::JJWorkspace::new(&repo_root);

        // Check if Git repository already exists
        // If .git exists, use init_on_existing_git (external mode with colocated .git)
        // If .git doesn't exist, use init_colocated (creates both .jj and .git)
        // Both approaches create a colocated workspace where JJ and Git share the working copy
        if repo_root.join(".git").exists() {
            // Resolve .git path (handles both directories and worktree/submodule files)
            let git_dir = RepoDetector::resolve_git_dir(&repo_root)
                .context("Failed to resolve Git directory path")?;

            workspace
                .init_with_git_dir(&git_dir)
                .context("Failed to initialize JJ repository")?;
        } else {
            workspace
                .init_colocated()
                .context("Failed to initialize JJ repository")?;
        }

        println!("✓ Initialized JJ repository (colocated with Git)");
    }

    // Create .aiki directory structure and config
    config::initialize_aiki_directory(&repo_root)?;

    // Install Claude Code hooks
    config::install_claude_code_hooks(&repo_root)?;

    // Initialize provenance database
    db::initialize_provenance_db(&repo_root)?;
    println!("✓ Initialized provenance database");

    println!("\n✓ Aiki initialized successfully!");
    println!("\nNext steps:");
    println!("  • Restart Claude Code to load the new hooks");
    println!("  • Your AI changes will now be automatically tracked");
    println!("  • Run 'aiki status' to see tracked activity");

    Ok(())
}

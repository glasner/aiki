mod authors;
mod blame;
mod config;
mod jj;
mod provenance;
mod record_change;
mod repo;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use repo::RepoDetector;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use sysinfo::{ProcessesToUpdate, System};

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
        /// Run synchronously (for testing - waits for background thread)
        #[arg(long)]
        sync: bool,
    },
    /// Show line-by-line AI attribution for a file
    Blame {
        /// File to show blame for
        file: std::path::PathBuf,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init_command(),
        Commands::RecordChange { claude_code, sync } => {
            if claude_code {
                record_change::record_change(provenance::AgentType::ClaudeCode, sync)
            } else {
                eprintln!("Error: Agent type flag required (e.g., --claude-code)");
                std::process::exit(1);
            }
        }
        Commands::Blame { file } => blame_command(file),
        Commands::Authors { changes, format } => authors_command(changes, format),
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

            // Import existing Git commits into JJ
            println!("Importing Git history...");
            workspace
                .git_import()
                .context("Failed to import Git history")?;
            println!("✓ Imported Git history into JJ");
        } else {
            workspace
                .init_colocated()
                .context("Failed to initialize JJ repository")?;
        }

        println!("✓ Initialized JJ repository (colocated with Git)");
    }

    // Create .aiki directory structure and config
    config::initialize_aiki_directory(&repo_root)?;

    // Save previous git hooks path before installing aiki hooks
    config::save_previous_hooks_path(&repo_root)?;

    // Install Git hooks for co-author attribution
    config::install_git_hooks(&repo_root)?;

    // Configure git to use aiki's hooks directory
    config::configure_git_hooks_path(&repo_root)?;

    // Install Claude Code hooks
    config::install_claude_code_hooks(&repo_root)?;

    println!("\n✓ Aiki initialized successfully!");
    println!("\nNext steps:");
    println!("  • Your AI changes will now be automatically tracked");
    println!("  • Git commits will automatically include AI co-authors");

    // Check if Claude Code is running and offer to restart
    if is_claude_code_running() {
        println!(
            "\n⚠️  Claude Code is currently running and needs to restart to activate the hooks."
        );
        print!("   Would you like to restart Claude Code now? (y/N): ");
        io::stdout().flush().ok();

        let stdin = io::stdin();
        let mut response = String::new();
        stdin.lock().read_line(&mut response).ok();

        if response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes")
        {
            println!("\n   Restarting Claude Code...");
            restart_claude_code()?;
            println!("   ✓ Claude Code restarted successfully");
        } else {
            println!("\n   Please restart Claude Code manually when ready:");
            println!("   • macOS: Cmd+Q then reopen");
            println!("   • Linux/Windows: Close and reopen the application");
        }
    } else {
        println!("\n   If using Claude Code, restart it when you start it to activate the hooks");
    }

    Ok(())
}

/// Find the JJ workspace root by walking up the directory tree looking for .jj
fn find_jj_workspace(start_dir: &std::path::Path) -> Option<PathBuf> {
    let mut current = start_dir;
    loop {
        let jj_dir = current.join(".jj");
        if jj_dir.exists() && jj_dir.is_dir() {
            return Some(current.to_path_buf());
        }

        // Move up one directory
        match current.parent() {
            Some(parent) => current = parent,
            None => return None, // Reached filesystem root
        }
    }
}

fn blame_command(file: std::path::PathBuf) -> Result<()> {
    // Get current directory
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Find JJ workspace root (look for .jj directory)
    let jj_root = find_jj_workspace(&current_dir)
        .context("Not in a JJ workspace. Run this command from within a JJ repository.")?;

    // Check if file exists
    let file_path = if file.is_absolute() {
        file
    } else {
        current_dir.join(&file)
    };

    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }

    // Convert to relative path from JJ workspace root
    let relative_path = file_path
        .strip_prefix(&jj_root)
        .context("File is not in repository")?;

    // Create blame command
    let blame_cmd = blame::BlameCommand::new(jj_root);

    // Get attributions
    let attributions = blame_cmd
        .blame_file(relative_path)
        .context("Failed to generate blame information")?;

    // Format and print output
    let output = blame_cmd.format_blame(&attributions);
    print!("{}", output);

    Ok(())
}

fn authors_command(changes: Option<String>, format: String) -> Result<()> {
    // Get current directory
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Find JJ workspace root (look for .jj directory)
    let jj_root = find_jj_workspace(&current_dir)
        .context("Not in a JJ workspace. Run this command from within a JJ repository.")?;

    // Parse scope
    let scope = match changes.as_deref() {
        Some("staged") => authors::AuthorScope::GitStaged,
        Some(other) => {
            anyhow::bail!("Unknown scope: '{}'. Supported values: 'staged'", other);
        }
        None => authors::AuthorScope::WorkingCopy,
    };

    // Parse format
    let output_format = match format.as_str() {
        "plain" => authors::OutputFormat::Plain,
        "git" => authors::OutputFormat::Git,
        "json" => authors::OutputFormat::Json,
        other => {
            anyhow::bail!(
                "Unknown format: '{}'. Supported values: 'plain', 'git', 'json'",
                other
            );
        }
    };

    // Create authors command
    let authors_cmd = authors::AuthorsCommand::new(jj_root);

    // Get authors
    let output = authors_cmd
        .get_authors(scope, output_format)
        .context("Failed to get authors")?;

    // Print to stdout
    if !output.is_empty() {
        print!("{}", output);
    }

    Ok(())
}

/// Check if Claude Code is currently running
fn is_claude_code_running() -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    // Claude Code process names vary by platform:
    // - macOS: "Claude Code" or "claude-code"
    // - Linux: "claude-code" or "claude"
    // - Windows: "Claude Code.exe" or "claude-code.exe"
    sys.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        name.contains("claude") && (name.contains("code") || name == "claude")
    })
}

/// Restart Claude Code application
fn restart_claude_code() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        // On macOS, use osascript to quit and reopen Claude Code
        std::process::Command::new("osascript")
            .args(["-e", "tell application \"Claude Code\" to quit"])
            .output()
            .context("Failed to quit Claude Code")?;

        // Wait a moment for the app to fully quit
        std::thread::sleep(std::time::Duration::from_secs(1));

        std::process::Command::new("open")
            .args(["-a", "Claude Code"])
            .spawn()
            .context("Failed to reopen Claude Code")?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, kill the process and restart it
        std::process::Command::new("pkill")
            .arg("claude-code")
            .output()
            .context("Failed to quit Claude Code")?;

        std::thread::sleep(std::time::Duration::from_secs(1));

        std::process::Command::new("claude-code")
            .spawn()
            .context("Failed to reopen Claude Code")?;
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, use taskkill and restart
        std::process::Command::new("taskkill")
            .args(["/F", "/IM", "claude-code.exe"])
            .output()
            .context("Failed to quit Claude Code")?;

        std::thread::sleep(std::time::Duration::from_secs(1));

        std::process::Command::new("claude-code")
            .spawn()
            .context("Failed to reopen Claude Code")?;
    }

    Ok(())
}

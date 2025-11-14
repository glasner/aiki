mod authors;
mod blame;
mod config;
mod event_bus;
mod events;
mod handlers;
mod jj;
mod provenance;
mod record_change;
mod repo;
mod vendors;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use repo::RepoDetector;
use std::env;
use std::fs;
use std::io::{BufRead, Write};
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
    Init {
        /// Only print error and warning messages (suppress normal output)
        #[arg(short, long)]
        quiet: bool,
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

#[derive(Subcommand)]
enum HooksCommands {
    /// Install global hooks for AI editors
    Install,
    /// Show status of all installed hooks
    Status,
    /// Diagnose hook configuration issues
    Doctor {
        /// Automatically fix detected issues
        #[arg(long)]
        fix: bool,
    },
    /// List available vendor integrations
    List,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { quiet } => init_command(quiet),
        Commands::Hooks { command } => match command {
            HooksCommands::Install => hooks_install_command(),
            HooksCommands::Status => hooks_status_command(),
            HooksCommands::Doctor { fix } => hooks_doctor_command(fix),
            HooksCommands::List => hooks_list_command(),
            HooksCommands::Handle { agent, event } => {
                let agent_type = parse_agent_type(&agent)?;
                handle_event(agent_type, &event)
            }
        },
        Commands::RecordChange {
            claude_code,
            cursor,
            sync,
        } => {
            eprintln!("Warning: 'aiki record-change' is deprecated.");
            eprintln!("  Use 'aiki hooks <vendor> post-change' instead.");
            eprintln!();

            if claude_code {
                record_change::record_change_legacy(provenance::AgentType::ClaudeCode, sync)
            } else if cursor {
                record_change::record_change_legacy(provenance::AgentType::Cursor, sync)
            } else {
                eprintln!("Error: Agent type flag required (e.g., --claude-code, --cursor)");
                std::process::exit(1);
            }
        }
        Commands::Blame { file } => blame_command(file),
        Commands::Authors { changes, format } => authors_command(changes, format),
    }
}

fn hooks_install_command() -> Result<()> {
    if !cfg!(target_os = "macos") && !cfg!(target_os = "linux") && !cfg!(target_os = "windows") {
        eprintln!("Warning: Unsupported platform for automatic hook installation");
    }

    println!("Installing Aiki global hooks...\n");

    // Install global Git hooks
    config::install_global_git_hooks()?;

    // Install all editor hooks
    config::install_claude_code_hooks_global()?;
    config::install_cursor_hooks_global()?;

    println!("\n✓ Global hooks installed successfully!");
    println!("\nRepositories will be automatically initialized when you:");
    println!("  • Claude Code: Open a project");
    println!("  • Cursor: Submit your first prompt");
    println!("\nYour AI changes will now be tracked automatically.");

    // Check if editors are running and offer to restart
    let claude_running = is_claude_code_running();
    let cursor_running = is_cursor_running();

    if claude_running || cursor_running {
        let editors_text = match (claude_running, cursor_running) {
            (true, true) => "Claude Code and Cursor are",
            (true, false) => "Claude Code is",
            (false, true) => "Cursor is",
            (false, false) => unreachable!(),
        };

        println!(
            "\n⚠️  {} currently running and need to restart to activate hooks.",
            editors_text
        );
        print!("   Would you like to restart them now? (y/N): ");
        std::io::stdout().flush().ok();

        let stdin = std::io::stdin();
        let mut response = String::new();
        stdin.lock().read_line(&mut response).ok();

        if response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes")
        {
            if claude_running {
                println!("\n   Restarting Claude Code...");
                restart_claude_code()?;
                println!("   ✓ Claude Code restarted successfully");
            }
            if cursor_running {
                println!("\n   Note: Cursor must be restarted manually:");
                println!("   • macOS: Cmd+Q then reopen");
                println!("   • Linux/Windows: Close and reopen the application");
            }
        } else {
            println!("\n   Please restart editors manually when ready:");
            if claude_running {
                println!("   • Claude Code: Cmd+Q (macOS) or close and reopen");
            }
            if cursor_running {
                println!("   • Cursor: Cmd+Q (macOS) or close and reopen");
            }
        }
    } else {
        println!("\n💡 Restart your editor when you open it to activate the hooks.");
    }

    Ok(())
}

fn init_command(quiet: bool) -> Result<()> {
    // Get current directory
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Detect repository
    let detector = RepoDetector::new(&current_dir);

    // Find the Git repository root
    let repo_root = detector.find_repo_root()?;

    // Check if already initialized by looking at git config
    let git_hooks_path = std::process::Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        });

    // Check if pointing to global hooks
    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let global_hooks = home_dir.join(".aiki/githooks");

    if let Some(ref hooks_path) = git_hooks_path {
        if hooks_path.contains(".aiki/githooks") {
            if quiet {
                // Silent success for auto mode
                return Ok(());
            }
            println!("Repository already initialized at {}", repo_root.display());
            return Ok(());
        }
    }

    if !quiet {
        println!("Initializing Aiki in: {}", repo_root.display());
    }

    // Check if JJ is already initialized
    if RepoDetector::has_jj(&repo_root) {
        if !quiet {
            println!("✓ Found existing JJ repository");
        }
    } else {
        if !quiet {
            println!("Initializing JJ repository...");
        }
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
            if !quiet {
                println!("Importing Git history...");
            }
            workspace
                .git_import()
                .context("Failed to import Git history")?;
            if !quiet {
                println!("✓ Imported Git history into JJ");
            }
        } else {
            workspace
                .init_colocated()
                .context("Failed to initialize JJ repository")?;
        }

        if !quiet {
            println!("✓ Initialized JJ repository (colocated with Git)");
        }
    }

    // Create minimal .aiki directory (only if we need to save previous hooks path)
    let aiki_dir = repo_root.join(".aiki");

    // Save previous git hooks path before configuring global hooks
    // Only create .aiki directory if there was a previous hooks path
    if git_hooks_path.is_some() && git_hooks_path.as_ref().unwrap() != "" {
        fs::create_dir_all(&aiki_dir).context("Failed to create .aiki directory")?;
        config::save_previous_hooks_path(&repo_root)?;
    }

    // Configure git to use global hooks directory
    let global_hooks_str = global_hooks.to_str().context("Invalid global hooks path")?;
    std::process::Command::new("git")
        .args(["config", "core.hooksPath", global_hooks_str])
        .current_dir(&repo_root)
        .output()
        .context("Failed to set git config core.hooksPath")?;

    if !quiet {
        println!("✓ Configured Git hooks (→ {})", global_hooks.display());
        println!("\n✓ Repository initialized successfully!");
        println!("\nYour AI changes will now be tracked automatically.");
        println!("Git commits will include AI co-authors.");
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

/// Check if Cursor is currently running
fn is_cursor_running() -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    sys.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        name.contains("cursor")
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

/// Parse agent type from string
fn parse_agent_type(agent: &str) -> Result<provenance::AgentType> {
    match agent {
        "claude-code" => Ok(provenance::AgentType::ClaudeCode),
        "cursor" => Ok(provenance::AgentType::Cursor),
        _ => anyhow::bail!(
            "Unknown agent type: '{}'. Supported values: 'claude-code', 'cursor'",
            agent
        ),
    }
}

/// Handle vendor event (called by hooks)
fn handle_event(agent: provenance::AgentType, event: &str) -> Result<()> {
    use provenance::AgentType;

    match agent {
        AgentType::ClaudeCode => vendors::claude_code::handle(event),
        AgentType::Cursor => vendors::cursor::handle(event),
        _ => anyhow::bail!("Unsupported agent type: {:?}", agent),
    }
}

/// Show status of all installed hooks
fn hooks_status_command() -> Result<()> {
    println!("Hook Status:\n");

    // TODO: Implement full status checking
    println!("Claude Code:");
    println!("  Status: Not yet implemented");
    println!();

    println!("Cursor:");
    println!("  Status: Not yet implemented");
    println!();

    println!("Git Hooks:");
    println!("  Status: Not yet implemented");

    Ok(())
}

/// Diagnose hook configuration issues
fn hooks_doctor_command(fix: bool) -> Result<()> {
    if fix {
        println!("Diagnosing and fixing hook issues...\n");
    } else {
        println!("Diagnosing hook issues...\n");
    }

    // TODO: Implement hook diagnostics
    println!("Hook diagnostics not yet implemented");

    Ok(())
}

/// List available vendor integrations
fn hooks_list_command() -> Result<()> {
    println!("Available Vendor Integrations:\n");
    println!("  • Claude Code");
    println!("  • Cursor");
    println!("\nUse 'aiki hooks install' to install hooks for all vendors.");

    Ok(())
}

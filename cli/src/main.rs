mod authors;
mod blame;
mod config;
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { quiet } => init_command(quiet),
        Commands::Doctor { fix } => doctor_command(fix),
        Commands::Hooks { command } => match command {
            HooksCommands::Install => hooks_install_command(),
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
        Commands::Blame {
            file,
            agent,
            verify,
        } => blame_command(file, agent, verify),
        Commands::Authors { changes, format } => authors_command(changes, format),
        Commands::Verify { revision } => verify_command(revision),
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

fn verify_command(revision: String) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Verify we're in a JJ repository
    if !current_dir.join(".jj").exists() {
        anyhow::bail!("Not in a JJ repository. Run 'jj init' or 'aiki init' first.");
    }

    // Perform verification
    let result =
        verify::verify_change(&current_dir, &revision).context("Failed to verify change")?;

    // Display results
    verify::format_verification_result(&result);

    // Exit with error code if verification failed
    if !result.is_verified() && result.signature_status != verify::SignatureStatus::Unsigned {
        std::process::exit(1);
    }

    Ok(())
}

/// Prompt for a numbered choice
fn prompt_choice(prompt: &str, min: usize, max: usize) -> Result<usize> {
    loop {
        print!("{} [{}]: ", prompt, min);
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            return Ok(min);
        }

        match input.parse::<usize>() {
            Ok(n) if n >= min && n <= max => return Ok(n),
            _ => println!("Please enter a number between {} and {}", min, max),
        }
    }
}

/// Prompt for a string value
fn prompt_string(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(def) = default {
        print!("{} [{}]: ", prompt, def);
    } else {
        print!("{}: ", prompt);
    }
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        if let Some(def) = default {
            return Ok(def.to_string());
        }
    }

    Ok(input)
}

/// Prompt for yes/no
fn prompt_yes_no(prompt: &str, default: bool) -> Result<bool> {
    let default_str = if default { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", prompt, default_str);
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        return Ok(default);
    }

    Ok(input == "y" || input == "yes")
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
    }

    // Configure commit signing
    match signing::detect_signing_config()? {
        Some(signing_config) => {
            config::update_jj_signing_config(
                &repo_root,
                &signing_config.backend.to_string(),
                Some(&signing_config.key),
                "own",
            )?;

            // For SSH backend, create allowed-signers file
            if matches!(signing_config.backend, signing::SigningBackend::Ssh) {
                let email = signing::get_user_email(&repo_root)?;
                signing::create_ssh_allowed_signers(&repo_root, &email, &signing_config.key)?;
            }

            if !quiet {
                println!(
                    "✓ Configured JJ commit signing ({:?})",
                    signing_config.backend
                );
                println!("  Using key: {}", signing_config.key);
            }
        }
        None => {
            if !quiet {
                println!("⚠ No signing keys detected");
                println!();
                println!("Commit signing provides cryptographic proof of AI authorship.");
                println!();

                // Check if we're in an interactive terminal
                let is_interactive = atty::is(atty::Stream::Stdin);

                if !is_interactive {
                    println!("Run 'aiki doctor --fix' to set up signing interactively.");
                    println!();
                    println!("Continuing without signing...");
                    println!();
                } else {
                    println!("What would you like to do?");
                    println!("  1. Generate new signing key (recommended)");
                    println!("  2. I have a key, let me specify it manually");
                    println!("  3. Skip signing for now");
                    println!();

                    let choice = prompt_choice("Choice", 1, 3)?;

                    match choice {
                        1 => {
                            // Launch wizard in generate mode
                            let wizard = sign_setup_wizard::SignSetupWizard::new(repo_root.clone());
                            wizard.run(None)?;
                        }
                        2 => {
                            // Manual key configuration
                            println!();
                            println!("Manual Key Configuration");
                            println!("========================");
                            println!();

                            println!("Which backend?");
                            println!("  1. GPG");
                            println!("  2. SSH");
                            println!();

                            let backend_choice = prompt_choice("Choice", 1, 2)?;
                            let backend = if backend_choice == 1 {
                                signing::SigningBackend::Gpg
                            } else {
                                signing::SigningBackend::Ssh
                            };

                            let key = prompt_string(
                                if backend == signing::SigningBackend::Gpg {
                                    "GPG Key ID (e.g., 4ED556E9729E000F)"
                                } else {
                                    "SSH public key path (e.g., ~/.ssh/id_ed25519.pub)"
                                },
                                None,
                            )?;

                            let wizard = sign_setup_wizard::SignSetupWizard::new(repo_root.clone());
                            wizard
                                .run(Some(sign_setup_wizard::SetupMode::Manual { backend, key }))?;
                        }
                        3 => {
                            println!();
                            println!("Skipping signing setup.");
                            println!("You can set up signing later by running: aiki sign setup");
                            println!();
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    if !quiet {
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

fn blame_command(file: std::path::PathBuf, agent: Option<String>, verify: bool) -> Result<()> {
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

    // Parse agent filter if provided
    let agent_filter = match agent {
        Some(agent_str) => Some(parse_agent_type(&agent_str)?),
        None => None,
    };

    // Create blame command
    let blame_cmd = blame::BlameCommand::new(jj_root);

    // Get attributions
    let attributions = blame_cmd
        .blame_file(relative_path)
        .context("Failed to generate blame information")?;

    // Format and print output
    let output = blame_cmd.format_blame(&attributions, agent_filter, verify);
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

/// Diagnose and fix repository and hook configuration issues
fn doctor_command(fix: bool) -> Result<()> {
    let mut issues_found = 0;
    let fixes_applied = 0;

    if fix {
        println!("Diagnosing and fixing issues...\n");
    } else {
        println!("Checking Aiki health...\n");
    }

    // Check repository setup
    println!("Repository:");

    let current_dir = env::current_dir().context("Failed to get current directory")?;

    // Check JJ
    if RepoDetector::has_jj(&current_dir) {
        println!("  ✓ JJ workspace initialized");
    } else {
        println!("  ✗ JJ workspace not found");
        println!("    → Run: aiki init");
        issues_found += 1;
    }

    // Check Git
    if current_dir.join(".git").exists() {
        println!("  ✓ Git repository detected");
    } else {
        println!("  ⚠ No Git repository (optional)");
    }

    // Check Aiki directory
    let aiki_dir = current_dir.join(".aiki");
    if aiki_dir.exists() {
        println!("  ✓ Aiki directory exists");
    } else {
        println!("  ✗ Aiki directory missing");
        println!("    → Run: aiki init");
        issues_found += 1;
    }

    println!();

    // Check global hooks
    println!("Global Hooks:");

    let home_dir = dirs::home_dir().context("Failed to get home directory")?;

    // Check Git hooks
    let git_hooks_dir = home_dir.join(".aiki/githooks");
    if git_hooks_dir.exists() {
        println!("  ✓ Git hooks installed (~/.aiki/githooks/)");
    } else {
        println!("  ✗ Git hooks missing");
        println!("    → Run: aiki hooks install");
        issues_found += 1;
    }

    // Check Claude Code hooks
    let claude_settings = home_dir.join(".claude/settings.json");
    if claude_settings.exists() {
        println!("  ✓ Claude Code hooks configured");
    } else {
        println!("  ⚠ Claude Code hooks not configured");
        println!("    → Run: aiki hooks install");
    }

    // Check Cursor hooks
    let cursor_hooks = home_dir.join(".cursor/hooks.json");
    if cursor_hooks.exists() {
        println!("  ✓ Cursor hooks configured");
    } else {
        println!("  ⚠ Cursor hooks not configured");
        println!("    → Run: aiki hooks install");
    }

    println!();

    // Check local configuration (only if in a repo)
    if current_dir.join(".git").exists() {
        println!("Local Configuration:");

        // Check git core.hooksPath
        let output = std::process::Command::new("git")
            .args(["config", "core.hooksPath"])
            .current_dir(&current_dir)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let hooks_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if hooks_path.contains(".aiki/githooks") {
                    println!("  ✓ Git core.hooksPath configured");
                } else {
                    println!("  ⚠ Git core.hooksPath points elsewhere: {}", hooks_path);
                }
            } else {
                println!("  ✗ Git core.hooksPath not set");
                println!("    → Run: aiki init");
                issues_found += 1;
            }
        }

        println!();
    }

    // Check commit signing (only if in a repo)
    if current_dir.join(".jj").exists() {
        println!("Commit Signing:");

        match signing::read_signing_config(&current_dir) {
            Ok(Some(config)) => {
                println!("  ✓ JJ signing enabled ({:?})", config.backend);

                match signing::verify_key_accessible(&config) {
                    Ok(true) => {
                        println!("  ✓ Signing key accessible: {}", config.key);
                    }
                    Ok(false) => {
                        println!("  ✗ Signing key not found: {}", config.key);
                        println!("    → Run: aiki sign setup");
                        issues_found += 1;
                    }
                    Err(e) => {
                        println!("  ✗ Error verifying key: {}", e);
                        issues_found += 1;
                    }
                }
            }
            Ok(None) => {
                println!("  ⚠ JJ signing not configured");

                if fix {
                    println!();
                    println!("Would you like to set up signing now?");
                    let setup = prompt_yes_no("Set up signing", true)?;

                    if setup {
                        let wizard = sign_setup_wizard::SignSetupWizard::new(current_dir.clone());
                        wizard.run(None)?;
                    } else {
                        println!("Skipping signing setup.");
                    }
                } else {
                    println!("    → Run: aiki doctor --fix (to set up signing)");
                }
                // Not counted as error, just a warning
            }
            Err(e) => {
                println!("  ✗ Error reading JJ config: {}", e);
                issues_found += 1;
            }
        }

        println!();
    }

    // Summary
    if issues_found == 0 {
        println!("✓ All checks passed! Aiki is healthy.");
    } else {
        println!("Found {} issue(s).", issues_found);
        if !fix {
            println!("\nRun 'aiki doctor --fix' to automatically fix issues.");
        } else if fixes_applied > 0 {
            println!("\nFixed {} issue(s).", fixes_applied);
        }
    }

    Ok(())
}

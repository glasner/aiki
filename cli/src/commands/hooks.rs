use crate::config;
use crate::error::Result;
use crate::provenance;
use crate::vendors;
use anyhow::Context;
use std::io::{BufRead, Write};
use sysinfo::{ProcessesToUpdate, System};

pub fn run_install() -> Result<()> {
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
    let (claude_running, cursor_running) = get_running_editors();

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

pub fn run_handle(agent: String, event: String) -> Result<()> {
    let agent_type = parse_agent_type(&agent)?;
    handle_event(agent_type, &event)
}

/// Check which editors are currently running (single process scan)
/// Returns (claude_running, cursor_running)
fn get_running_editors() -> (bool, bool) {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    // Scan all processes once and check for both editors
    // Claude Code process names vary by platform:
    // - macOS: "Claude Code" or "claude-code"
    // - Linux: "claude-code" or "claude"
    // - Windows: "Claude Code.exe" or "claude-code.exe"
    sys.processes()
        .values()
        .fold((false, false), |(claude, cursor), process| {
            let name = process.name().to_string_lossy().to_lowercase();
            (
                claude || (name.contains("claude") && (name.contains("code") || name == "claude")),
                cursor || name.contains("cursor"),
            )
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
    use crate::error::AikiError;

    match agent {
        "claude-code" => Ok(provenance::AgentType::Claude),
        "cursor" => Ok(provenance::AgentType::Cursor),
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}

/// Handle vendor event (called by hooks)
fn handle_event(agent: provenance::AgentType, event: &str) -> Result<()> {
    use crate::error::AikiError;
    use provenance::AgentType;

    match agent {
        AgentType::Claude => Ok(vendors::claude_code::handle(event)?),
        AgentType::Cursor => Ok(vendors::cursor::handle(event)?),
        _ => Err(AikiError::UnsupportedAgentType(format!("{:?}", agent))),
    }
}

use crate::commands::agents_template::{AIKI_BLOCK_TEMPLATE, AIKI_BLOCK_VERSION};
use crate::commands::zed_detection;
use crate::config;
use crate::editors::zed as ide_config;
use crate::error::Result;
use crate::repo::RepoDetector;
use crate::signing;
use anyhow::Context;
use std::env;
use std::fs;
use std::io::{self, Write};

pub fn run(fix: bool) -> Result<()> {
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

    // Check Claude Code hooks - verify file exists AND contains hooks
    let claude_settings = home_dir.join(".claude/settings.json");
    let claude_hooks_ok = check_claude_code_hooks(&claude_settings);
    if claude_hooks_ok {
        println!("  ✓ Claude Code hooks configured");
    } else {
        println!("  ✗ Claude Code hooks not configured");
        if fix {
            println!("    Installing Claude Code hooks...");
            match config::install_claude_code_hooks_global() {
                Ok(()) => {
                    println!("    ✓ Claude Code hooks installed");
                }
                Err(e) => {
                    println!("    ✗ Failed to install: {}", e);
                    issues_found += 1;
                }
            }
        } else {
            println!("    → Run: aiki doctor --fix");
            issues_found += 1;
        }
    }

    // Check Cursor hooks - verify file exists AND contains aiki hooks
    let cursor_hooks_path = home_dir.join(".cursor/hooks.json");
    let cursor_hooks_ok = check_cursor_hooks(&cursor_hooks_path);
    if cursor_hooks_ok {
        println!("  ✓ Cursor hooks configured");
    } else {
        println!("  ✗ Cursor hooks not configured");
        if fix {
            println!("    Installing Cursor hooks...");
            match config::install_cursor_hooks_global() {
                Ok(()) => {
                    println!("    ✓ Cursor hooks installed");
                }
                Err(e) => {
                    println!("    ✗ Failed to install: {}", e);
                    issues_found += 1;
                }
            }
        } else {
            println!("    → Run: aiki doctor --fix");
            issues_found += 1;
        }
    }

    println!();

    // Check ACP (Agent Client Protocol) configuration
    println!("ACP Configuration:");

    // Check Zed ACP configuration
    match ide_config::is_zed_configured() {
        Ok(true) => {
            println!("  ✓ Zed editor configured for ACP");
            if let Some(path) = ide_config::zed_settings_path() {
                println!("    Settings: {}", path.display());
            }
        }
        Ok(false) => {
            if let Some(path) = ide_config::zed_settings_path() {
                if path.parent().map(|p| p.exists()).unwrap_or(false) {
                    println!("  ✗ Zed editor not configured for ACP");
                    issues_found += 1; // Count unconfigured state as an issue
                    if fix {
                        println!("    Configuring Zed for ACP...");
                        match ide_config::configure_zed() {
                            Ok(()) => {
                                println!("    ✓ Configured Zed editor");
                                issues_found -= 1; // Clear the issue since we fixed it
                            }
                            Err(e) => {
                                println!("    ✗ Failed to configure Zed: {}", e);
                                // Issue already counted above
                            }
                        }
                    } else {
                        println!("    → Run: aiki doctor --fix (to configure Zed)");
                    }
                } else {
                    println!("  - Zed editor not installed");
                }
            }
        }
        Err(e) => {
            println!("  ✗ Error checking Zed configuration: {}", e);
            issues_found += 1;
        }
    }

    // Check ACP binary availability
    println!("\n  ACP Agent Binaries:");

    // Check common agents
    let agents_to_check = vec![
        ("claude-code", "Claude Code"),
        ("codex", "Codex"),
        ("gemini", "Gemini"),
    ];

    for (agent_type, display_name) in agents_to_check {
        match zed_detection::resolve_agent_binary(agent_type) {
            Ok(resolved) => match resolved {
                zed_detection::ResolvedBinary::ZedNodeJs(path) => {
                    println!("    ✓ {} (Zed Node.js)", display_name);
                    if std::env::var("VERBOSE").is_ok() {
                        println!("      {}", path.display());
                    }
                }
                zed_detection::ResolvedBinary::ZedNative(path) => {
                    println!("    ✓ {} (Zed native)", display_name);
                    if std::env::var("VERBOSE").is_ok() {
                        println!("      {}", path.display());
                    }
                }
                zed_detection::ResolvedBinary::InPath(exe) => {
                    println!("    ✓ {} (system PATH)", display_name);
                    if std::env::var("VERBOSE").is_ok() {
                        println!("      {}", exe);
                    }
                }
            },
            Err(_) => {
                println!("    - {} not installed", display_name);
            }
        }
    }

    // Check Node.js for Node.js-based agents
    if let Ok(_) = zed_detection::check_nodejs_installed() {
        // Node.js check already prints version to stderr
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
                        let wizard = signing::SignSetupWizard::new(current_dir.clone());
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

    // Check AGENTS.md for task system instructions
    println!("Agent Instructions:");

    let agents_path = current_dir.join("AGENTS.md");
    if agents_path.exists() {
        match fs::read_to_string(&agents_path) {
            Ok(content) => {
                if content.contains(&format!("<aiki version=\"{}\">", AIKI_BLOCK_VERSION)) {
                    println!("  ✓ AGENTS.md has current <aiki> block");
                } else if content.contains("<aiki version=") {
                    println!("  ⚠ AGENTS.md has outdated <aiki> block");
                    if fix {
                        // Replace old block with new one
                        if let Some(start) = content.find("<aiki version=") {
                            if let Some(end) = content.find("</aiki>") {
                                let before = &content[..start];
                                let after = &content[end + "</aiki>".len()..];
                                let updated = format!(
                                    "{}{}{}",
                                    before.trim_end(),
                                    AIKI_BLOCK_TEMPLATE,
                                    after.trim_start()
                                );
                                match fs::write(&agents_path, updated) {
                                    Ok(()) => {
                                        println!(
                                            "    ✓ Updated <aiki> block to version {}",
                                            AIKI_BLOCK_VERSION
                                        );
                                    }
                                    Err(e) => {
                                        println!("    ✗ Failed to update AGENTS.md: {}", e);
                                        issues_found += 1;
                                    }
                                }
                            }
                        }
                    } else {
                        println!("    → Run: aiki doctor --fix (to update block)");
                    }
                } else {
                    println!("  ⚠ AGENTS.md missing <aiki> block");
                    if fix {
                        // Prepend block to existing content
                        let updated = format!("{}\n{}", AIKI_BLOCK_TEMPLATE, content);
                        match fs::write(&agents_path, updated) {
                            Ok(()) => {
                                println!("    ✓ Added <aiki> block to AGENTS.md");
                            }
                            Err(e) => {
                                println!("    ✗ Failed to update AGENTS.md: {}", e);
                                issues_found += 1;
                            }
                        }
                    } else {
                        println!("    → Run: aiki doctor --fix (to add block)");
                    }
                }
            }
            Err(e) => {
                println!("  ✗ Failed to read AGENTS.md: {}", e);
                issues_found += 1;
            }
        }
    } else {
        println!("  ⚠ AGENTS.md not found");
        if fix {
            match fs::write(&agents_path, AIKI_BLOCK_TEMPLATE) {
                Ok(()) => {
                    println!("    ✓ Created AGENTS.md with task system instructions");
                }
                Err(e) => {
                    println!("    ✗ Failed to create AGENTS.md: {}", e);
                    issues_found += 1;
                }
            }
        } else {
            println!("    → Run: aiki doctor --fix (to create)");
        }
    }

    println!();

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

/// Prompt for yes/no
fn prompt_yes_no(prompt: &str, default: bool) -> Result<bool> {
    let default_str = if default { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", prompt, default_str);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        return Ok(default);
    }

    Ok(input == "y" || input == "yes")
}

/// Check if a command string invokes aiki hooks handle with specific agent/event
///
/// Matches commands like:
/// - `aiki hooks handle --agent claude-code --event session.started`
/// - `/path/to/aiki.exe hooks handle --agent cursor --event prompt.submitted`
///
/// If expected_agent or expected_event is Some, validates those flags are present.
fn is_aiki_hooks_command_with_params(
    cmd: &str,
    expected_agent: Option<&str>,
    expected_event: Option<&str>,
) -> bool {
    // Split command into words
    let words: Vec<&str> = cmd.split_whitespace().collect();

    // Look for pattern: <something-ending-with-aiki> hooks handle
    let mut found_hooks_handle = false;
    for (i, word) in words.iter().enumerate() {
        // Check if this word is the aiki binary (with or without path, with or without .exe)
        let is_aiki_binary = word.ends_with("aiki") || word.ends_with("aiki.exe");

        if is_aiki_binary {
            // Check if followed by "hooks handle"
            if i + 2 < words.len() && words[i + 1] == "hooks" && words[i + 2] == "handle" {
                found_hooks_handle = true;
                break;
            }
        }
    }

    if !found_hooks_handle {
        return false;
    }

    // If no specific agent/event required, we're done
    if expected_agent.is_none() && expected_event.is_none() {
        return true;
    }

    // Check for --agent flag
    if let Some(agent) = expected_agent {
        let has_agent = words.windows(2).any(|w| w[0] == "--agent" && w[1] == agent);
        if !has_agent {
            return false;
        }
    }

    // Check for --event flag
    if let Some(event) = expected_event {
        let has_event = words.windows(2).any(|w| w[0] == "--event" && w[1] == event);
        if !has_event {
            return false;
        }
    }

    true
}

/// Check if Claude Code hooks are properly configured
///
/// Returns true if ~/.claude/settings.json exists AND contains both:
/// - hooks.SessionStart with aiki command
/// - hooks.PostToolUse with aiki command
fn check_claude_code_hooks(settings_path: &std::path::Path) -> bool {
    if !settings_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => return false,
    };

    // Check SessionStart hook contains aiki command with correct agent/event
    let has_session_start = hooks
        .get("SessionStart")
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks| {
                        hooks.iter().any(|hook| {
                            hook.get("command")
                                .and_then(|c| c.as_str())
                                .map(|c| {
                                    is_aiki_hooks_command_with_params(
                                        c,
                                        Some("claude-code"),
                                        Some("session.started"),
                                    )
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    // Check PostToolUse hook contains aiki command with correct agent/event
    let has_post_tool_use = hooks
        .get("PostToolUse")
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks| {
                        hooks.iter().any(|hook| {
                            hook.get("command")
                                .and_then(|c| c.as_str())
                                .map(|c| {
                                    is_aiki_hooks_command_with_params(
                                        c,
                                        Some("claude-code"),
                                        Some("change.completed"),
                                    )
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    has_session_start && has_post_tool_use
}

/// Check if Cursor hooks are properly configured
///
/// Returns true if ~/.cursor/hooks.json exists AND contains both:
/// - hooks.beforeSubmitPrompt with aiki hooks handle command
/// - hooks.afterFileEdit with aiki hooks handle command
fn check_cursor_hooks(hooks_path: &std::path::Path) -> bool {
    if !hooks_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(hooks_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let hooks = match config.get("hooks") {
        Some(h) => h,
        None => return false,
    };

    // Helper to check if an array contains an aiki hooks handle command with specific agent/event
    let has_aiki_hook_with_params =
        |arr: &serde_json::Value, agent: &str, event: &str| -> bool {
            arr.as_array()
                .map(|arr| {
                    arr.iter().any(|hook| {
                        hook.get("command")
                            .and_then(|c| c.as_str())
                            .map(|c| is_aiki_hooks_command_with_params(c, Some(agent), Some(event)))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        };

    // Check for hooks.beforeSubmitPrompt with cursor agent and prompt.submitted event
    let has_before_submit = hooks
        .get("beforeSubmitPrompt")
        .map(|arr| has_aiki_hook_with_params(arr, "cursor", "prompt.submitted"))
        .unwrap_or(false);

    // Check for hooks.afterFileEdit with cursor agent and change.completed event
    let has_after_file_edit = hooks
        .get("afterFileEdit")
        .map(|arr| has_aiki_hook_with_params(arr, "cursor", "change.completed"))
        .unwrap_or(false);

    has_before_submit && has_after_file_edit
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_check_claude_code_hooks_complete() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/aiki hooks handle --agent claude-code --event session.started"
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/aiki hooks handle --agent claude-code --event change.completed"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        assert!(check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_claude_code_hooks_missing_post_tool_use() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/aiki hooks handle --agent claude-code --event session.started"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        assert!(!check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_claude_code_hooks_missing_session_start() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/aiki hooks handle --agent claude-code --event change.completed"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        assert!(!check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_claude_code_hooks_wrong_command() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/some-other-tool"
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/aiki hooks handle --agent claude-code --event change.completed"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        assert!(!check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_claude_code_hooks_no_file() {
        let path = std::path::Path::new("/nonexistent/path/settings.json");
        assert!(!check_claude_code_hooks(path));
    }

    #[test]
    fn test_check_cursor_hooks_complete() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "/path/to/aiki hooks handle --agent cursor --event prompt.submitted"
                }],
                "afterFileEdit": [{
                    "command": "/path/to/aiki hooks handle --agent cursor --event change.completed"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_missing_after_file_edit() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "/path/to/aiki hooks handle --agent cursor --event prompt.submitted"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(!check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_missing_before_submit() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "afterFileEdit": [{
                    "command": "/path/to/aiki hooks handle --agent cursor --event change.completed"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(!check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_wrong_command() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "/path/to/some-other-tool"
                }],
                "afterFileEdit": [{
                    "command": "/path/to/aiki hooks handle --agent cursor --event change.completed"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(!check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_generic_aiki_not_enough() {
        // Ensure just "aiki" without "hooks handle" doesn't match
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "/path/to/aiki init"
                }],
                "afterFileEdit": [{
                    "command": "/path/to/aiki record"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(!check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_no_file() {
        let path = std::path::Path::new("/nonexistent/path/hooks.json");
        assert!(!check_cursor_hooks(path));
    }

    // Tests for is_aiki_hooks_command_with_params

    #[test]
    fn test_is_aiki_hooks_command_basic() {
        assert!(is_aiki_hooks_command_with_params(
            "aiki hooks handle --agent claude-code --event session.started",
            Some("claude-code"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_with_exe() {
        assert!(is_aiki_hooks_command_with_params(
            "aiki.exe hooks handle --agent claude-code --event session.started",
            Some("claude-code"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_with_path() {
        assert!(is_aiki_hooks_command_with_params(
            "/usr/local/bin/aiki hooks handle --agent cursor --event prompt.submitted",
            Some("cursor"),
            Some("prompt.submitted")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_with_path_and_exe() {
        assert!(is_aiki_hooks_command_with_params(
            "C:\\Program Files\\aiki.exe hooks handle --agent claude-code --event change.completed",
            Some("claude-code"),
            Some("change.completed")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_relative_path() {
        assert!(is_aiki_hooks_command_with_params(
            "./aiki hooks handle --agent cursor --event change.completed",
            Some("cursor"),
            Some("change.completed")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_wrong_agent() {
        // Should fail: command has claude-code but we expect cursor
        assert!(!is_aiki_hooks_command_with_params(
            "aiki hooks handle --agent claude-code --event session.started",
            Some("cursor"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_wrong_event() {
        // Should fail: command has session.started but we expect change.completed
        assert!(!is_aiki_hooks_command_with_params(
            "aiki hooks handle --agent claude-code --event session.started",
            Some("claude-code"),
            Some("change.completed")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_missing_agent() {
        // Should fail: no --agent flag
        assert!(!is_aiki_hooks_command_with_params(
            "aiki hooks handle --event session.started",
            Some("claude-code"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_missing_event() {
        // Should fail: no --event flag
        assert!(!is_aiki_hooks_command_with_params(
            "aiki hooks handle --agent claude-code",
            Some("claude-code"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_not_hooks_handle() {
        // Should fail: not "hooks handle"
        assert!(!is_aiki_hooks_command_with_params(
            "aiki init --agent claude-code --event session.started",
            Some("claude-code"),
            Some("session.started")
        ));
    }

    #[test]
    fn test_is_aiki_hooks_command_no_params_check() {
        // Should pass with no param requirements
        assert!(is_aiki_hooks_command_with_params(
            "aiki hooks handle",
            None,
            None
        ));
    }

    #[test]
    fn test_check_claude_code_hooks_with_exe() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": "aiki.exe hooks handle --agent claude-code --event session.started"
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": "C:\\Users\\foo\\aiki.exe hooks handle --agent claude-code --event change.completed"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        assert!(check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_with_exe() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "aiki.exe hooks handle --agent cursor --event prompt.submitted"
                }],
                "afterFileEdit": [{
                    "command": "./aiki.exe hooks handle --agent cursor --event change.completed"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        assert!(check_cursor_hooks(file.path()));
    }

    #[test]
    fn test_check_claude_code_hooks_wrong_agent() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup",
                    "hooks": [{
                        "type": "command",
                        "command": "aiki hooks handle --agent cursor --event session.started"
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": "aiki hooks handle --agent claude-code --event change.completed"
                    }]
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&settings).unwrap()).unwrap();

        // Should fail: SessionStart has wrong agent (cursor instead of claude-code)
        assert!(!check_claude_code_hooks(file.path()));
    }

    #[test]
    fn test_check_cursor_hooks_wrong_event() {
        let mut file = NamedTempFile::new().unwrap();
        let hooks = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": "aiki hooks handle --agent cursor --event session.started"
                }],
                "afterFileEdit": [{
                    "command": "aiki hooks handle --agent cursor --event change.completed"
                }]
            }
        });
        write!(file, "{}", serde_json::to_string(&hooks).unwrap()).unwrap();

        // Should fail: beforeSubmitPrompt has wrong event (session.started instead of prompt.submitted)
        assert!(!check_cursor_hooks(file.path()));
    }
}

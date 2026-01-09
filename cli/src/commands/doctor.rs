use crate::commands::zed_detection;
use crate::editors::zed as ide_config;
use crate::error::Result;
use crate::repo::RepoDetector;
use crate::signing;
use anyhow::Context;
use std::env;
use std::fs;
use std::io::{self, Write};

/// Template for the <aiki> block in AGENTS.md
const AIKI_BLOCK_TEMPLATE: &str = r#"<aiki version="1.0">
## Aiki Task System

You have access to an AI-first task management system. Tasks are:
- **Automatically created** from errors you encounter (type errors, test failures, etc.)
- **Automatically closed** when you fix the underlying issue
- **Always visible** via context injection (survives Claude's context compaction)
- **Stored persistently** on the `aiki/tasks` JJ branch

### Quick Reference

```bash
# See what's ready to work on (3-5 tasks with context)
aiki task

# Start working on a task (shows full details with body)
aiki task start <task-id>

# Start multiple related tasks for batch work
aiki task start <id1> <id2> <id3>

# Stop current task (with optional reason)
aiki task stop --reason "Blocked by API credentials"

# Close completed task
aiki task close <task-id>

# Add new task manually
aiki task add "Task name" --p0
```

### Task Output Format

All task commands return XML with this structure:

```xml
<aiki_task cmd="list" status="ok">
  <!-- What just happened -->
  <started>...</started>

  <!-- Current state -->
  <context>
    <in_progress>
      <task id="a1b2" name="Fix null check"/>
    </in_progress>
    <list ready="3">
      <task id="def" priority="p0" name="Fix missing return"/>
      <task id="ghi" priority="p1" name="Consider using const"/>
    </list>
  </context>
</aiki_task>
```

The `<context>` element shows:
- What you're currently working on (`<in_progress>`)
- What's ready to work on next (`<list>`)
- Enough context to make batching decisions

### Workflow Tips

1. **Check tasks regularly** - Run `aiki task` to see what's ready
2. **Batch related work** - Start multiple tasks together when they're related
3. **Use task bodies** - When you start a task, read its `<body>` for full context
4. **Stop when blocked** - Use `aiki task stop --reason` to explain blockers
5. **Close when done** - Use `aiki task close` when you complete a task

### Task Priorities

Priorities: `p0` (urgent) → `p1` (high) → `p2` (normal, default) → `p3` (low)

Tasks are automatically sorted by priority in the ready queue.
</aiki>
"#;

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
                if content.contains("<aiki version=\"1.0\">") {
                    println!("  ✓ AGENTS.md has current <aiki> block");
                } else if content.contains("<aiki version=") {
                    println!("  ⚠ AGENTS.md has outdated <aiki> block");
                    if fix {
                        // Replace old block with new one
                        if let Some(start) = content.find("<aiki version=") {
                            if let Some(end) = content.find("</aiki>") {
                                let before = &content[..start];
                                let after = &content[end + "</aiki>".len()..];
                                let updated = format!("{}{}{}", before.trim_end(), AIKI_BLOCK_TEMPLATE, after.trim_start());
                                match fs::write(&agents_path, updated) {
                                    Ok(()) => {
                                        println!("    ✓ Updated <aiki> block to version 1.0");
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

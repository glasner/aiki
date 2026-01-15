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
const AIKI_BLOCK_TEMPLATE: &str = r#"<aiki version="1.3">

## ⛔ STOP - Read This First

**Before doing ANY substantive work, you MUST run:**
```bash
aiki task add "Description of what you're about to do"
aiki task start <task-id>
```

**FORBIDDEN:** Do NOT use `TodoWrite`, the `Task` tool, or mental checklists. These do not persist.

**When closing tasks, summarize your work:**
```bash
aiki task close <task-id> --comment "What you did"
```

---

## Aiki Task System

**IMPORTANT: Use `aiki task` for ALL task management.** Do not use built-in todo tools (TodoWrite, task lists, etc.). Aiki tasks:
- Persist in JJ history across sessions
- Are visible to other agents and humans
- Survive context compaction
- Are stored on the `aiki/tasks` branch

### TL;DR (First-Time Use)

```bash
# 1) List ready tasks
aiki task

# 2) Add a task
aiki task add "Task description"

# 3) Start the task (copy full ID from output)
aiki task start <task-id>

# 4) Close it when done (with comment describing your work)
aiki task close <task-id> --comment "What I did to fix this"
```

### First Action Rule

**Before doing any substantive work, create and start a task.** This includes:
- Code reviews (`review @file`)
- Document reviews (`review @doc.md`)
- Bug investigations
- Feature implementations
- Refactoring

```bash
# ALWAYS do this first, before reading/analyzing/implementing:
aiki task add "Review assign-tasks.md design"
aiki task start <task-id>
# ... now do the work ...
aiki task close <task-id> --comment "Reviewed, found 3 issues: ..."
```

### When to Use Tasks

- Any work beyond a quick one-liner or immediate response
- Any multi-step change, investigation, or review
- Anything that could carry over across sessions

### Quick Reference

```bash
# See what's ready to work on
aiki task

# Add a new task (do this instead of TodoWrite!)
aiki task add "Task description"

# Start working on a task
aiki task start <task-id>

# Start multiple related tasks for batch work
aiki task start <id1> <id2> <id3>

# Stop current task (with optional reason)
aiki task stop --reason "Blocked on X"

# Add a comment (without closing)
aiki task comment --id <task-id> "Progress update: ..."

# Show task details including comments
aiki task show <task-id>

# Close with comment (preferred - atomic operation)
aiki task close <task-id> --comment "Fixed by updating X to do Y"

# Close multiple tasks
aiki task close <id1> <id2> <id3> --comment "All done"
```

### Parent + Subtasks (Example)

```bash
# Create a parent task
aiki task add "Review prompt-history findings"

# Add subtasks under the parent
aiki task add --parent <parent-id> "Check attribution range collisions"
aiki task add --parent <parent-id> "Define intent summary field"
aiki task add --parent <parent-id> "Add privacy redaction rules"

# Start the parent - this reveals subtasks
aiki task start <parent-id>

# Work through subtasks, closing each with a comment
aiki task start <parent-id>.1
# ... do the work ...
aiki task close <parent-id>.1 --comment "Fixed by ..."
```

### Parent Task Behavior

When you start a parent task with subtasks:
1. A `.0` subtask auto-starts: "Review all subtasks and start first batch"
2. `aiki task` now shows only subtasks (scoped view)
3. Subtask IDs are `<parent-id>.1`, `<parent-id>.2`, etc.
4. When all subtasks are closed, the parent auto-starts for final review
5. Close the parent task when everything is complete

### When Planning Work

Instead of creating a mental todo list or using built-in tools:

```bash
# Break down the work
aiki task add "Research existing implementation"
aiki task add "Design the solution"
aiki task add "Implement changes"
aiki task add "Add tests"

# Start the first task
aiki task start <id>
```

### Task Output Format

Commands return XML showing current state:

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress>
      <task id="abc" name="Current task"/>
    </in_progress>
    <list ready="3">
      <task id="def" priority="p0" name="Next task"/>
    </list>
  </context>
</aiki_task>
```

**Reading the output:**
- `<in_progress>` - Tasks you're currently working on
- `<list ready="N">` - Tasks ready to be started
- `scope="<id>"` attribute means you're inside a parent task (only subtasks shown)

### Task IDs

- IDs are 32-character strings (e.g., `xtuttnyvykpulsxzqnznsxylrzkkqssy`)
- Copy the full ID from command output
- Subtask IDs append a number: `<parent-id>.1`, `<parent-id>.2`

### Workflow

1. **Plan with tasks** - Use `aiki task add` to break down work
2. **Start before working** - Run `aiki task start` before implementation
3. **Stop when blocked** - Use `aiki task stop --reason` to document blockers
4. **Close with comment** - Use `aiki task close --comment` to document your work
5. **Close immediately** - Don't leave tasks open after finishing

### Common Pitfalls

- **Doing reviews without creating a task first** ← Most common mistake!
- **Using TodoWrite instead of `aiki task`** ← Second most common!
- Forgetting to `start` before you begin work
- Closing tasks without `--comment` to describe what you did
- Leaving tasks open after finishing
- Creating long tasks without subtasks for multi-step work
- Trying to `start` a task that's already in progress
- Forgetting to close the parent task after all subtasks are done

### Task Priorities

`p0` (urgent) → `p1` (high) → `p2` (normal, default) → `p3` (low)
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
                if content.contains("<aiki version=\"1.3\">") {
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

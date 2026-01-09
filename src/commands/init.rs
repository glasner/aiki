use crate::config;
use crate::editors::zed as ide_config;
use crate::error::Result;
use crate::jj;
use crate::repo::RepoDetector;
use crate::signing;
use anyhow::Context;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

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

pub fn run(quiet: bool) -> Result<()> {
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

        // Initialize pure JJ storage (independent from Git)
        workspace
            .init()
            .context("Failed to initialize JJ repository")?;

        if !quiet {
            println!("✓ Initialized JJ repository");
        }
    }

    // Create .aiki directory to store repository-specific configuration
    let aiki_dir = repo_root.join(".aiki");
    fs::create_dir_all(&aiki_dir).context("Failed to create .aiki directory")?;

    // Save previous git hooks path before configuring global hooks
    // This allows Git hooks to chain to pre-existing hooks
    config::save_previous_hooks_path(&repo_root)?;

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
                            let wizard = signing::SignSetupWizard::new(repo_root.clone());
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

                            let wizard = signing::SignSetupWizard::new(repo_root.clone());
                            wizard.run(Some(signing::SetupMode::Manual { backend, key }))?;
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

    // Configure IDE settings (Zed)
    if !quiet {
        println!("\nConfiguring IDE settings...");
    }

    match ide_config::configure_zed() {
        Ok(()) => {
            if !quiet {
                println!("✓ Configured Zed editor for ACP support");
                if let Some(path) = ide_config::zed_settings_path() {
                    println!("  Settings: {}", path.display());
                }
            }
        }
        Err(e) => {
            if !quiet {
                println!("⚠ Failed to configure Zed: {}", e);
                println!("  You can configure manually later");
            }
        }
    }

    // Ensure AGENTS.md has task system instructions
    if !quiet {
        println!("\nConfiguring agent instructions...");
    }
    ensure_agents_md(&repo_root, quiet)?;

    if !quiet {
        println!("\n✓ Repository initialized successfully!");
        println!("\nYour AI changes will now be tracked automatically.");
        println!("Git commits will include AI co-authors.");
    }

    Ok(())
}

fn prompt_choice(prompt: &str, min: usize, max: usize) -> Result<usize> {
    loop {
        print!("{} [{}]: ", prompt, min);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
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

fn prompt_string(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(def) = default {
        print!("{} [{}]: ", prompt, def);
    } else {
        print!("{}: ", prompt);
    }
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        if let Some(def) = default {
            return Ok(def.to_string());
        }
    }

    Ok(input)
}

/// Ensure AGENTS.md exists with the <aiki> block for task system instructions
fn ensure_agents_md(repo_root: &Path, quiet: bool) -> Result<()> {
    let agents_path = repo_root.join("AGENTS.md");

    if agents_path.exists() {
        // Read existing file
        let content = fs::read_to_string(&agents_path)
            .context("Failed to read AGENTS.md")?;

        // Check for <aiki> block
        if !content.contains("<aiki version=") {
            // Prepend block
            let updated = format!("{}\n{}", AIKI_BLOCK_TEMPLATE, content);
            fs::write(&agents_path, updated)
                .context("Failed to update AGENTS.md")?;
            if !quiet {
                println!("✓ Added <aiki> block to AGENTS.md");
            }
        } else if !content.contains("<aiki version=\"1.0\">") {
            // Version is outdated
            if !quiet {
                println!("⚠ AGENTS.md has outdated <aiki> block");
                println!("  Run `aiki doctor --fix` to update");
            }
        } else if !quiet {
            println!("✓ AGENTS.md already has <aiki> block");
        }
    } else {
        // Create new AGENTS.md with just the block
        fs::write(&agents_path, AIKI_BLOCK_TEMPLATE)
            .context("Failed to create AGENTS.md")?;
        if !quiet {
            println!("✓ Created AGENTS.md with task system instructions");
        }
    }

    Ok(())
}

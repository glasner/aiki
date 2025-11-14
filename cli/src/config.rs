use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Aiki configuration structure
#[derive(Debug, Serialize, Deserialize)]
pub struct AikiConfig {
    pub aiki: AikiMeta,
    pub review: ReviewConfig,
    pub workers: WorkersConfig,
    pub git: GitConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AikiMeta {
    pub version: String,
    pub initialized_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Debounce duration for rapid file changes (milliseconds)
    pub debounce_ms: u64,
    /// Cache size limit (megabytes)
    pub cache_size_mb: u64,
    /// Enable AI review (requires API key)
    pub ai_review_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkersConfig {
    /// Run static analysis (clippy, eslint, etc.)
    pub static_analysis: bool,
    /// Run type checking (tsc, rust-analyzer, etc.)
    pub type_checking: bool,
    /// Number of parallel review workers
    pub parallelism: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitConfig {
    /// Block changes from being exported to git on critical issues
    pub block_on_critical: bool,
    /// Block changes from being exported to git on warnings
    pub block_on_warnings: bool,
    /// Auto-escalate to human after N failed attempts
    pub auto_escalate_after: u32,
}

impl Default for AikiConfig {
    fn default() -> Self {
        Self {
            aiki: AikiMeta {
                version: env!("CARGO_PKG_VERSION").to_string(),
                initialized_at: Utc::now().to_rfc3339(),
            },
            review: ReviewConfig {
                debounce_ms: 300,
                cache_size_mb: 100,
                ai_review_enabled: false,
            },
            workers: WorkersConfig {
                static_analysis: true,
                type_checking: true,
                parallelism: 4,
            },
            git: GitConfig {
                block_on_critical: true,
                block_on_warnings: false,
                auto_escalate_after: 3,
            },
        }
    }
}

/// Configure git to use aiki's hooks directory
pub fn configure_git_hooks_path(repo_root: &Path) -> Result<()> {
    // Set core.hooksPath to .aiki/githooks
    let output = Command::new("git")
        .args(["config", "core.hooksPath", ".aiki/githooks"])
        .current_dir(repo_root)
        .output()
        .context("Failed to run git config core.hooksPath")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to configure git hooks path: {}", stderr);
    }

    println!("✓ Configured git to use .aiki/githooks");
    Ok(())
}

/// Install Git hooks for automatic co-author attribution
///
/// This reads the hook template, replaces __PREVIOUS_HOOK_PATH__ with the saved
/// previous hooks path, and writes it to .aiki/githooks/prepare-commit-msg.
pub fn install_git_hooks(repo_root: &Path) -> Result<()> {
    let aiki_dir = repo_root.join(".aiki");
    let githooks_dir = aiki_dir.join("githooks");
    let hook_file = githooks_dir.join("prepare-commit-msg");

    // Read previous hooks path
    let previous_path_file = aiki_dir.join(".previous_hooks_path");
    let previous_hooks_path =
        fs::read_to_string(&previous_path_file).context("Failed to read .previous_hooks_path")?;

    // Read hook template (embedded in binary)
    let template = include_str!("../templates/prepare-commit-msg.sh");

    // Replace placeholder with actual previous hooks path
    let hook_content = template.replace("__PREVIOUS_HOOK_PATH__", &previous_hooks_path);

    // Write hook file
    fs::write(&hook_file, hook_content).context("Failed to write prepare-commit-msg hook")?;

    // Make hook executable (Unix/macOS/Linux)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_file)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_file, perms)?;
    }

    println!("✓ Installed prepare-commit-msg hook");
    Ok(())
}

/// Save the current git core.hooksPath configuration before installing aiki hooks
///
/// This preserves the previous hooks path so that aiki hooks can chain to it.
/// The path is saved to `.aiki/.previous_hooks_path`.
///
/// Three states are handled:
/// 1. Not set (git config returns empty) - saves ".git/hooks" (Git's default)
/// 2. Empty string - saves "EMPTY"
/// 3. Valid path - saves the actual path
pub fn save_previous_hooks_path(repo_root: &Path) -> Result<()> {
    let aiki_dir = repo_root.join(".aiki");
    let previous_path_file = aiki_dir.join(".previous_hooks_path");

    // Get current core.hooksPath value
    let output = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(repo_root)
        .output()
        .context("Failed to run git config core.hooksPath")?;

    let hooks_path = if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            "EMPTY".to_string()
        } else {
            path
        }
    } else {
        // Config key doesn't exist - use Git's default hooks path
        // This allows chaining to existing hooks in .git/hooks
        ".git/hooks".to_string()
    };

    // Save to .aiki/.previous_hooks_path
    fs::write(&previous_path_file, &hooks_path).context("Failed to write .previous_hooks_path")?;

    println!("✓ Saved previous hooks path: {}", hooks_path);
    Ok(())
}

/// Initialize the .aiki directory structure and configuration
pub fn initialize_aiki_directory(repo_root: &Path) -> Result<()> {
    let aiki_dir = repo_root.join(".aiki");

    // Check if already initialized
    if aiki_dir.exists() {
        println!("✓ Aiki directory already exists");
        return Ok(());
    }

    println!("Creating .aiki directory structure...");

    // Create directory structure
    for dir in ["cache", "logs", "tmp", "githooks"] {
        fs::create_dir_all(aiki_dir.join(dir))
            .with_context(|| format!("Failed to create .aiki/{} directory", dir))?;
    }

    // Create cache index
    let cache_index = aiki_dir.join("cache").join("index.json");
    fs::write(&cache_index, "{}").context("Failed to create cache index")?;

    // Create .gitignore for SQLite WAL files and other temporary files
    let aiki_gitignore = aiki_dir.join(".gitignore");
    fs::write(
        &aiki_gitignore,
        "# SQLite Write-Ahead Logging files\n*.db-wal\n*.db-shm\n",
    )
    .context("Failed to create .aiki/.gitignore")?;

    // Create default configuration
    let config = AikiConfig::default();
    let config_toml =
        toml::to_string_pretty(&config).context("Failed to serialize configuration")?;
    let config_path = aiki_dir.join("config.toml");
    fs::write(&config_path, config_toml).context("Failed to write configuration file")?;

    println!("✓ Created .aiki directory");
    println!("  ├── cache/     (review cache)");
    println!("  ├── logs/      (watcher logs)");
    println!("  ├── tmp/       (temporary files)");
    println!("  ├── githooks/  (git hooks - version controlled)");
    println!("  └── config.toml");

    // Update .gitignore to ignore only runtime directories
    update_gitignore_for_runtime(repo_root)?;

    Ok(())
}

/// Update .gitignore to exclude only runtime .aiki subdirectories (logs, cache, tmp)
/// The .aiki/githooks and .aiki/config.toml should be version controlled
fn update_gitignore_for_runtime(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    // Read existing .gitignore or create empty string
    let mut gitignore_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).context("Failed to read .gitignore")?
    } else {
        String::new()
    };

    // Check if aiki runtime directories are already in .gitignore
    let has_aiki_entries = gitignore_content
        .lines()
        .any(|line| line.contains(".aiki/logs/") || line.contains(".aiki/cache/"));

    if has_aiki_entries {
        return Ok(());
    }

    // Add runtime directories to .gitignore (but not .aiki/ itself)
    if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
        gitignore_content.push('\n');
    }
    gitignore_content.push_str("\n# Aiki runtime (hooks and config are version controlled)\n");
    gitignore_content.push_str(".aiki/logs/\n");
    gitignore_content.push_str(".aiki/cache/\n");
    gitignore_content.push_str(".aiki/tmp/\n");

    fs::write(&gitignore_path, gitignore_content).context("Failed to update .gitignore")?;
    println!("✓ Added .aiki runtime directories to .gitignore");
    println!("  Note: .aiki/githooks/ and .aiki/config.toml can be version controlled");

    Ok(())
}

/// Install Claude Code plugin configuration for provenance tracking
pub fn install_claude_code_hooks(repo_root: &Path) -> Result<()> {
    let settings_dir = repo_root.join(".claude");
    let settings_file = settings_dir.join("settings.json");

    // Create .claude directory if it doesn't exist
    fs::create_dir_all(&settings_dir).context("Failed to create .claude directory")?;

    // Read existing settings or create new
    let mut settings: serde_json::Value = if settings_file.exists() {
        let content = fs::read_to_string(&settings_file).context("Failed to read settings.json")?;
        serde_json::from_str(&content).context("Failed to parse settings.json")?
    } else {
        serde_json::json!({})
    };

    // Add aiki marketplace to extraKnownMarketplaces
    if settings.get("extraKnownMarketplaces").is_none() {
        settings["extraKnownMarketplaces"] = serde_json::json!({});
    }

    // Use local plugin directory (relative to repo root)
    settings["extraKnownMarketplaces"]["aiki"] = serde_json::json!({
        "source": {
            "source": "directory",
            "path": "./claude-code-plugin"
        }
    });

    // Enable aiki plugin
    if settings.get("enabledPlugins").is_none() {
        settings["enabledPlugins"] = serde_json::json!({});
    }

    settings["enabledPlugins"]["aiki@aiki"] = serde_json::json!(true);

    // Write back
    let settings_json =
        serde_json::to_string_pretty(&settings).context("Failed to serialize settings")?;
    fs::write(&settings_file, settings_json).context("Failed to write settings.json")?;

    println!("✓ Configured Claude Code plugin (.claude/settings.json)");
    println!("  → Aiki plugin will auto-install when you open this project in Claude Code");
    println!("  → After trusting the repository, restart Claude Code to activate the plugin");

    Ok(())
}

/// Install Cursor hooks for provenance tracking
///
/// Unlike Claude Code hooks (per-repo), Cursor hooks are global (user-level).
/// Cursor hook events accept arrays of commands, so we can safely append without
/// overwriting existing hooks.
pub fn install_cursor_hooks(_repo_root: &Path) -> Result<()> {
    // Get user home directory
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;
    let cursor_dir = home_dir.join(".cursor");
    let hooks_file = cursor_dir.join("hooks.json");

    // Create .cursor directory if it doesn't exist
    fs::create_dir_all(&cursor_dir).context("Failed to create .cursor directory")?;

    // Read existing hooks.json or create new
    let mut hooks: serde_json::Value = if hooks_file.exists() {
        let content =
            fs::read_to_string(&hooks_file).context("Failed to read ~/.cursor/hooks.json")?;
        serde_json::from_str(&content).context("Failed to parse ~/.cursor/hooks.json")?
    } else {
        // Create new hooks config
        serde_json::json!({
            "version": 1,
            "hooks": {}
        })
    };

    // Ensure hooks object exists
    if hooks.get("hooks").is_none() {
        hooks["hooks"] = serde_json::json!({});
    }

    // Get or create afterFileEdit array
    let after_file_edit = hooks["hooks"]["afterFileEdit"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // Check if aiki hook already exists
    let aiki_command = "aiki record-change --cursor";
    let already_installed = after_file_edit.iter().any(|hook| {
        hook.get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains("aiki record-change --cursor"))
            .unwrap_or(false)
    });

    if already_installed {
        println!("✓ Cursor hooks already configured");
        return Ok(());
    }

    // Append aiki hook to array (preserves existing hooks!)
    let mut new_hooks = after_file_edit;
    new_hooks.push(serde_json::json!({
        "command": aiki_command
    }));

    hooks["hooks"]["afterFileEdit"] = serde_json::Value::Array(new_hooks);

    // Write back
    let hooks_json =
        serde_json::to_string_pretty(&hooks).context("Failed to serialize hooks.json")?;
    fs::write(&hooks_file, hooks_json).context("Failed to write ~/.cursor/hooks.json")?;

    println!("✓ Configured Cursor hooks (~/.cursor/hooks.json)");
    println!("  → Added to afterFileEdit hook array (existing hooks preserved)");
    println!("  → Cursor must be restarted to activate hooks");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = AikiConfig::default();
        assert_eq!(config.aiki.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(config.review.debounce_ms, 300);
        assert_eq!(config.workers.parallelism, 4);
        assert!(config.git.block_on_critical);
        assert!(!config.git.block_on_warnings);
    }

    #[test]
    fn config_serialization_includes_all_sections() {
        let config = AikiConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[aiki]"));
        assert!(toml_str.contains("[review]"));
        assert!(toml_str.contains("[workers]"));
        assert!(toml_str.contains("[git]"));
    }

    #[test]
    fn initialize_aiki_directory_creates_structure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = initialize_aiki_directory(temp_dir.path());
        assert!(result.is_ok());

        // Check directory structure
        assert!(temp_dir.path().join(".aiki").exists());
        assert!(temp_dir.path().join(".aiki/cache").exists());
        assert!(temp_dir.path().join(".aiki/logs").exists());
        assert!(temp_dir.path().join(".aiki/tmp").exists());
        assert!(temp_dir.path().join(".aiki/githooks").exists());
        assert!(temp_dir.path().join(".aiki/config.toml").exists());
        assert!(temp_dir.path().join(".aiki/cache/index.json").exists());

        // Check .gitignore was created with runtime directories only
        assert!(temp_dir.path().join(".gitignore").exists());
        let gitignore = fs::read_to_string(temp_dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".aiki/logs/"));
        assert!(gitignore.contains(".aiki/cache/"));
        assert!(gitignore.contains(".aiki/tmp/"));
        // Should NOT ignore the entire .aiki/ directory
        assert!(!gitignore.lines().any(|line| line.trim() == ".aiki/"));
    }

    #[test]
    fn initialize_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Initialize once
        let result1 = initialize_aiki_directory(temp_dir.path());
        assert!(result1.is_ok());

        // Initialize again
        let result2 = initialize_aiki_directory(temp_dir.path());
        assert!(result2.is_ok());
    }

    #[test]
    fn install_claude_code_hooks_creates_settings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = install_claude_code_hooks(temp_dir.path());
        assert!(result.is_ok());

        // Check .claude/settings.json exists
        let settings_file = temp_dir.path().join(".claude/settings.json");
        assert!(settings_file.exists());

        // Parse and verify contents
        let content = fs::read_to_string(&settings_file).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Verify extraKnownMarketplaces
        assert!(settings.get("extraKnownMarketplaces").is_some());
        assert!(settings["extraKnownMarketplaces"].get("aiki").is_some());

        let marketplace = &settings["extraKnownMarketplaces"]["aiki"];
        assert_eq!(marketplace["source"]["source"], "directory");
        assert_eq!(marketplace["source"]["path"], "./claude-code-plugin");

        // Verify enabledPlugins
        assert!(settings.get("enabledPlugins").is_some());
        assert_eq!(settings["enabledPlugins"]["aiki@aiki"], true);
    }

    #[test]
    fn install_claude_code_hooks_preserves_existing_settings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_dir = temp_dir.path().join(".claude");
        let settings_file = settings_dir.join("settings.json");

        // Create existing settings
        fs::create_dir_all(&settings_dir).unwrap();
        let existing = serde_json::json!({
            "other_setting": "value",
            "extraKnownMarketplaces": {
                "other-marketplace": {
                    "source": {
                        "source": "github",
                        "repo": "other/repo"
                    }
                }
            }
        });
        fs::write(
            &settings_file,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        // Install plugin configuration
        let result = install_claude_code_hooks(temp_dir.path());
        assert!(result.is_ok());

        // Verify existing settings preserved
        let content = fs::read_to_string(&settings_file).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["other_setting"], "value");
        assert!(settings["extraKnownMarketplaces"]["other-marketplace"].is_object());
        assert!(settings["extraKnownMarketplaces"]["aiki"].is_object());
        assert_eq!(settings["enabledPlugins"]["aiki@aiki"], true);
    }

    #[test]
    fn initialize_creates_gitignore_for_wal_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = initialize_aiki_directory(temp_dir.path());
        assert!(result.is_ok());

        // Check .aiki/.gitignore exists
        let aiki_gitignore = temp_dir.path().join(".aiki/.gitignore");
        assert!(aiki_gitignore.exists());

        // Verify contents
        let content = fs::read_to_string(&aiki_gitignore).unwrap();
        assert!(content.contains("*.db-wal"));
        assert!(content.contains("*.db-shm"));
    }

    #[test]
    fn save_previous_hooks_path_handles_not_set() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Create .aiki directory
        initialize_aiki_directory(temp_dir.path()).unwrap();

        // Save hooks path (should default to .git/hooks when not set)
        let result = save_previous_hooks_path(temp_dir.path());
        assert!(result.is_ok());

        // Verify file contents
        let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
        assert!(previous_path_file.exists());
        let content = fs::read_to_string(&previous_path_file).unwrap();
        assert_eq!(content, ".git/hooks");
    }

    #[test]
    fn save_previous_hooks_path_handles_custom_path() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Set custom hooks path
        Command::new("git")
            .args(["config", "core.hooksPath", ".custom-hooks"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Create .aiki directory
        initialize_aiki_directory(temp_dir.path()).unwrap();

        // Save hooks path
        let result = save_previous_hooks_path(temp_dir.path());
        assert!(result.is_ok());

        // Verify file contents
        let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
        assert!(previous_path_file.exists());
        let content = fs::read_to_string(&previous_path_file).unwrap();
        assert_eq!(content, ".custom-hooks");
    }
}

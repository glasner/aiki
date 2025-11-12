use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
    /// Block commits on critical issues
    pub block_on_critical: bool,
    /// Block commits on warnings
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
    for dir in ["cache", "logs", "tmp"] {
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
    println!("  ├── cache/  (review cache)");
    println!("  ├── logs/   (watcher logs)");
    println!("  ├── tmp/    (temporary files)");
    println!("  └── config.toml");

    // Update .gitignore
    update_gitignore(repo_root)?;

    Ok(())
}

/// Update .gitignore to exclude .aiki directory
fn update_gitignore(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    // Read existing .gitignore or create empty string
    let mut gitignore_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).context("Failed to read .gitignore")?
    } else {
        String::new()
    };

    // Check if .aiki is already in .gitignore
    if gitignore_content
        .lines()
        .any(|line| line.trim() == ".aiki/")
    {
        return Ok(());
    }

    // Add .aiki to .gitignore
    if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
        gitignore_content.push('\n');
    }
    gitignore_content.push_str("\n# Aiki\n.aiki/\n");

    fs::write(&gitignore_path, gitignore_content).context("Failed to update .gitignore")?;
    println!("✓ Added .aiki/ to .gitignore");

    Ok(())
}

/// Install Claude Code hooks for provenance tracking
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

    // Add PostToolUse hooks
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }

    settings["hooks"]["PostToolUse"] = serde_json::json!([
        {
            "matcher": "Edit|Write",
            "hooks": [
                {
                    "type": "command",
                    "command": "aiki record-change",
                    "timeout": 5
                }
            ]
        }
    ]);

    // Write back
    let settings_json =
        serde_json::to_string_pretty(&settings).context("Failed to serialize settings")?;
    fs::write(&settings_file, settings_json).context("Failed to write settings.json")?;

    println!("✓ Installed Claude Code hooks (.claude/settings.json)");

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
        assert!(temp_dir.path().join(".aiki/config.toml").exists());
        assert!(temp_dir.path().join(".aiki/cache/index.json").exists());

        // Check .gitignore was created
        assert!(temp_dir.path().join(".gitignore").exists());
        let gitignore = fs::read_to_string(temp_dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".aiki/"));
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

        assert!(settings.get("hooks").is_some());
        assert!(settings["hooks"].get("PostToolUse").is_some());

        let hooks = settings["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);

        let hook = &hooks[0];
        assert_eq!(hook["matcher"], "Edit|Write");
        assert_eq!(hook["hooks"][0]["command"], "aiki record-change");
        assert_eq!(hook["hooks"][0]["timeout"], 5);
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
            "hooks": {
                "OtherHook": []
            }
        });
        fs::write(
            &settings_file,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        // Install hooks
        let result = install_claude_code_hooks(temp_dir.path());
        assert!(result.is_ok());

        // Verify existing settings preserved
        let content = fs::read_to_string(&settings_file).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["other_setting"], "value");
        assert!(settings["hooks"]["PostToolUse"].is_array());
        // Note: OtherHook would be overwritten if hooks object was replaced entirely
        // but our implementation only sets PostToolUse
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
}

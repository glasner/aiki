use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;

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

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            // A custom hooks path is configured - save it
            fs::write(&previous_path_file, &path)
                .context("Failed to write .previous_hooks_path")?;
            println!("✓ Saved previous hooks path: {}", path);
        } else {
            // Empty string - save "EMPTY" to distinguish from not-set
            fs::write(&previous_path_file, "EMPTY")
                .context("Failed to write .previous_hooks_path")?;
            println!("✓ Saved previous hooks path: EMPTY");
        }
    } else {
        // Config key doesn't exist - no previous hooks path to save
        // Don't create .previous_hooks_path file at all
        println!("✓ No previous hooks path configured");
    }

    Ok(())
}

/// Get the absolute path to the aiki binary
pub fn get_aiki_binary_path() -> Result<String> {
    let output = Command::new("which")
        .arg("aiki")
        .output()
        .context("Failed to run 'which aiki'")?;

    if output.status.success() {
        let path = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in aiki path")?
            .trim()
            .to_string();
        return Ok(path);
    }

    // Fallback: try to get the current executable path
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(path_str) = current_exe.to_str() {
            eprintln!(
                "Note: Using current executable path (aiki not in PATH): {}",
                path_str
            );
            return Ok(path_str.to_string());
        }
    }

    anyhow::bail!(
        "Could not find 'aiki' binary in PATH.\n\
         Please install aiki or ensure it's in your PATH:\n\
         • cargo install --path .\n\
         • Or add the target directory to PATH"
    );
}

/// Install global Git hooks in ~/.aiki/githooks/
pub fn install_global_git_hooks() -> Result<()> {
    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let githooks_dir = home_dir.join(".aiki/githooks");

    // Create directory if it doesn't exist
    fs::create_dir_all(&githooks_dir).context("Failed to create ~/.aiki/githooks directory")?;

    // Read hook template (embedded in binary)
    let template = include_str!("../templates/prepare-commit-msg.sh");

    // For global hook, we read the previous path at runtime from .aiki/.previous_hooks_path
    // The template already handles this - we replace the placeholder with a shell command
    let hook_content = template.replace(
        "PREVIOUS_HOOK=\"__PREVIOUS_HOOK_PATH__\"",
        "PREVIOUS_HOOK=\"$(cat .aiki/.previous_hooks_path 2>/dev/null || echo '')\"",
    );

    let hook_file = githooks_dir.join("prepare-commit-msg");
    fs::write(&hook_file, hook_content).context("Failed to write prepare-commit-msg hook")?;

    // Make hook executable (Unix/macOS/Linux)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_file)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_file, perms)?;
    }

    println!("✓ Installed Git hooks at {}", githooks_dir.display());
    Ok(())
}

/// Install global Claude Code hooks in ~/.claude/settings.json
pub fn install_claude_code_hooks_global() -> Result<()> {
    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let settings_path = home_dir.join(".claude/settings.json");
    let aiki_path = get_aiki_binary_path()?;

    // Create ~/.claude if it doesn't exist
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).context("Failed to create ~/.claude directory")?;
    }

    // Load existing settings or create new
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content =
            fs::read_to_string(&settings_path).context("Failed to read ~/.claude/settings.json")?;
        serde_json::from_str(&content).context("Failed to parse ~/.claude/settings.json")?
    } else {
        json!({})
    };

    // Ensure hooks object exists
    if settings.get("hooks").is_none() {
        settings["hooks"] = json!({});
    }

    // SessionStart hook for auto-initialization
    settings["hooks"]["SessionStart"] = json!([{
        "matcher": "startup",
        "hooks": [{
            "type": "command",
            "command": format!("{} hooks handle --agent claude-code --event SessionStart", aiki_path),
            "timeout": 10
        }]
    }]);

    // PostToolUse hook for change tracking
    settings["hooks"]["PostToolUse"] = json!([{
        "matcher": "Edit|Write",
        "hooks": [{
            "type": "command",
            "command": format!("{} hooks handle --agent claude-code --event PostToolUse", aiki_path),
            "timeout": 5
        }]
    }]);

    // Write updated settings
    let content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize settings.json")?;
    fs::write(&settings_path, content).context("Failed to write ~/.claude/settings.json")?;

    println!(
        "✓ Installed Claude Code hooks at {}",
        settings_path.display()
    );
    println!("  - SessionStart: Auto-initialize repositories");
    println!("  - PostToolUse: Track AI-assisted changes");

    Ok(())
}

/// Install global Cursor hooks in ~/.cursor/hooks.json
pub fn install_cursor_hooks_global() -> Result<()> {
    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let hooks_path = home_dir.join(".cursor/hooks.json");
    let aiki_path = get_aiki_binary_path()?;

    // Create ~/.cursor if it doesn't exist
    if let Some(parent) = hooks_path.parent() {
        fs::create_dir_all(parent).context("Failed to create ~/.cursor directory")?;
    }

    // Read existing hooks or create new
    let mut hooks: serde_json::Value = if hooks_path.exists() {
        let content =
            fs::read_to_string(&hooks_path).context("Failed to read ~/.cursor/hooks.json")?;
        serde_json::from_str(&content).context("Failed to parse ~/.cursor/hooks.json")?
    } else {
        json!({
            "version": 1,
            "hooks": {}
        })
    };

    // Ensure hooks object exists
    if hooks.get("hooks").is_none() {
        hooks["hooks"] = json!({});
    }

    // beforeSubmitPrompt hook for auto-initialization
    let before_submit = hooks["hooks"]["beforeSubmitPrompt"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let aiki_init_hook = json!({
        "command": format!("{} hooks handle --agent cursor --event beforeSubmitPrompt", aiki_path)
    });

    // Check if already installed
    let init_already_installed = before_submit.iter().any(|hook| {
        hook.get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains("aiki hooks handle"))
            .unwrap_or(false)
    });

    if !init_already_installed {
        let mut new_hooks = before_submit;
        new_hooks.push(aiki_init_hook);
        hooks["hooks"]["beforeSubmitPrompt"] = json!(new_hooks);
    }

    // afterFileEdit hook for change tracking
    let after_file_edit = hooks["hooks"]["afterFileEdit"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let aiki_record_hook = json!({
        "command": format!("{} hooks handle --agent cursor --event afterFileEdit", aiki_path)
    });

    // Check if already installed
    let record_already_installed = after_file_edit.iter().any(|hook| {
        hook.get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains("aiki hooks handle"))
            .unwrap_or(false)
    });

    if !record_already_installed {
        let mut new_hooks = after_file_edit;
        new_hooks.push(aiki_record_hook);
        hooks["hooks"]["afterFileEdit"] = json!(new_hooks);
    }

    // Write updated hooks
    let content = serde_json::to_string_pretty(&hooks).context("Failed to serialize hooks.json")?;
    fs::write(&hooks_path, content).context("Failed to write ~/.cursor/hooks.json")?;

    println!("✓ Installed Cursor hooks at {}", hooks_path.display());
    println!("  - beforeSubmitPrompt: Auto-initialize repositories");
    println!("  - afterFileEdit: Track AI-assisted changes");

    Ok(())
}

/// Read JJ repository config from .jj/repo/config.toml
pub fn read_jj_repo_config(repo_path: &Path) -> Result<toml::Value> {
    let config_path = repo_path.join(".jj").join("repo").join("config.toml");

    if !config_path.exists() {
        // Return empty config if file doesn't exist
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }

    let config_content =
        fs::read_to_string(&config_path).context("Failed to read .jj/repo/config.toml")?;

    toml::from_str(&config_content).context("Failed to parse .jj/repo/config.toml")
}

/// Write JJ repository config to .jj/repo/config.toml
pub fn write_jj_repo_config(repo_path: &Path, config: &toml::Value) -> Result<()> {
    let config_path = repo_path.join(".jj").join("repo").join("config.toml");

    // Ensure .jj/repo directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create .jj/repo directory")?;
    }

    let config_content =
        toml::to_string_pretty(config).context("Failed to serialize config to TOML")?;

    fs::write(&config_path, config_content).context("Failed to write .jj/repo/config.toml")
}

/// Update JJ signing configuration in .jj/repo/config.toml
pub fn update_jj_signing_config(
    repo_path: &Path,
    backend: &str,
    key: Option<&str>,
    behavior: &str,
) -> Result<()> {
    let mut config = read_jj_repo_config(repo_path)?;

    // Ensure config is a table
    let config_table = config
        .as_table_mut()
        .context("Config root is not a table")?;

    // Create [signing] section
    let mut signing_table = toml::map::Map::new();
    signing_table.insert(
        "behavior".to_string(),
        toml::Value::String(behavior.to_string()),
    );
    signing_table.insert(
        "backend".to_string(),
        toml::Value::String(backend.to_string()),
    );

    // For SSH backend, add key and allowed-signers configuration
    if backend == "ssh" {
        if let Some(key_path) = key {
            signing_table.insert("key".to_string(), toml::Value::String(key_path.to_string()));
        }

        // Add [signing.backends.ssh] configuration
        let mut ssh_config = toml::map::Map::new();
        ssh_config.insert(
            "allowed-signers".to_string(),
            toml::Value::String(".jj/allowed-signers".to_string()),
        );

        let mut backends = toml::map::Map::new();
        backends.insert("ssh".to_string(), toml::Value::Table(ssh_config));
        signing_table.insert("backends".to_string(), toml::Value::Table(backends));
    }

    // Insert signing section into config
    config_table.insert("signing".to_string(), toml::Value::Table(signing_table));

    write_jj_repo_config(repo_path, &config)
}

/// Check if Claude Code is installed
#[cfg(test)]
mod tests {
    use super::*;

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
        fs::create_dir_all(temp_dir.path().join(".aiki")).unwrap();

        // Save hooks path (should not create file when not set)
        let result = save_previous_hooks_path(temp_dir.path());
        assert!(result.is_ok());

        // Verify file does NOT exist (no custom hooks path to preserve)
        let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
        assert!(
            !previous_path_file.exists(),
            "File should not exist when there's no custom hooks path configured"
        );
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

        // Create .aiki directory (minimal - only if needed)
        fs::create_dir_all(temp_dir.path().join(".aiki")).unwrap();

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

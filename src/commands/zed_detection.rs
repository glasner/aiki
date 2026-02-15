use crate::error::{AikiError, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Finds the Zed external agents directory based on the platform
///
/// # Platform-specific paths
/// - macOS: `~/Library/Application Support/Zed/external_agents`
/// - Linux: `$XDG_DATA_HOME/zed/external_agents` or `~/.local/share/zed/external_agents`
///
/// # Returns
/// The path to the external_agents directory if it exists
pub fn find_zed_external_agents_dir() -> Result<PathBuf> {
    let base_dir = if cfg!(target_os = "macos") {
        dirs::home_dir()
            .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Could not find home directory")))?
            .join("Library/Application Support/Zed/external_agents")
    } else if cfg!(target_os = "linux") {
        // Respect XDG_DATA_HOME if set
        let base_dir = env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .expect("Could not find home directory")
                    .join(".local/share")
            });
        base_dir.join("zed/external_agents")
    } else {
        return Err(AikiError::UnsupportedPlatform(
            "Zed integration only supported on macOS and Linux".to_string(),
        ));
    };

    if !base_dir.exists() {
        return Err(AikiError::ZedNotInstalled(base_dir));
    }

    Ok(base_dir)
}

/// Maps agent type to the directory name used by Zed
///
/// # Examples
/// - "claude-code" -> "claude-code-acp"
/// - "codex" -> "codex"
/// - "gemini" -> "gemini-cli"
fn agent_type_to_directory(agent_type: &str) -> &str {
    match agent_type {
        "claude-code" => "claude-code-acp",
        "codex" => "codex",
        "gemini" => "gemini-cli",
        other => other,
    }
}

/// Finds the latest version of an agent installed by Zed
///
/// Zed installs agents in versioned directories like:
/// `external_agents/claude-code-acp/0.10.6/`
///
/// This function finds the highest version number available.
fn find_latest_agent_version(agent_dir: &PathBuf) -> Result<PathBuf> {
    if !agent_dir.exists() {
        return Err(AikiError::ZedAgentNotInstalled(
            agent_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
        ));
    }

    // Read all version directories
    let mut versions: Vec<PathBuf> = std::fs::read_dir(agent_dir)
        .map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to read agent directory: {}", e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect();

    if versions.is_empty() {
        return Err(AikiError::ZedAgentNotInstalled(
            agent_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
        ));
    }

    // Sort by version number (simple lexicographic sort works for semver)
    versions.sort();

    // Return the highest version
    Ok(versions
        .last()
        .expect("BUG: versions vec is empty despite is_empty() check")
        .clone())
}

/// Finds the path to an ACP binary installed by Zed
///
/// # Arguments
/// * `agent_type` - The agent type (e.g., "claude-code", "codex")
///
/// # Returns
/// The full path to the Node.js entry point for the agent
///
/// # Example
/// For "claude-code", returns:
/// `~/Library/Application Support/Zed/external_agents/claude-code-acp/0.10.6/node_modules/@zed-industries/claude-code-acp/dist/index.js`
pub fn find_zed_acp_binary(agent_type: &str) -> Result<PathBuf> {
    let external_agents_dir = find_zed_external_agents_dir()?;
    let agent_dir_name = agent_type_to_directory(agent_type);
    let agent_dir = external_agents_dir.join(agent_dir_name);

    let version_dir = find_latest_agent_version(&agent_dir)?;

    // Try pattern 1: Native binary (e.g., codex)
    // Structure: external_agents/{agent}/{version}/{agent-name}
    let executable_name = derive_executable_name(agent_type);
    let native_binary = version_dir.join(&executable_name);
    if native_binary.exists() {
        return Ok(native_binary);
    }

    // Try pattern 2: Node.js package (e.g., claude-code)
    // Structure: external_agents/{agent}/{version}/node_modules/@zed-industries/{agent}/dist/index.js
    let nodejs_binary = version_dir
        .join("node_modules")
        .join("@zed-industries")
        .join(agent_dir_name)
        .join("dist")
        .join("index.js");

    if nodejs_binary.exists() {
        return Ok(nodejs_binary);
    }

    // Neither pattern found
    Err(AikiError::Other(anyhow::anyhow!(
        "Zed installed agent '{}' in {} but binary not found.\nTried:\n  - Native: {}\n  - Node.js: {}",
        agent_type,
        version_dir.display(),
        native_binary.display(),
        nodejs_binary.display()
    )))
}

/// Checks if Node.js is installed and accessible
///
/// # Returns
/// Ok if Node.js is available, Err with helpful message otherwise
pub fn check_nodejs_installed() -> Result<()> {
    match Command::new("node").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                eprintln!("  Using Node.js {}", version.trim());
                Ok(())
            } else {
                Err(AikiError::NodeJsNotFound)
            }
        }
        Err(_) => Err(AikiError::NodeJsNotFound),
    }
}

/// Resolves the path to an ACP agent binary, trying multiple sources
///
/// Resolution order:
/// 1. Zed installation (preferred)
/// 2. System PATH (backwards compatibility)
///
/// # Arguments
/// * `agent_type` - The agent type (e.g., "claude-code")
///
/// # Returns
/// Either the full path to the Node.js entry point (for Zed-installed agents)
/// or the executable name (for PATH-based agents)
pub fn resolve_agent_binary(agent_type: &str) -> Result<ResolvedBinary> {
    // Try Zed installation first
    match find_zed_acp_binary(agent_type) {
        Ok(path) => {
            eprintln!("  Using Zed-installed agent: {}", path.display());

            // Determine if this is a Node.js script or native binary
            if path.extension().and_then(|s| s.to_str()) == Some("js") {
                // Node.js script - check for Node.js
                check_nodejs_installed()?;
                return Ok(ResolvedBinary::ZedNodeJs(path));
            } else {
                // Native binary - no Node.js needed
                return Ok(ResolvedBinary::ZedNative(path));
            }
        }
        Err(e) => {
            eprintln!("  Zed installation not found: {}", e);
        }
    }

    // Fall back to PATH
    let executable = derive_executable_name(agent_type);
    match which::which(&executable) {
        Ok(path) => {
            eprintln!("  Using PATH binary: {}", path.display());
            Ok(ResolvedBinary::InPath(executable))
        }
        Err(_) => Err(AikiError::AcpBinaryNotFound {
            agent_type: agent_type.to_string(),
            package_name: derive_package_name(agent_type),
            executable_name: executable,
        }),
    }
}

/// Derives the executable name for an agent type (used for PATH fallback)
fn derive_executable_name(agent_type: &str) -> String {
    match agent_type {
        "claude-code" => "cc-acp".to_string(),
        "codex" => "codex-acp".to_string(),
        "gemini" => "gemini-cli".to_string(),
        other => other.to_string(),
    }
}

/// Derives the npm package name for an agent type (used in error messages)
fn derive_package_name(agent_type: &str) -> String {
    match agent_type {
        "claude-code" => "claude-code-acp".to_string(),
        "codex" => "codex-acp".to_string(),
        "gemini" => "gemini-cli".to_string(),
        other => other.to_string(),
    }
}

/// Represents a resolved ACP binary location
#[derive(Debug)]
pub enum ResolvedBinary {
    /// Node.js script installed by Zed (requires Node.js)
    ZedNodeJs(PathBuf),
    /// Native binary installed by Zed (no Node.js needed)
    ZedNative(PathBuf),
    /// Binary in system PATH (executable name)
    InPath(String),
}

impl ResolvedBinary {
    /// Gets the command to execute
    pub fn command(&self) -> String {
        match self {
            ResolvedBinary::ZedNodeJs(_) => "node".to_string(),
            ResolvedBinary::ZedNative(path) => path.display().to_string(),
            ResolvedBinary::InPath(exe) => exe.clone(),
        }
    }

    /// Gets the arguments to pass to the command
    pub fn args(&self) -> Vec<String> {
        match self {
            ResolvedBinary::ZedNodeJs(path) => vec![path.display().to_string()],
            ResolvedBinary::ZedNative(_) => vec![],
            ResolvedBinary::InPath(_) => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_mapping() {
        assert_eq!(agent_type_to_directory("claude-code"), "claude-code-acp");
        assert_eq!(agent_type_to_directory("codex"), "codex");
        assert_eq!(agent_type_to_directory("gemini"), "gemini-cli");
        assert_eq!(agent_type_to_directory("custom"), "custom");
    }

    #[test]
    fn test_executable_name_derivation() {
        assert_eq!(derive_executable_name("claude-code"), "cc-acp");
        assert_eq!(derive_executable_name("codex"), "codex-acp");
        assert_eq!(derive_executable_name("gemini"), "gemini-cli");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_find_zed_external_agents_dir() {
        // This test will fail if Zed isn't installed, which is expected
        let result = find_zed_external_agents_dir();
        if let Ok(path) = result {
            assert!(path.ends_with("Library/Application Support/Zed/external_agents"));
        }
    }
}

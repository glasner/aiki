# Claude Code Version Check Optimization

## Problem

The current implementation shells out to `claude --version` to detect the installed Claude Code version. This adds ~120ms to hook execution due to Node.js startup overhead, nearly doubling the total hook time from 130ms to 250ms.

## Solution

Read the version directly from Claude Code's `package.json` file instead of spawning a process.

## Implementation

### NPM Global Root Detection

The "correct" way to find npm's global root (`npm root -g` or `npm config get prefix`) spawns Node.js with the same ~100ms overhead. Instead, we check config files and environment variables directly:

```rust
use std::path::PathBuf;

/// Find npm global node_modules directory without spawning a process.
/// Checks env vars, .npmrc files, NVM, and platform defaults.
fn find_npm_global_root() -> Option<PathBuf> {
    // 1. Check env vars (free)
    if let Ok(prefix) = std::env::var("NPM_CONFIG_PREFIX") {
        return Some(PathBuf::from(prefix).join("lib/node_modules"));
    }

    // 2. Check .npmrc files for prefix setting
    let home = std::env::var("HOME").ok();
    let npmrc_paths = [
        home.as_ref().map(|h| format!("{}/.npmrc", h)),
        Some("/etc/npmrc".to_string()),
    ];

    for path in npmrc_paths.into_iter().flatten() {
        if let Some(prefix) = parse_npmrc_prefix(&path) {
            return Some(PathBuf::from(prefix).join("lib/node_modules"));
        }
    }

    // 3. Check NVM installation
    if let Some(path) = find_nvm_node_modules() {
        return Some(path);
    }

    // 4. Check user's npm-global directory
    if let Some(ref home) = home {
        let user_global = PathBuf::from(home).join(".npm-global/lib/node_modules");
        if user_global.exists() {
            return Some(user_global);
        }
    }

    // 5. Platform-specific defaults
    find_platform_default_root()
}

fn parse_npmrc_prefix(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("prefix=") || line.starts_with("prefix =") {
            return Some(line.split('=').nth(1)?.trim().to_string());
        }
    }
    None
}

fn find_nvm_node_modules() -> Option<PathBuf> {
    let nvm_dir = std::env::var("NVM_DIR").ok()?;
    let nvm_path = PathBuf::from(&nvm_dir);

    // Try to read default alias
    let default_alias = nvm_path.join("alias/default");
    let version = std::fs::read_to_string(&default_alias).ok()?;
    let version = version.trim();

    // NVM aliases can be indirect (e.g., "lts/*" -> "lts/iron" -> "v20.x.x")
    // For simplicity, check versions directory directly
    let versions_dir = nvm_path.join("versions/node");
    
    // If version is a direct version number
    let direct_path = versions_dir.join(version).join("lib/node_modules");
    if direct_path.exists() {
        return Some(direct_path);
    }

    // Otherwise, find most recent version
    let mut versions: Vec<_> = std::fs::read_dir(&versions_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    
    versions.sort_by(|a, b| b.path().cmp(&a.path())); // Reverse sort for newest first
    
    versions.first().map(|v| v.path().join("lib/node_modules"))
}

#[cfg(target_os = "macos")]
fn find_platform_default_root() -> Option<PathBuf> {
    let defaults = [
        "/opt/homebrew/lib/node_modules",  // Apple Silicon Homebrew
        "/usr/local/lib/node_modules",      // Intel Homebrew / default
    ];
    
    defaults.iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

#[cfg(target_os = "linux")]
fn find_platform_default_root() -> Option<PathBuf> {
    let defaults = [
        "/usr/local/lib/node_modules",
        "/usr/lib/node_modules",
    ];
    
    defaults.iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn find_platform_default_root() -> Option<PathBuf> {
    None
}
```

### Version Detection

```rust
use serde_json::Value;

const CLAUDE_PACKAGE_NAME: &str = "@anthropic-ai/claude-code";

/// Get Claude Code version by reading package.json directly.
/// Avoids ~120ms Node.js startup overhead from `claude --version`.
pub fn get_claude_version() -> Option<String> {
    // Try npm global root detection first
    if let Some(npm_root) = find_npm_global_root() {
        let package_json = npm_root.join(CLAUDE_PACKAGE_NAME).join("package.json");
        if let Some(version) = read_version_from_package_json(&package_json) {
            return Some(version);
        }
    }

    // Fallback: resolve via `which` (adds ~10ms but handles edge cases)
    resolve_via_which()
}

fn read_version_from_package_json(path: &PathBuf) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    json["version"].as_str().map(String::from)
}

fn resolve_via_which() -> Option<String> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let bin_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let resolved = std::fs::canonicalize(&bin_path).ok()?;

    // Walk up directory tree to find package.json
    let mut dir = resolved.parent()?;
    for _ in 0..5 {
        let pkg = dir.join("package.json");
        if let Some(version) = read_version_from_package_json(&pkg.to_path_buf()) {
            return Some(version);
        }
        dir = dir.parent()?;
    }

    None
}
```

### Caching (Optional)

If version is checked multiple times per process:

```rust
use std::sync::OnceLock;

static CLAUDE_VERSION: OnceLock<Option<String>> = OnceLock::new();

pub fn get_claude_version_cached() -> Option<&'static str> {
    CLAUDE_VERSION
        .get_or_init(get_claude_version)
        .as_deref()
}
```

## Performance Comparison

| Approach | Time | Notes |
|----------|------|-------|
| `claude --version` | ~120ms | Node.js startup overhead |
| `npm root -g` | ~100ms | Also spawns Node.js |
| `which` + fs read | ~10ms | Process spawn for path resolution |
| Direct fs read | <1ms | No process spawn |
| Cached | ~0ms | After first call |

## Detection Priority

1. **`NPM_CONFIG_PREFIX` env var** - Explicitly configured prefix
2. **`~/.npmrc` prefix setting** - User-level npm configuration
3. **`/etc/npmrc` prefix setting** - System-level npm configuration  
4. **NVM installation** - Checks `$NVM_DIR/versions/node/*/lib/node_modules`
5. **`~/.npm-global`** - Common user global directory
6. **Platform defaults** - `/opt/homebrew/...` (macOS ARM), `/usr/local/...` (macOS Intel/Linux)
7. **`which` fallback** - Resolve binary symlink and walk up (~10ms penalty)

## Dependencies

```toml
[dependencies]
serde_json = "1.0"
```

## Edge Cases Handled

| Scenario | How It's Handled |
|----------|------------------|
| Custom `NPM_CONFIG_PREFIX` | Checked first via env var |
| Custom prefix in `.npmrc` | Parsed from `~/.npmrc` and `/etc/npmrc` |
| NVM installation | Detects `$NVM_DIR`, finds current Node version |
| Homebrew (Apple Silicon) | Checks `/opt/homebrew/lib/node_modules` |
| Homebrew (Intel) | Checks `/usr/local/lib/node_modules` |
| Linux standard | Checks `/usr/local/lib/node_modules`, `/usr/lib/node_modules` |
| Symlinked binary | `canonicalize()` resolves before walking up |
| Claude not installed | Returns `None` |
| Unknown install location | Falls back to `which` resolution |

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_detection() {
        // Only run if Claude is installed
        if let Some(version) = get_claude_version() {
            assert!(version.split('.').count() >= 2, "Expected semver format");
            println!("Detected Claude Code version: {}", version);
        }
    }

    #[test]
    fn test_npm_root_detection() {
        if let Some(root) = find_npm_global_root() {
            assert!(root.exists(), "Detected npm root should exist");
            println!("Detected npm global root: {:?}", root);
        }
    }

    #[test]
    fn test_npmrc_parsing() {
        // Test with a temp .npmrc
        let temp_dir = std::env::temp_dir();
        let npmrc_path = temp_dir.join(".npmrc_test");
        std::fs::write(&npmrc_path, "prefix=/custom/path\n").unwrap();
        
        let prefix = parse_npmrc_prefix(&npmrc_path.to_string_lossy());
        assert_eq!(prefix, Some("/custom/path".to_string()));
        
        std::fs::remove_file(npmrc_path).ok();
    }
}
```

//! Flow loading with path resolution and caching.
//!
//! This module provides the [`FlowLoader`] struct which handles:
//! - Path resolution via [`FlowResolver`](super::flow_resolver::FlowResolver)
//! - YAML parsing via [`FlowParser`](super::parser::FlowParser)
//! - Flow caching by canonical path (avoids reloading the same file)
//!
//! # Example
//!
//! ```rust,ignore
//! use aiki::flows::loader::FlowLoader;
//!
//! let mut loader = FlowLoader::new()?;
//!
//! // Load a flow (automatically cached)
//! let (flow, canonical_path) = loader.load("aiki/quick-lint")?;
//!
//! // Loading the same flow again returns cached version
//! let (flow2, _) = loader.load("aiki/quick-lint")?;
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::flow_resolver::FlowResolver;
use super::parser::FlowParser;
use super::types::Flow;
use crate::error::{AikiError, Result};

/// Flow loader with path resolution and caching.
///
/// The loader resolves flow paths using [`FlowResolver`], parses the YAML using
/// [`FlowParser`], and caches flows by their canonical path to avoid redundant
/// file reads and parsing.
///
/// # Caching
///
/// Flows are cached by their canonical (resolved) path. This means:
/// - `./flow.yml` and `../flows/flow.yml` that point to the same file will use the same cache entry
/// - The cache persists for the lifetime of the loader
/// - Use [`clear_cache`](Self::clear_cache) to reset if needed
#[derive(Debug)]
pub struct FlowLoader {
    resolver: FlowResolver,
    cache: HashMap<PathBuf, Flow>,
}

impl FlowLoader {
    /// Create a new FlowLoader by discovering project root.
    ///
    /// # Errors
    ///
    /// Returns `AikiError::NotInAikiProject` if no `.aiki/` directory is found.
    pub fn new() -> Result<Self> {
        Ok(Self {
            resolver: FlowResolver::new()?,
            cache: HashMap::new(),
        })
    }

    /// Create a new FlowLoader starting search from a specific directory.
    ///
    /// This is useful for testing or when you need to load flows from
    /// a directory other than the current working directory.
    ///
    /// # Errors
    ///
    /// Returns `AikiError::NotInAikiProject` if no `.aiki/` directory is found.
    pub fn with_start_dir(start_dir: &Path) -> Result<Self> {
        Ok(Self {
            resolver: FlowResolver::with_start_dir(start_dir)?,
            cache: HashMap::new(),
        })
    }

    /// Load a flow and return both the flow and its canonical path.
    ///
    /// The canonical path is used by [`FlowComposer`](super::composer::FlowComposer)
    /// for cycle detection. Caching is done by canonical path to avoid loading
    /// the same file multiple times.
    ///
    /// # Arguments
    ///
    /// * `path` - The namespaced flow path (e.g., "aiki/quick-lint", "eslint/check")
    ///
    /// # Returns
    ///
    /// A tuple of (Flow, canonical PathBuf). The canonical path is always absolute
    /// and resolved (no symlinks).
    ///
    /// # Errors
    ///
    /// Returns:
    /// - `AikiError::InvalidFlowPath` if path is not in {namespace}/{name} format
    /// - `AikiError::FlowNotFound` if the file doesn't exist
    /// - `AikiError::Other` if YAML parsing fails
    pub fn load(&mut self, path: &str) -> Result<(Flow, PathBuf)> {
        // Resolve to canonical path
        let canonical_path = self.resolver.resolve(path)?;

        // Check cache (by canonical path)
        if let Some(flow) = self.cache.get(&canonical_path) {
            return Ok((flow.clone(), canonical_path));
        }

        // Load and parse flow file
        let flow = self.load_from_file(&canonical_path, path)?;

        // Cache by canonical path and return both flow and path
        self.cache.insert(canonical_path.clone(), flow.clone());
        Ok((flow, canonical_path))
    }

    /// Load a flow from an absolute file path.
    ///
    /// This is used for loading flows that aren't in the standard namespace structure,
    /// such as .aiki/flows/default.yml.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Absolute path to the flow file
    ///
    /// # Returns
    ///
    /// A tuple of (Flow, canonical PathBuf). The canonical path is always absolute
    /// and resolved (no symlinks).
    ///
    /// # Errors
    ///
    /// Returns:
    /// - `AikiError::FlowNotFound` if the file doesn't exist
    /// - `AikiError::Other` if YAML parsing fails
    pub fn load_from_file_path(&mut self, file_path: &Path) -> Result<(Flow, PathBuf)> {
        // Canonicalize the path
        let canonical_path = file_path
            .canonicalize()
            .map_err(|e| AikiError::FlowNotFound {
                path: file_path.display().to_string(),
                resolved_path: file_path.display().to_string(),
                source: e,
            })?;

        // Check cache (by canonical path)
        if let Some(flow) = self.cache.get(&canonical_path) {
            return Ok((flow.clone(), canonical_path));
        }

        // Load and parse flow file
        let flow = self.load_from_file(&canonical_path, &file_path.display().to_string())?;

        // Cache by canonical path and return both flow and path
        self.cache.insert(canonical_path.clone(), flow.clone());
        Ok((flow, canonical_path))
    }

    /// Load a flow from the core system (bundled in binary).
    ///
    /// This returns the core flow without caching since it's already
    /// statically compiled into the binary.
    ///
    /// # Returns
    ///
    /// A reference to the cached core flow.
    #[must_use]
    pub fn load_core_flow() -> &'static Flow {
        super::bundled::load_core_flow()
    }

    /// Clear the flow cache.
    ///
    /// This is useful for testing or when you need to reload flows
    /// that may have changed on disk.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached flows.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Get the project root directory.
    #[must_use]
    pub fn project_root(&self) -> &Path {
        self.resolver.project_root()
    }

    /// Get the home directory.
    #[must_use]
    pub fn home_dir(&self) -> &Path {
        self.resolver.home_dir()
    }

    /// Get the default flows directory for this project.
    ///
    /// Returns `{project_root}/.aiki/flows`.
    #[must_use]
    pub fn default_flows_dir(&self) -> PathBuf {
        self.resolver.project_root().join(".aiki/flows")
    }

    /// Load and parse a flow from a file path.
    fn load_from_file(&self, path: &Path, original_path: &str) -> Result<Flow> {
        let contents = fs::read_to_string(path).map_err(|e| AikiError::FlowNotFound {
            path: original_path.to_string(),
            resolved_path: path.display().to_string(),
            source: e,
        })?;

        FlowParser::parse_str(&contents).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to parse flow '{}' ({}): {}",
                original_path,
                path.display(),
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a test project with .aiki/ directory structure
    fn create_test_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/aiki")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/eslint")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/helpers")).unwrap();
        temp_dir
    }

    /// Create a flow file with specified before/after dependencies
    fn create_flow_file(path: &Path, name: &str, before: &[&str], after: &[&str]) {
        // Helper to quote paths that need it (start with @ or contain special chars)
        let quote_if_needed = |s: &str| -> String {
            if s.starts_with('@') || s.contains(':') || s.contains('#') {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        };

        let before_yaml = if before.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = before
                .iter()
                .map(|b| format!("  - {}", quote_if_needed(b)))
                .collect();
            format!("before:\n{}\n", items.join("\n"))
        };

        let after_yaml = if after.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = after
                .iter()
                .map(|a| format!("  - {}", quote_if_needed(a)))
                .collect();
            format!("after:\n{}\n", items.join("\n"))
        };

        let content = format!(
            r#"name: {}
version: "1"
{}{}
"#,
            name, before_yaml, after_yaml
        );
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_load_simple_flow() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/simple.yml");
        create_flow_file(&flow_path, "Simple Flow", &[], &[]);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let (flow, canonical) = loader.load("aiki/simple").unwrap();

        assert_eq!(flow.name, "Simple Flow");
        assert_eq!(
            canonical.canonicalize().unwrap(),
            flow_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_load_flow_with_before_after() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/composed.yml");
        create_flow_file(
            &flow_path,
            "Composed Flow",
            &["aiki/base", "./helpers/lint.yml"],
            &["aiki/cleanup"],
        );

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let (flow, _) = loader.load("aiki/composed").unwrap();

        assert_eq!(flow.name, "Composed Flow");
        assert_eq!(flow.before.len(), 2);
        assert_eq!(flow.before[0], "aiki/base");
        assert_eq!(flow.before[1], "./helpers/lint.yml");
        assert_eq!(flow.after.len(), 1);
        assert_eq!(flow.after[0], "aiki/cleanup");
    }

    #[test]
    fn test_load_caching() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/cached.yml");
        create_flow_file(&flow_path, "Cached Flow", &[], &[]);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();

        // First load
        assert_eq!(loader.cache_size(), 0);
        let (flow1, _) = loader.load("aiki/cached").unwrap();
        assert_eq!(loader.cache_size(), 1);

        // Second load (should hit cache)
        let (flow2, _) = loader.load("aiki/cached").unwrap();
        assert_eq!(loader.cache_size(), 1);

        // Same flow should be returned
        assert_eq!(flow1.name, flow2.name);
    }

    #[test]
    fn test_clear_cache() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/clearable.yml");
        create_flow_file(&flow_path, "Clearable Flow", &[], &[]);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();

        // Load and verify cache
        loader.load("aiki/clearable").unwrap();
        assert_eq!(loader.cache_size(), 1);

        // Clear cache
        loader.clear_cache();
        assert_eq!(loader.cache_size(), 0);

        // Load again (should reload from disk)
        loader.load("aiki/clearable").unwrap();
        assert_eq!(loader.cache_size(), 1);
    }

    #[test]
    fn test_load_flow_not_found() {
        let temp_dir = create_test_project();
        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();

        let result = loader.load("aiki/nonexistent");
        assert!(matches!(result, Err(AikiError::FlowNotFound { .. })));
    }

    #[test]
    fn test_load_invalid_yaml() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/invalid.yml");
        fs::write(&flow_path, "invalid: yaml: content: [").unwrap();

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let result = loader.load("aiki/invalid");

        assert!(matches!(result, Err(AikiError::Other(_))));
    }

    #[test]
    fn test_default_flows_dir() {
        let temp_dir = create_test_project();
        let loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();

        let flows_dir = loader.default_flows_dir();
        assert_eq!(flows_dir, temp_dir.path().join(".aiki/flows"));
    }

    #[test]
    fn test_load_core_flow() {
        let core = FlowLoader::load_core_flow();
        assert_eq!(core.name, "Aiki Core");
    }
}

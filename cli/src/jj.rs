use anyhow::{Context, Result};
use jj_lib::config::StackedConfig;
use jj_lib::repo::StoreFactories;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;
use std::path::Path;

/// Wrapper for JJ workspace operations using jj-lib
pub struct JJWorkspace {
    workspace_root: std::path::PathBuf,
}

impl JJWorkspace {
    /// Create a new JJ workspace manager for the given path
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    /// Create user settings with default configuration for JJ operations
    pub(crate) fn create_user_settings() -> Result<UserSettings> {
        let config = StackedConfig::with_defaults();
        UserSettings::from_config(config)
            .context("Failed to create user settings for JJ operations")
    }

    /// Initialize a pure JJ repository (no Git backend)
    /// This creates a .jj directory with independent storage, completely separate from Git
    pub fn init(&self) -> Result<()> {
        let settings = Self::create_user_settings()?;
        let store_factories = StoreFactories::default();

        // Initialize pure JJ workspace with no Git backend
        let (_workspace, _repo) =
            Workspace::init(&settings, &self.workspace_root, &store_factories)
                .context("Failed to initialize JJ workspace")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that colocated initialization creates both .jj and .git directories
    #[test]
    fn workspace_init_colocated_creates_both_directories() {
        // Create a temporary directory for testing
        let temp_dir = tempfile::tempdir().unwrap();

        let workspace = JJWorkspace::new(temp_dir.path());
        let result = workspace.init_colocated();

        // Should succeed
        assert!(
            result.is_ok(),
            "Workspace initialization should succeed: {:?}",
            result.err()
        );

        // Verify .jj directory was created
        assert!(
            temp_dir.path().join(".jj").exists(),
            ".jj directory should exist"
        );

        // Verify .git directory was created (colocated)
        assert!(
            temp_dir.path().join(".git").exists(),
            ".git directory should exist for colocated workspace"
        );
    }

    #[test]
    fn workspace_new_stores_root_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = JJWorkspace::new(temp_dir.path());

        assert_eq!(workspace.workspace_root, temp_dir.path());
    }
}

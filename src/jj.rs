use anyhow::{Context, Result};
use jj_lib::config::StackedConfig;
use jj_lib::git;
use jj_lib::repo::StoreFactories;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::{default_working_copy_factories, Workspace};
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

    /// Initialize JJ on top of an existing Git repository
    /// This creates a .jj directory and configures it to use the existing .git directory
    ///
    /// DEPRECATED: Use init_with_git_dir() instead to properly handle worktrees and submodules
    #[deprecated(since = "0.1.0", note = "use `init_with_git_dir` instead")]
    #[allow(dead_code)]
    pub fn init_on_existing_git(&self) -> Result<()> {
        let git_repo_path = self.workspace_root.join(".git");
        self.init_with_git_dir(&git_repo_path)
    }

    /// Initialize JJ with a specific Git directory path
    ///
    /// This handles both normal Git repositories (where .git is a directory)
    /// and Git worktrees/submodules (where .git is a file pointing to the real Git directory)
    ///
    /// # Arguments
    /// * `git_dir` - Path to the resolved Git directory (obtained via RepoDetector::resolve_git_dir)
    pub fn init_with_git_dir(&self, git_dir: &Path) -> Result<()> {
        let settings = Self::create_user_settings()?;

        // Initialize JJ using the existing Git repository
        // When git_dir is in the same location as workspace_root/.git, this creates
        // a colocated workspace where JJ and Git share the working copy
        let (_workspace, _repo) =
            Workspace::init_external_git(&settings, &self.workspace_root, git_dir)
                .context("Failed to initialize JJ on existing Git repository")?;

        Ok(())
    }

    /// Initialize a colocated JJ repository (Git-backed)
    /// This creates both .jj and .git directories in the workspace root
    pub fn init_colocated(&self) -> Result<()> {
        let settings = Self::create_user_settings()?;

        // Initialize the colocated workspace
        let (_workspace, _repo) = Workspace::init_colocated_git(&settings, &self.workspace_root)
            .context("Failed to initialize colocated JJ workspace")?;

        Ok(())
    }

    /// Import Git refs and commits into JJ
    /// This should be called after init_with_git_dir() to import existing Git history
    pub fn git_import(&self) -> Result<()> {
        let settings = Self::create_user_settings()?;
        let store_factories = StoreFactories::default();
        let working_copy_factories = default_working_copy_factories();

        // Load the workspace
        let workspace = Workspace::load(
            &settings,
            &self.workspace_root,
            &store_factories,
            &working_copy_factories,
        )
        .context("Failed to load JJ workspace for git import")?;

        let repo = workspace
            .repo_loader()
            .load_at_head()
            .context("Failed to load repository")?;

        // Import Git refs
        let mut tx = repo.start_transaction();
        let git_settings = settings.git_settings()?;
        git::import_refs(tx.repo_mut(), &git_settings)?;
        tx.commit("import git refs")?;

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

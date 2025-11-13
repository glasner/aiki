//! Common test utilities shared across integration tests

#![allow(dead_code)]

use std::path::Path;
use std::time::{Duration, Instant};

/// Check if jj binary is available in PATH
pub fn jj_available() -> bool {
    std::process::Command::new("jj")
        .arg("--version")
        .output()
        .is_ok()
}

/// Initialize a Git repository at the given path
pub fn init_git_repo(path: &Path) {
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(path)
        .output()
        .expect("Failed to initialize Git repository");
}

/// Initialize a JJ workspace (colocated with git)
pub fn init_jj_workspace(path: &Path) -> anyhow::Result<()> {
    let output = std::process::Command::new("jj")
        .arg("git")
        .arg("init")
        .arg("--colocate")
        .current_dir(path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to initialize JJ workspace");
    }

    Ok(())
}

/// Wait for background thread to update commit description
///
/// Polls the JJ commit description until it contains the expected content,
/// or times out after the specified duration.
///
/// Returns true if the expected content was found, false if timed out.
pub fn wait_for_description_update(
    repo_path: &Path,
    expected_content: &str,
    timeout: Duration,
) -> bool {
    let start = Instant::now();

    while start.elapsed() < timeout {
        if let Ok(output) = std::process::Command::new("jj")
            .arg("log")
            .arg("-r")
            .arg("@")
            .arg("-T")
            .arg("description")
            .current_dir(repo_path)
            .output()
        {
            let description = String::from_utf8_lossy(&output.stdout);
            if description.contains(expected_content) {
                return true;
            }
        }

        // Poll every 50ms
        std::thread::sleep(Duration::from_millis(50));
    }

    false
}

/// Get the current commit description from JJ
pub fn get_commit_description(repo_path: &Path) -> String {
    let output = std::process::Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("-T")
        .arg("description")
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log");

    String::from_utf8_lossy(&output.stdout).to_string()
}

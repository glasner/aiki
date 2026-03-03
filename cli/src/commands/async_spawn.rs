use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Spawn `aiki <args>` as a detached background process.
///
/// The child process inherits cwd but detaches stdin/stdout/stderr
/// so the parent can exit immediately.
pub fn spawn_aiki_background(cwd: &Path, args: &[&str]) -> Result<()> {
    let binary = crate::config::get_aiki_binary_path();

    Command::new(&binary)
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn background aiki process: {binary}"))?;

    Ok(())
}

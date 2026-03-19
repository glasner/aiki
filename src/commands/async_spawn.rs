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

/// Spawn `aiki <args>` as a detached background process, piping `stdin_payload` to its stdin.
///
/// Like `spawn_aiki_background`, but uses `Stdio::piped()` for stdin so the
/// caller can pass a payload (e.g. JSON) to the child process. The stdin handle
/// is dropped after writing to send EOF. The child is not waited on.
pub fn spawn_aiki_background_with_stdin(cwd: &Path, args: &[&str], stdin_payload: &[u8]) -> Result<()> {
    let binary = crate::config::get_aiki_binary_path();

    let mut child = Command::new(&binary)
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn background aiki process: {binary}"))?;

    // Write payload to child's stdin, then drop to send EOF
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(stdin_payload); // best-effort
    }
    // Child is detached — don't wait

    Ok(())
}

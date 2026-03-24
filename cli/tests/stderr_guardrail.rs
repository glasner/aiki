use std::process::Command;

#[test]
fn check_stderr_writes() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .parent()
        .expect("cli/ should have a parent directory (project root)");

    let script = std::path::Path::new(manifest_dir).join("tests/scripts/check-stderr-writes.sh");

    let output = Command::new(&script)
        .current_dir(project_root)
        .output()
        .expect("failed to execute check-stderr-writes.sh");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "check-stderr-writes.sh failed (exit {}):\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        );
    }
}

#[test]
fn test_check_stderr_writes_regression() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .parent()
        .expect("cli/ should have a parent directory (project root)");

    let script =
        std::path::Path::new(manifest_dir).join("tests/scripts/test-check-stderr-writes.sh");

    let output = Command::new(&script)
        .current_dir(project_root)
        .output()
        .expect("failed to execute test-check-stderr-writes.sh");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "test-check-stderr-writes.sh failed (exit {}):\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        );
    }
}

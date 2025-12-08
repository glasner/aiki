//! Panic hook installation for ACP proxy diagnostics

use std::io::Write;

/// Install ACP proxy panic hook that writes to both file and stderr
///
/// Unlike human-panic, this runs in both debug and release mode since
/// the proxy is often run in development and we need immediate stderr
/// feedback for diagnosing agent crashes.
///
/// Writes panic information to:
/// - `$TMPDIR/aiki-proxy-panic.log` for persistent debugging
/// - stderr for immediate visibility
pub fn install_acp_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        // Try to write to a debug file in system temp directory
        let log_path = std::env::temp_dir().join("aiki-proxy-panic.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = write_panic_report(&mut file, panic_info);
        }

        // Also write to stderr for immediate visibility
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = write_panic_report(&mut handle, panic_info);
    }));
}

/// Write formatted panic report to a writer (file or stderr)
fn write_panic_report(
    writer: &mut dyn Write,
    panic_info: &std::panic::PanicHookInfo,
) -> std::io::Result<()> {
    writeln!(
        writer,
        "\n=== PANIC IN ACP PROXY at {} ===",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
    )?;
    writeln!(writer, "{}", panic_info)?;

    if let Some(location) = panic_info.location() {
        writeln!(
            writer,
            "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )?;
    }

    writeln!(writer, "=== END PANIC ===\n")?;
    writer.flush()
}

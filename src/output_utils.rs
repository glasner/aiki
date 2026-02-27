//! Centralized output utilities for TTY vs non-TTY contexts.
//!
//! # Output Conventions
//!
//! | Context              | stdout                    | stderr                                    |
//! |----------------------|---------------------------|-------------------------------------------|
//! | TTY (interactive)    | (empty)                   | Human-readable markdown, colors, tables   |
//! | Piped (non-TTY)      | Machine-readable IDs only | Same as TTY (logs, progress)              |
//!
//! # Performance
//!
//! The helpers check `stderr.is_terminal()` before calling formatting closures.
//! This means expensive markdown/table formatting is skipped entirely in fully
//! automated contexts (CI, cron, background agents) where nobody is watching.
//!
//! | Context                          | stderr TTY? | stdout TTY? | Behavior                                |
//! |----------------------------------|-------------|-------------|-----------------------------------------|
//! | Interactive terminal             | Yes         | Yes         | Format + show on stderr, no stdout      |
//! | Typical pipe: `aiki task \| head`| Yes         | No          | Format + show on stderr, IDs on stdout  |
//! | Fully piped: `aiki task 2>&1 \| cat` | No      | No          | Skip formatting, IDs on stdout only     |
//! | Background job: `aiki task &`    | No          | No          | Skip formatting, IDs on stdout only     |

use std::io::IsTerminal;

/// Returns true if stdout is connected to a terminal (not piped).
pub fn is_tty_stdout() -> bool {
    std::io::stdout().is_terminal()
}

/// Returns true if stderr is connected to a terminal.
pub fn is_tty_stderr() -> bool {
    std::io::stderr().is_terminal()
}

/// Emit formatted output to stderr (lazy) and an ID to stdout (when piped).
///
/// The formatter closure is only called when stderr is a TTY, avoiding
/// expensive markdown/table formatting in fully automated contexts.
///
/// # Example
/// ```ignore
/// emit(&task_id, || {
///     let content = format_command_output(&output);
///     MdBuilder::new("review").build(&content, &in_progress, &ready)
/// });
/// ```
pub fn emit(id: &str, formatter: impl FnOnce() -> String) {
    if is_tty_stderr() {
        eprintln!("{}", formatter());
    }
    if !is_tty_stdout() {
        println!("{}", id);
    }
}

/// Emit formatted output to stderr only (lazy).
///
/// The formatter closure is only called when stderr is a TTY.
/// Use this for intermediate status messages or output that doesn't
/// need a corresponding ID on stdout.
///
/// # Example
/// ```ignore
/// emit_stderr(|| {
///     let content = format_command_output(&output);
///     MdBuilder::new("explore").build(&content, &[], &[])
/// });
/// ```
pub fn emit_stderr(formatter: impl FnOnce() -> String) {
    if is_tty_stderr() {
        eprintln!("{}", formatter());
    }
}

/// Emit an ID to stdout when piped (non-TTY stdout).
///
/// Use this for the final machine-readable output of a command.
///
/// # Example
/// ```ignore
/// emit_stdout(&task_id);
/// ```
pub fn emit_stdout(id: &str) {
    if !is_tty_stdout() {
        println!("{}", id);
    }
}

pub mod diff;
pub mod workspace;

pub use workspace::JJWorkspace;

use std::ffi::OsStr;
use std::process::Command;
use std::sync::OnceLock;

/// Common locations where `jj` may be installed (not in default PATH for
/// processes spawned by GUI apps / OTel receivers).
const JJ_FALLBACK_PATHS: &[&str] = &[
    "/opt/homebrew/bin/jj",
    "/usr/local/bin/jj",
    "/usr/bin/jj",
];

/// Resolve the absolute path to the `jj` binary, caching the result.
///
/// When aiki is invoked by a process with a limited PATH (e.g. the OTel
/// receiver spawned on-demand by Codex), `jj` may not be discoverable via
/// the default search path. This function:
///
/// 1. Tries the plain `"jj"` name (works when PATH is correct).
/// 2. Falls back to well-known installation directories.
/// 3. Returns `"jj"` as a last resort so error messages stay meaningful.
pub fn jj_binary() -> &'static str {
    static RESOLVED: OnceLock<String> = OnceLock::new();
    RESOLVED.get_or_init(|| {
        // Fast path: `jj` is on PATH
        if Command::new("jj")
            .arg("version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return "jj".to_string();
        }

        // Check well-known locations
        for path in JJ_FALLBACK_PATHS {
            if std::path::Path::new(path).is_file() {
                return (*path).to_string();
            }
        }

        // Last resort — let the caller get a clear ENOENT
        "jj".to_string()
    })
}

/// Create a `std::process::Command` for the `jj` binary.
///
/// Equivalent to `Command::new(jj_binary())` — use this everywhere instead
/// of `Command::new("jj")` so the binary is resolved via [`jj_binary`].
pub fn jj_cmd() -> Command {
    Command::new(OsStr::new(jj_binary()))
}

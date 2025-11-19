pub mod claude_code;
pub mod cursor;

use anyhow::Result;
use std::io::{self, Read};

/// Read and parse JSON from stdin
///
/// Shared utility for all vendor handlers to read hook payload data.
pub fn read_stdin_json<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;

    // Debug: log raw JSON to see what we actually receive
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[aiki] Raw hook payload JSON:\n{}", buffer);
    }

    Ok(serde_json::from_str(&buffer)?)
}

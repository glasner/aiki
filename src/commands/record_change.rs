use crate::error::Result;
use crate::provenance;
use crate::record_change;

pub fn run(claude_code: bool, cursor: bool, sync: bool) -> Result<()> {
    eprintln!("Warning: 'aiki record-change' is deprecated.");
    eprintln!("  Use 'aiki hooks <vendor> post-change' instead.");
    eprintln!();

    if claude_code {
        Ok(record_change::record_change_legacy(
            provenance::AgentType::ClaudeCode,
            sync,
        )?)
    } else if cursor {
        Ok(record_change::record_change_legacy(
            provenance::AgentType::Cursor,
            sync,
        )?)
    } else {
        eprintln!("Error: Agent type flag required (e.g., --claude-code, --cursor)");
        std::process::exit(1);
    }
}

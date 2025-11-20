//! Built-in function: aiki/core.separate_edits
//!
//! Separates AI changes from user edits using `jj split` when user modifications
//! are detected.
//!
//! This function reconstructs the AI-only file content and uses `jj split` to
//! create two changes: one for AI edits, one for user edits.

use crate::error::{AikiError, Result};
use crate::events::AikiPostChangeEvent;
use crate::flows::state::ActionResult;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Separate AI changes from user edits using jj split
///
/// This function reads metadata and classification from the context (previous let bindings)
/// and uses them to separate changes. It expects:
/// - `metadata` variable with `author` and `message` fields (from build_metadata)
/// - `detection` variable with classification info (from classify_edits)
///
/// # Algorithm
/// 1. For each file with edit details:
///    - Read current file content (has both AI + user changes)
///    - Reconstruct AI-only content by applying AI's edits in reverse
///    - Write AI-only content to file
/// 2. Run `jj split` to create AI change
/// 3. Restore current content (user changes will be in remaining change)
///
/// # Returns
/// JSON with separation results:
/// ```json
/// {
///   "success": true,
///   "ai_change_id": "abc123...",
///   "user_change_id": "def456...",
///   "separated_files": ["file1.rs", "file2.rs"]
/// }
/// ```
///
/// # Example Flow Usage
/// ```yaml
/// PostChange:
///   - let: metadata = self.build_metadata
///   - let: detection = self.classify_edits
///   - let: sep = self.separate_edits
///     on_failure:
///       - log: "Failed to separate edits, recording as single change"
/// ```
///
/// Note: This simplified version derives all needed data from the event and doesn't
/// require function arguments. A future enhancement could add argument support to the
/// flow executor.
pub fn separate_edits(event: &AikiPostChangeEvent) -> Result<ActionResult> {
    // If no edit details, return success (nothing to separate)
    // This allows graceful degradation for hook-based detection where
    // edit details might not be available
    if event.edit_details.is_empty() {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows/core] No edit details available, skipping separation");
        }
        return Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: serde_json::json!({
                "skipped": true,
                "reason": "no_edit_details"
            })
            .to_string(),
            stderr: String::new(),
        });
    }

    // Generate metadata for AI change
    let provenance = crate::provenance::ProvenanceRecord::from_post_change_event(event);
    let ai_message = provenance.to_description();
    let ai_author = event.agent_type.git_author();

    // Files that will be separated (have edit details)
    let files_with_edits: std::collections::HashSet<String> = event
        .edit_details
        .iter()
        .map(|e| e.file_path.clone())
        .collect();

    // Extra files are those in file_paths but not in edit_details (AI-only, no separation)
    let ai_only_files: Vec<String> = event
        .file_paths
        .iter()
        .filter(|p| !files_with_edits.contains(*p))
        .cloned()
        .collect();

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[flows/core] Separating {} files with edits, {} AI-only files",
            files_with_edits.len(),
            ai_only_files.len()
        );
    }

    // Step 1: Reconstruct AI-only content for files with edits
    let mut original_contents: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for file_path in &files_with_edits {
        let full_path = event.cwd.join(file_path);

        // Save current content (AI + user changes)
        let current_content = fs::read_to_string(&full_path).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to read file '{}': {}",
                full_path.display(),
                e
            ))
        })?;
        original_contents.insert(file_path.clone(), current_content.clone());

        // Reconstruct AI-only content
        let ai_only_content = reconstruct_ai_only_content(&current_content, file_path, event)?;

        // Write AI-only content temporarily
        fs::write(&full_path, ai_only_content).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to write AI-only content to '{}': {}",
                full_path.display(),
                e
            ))
        })?;
    }

    // Step 2: Run jj split to create AI change
    // Include both files with edits AND AI-only files in the split
    // IMPORTANT: Convert absolute paths to relative paths for jj split
    let mut all_ai_files: Vec<String> = files_with_edits
        .iter()
        .map(|p| normalize_path_for_jj(p, &event.cwd))
        .collect();
    all_ai_files.extend(
        ai_only_files
            .iter()
            .map(|p| normalize_path_for_jj(p, &event.cwd)),
    );

    // Step 2: Run jj split to create AI change
    // IMPORTANT: If this fails, we must restore original content to avoid leaving
    // working copy in an inconsistent state (with AI-only content)
    let split_result = match run_jj_split(&event.cwd, &ai_message, &ai_author, &all_ai_files) {
        Ok(output) => output,
        Err(e) => {
            // Cleanup: Restore original content before returning error
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[flows/core] jj split failed, restoring original content");
            }
            for (file_path, content) in &original_contents {
                let full_path = event.cwd.join(file_path);
                // Best-effort cleanup - ignore errors since we're already in error path
                let _ = fs::write(&full_path, content);
            }
            return Err(e);
        }
    };

    // Step 3: Restore original content (user changes will be in remaining change)
    for (file_path, content) in &original_contents {
        let full_path = event.cwd.join(file_path);
        fs::write(&full_path, content).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to restore content to '{}': {}",
                full_path.display(),
                e
            ))
        })?;
    }

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[flows/core] Successfully separated edits: {}",
            split_result
        );
    }

    // Parse jj split output to extract change IDs
    // Output format: "Split into 'AI change' <change_id> and 'User change' <change_id>"
    let (ai_change_id, user_change_id) = parse_split_output(&split_result)?;

    let json = serde_json::json!({
        "ai_change_id": ai_change_id,
        "user_change_id": user_change_id,
        "separated_files": files_with_edits.iter().collect::<Vec<_>>(),
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

/// Normalize a file path for use with jj commands
///
/// JJ commands expect relative paths from the repo root. This function:
/// 1. If path is absolute and starts with cwd, make it relative to cwd
/// 2. If path is already relative, return as-is
/// 3. If path is absolute but not under cwd, return as-is (will likely fail, but let jj handle the error)
fn normalize_path_for_jj(file_path: &str, cwd: &Path) -> String {
    let path = Path::new(file_path);

    // If already relative, return as-is
    if path.is_relative() {
        return file_path.to_string();
    }

    // If absolute, try to make it relative to cwd
    if let Ok(relative) = path.strip_prefix(cwd) {
        relative.to_string_lossy().to_string()
    } else {
        // Path is absolute but not under cwd - return as-is
        file_path.to_string()
    }
}

/// Reconstruct AI-only content by applying AI's edits to the base content
///
/// This is a simplified implementation that assumes:
/// 1. AI's edits were applied sequentially
/// 2. User didn't change the portions AI edited
///
/// For overlapping edits, this may not produce perfect results, but it's
/// better than not separating at all.
fn reconstruct_ai_only_content(
    current_content: &str,
    file_path: &str,
    event: &AikiPostChangeEvent,
) -> Result<String> {
    let mut ai_content = current_content.to_string();

    // Get all edits for this file
    let file_edits: Vec<_> = event
        .edit_details
        .iter()
        .filter(|e| e.file_path == file_path)
        .collect();

    // Apply edits in reverse to reconstruct AI-only state
    // Strategy: For each edit, if new_string is present, we keep it
    // If new_string is NOT present but old_string is, user likely reverted it
    for edit in &file_edits {
        if !edit.new_string.is_empty() && ai_content.contains(&edit.new_string) {
            // AI's change is present - keep it
            continue;
        } else if !edit.old_string.is_empty() && ai_content.contains(&edit.old_string) {
            // Old string still present - apply AI's intended change
            ai_content = ai_content.replace(&edit.old_string, &edit.new_string);
        }
        // If neither is present, user made different changes - we can't reconstruct perfectly
        // In this case, we'll just keep what's there
    }

    Ok(ai_content)
}

/// Run jj split to create AI change and user change
fn run_jj_split(cwd: &Path, message: &str, author: &str, files: &[String]) -> Result<String> {
    // Build jj split command
    // Note: jj split doesn't support --author, we'll set it separately
    let mut cmd = Command::new("jj");
    cmd.current_dir(cwd);
    cmd.arg("split");
    cmd.arg("--message").arg(message);

    // Add file paths
    for file in files {
        cmd.arg(file);
    }

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[flows/core] Running: {:?}", cmd);
    }

    let output = cmd
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to execute jj split: {}", e)))?;

    if !output.status.success() {
        return Err(AikiError::JjCommandFailed(format!(
            "jj split failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let split_output = String::from_utf8_lossy(&output.stdout).to_string();

    // Step 2: Set author on the first part (AI change) using jj metaedit
    // After split, the first part is at @- (parent of working copy)
    let mut metaedit_cmd = Command::new("jj");
    metaedit_cmd.current_dir(cwd);
    metaedit_cmd.arg("metaedit");
    metaedit_cmd.arg("-r").arg("@-");
    metaedit_cmd.arg("--author").arg(author);
    metaedit_cmd.arg("--no-edit"); // Don't open editor

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[flows/core] Setting author on AI change: {:?}",
            metaedit_cmd
        );
    }

    let metaedit_output = metaedit_cmd
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to execute jj metaedit: {}", e)))?;

    if !metaedit_output.status.success() {
        return Err(AikiError::JjCommandFailed(format!(
            "jj metaedit failed: {}",
            String::from_utf8_lossy(&metaedit_output.stderr)
        )));
    }

    Ok(split_output)
}

/// Parse jj split output to extract change IDs
fn parse_split_output(output: &str) -> Result<(String, String)> {
    // jj split output format (example):
    // "First part: qpvuntsm 12345678 AI changes
    //  Second part: rlvkpnrz 87654321 (no description set)"
    //
    // The format is: "First part: <change_hash_short> <change_hash_long> <description>"
    // We want the long hash (position 3 in split_whitespace)

    let lines: Vec<&str> = output.lines().collect();

    let ai_change_id = lines
        .iter()
        .find(|l| l.contains("First part"))
        .and_then(|l| {
            // Extract third word (long change ID hash)
            // Words: [0]="First", [1]="part:", [2]=short_hash, [3]=long_hash
            l.split_whitespace().nth(3).map(String::from)
        })
        .ok_or_else(|| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to parse AI change ID from jj split output"
            ))
        })?;

    let user_change_id = lines
        .iter()
        .find(|l| l.contains("Second part"))
        .and_then(|l| {
            // Extract third word (long change ID hash)
            l.split_whitespace().nth(3).map(String::from)
        })
        .ok_or_else(|| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to parse user change ID from jj split output"
            ))
        })?;

    Ok((ai_change_id, user_change_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstruct_ai_only_simple() {
        use crate::events::EditDetail;
        use crate::provenance::AgentType;

        let current_content = "Hello World\nExtra user line";
        let event = AikiPostChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["test.txt".to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![EditDetail::new("test.txt", "", "Hello World")],
        };

        let ai_only = reconstruct_ai_only_content(current_content, "test.txt", &event).unwrap();

        // AI's edit is present, so content should be unchanged for reconstruction purposes
        assert!(ai_only.contains("Hello World"));
    }

    #[test]
    fn test_reconstruct_ai_only_revert() {
        use crate::events::EditDetail;
        use crate::provenance::AgentType;

        let current_content = "Hello"; // User reverted AI's change
        let event = AikiPostChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["test.txt".to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![EditDetail::new("test.txt", "Hello", "Hello World")],
        };

        let ai_only = reconstruct_ai_only_content(current_content, "test.txt", &event).unwrap();

        // Should reconstruct AI's intended change
        assert_eq!(ai_only, "Hello World");
    }

    #[test]
    fn test_parse_split_output() {
        let output = "First part: qpvuntsm 12345678 AI changes\nSecond part: rlvkpnrz 87654321 (no description set)\n";

        let (ai_id, user_id) = parse_split_output(output).unwrap();

        assert_eq!(ai_id, "12345678");
        assert_eq!(user_id, "87654321");
    }
}

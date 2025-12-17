//! Built-in functions for the aiki/core flow namespace
//!
//! This module contains native Rust implementations of functions that can be called
//! from flow definitions using the function call syntax.
//!
//! # Functions
//!
//! ## Metadata Generation
//! - [`build_metadata`] - Build complete provenance metadata (message + author) from event context
//! - [`build_user_metadata`] - Build metadata for human changes during an AI edit session
//!
//! ## Edit Analysis
//! - [`classify_edits`] - Classify edits to detect user modifications
//!
//! ## Edit Separation
//! - [`prepare_separation`] - Prepare files for separation by reconstructing AI-only content
//! - [`write_ai_files`] - Write AI-only content to working copy
//! - [`restore_original_files`] - Restore original content after jj split
//! - [`separate_edits`] - Separate AI changes from user edits using jj split
//!
//! ## Git Integration
//! - [`generate_coauthors`] - Generate co-authors for Git commit from staged changes
//!
//! ## Utility Functions
//! - [`get_git_user`] - Get the git user (name + email) from git config

use crate::authors::{AuthorScope, AuthorsCommand, OutputFormat};
use crate::cache::debug_log;
use crate::error::{AikiError, Result};
use crate::events::{
    AikiCommitMessageStartedPayload, AikiWriteCompletedPayload, AikiWritePermissionAskedPayload,
};
use crate::flows::state::ActionResult;
use crate::provenance::ProvenanceRecord;
use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

// =============================================================================
// Utility Functions
// =============================================================================

/// Get the git user (name + email) from git config
///
/// Returns the user in "Name <email>" format, or None if git config is not set.
pub fn get_git_user() -> Option<String> {
    let name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())?;

    let email = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())?;

    Some(format!("{} <{}>", name, email))
}

/// Get git user as a flow-callable function
///
/// Returns the git user in "Name <email>" format from git config.
///
/// # Returns
/// An ActionResult with the git user in stdout, or an error if git is not configured.
///
/// # Example Flow Usage
/// ```yaml
/// - with_author: self.get_git_user
///   jj: describe --message "User changes"
/// ```
pub fn get_git_user_function(
    _event: &AikiWriteCompletedPayload,
    _context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    let git_user = get_git_user().ok_or_else(|| {
        AikiError::Other(anyhow::anyhow!(
            "Git user not configured. Run 'git config user.name' and 'git config user.email'"
        ))
    })?;

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: git_user,
        stderr: String::new(),
    })
}

// =============================================================================
// Metadata Generation Functions
// =============================================================================

/// Build complete metadata (message + author) from event context
///
/// This function returns both the commit message and author in a single call,
/// avoiding duplicate event field access. The output is JSON for easy parsing
/// with native field access syntax.
///
/// # Context-Aware Behavior
/// If the flow context contains `$detection.classification_type == "OverlappingUserEdits"`,
/// this function will add a coauthor field to the provenance record using the git user.
///
/// # Required Event Variables
/// - `$event.agent_type` - Agent type
/// - `$event.session_id` - Session identifier
/// - `$event.tool_name` - Tool name
///
/// # Optional Context Variables
/// - `$detection.classification_type` - Edit classification type
///
/// # Returns
/// An ActionResult with JSON output: `{"author": "...", "message": "..."}`
///
/// # Example Flow Usage
/// ```yaml
/// change.done:
///   - let: detection = self.classify_edits
///   - let: metadata = self.build_metadata
///     on_failure: stop
///   - jj: metaedit -m "$metadata.message" --author "$metadata.author"
/// ```
pub fn build_metadata(
    event: &AikiWriteCompletedPayload,
    context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    let mut provenance = ProvenanceRecord::from_write_completed_event(event);

    // Check if we have overlapping user edits and should add coauthor
    if let Some(ctx) = context {
        if let Some(detection) = ctx.get_variable("detection") {
            // Try to parse the detection JSON to check classification_type
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(detection) {
                if let Some(classification_type) =
                    json.get("classification_type").and_then(|v| v.as_str())
                {
                    if classification_type == "OverlappingUserEdits" {
                        // Add coauthor from git config
                        provenance.coauthor = get_git_user();
                    }
                }
            }
        }
    }

    let message = provenance.to_description();
    let author = event.session.agent_type().git_author();

    debug_log(|| {
        format!(
            "[flows/core] Generated metadata - author: {}, message length: {}",
            author,
            message.len()
        )
    });

    // Return JSON output for structured data
    let json = serde_json::json!({
        "author": author,
        "message": message,
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

/// Build metadata for human changes during an AI edit session
///
/// Creates an `[aiki]` block where the author is the git user (human)
/// in "Name <email>" format. This is used when we detect user modifications
/// during an AI editing session (AdditiveUserEdits case).
///
/// # Required Event Variables
/// - `$event.session_id` - Session identifier
///
/// # Returns
/// An ActionResult with JSON output: `{"author": "...", "message": "..."}`
///
/// # Example Flow Usage
/// ```yaml
/// change.done:
///   - if: $detection.classification_type == "AdditiveUserEdits"
///     then:
///       - let: user_metadata = self.build_user_metadata
///       - jj: metaedit --message "$user_metadata.message" --author "$user_metadata.author"
/// ```
/// Build metadata for human changes (works with both change.permission_asked and change.done)
///
/// Creates an [aiki] metadata block with the git user as author and author_type=human.
/// This is used for any changes made by the human user (before or during AI edits).
///
/// # Returns
/// JSON with author and message fields:
/// ```json
/// {
///   "author": "Name <email>",
///   "message": "[aiki]\nauthor=Name <email>\nauthor_type=human\nsession=session-id\n[/aiki]"
/// }
/// ```
fn build_human_metadata_impl(session: &crate::session::AikiSession) -> Result<ActionResult> {
    let git_user = get_git_user().ok_or_else(|| {
        crate::error::AikiError::Other(anyhow::anyhow!(
            "Git user not configured. Run 'git config user.name' and 'git config user.email'"
        ))
    })?;

    // Create a metadata block with the git user as author (no coauthor)
    let lines = vec![
        "[aiki]".to_string(),
        format!("author={}", git_user),
        "author_type=human".to_string(),
        format!("session={}", session.external_id()),
    ];

    let message = format!("{}\n[/aiki]", lines.join("\n"));

    debug_log(|| {
        format!(
            "[flows/core] Generated human metadata - author: {}",
            git_user
        )
    });

    let json = serde_json::json!({
        "author": git_user,
        "message": message,
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

/// Build human metadata - works with change.permission_asked events
pub fn build_human_metadata(
    event: &AikiWritePermissionAskedPayload,
    _context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    build_human_metadata_impl(&event.session)
}

/// Build human metadata - works with change.done events
pub fn build_human_metadata_post(
    event: &AikiWriteCompletedPayload,
    _context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    build_human_metadata_impl(&event.session)
}

// =============================================================================
// Edit Analysis Functions
// =============================================================================

/// Classification result for edit detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditClassification {
    /// All AI edits found exactly in file (no user modifications)
    ExactMatch,
    /// User added more changes to same file (AI edits present + extra changes)
    AdditiveUserEdits,
    /// User modified AI's changes (AI edits not fully present or conflicting)
    OverlappingUserEdits,
}

/// Classify edits to detect user modifications
///
/// Compares the AI's intended edits (from edit_details) with the actual file
/// state to determine if the user made additional or conflicting changes.
///
/// # Algorithm
/// For each file with edit details:
/// 1. Read current file content
/// 2. For each edit: check if new_string is present in current content
/// 3. Classify based on presence:
///    - All new_strings present → ExactMatch
///    - Some new_strings present, old_strings absent → AdditiveUserEdits
///    - Missing new_strings or lingering old_strings → OverlappingEdits
///
/// Files without edit details are treated as "unknown" (assumed AI-only for now).
/// Extra files (modified but not in edit_details) are flagged separately.
///
/// # Returns
/// JSON with classification results:
/// ```json
/// {
///   "all_exact_match": true/false,
///   "has_additive_edits": true/false,
///   "has_overlapping_edits": true/false,
///   "extra_files": ["file1.rs", "file2.rs"],
///   "details": {
///     "file.rs": "ExactMatch" | "AdditiveUserEdits" | "OverlappingEdits"
///   }
/// }
/// ```
///
/// # Example Flow Usage
/// ```yaml
/// change.done:
///   - let: detection = self.classify_edits
///     on_failure: continue
///   - if: $detection.all_exact_match == true
///     then:
///       - jj: metaedit -m "$metadata.message"
/// ```
pub fn classify_edits(event: &AikiWriteCompletedPayload) -> Result<ActionResult> {
    // If no edit details, we can't classify - treat as exact match (AI-only)
    if event.edit_details.is_empty() {
        debug_log(|| "[flows/core] No edit details available - assuming AI-only changes");

        return Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: "ExactMatch".to_string(),
            stderr: String::new(),
        });
    }

    // Group edit details by file
    let mut files_with_edits: HashSet<String> = HashSet::new();
    let mut details = serde_json::Map::new();

    for edit_detail in &event.edit_details {
        files_with_edits.insert(edit_detail.file_path.clone());
    }

    // Classify each file
    let mut all_exact = true;
    let mut has_additive = false;
    let mut has_overlapping = false;

    for file_path in &files_with_edits {
        let classification = classify_file(file_path, event)?;

        let class_str = match classification {
            EditClassification::ExactMatch => "ExactMatch",
            EditClassification::AdditiveUserEdits => {
                has_additive = true;
                all_exact = false;
                "AdditiveUserEdits"
            }
            EditClassification::OverlappingUserEdits => {
                has_overlapping = true;
                all_exact = false;
                "OverlappingUserEdits"
            }
        };

        details.insert(file_path.clone(), serde_json::json!(class_str));
    }

    // Check for extra files (in file_paths but not in edit_details)
    let extra_files: Vec<String> = event
        .file_paths
        .iter()
        .filter(|p| !files_with_edits.contains(*p))
        .cloned()
        .collect();

    if !extra_files.is_empty() {
        all_exact = false;
    }

    // Determine overall classification type
    let classification_type = if has_overlapping {
        "OverlappingUserEdits"
    } else if has_additive || !extra_files.is_empty() {
        "AdditiveUserEdits"
    } else if all_exact {
        "ExactMatch"
    } else {
        "Unknown"
    };

    debug_log(|| {
        format!(
            "[flows/core] Classification: type={}, extra_files={}",
            classification_type,
            extra_files.len()
        )
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: classification_type.to_string(),
        stderr: String::new(),
    })
}

/// Classify edits for a single file
fn classify_file(file_path: &str, event: &AikiWriteCompletedPayload) -> Result<EditClassification> {
    // Read current file content
    let full_path = event.cwd.join(file_path);
    let current_content = read_file_safe(&full_path)?;

    // Get all edits for this file
    let file_edits: Vec<_> = event
        .edit_details
        .iter()
        .filter(|e| e.file_path == file_path)
        .collect();

    if file_edits.is_empty() {
        // No edits for this file - shouldn't happen, but treat as exact match
        return Ok(EditClassification::ExactMatch);
    }

    // Check each edit
    let mut all_new_present = true;
    let mut any_old_present = false;

    for edit in &file_edits {
        // Check if new_string is present in current content
        if !edit.new_string.is_empty() && !current_content.contains(&edit.new_string) {
            all_new_present = false;
        }

        // Check if old_string is still present independently (not as substring of new_string)
        // Strategy: If the edit was applied, old_string should have been replaced by new_string
        if !edit.old_string.is_empty() {
            let old_present = current_content.contains(&edit.old_string);
            let new_present = current_content.contains(&edit.new_string);

            if old_present && new_present {
                // Both present - check if old is a substring of new
                // If old is substring of new, it's not really "still present" - it was replaced
                if !edit.new_string.contains(&edit.old_string) {
                    // Old and new are both present but don't overlap - problematic
                    any_old_present = true;
                }
                // If old is a substring of new, consider it as "replaced" (not still present)
            } else if old_present {
                // Only old present (new not present) - edit wasn't applied
                any_old_present = true;
            }
        }
    }

    // Classify based on presence checks
    if all_new_present && !any_old_present {
        // All new strings present, old strings gone → ExactMatch
        Ok(EditClassification::ExactMatch)
    } else if any_old_present {
        // Old strings still present → OverlappingUserEdits (edit wasn't fully applied)
        Ok(EditClassification::OverlappingUserEdits)
    } else {
        // New strings missing but old strings also missing → AdditiveUserEdits
        // (user likely modified AI's changes)
        Ok(EditClassification::AdditiveUserEdits)
    }
}

/// Read file content safely, handling missing files
fn read_file_safe(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| {
        AikiError::Other(anyhow::anyhow!(
            "Failed to read file '{}': {}",
            path.display(),
            e
        ))
    })
}

// =============================================================================
// Edit Separation Functions
// =============================================================================

/// Prepare files for separation by reconstructing AI-only content
///
/// This function analyzes the files that need separation and reconstructs what
/// the AI-only version should look like. It returns all the information needed
/// for the separation workflow.
///
/// # Returns
/// JSON with preparation data:
/// ```json
/// {
///   "ai_message": "[aiki]\nagent=claude\n...",
///   "ai_author": "Claude <noreply@anthropic.com>",
///   "file_list": "file1.rs file2.rs",
///   "files": {
///     "file1.rs": {
///       "ai_only_content": "...",
///       "original_content": "..."
///     }
///   }
/// }
/// ```
///
/// # Example Flow Usage
/// ```yaml
/// change.done:
///   - let: detection = self.classify_edits
///   - switch: $detection.classification_type
///     cases:
///       AdditiveUserEdits:
///         - let: prep = self.prepare_separation
///         - self: write_ai_files
///         - jj: split --message "$prep.ai_message" $prep.file_list
///         - jj: metaedit -r @- --author "$prep.ai_author" --no-edit
///         - self: restore_original_files
/// ```
pub fn prepare_separation(event: &AikiWriteCompletedPayload) -> Result<ActionResult> {
    // If no edit details, return early
    if event.edit_details.is_empty() {
        debug_log(|| "[flows/core] No edit details available, skipping preparation");
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
    let provenance = ProvenanceRecord::from_write_completed_event(event);
    let ai_message = provenance.to_description();
    let ai_author = event.session.agent_type().git_author();

    // Files that will be separated (have edit details)
    let files_with_edits: HashSet<String> = event
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

    debug_log(|| {
        format!(
            "[flows/core] Preparing separation for {} files with edits, {} AI-only files",
            files_with_edits.len(),
            ai_only_files.len()
        )
    });

    // Reconstruct AI-only content for files with edits
    let mut files_data = serde_json::Map::new();

    for file_path in &files_with_edits {
        let full_path = event.cwd.join(file_path);

        // Read current content (AI + user changes)
        let current_content = fs::read_to_string(&full_path).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to read file '{}': {}",
                full_path.display(),
                e
            ))
        })?;

        // Reconstruct AI-only content
        let ai_only_content = reconstruct_ai_only_content(&current_content, file_path, event)?;

        files_data.insert(
            file_path.clone(),
            serde_json::json!({
                "ai_only_content": ai_only_content,
                "original_content": current_content,
            }),
        );
    }

    // Build file list for jj split (space-separated, normalized paths)
    let mut all_ai_files: Vec<String> = files_with_edits
        .iter()
        .map(|p| normalize_path_for_jj(p, &event.cwd))
        .collect();
    all_ai_files.extend(
        ai_only_files
            .iter()
            .map(|p| normalize_path_for_jj(p, &event.cwd)),
    );
    let file_list = all_ai_files.join(" ");

    let json = serde_json::json!({
        "ai_message": ai_message,
        "ai_author": ai_author,
        "file_list": file_list,
        "files": files_data,
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

/// Write AI-only content to files in preparation for jj split
///
/// This function reads the preparation data from context (the $prep variable
/// set by prepare_separation) and writes the AI-only content to the working copy.
///
/// After this step, the flow should run `jj split` to separate the AI changes.
///
/// # Context Variables Required
/// - `$prep.files` - File data from prepare_separation
///
/// # Example Flow Usage
/// ```yaml
/// - let: prep = self.prepare_separation
/// - self: write_ai_files
/// - jj: split --message "$prep.ai_message" $prep.file_list
/// ```
pub fn write_ai_files(
    event: &AikiWriteCompletedPayload,
    context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    let ctx = context.ok_or_else(|| {
        AikiError::Other(anyhow::anyhow!(
            "write_ai_files requires context with $prep variable"
        ))
    })?;

    let prep = ctx.get_variable("prep").ok_or_else(|| {
        AikiError::Other(anyhow::anyhow!(
            "write_ai_files requires $prep variable from prepare_separation"
        ))
    })?;

    // Parse the prep JSON
    let prep_json: serde_json::Value = serde_json::from_str(prep)
        .map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to parse $prep variable: {}", e)))?;

    let files = prep_json
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Missing 'files' in $prep")))?;

    // Write AI-only content to each file
    for (file_path, file_data) in files {
        let ai_only_content = file_data
            .get("ai_only_content")
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                AikiError::Other(anyhow::anyhow!(
                    "Missing 'ai_only_content' for file '{}'",
                    file_path
                ))
            })?;

        let full_path = event.cwd.join(file_path);
        fs::write(&full_path, ai_only_content).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to write AI-only content to '{}': {}",
                full_path.display(),
                e
            ))
        })?;

        debug_log(|| {
            format!(
                "[flows/core] Wrote AI-only content to '{}'",
                full_path.display()
            )
        });
    }

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "AI-only content written".to_string(),
        stderr: String::new(),
    })
}

/// Restore original content after jj split
///
/// This function reads the preparation data from context and restores the
/// original file content (AI + user changes) to the working copy. This should
/// be called after `jj split` to ensure the user's changes end up in the
/// remaining change.
///
/// # Context Variables Required
/// - `$prep.files` - File data from prepare_separation
///
/// # Example Flow Usage
/// ```yaml
/// - jj: split --message "$prep.ai_message" $prep.file_list
/// - jj: metaedit -r @- --author "$prep.ai_author" --no-edit
/// - self: restore_original_files
/// ```
pub fn restore_original_files(
    event: &AikiWriteCompletedPayload,
    context: Option<&crate::flows::state::AikiState>,
) -> Result<ActionResult> {
    let ctx = context.ok_or_else(|| {
        AikiError::Other(anyhow::anyhow!(
            "restore_original_files requires context with $prep variable"
        ))
    })?;

    let prep = ctx.get_variable("prep").ok_or_else(|| {
        AikiError::Other(anyhow::anyhow!(
            "restore_original_files requires $prep variable from prepare_separation"
        ))
    })?;

    // Parse the prep JSON
    let prep_json: serde_json::Value = serde_json::from_str(prep)
        .map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to parse $prep variable: {}", e)))?;

    let files = prep_json
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Missing 'files' in $prep")))?;

    // Restore original content to each file
    for (file_path, file_data) in files {
        let original_content = file_data
            .get("original_content")
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                AikiError::Other(anyhow::anyhow!(
                    "Missing 'original_content' for file '{}'",
                    file_path
                ))
            })?;

        let full_path = event.cwd.join(file_path);
        fs::write(&full_path, original_content).map_err(|e| {
            AikiError::Other(anyhow::anyhow!(
                "Failed to restore content to '{}': {}",
                full_path.display(),
                e
            ))
        })?;

        debug_log(|| {
            format!(
                "[flows/core] Restored original content to '{}'",
                full_path.display()
            )
        });
    }

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "Original content restored".to_string(),
        stderr: String::new(),
    })
}

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
/// change.done:
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
pub fn separate_edits(event: &AikiWriteCompletedPayload) -> Result<ActionResult> {
    // If no edit details, return success (nothing to separate)
    // This allows graceful degradation for hook-based detection where
    // edit details might not be available
    if event.edit_details.is_empty() {
        debug_log(|| "[flows/core] No edit details available, skipping separation");
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
    let provenance = ProvenanceRecord::from_write_completed_event(event);
    let ai_message = provenance.to_description();
    let ai_author = event.session.agent_type().git_author();

    // Files that will be separated (have edit details)
    let files_with_edits: HashSet<String> = event
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

    debug_log(|| {
        format!(
            "[flows/core] Separating {} files with edits, {} AI-only files",
            files_with_edits.len(),
            ai_only_files.len()
        )
    });

    // Step 1: Reconstruct AI-only content for files with edits
    let mut original_contents: HashMap<String, String> = HashMap::new();

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
            debug_log(|| "[flows/core] jj split failed, restoring original content");
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

    debug_log(|| {
        format!(
            "[flows/core] Successfully separated edits: {}",
            split_result
        )
    });

    // Parse jj split output to extract change IDs
    // Output format: "First part: qpvuntsm 12345678 AI changes
    //  Second part: rlvkpnrz 87654321 (no description set)"
    //
    // The format is: "First part: <change_hash_short> <change_hash_long> <description>"
    // We want the long hash (position 3 in split_whitespace)
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
    event: &AikiWriteCompletedPayload,
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

    debug_log(|| format!("[flows/core] Running: {:?}", cmd));

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

    debug_log(|| {
        format!(
            "[flows/core] Setting author on AI change: {:?}",
            metaedit_cmd
        )
    });

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

// =============================================================================
// Git Integration Functions
// =============================================================================

/// Generate co-authors for Git commit from staged changes
///
/// This function is called during commit.message_started events to generate Git trailer
/// lines (Co-authored-by:) for AI agents that contributed to the staged changes.
pub fn generate_coauthors(event: &AikiCommitMessageStartedPayload) -> Result<ActionResult> {
    // Create authors command using the event's working directory
    let authors_cmd = AuthorsCommand::new(&event.cwd);

    // Get authors from Git staged changes in Git trailer format
    let coauthors = authors_cmd
        .get_authors(AuthorScope::GitStaged, OutputFormat::Git)
        .context("Failed to get co-authors from staged changes")?;

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: coauthors,
        stderr: String::new(),
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EditDetail;
    use crate::provenance::AgentType;
    use crate::session::AikiSession;
    use tempfile::TempDir;

    // =========================================================================
    // Metadata Generation Tests
    // =========================================================================

    #[test]
    fn test_build_metadata_with_claude() {
        let session = AikiSession::new(
            AgentType::Claude,
            "test-session-123".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        let event = AikiWriteCompletedPayload {
            session,
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/tmp/file.rs".to_string()],
            success: true,
            edit_details: vec![],
        };

        let result = build_metadata(&event, None).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));

        // Parse JSON output
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Claude <noreply@anthropic.com>");
        assert!(json["message"].as_str().unwrap().contains("[aiki]"));
        assert!(json["message"].as_str().unwrap().contains("author=claude"));
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("session=test-session-123"));
    }

    #[test]
    fn test_build_metadata_with_cursor() {
        let session = AikiSession::new(
            AgentType::Cursor,
            "cursor-session".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        let event = AikiWriteCompletedPayload {
            session,
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/tmp/file.rs".to_string()],
            success: true,
            edit_details: vec![],
        };

        let result = build_metadata(&event, None).unwrap();

        assert!(result.success);
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["author"], "Cursor <noreply@cursor.com>");
        assert!(json["message"].as_str().unwrap().contains("author=cursor"));
    }

    // =========================================================================
    // Edit Classification Tests
    // =========================================================================

    fn create_test_event(
        cwd: &Path,
        file_paths: Vec<String>,
        edit_details: Vec<EditDetail>,
    ) -> AikiWriteCompletedPayload {
        let session = AikiSession::new(
            AgentType::Claude,
            "test".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        AikiWriteCompletedPayload {
            session,
            cwd: cwd.to_path_buf(),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths,
            success: true,
            edit_details,
        }
    }

    #[test]
    fn test_classify_exact_match() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file with AI's changes applied
        fs::write(&file_path, "Hello World").unwrap();

        let event = create_test_event(
            temp_dir.path(),
            vec!["test.txt".to_string()],
            vec![EditDetail::new("test.txt", "Hello", "Hello World")],
        );

        let result = classify_edits(&event).unwrap();
        assert_eq!(result.stdout, "ExactMatch");
    }

    #[test]
    fn test_classify_additive() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // User added extra content after AI's edit
        fs::write(&file_path, "Hello World\nExtra line by user").unwrap();

        let event = create_test_event(
            temp_dir.path(),
            vec!["test.txt".to_string()],
            vec![EditDetail::new("test.txt", "", "Hello World")],
        );

        let result = classify_edits(&event).unwrap();
        // AI's edit is present but file has extra content → ExactMatch (just AI edits applied)
        assert_eq!(result.stdout, "ExactMatch");
    }

    #[test]
    fn test_classify_overlapping() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Old string still present (edit not applied)
        fs::write(&file_path, "Hello").unwrap();

        let event = create_test_event(
            temp_dir.path(),
            vec!["test.txt".to_string()],
            vec![EditDetail::new("test.txt", "Hello", "Hello World")],
        );

        let result = classify_edits(&event).unwrap();
        assert_eq!(result.stdout, "OverlappingUserEdits");
    }

    #[test]
    fn test_classify_extra_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("test1.txt");
        let file2 = temp_dir.path().join("test2.txt");

        fs::write(&file1, "Content 1").unwrap();
        fs::write(&file2, "Content 2").unwrap();

        let event = create_test_event(
            temp_dir.path(),
            vec!["test1.txt".to_string(), "test2.txt".to_string()],
            vec![EditDetail::new("test1.txt", "", "Content 1")],
        );

        let result = classify_edits(&event).unwrap();
        // Extra files detected → AdditiveUserEdits
        assert_eq!(result.stdout, "AdditiveUserEdits");
    }

    #[test]
    fn test_classify_no_edit_details() {
        let temp_dir = TempDir::new().unwrap();

        let event = create_test_event(temp_dir.path(), vec!["test.txt".to_string()], vec![]);

        let result = classify_edits(&event).unwrap();
        // No edit details → assume AI-only
        assert_eq!(result.stdout, "ExactMatch");
    }

    // =========================================================================
    // Edit Separation Tests
    // =========================================================================

    #[test]
    fn test_reconstruct_ai_only_simple() {
        let current_content = "Hello World\nExtra user line";
        let session = AikiSession::new(
            AgentType::Claude,
            "test".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        let event = AikiWriteCompletedPayload {
            session,
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["test.txt".to_string()],
            success: true,
            edit_details: vec![EditDetail::new("test.txt", "", "Hello World")],
        };

        let ai_only = reconstruct_ai_only_content(current_content, "test.txt", &event).unwrap();

        // AI's edit is present, so content should be unchanged for reconstruction purposes
        assert!(ai_only.contains("Hello World"));
    }

    #[test]
    fn test_reconstruct_ai_only_revert() {
        let current_content = "Hello"; // User reverted AI's change
        let session = AikiSession::new(
            AgentType::Claude,
            "test".to_string(),
            None::<&str>,
            crate::provenance::DetectionMethod::Hook,
        );
        let event = AikiWriteCompletedPayload {
            session,
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["test.txt".to_string()],
            success: true,
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

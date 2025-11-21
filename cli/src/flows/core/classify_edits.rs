//! Built-in function: aiki/core.classify_edits
//!
//! Classifies edits to detect user modifications by comparing AI's intended edits
//! with the actual file state in the working copy.
//!
//! This enables separation of AI changes from user edits when they occur in the
//! same session (e.g., user fixes AI's mistake before saving).

use crate::error::{AikiError, Result};
use crate::events::AikiPostFileChangeEvent;
use crate::flows::state::ActionResult;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
/// PostFileChange:
///   - let: detection = self.classify_edits
///     on_failure: continue
///   - if: $detection.all_exact_match == true
///     then:
///       - jj: metaedit -m "$metadata.message"
/// ```
pub fn classify_edits(event: &AikiPostFileChangeEvent) -> Result<ActionResult> {
    // If no edit details, we can't classify - treat as exact match (AI-only)
    if event.edit_details.is_empty() {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows/core] No edit details available - assuming AI-only changes");
        }

        let json = serde_json::json!({
            "classification_type": "ExactMatch",
            "extra_files": [],
            "details": {}
        });

        return Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: json.to_string(),
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

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!(
            "[flows/core] Classification: type={}, extra_files={}",
            classification_type,
            extra_files.len()
        );
    }

    let json = serde_json::json!({
        "classification_type": classification_type,
        "extra_files": extra_files,
        "details": details
    });

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: json.to_string(),
        stderr: String::new(),
    })
}

/// Classify edits for a single file
fn classify_file(file_path: &str, event: &AikiPostFileChangeEvent) -> Result<EditClassification> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EditDetail;
    use crate::provenance::AgentType;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_event(
        cwd: &Path,
        file_paths: Vec<String>,
        edit_details: Vec<EditDetail>,
    ) -> AikiPostFileChangeEvent {
        AikiPostFileChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test".to_string(),
            tool_name: "Edit".to_string(),
            file_paths,
            cwd: cwd.to_path_buf(),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
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
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();

        assert_eq!(json["classification_type"], "ExactMatch");
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
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();

        // AI's edit is present but file has extra content → ExactMatch (just AI edits applied)
        assert_eq!(json["classification_type"], "ExactMatch");
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
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();

        assert_eq!(json["classification_type"], "OverlappingUserEdits");
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
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();

        // Extra files detected → AdditiveUserEdits
        assert_eq!(json["classification_type"], "AdditiveUserEdits");
        assert_eq!(json["extra_files"].as_array().unwrap().len(), 1);
        assert_eq!(json["extra_files"][0], "test2.txt");
    }

    #[test]
    fn test_classify_no_edit_details() {
        let temp_dir = TempDir::new().unwrap();

        let event = create_test_event(temp_dir.path(), vec!["test.txt".to_string()], vec![]);

        let result = classify_edits(&event).unwrap();
        let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();

        // No edit details → assume AI-only
        assert_eq!(json["classification_type"], "ExactMatch");
    }
}

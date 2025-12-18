# User Edit Detection & Separation

## Problem Statement

When an AI agent session starts or during AI operations, users may manually edit files. Currently, these user edits can be incorrectly attributed to the AI agent, leading to:

1. **False attribution**: User's code changes are marked as AI-generated
2. **Provenance corruption**: Blame shows wrong agent for user's work
3. **Trust issues**: Users can't rely on attribution accuracy

### Scenarios

**Scenario 1: Edits between SessionStart and first PostChange**
```
1. SessionStart → jj new (clean working copy)
2. User manually edits file.txt
3. AI makes first edit to main.rs
4. PostChange fires → attributes BOTH edits to AI
```

**Scenario 2: Concurrent edits to different files**
```
1. AI starts editing main.rs
2. User manually edits utils.rs (in parallel)
3. PostChange fires → both files in working copy
4. Both attributed to AI
```

**Scenario 3: Concurrent edits to same file**
```
1. AI edits line 50 of file.txt
2. User edits line 10 of file.txt (concurrently)
3. PostChange fires → entire file diff attributed to AI
```

## Current State

### What We Have

All three integrations provide edit information:

| Integration | File Paths | Operation Type | Edit Details |
|-------------|-----------|----------------|--------------|
| **ACP** | ✅ Multiple | ✅ ToolKind | ✅ ToolCallContent diffs (old/new text) |
| **Claude Code** | ✅ Single | ✅ tool_name | ✅ old_string/new_string |
| **Cursor** | ✅ Single | ✅ Implicit | ✅ edits[] array (old_string/new_string) |

### What We're Missing

1. **Event structure doesn't capture edit details** - We only store file paths, not the actual edits
2. **No comparison logic** - We don't compare expected AI edits vs actual working copy changes
3. **No separation mechanism** - We don't split user edits into separate changes

## Solution Design

### Architecture Overview

This phase uses a **transformation-based approach** that leverages AI's intent to detect mixed edits:

1. **Capture**: Store AI's transformation (`old_string → new_string`) in events
2. **Classify**: Check if AI's transformation is present, use JJ parent to detect extra changes
3. **Separate**: Single universal separation function using JJ's native diff

**Key insight**: By storing what the AI *intended* to change (search/replace strings), we can:
- Verify if the AI's transformation was applied
- Use JJ parent state to reconstruct what the file *should* look like
- Compare with actual state to detect user additions/modifications

**Trade-offs**:
- **ACP**: Fast (1-2ms) - compares full file content directly
- **Claude/Cursor**: Slower (11-52ms) - needs JJ parent read to reconstruct AI-only result
- Accepts `.replace()` fragility for substring edits (~5-10% edge cases)

### Phase 1: Capture Edit Details in Events

**Goal**: Store expected AI edits in the event so we can compare later.

#### 1.1 Extend Event Structure

```rust
// cli/src/events.rs

pub struct AikiPostChangeEvent {
    // Existing fields
    pub agent_type: AgentType,
    pub session_id: String,
    pub tool_name: String,
    pub file_paths: Vec<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub detection_method: DetectionMethod,
    
    // NEW: Edit details for comparison
    pub edit_details: Option<Vec<EditDetail>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditDetail {
    pub file_path: String,
    pub old_string: String,  // What AI intended to replace (search string)
    pub new_string: String,  // What AI intended as replacement
}

impl EditDetail {
    /// Create EditDetail from transformation intent
    #[must_use]
    pub fn new(
        file_path: impl Into<String>, 
        old_string: impl Into<String>,
        new_string: impl Into<String>
    ) -> Self {
        Self {
            file_path: file_path.into(),
            old_string: old_string.into(),
            new_string: new_string.into(),
        }
    }
}
```

**Why this structure?**
- **Captures AI's intent**: The transformation AI intended (`old_string → new_string`)
- **Enables attribution checking**: Can verify if AI's change is present in actual file
- **Supports reconstruction**: Combined with JJ parent, can compute AI-only result
- **Unified structure across integrations**: All three provide `old`/`new` pairs, though semantics differ:
  - **ACP**: `old_string` = complete old file content, `new_string` = complete new file content
  - **Claude Code**: `old_string` = substring to find, `new_string` = substring to replace with
  - **Cursor**: `old_string` = substring to find, `new_string` = substring to replace with

#### 1.2 Populate from ACP

```rust
// cli/src/commands/acp.rs
fn process_tool_call(..., tool_call: &ToolCall) -> Result<()> {
    // ... existing code ...
    
    // NEW: Extract edit details from tool call content
    let edit_details = if let Some(content) = &tool_call.fields.content {
        extract_edit_details(content, &context.paths)
    } else {
        None
    };
    
    // Pass to record_post_change_events
}

fn extract_edit_details(
    content: &ToolCallContent, 
    paths: &[PathBuf]
) -> Option<Vec<EditDetail>> {
    match content {
        ToolCallContent::Diff { old_text, new_text } => {
            // ACP provides full old/new content (not substrings)
            Some(paths.iter().map(|path| {
                EditDetail::new(
                    path.to_string_lossy().to_string(),
                    old_text.clone(),
                    new_text.clone()
                )
            }).collect())
        }
        _ => None,
    }
}
```

**Note**: ACP provides complete file content in `old_text`/`new_text`, while Claude Code and Cursor provide search/replace substrings. The `EditDetail` structure handles both cases.

#### 1.3 Populate from Claude Code Hook

```rust
// cli/src/vendors/claude_code.rs
"PostToolUse" => {
    let tool_input = payload.tool_input
        .ok_or_else(|| anyhow::anyhow!("PostToolUse requires tool_input"))?;
    
    // NEW: Extract AI's transformation from payload
    let edit_details = match payload.tool_name.as_str() {
        "Edit" => {
            // Edit tool provides old_string → new_string transformation
            if !tool_input.old_string.is_empty() || !tool_input.new_string.is_empty() {
                Some(vec![EditDetail::new(
                    tool_input.file_path.clone(),
                    tool_input.old_string.clone(),
                    tool_input.new_string.clone(),
                )])
            } else {
                None
            }
        }
        "Write" => {
            // Write tool creates entire file - treat as replacing empty with content
            if let Some(content) = &tool_input.content {
                Some(vec![EditDetail::new(
                    tool_input.file_path.clone(),
                    String::new(),  // old_string is empty (new file)
                    content.clone(),
                )])
            } else {
                None
            }
        }
        _ => None,  // Other tools (Bash, etc.) don't need edit tracking
    };
    
    AikiEvent::PostChange(AikiPostChangeEvent {
        // ... existing fields ...
        edit_details,
    })
}
```

**Why use payload**: Claude Code provides `old_string`/`new_string` for the Edit tool and `content` for the Write tool. These represent the AI's intended transformation, which we'll use to determine if user edits are mixed in.

#### 1.4 Populate from Cursor Hook

```rust
// cli/src/vendors/cursor.rs
"afterFileEdit" => {
    let file_path = if !payload.file_path.is_empty() {
        payload.file_path.clone()
    } else {
        payload.edited_file.clone()
    };
    
    // NEW: Extract transformations from Cursor's edits array
    let edit_details = if !payload.edits.is_empty() {
        Some(
            payload.edits.iter().map(|edit| EditDetail::new(
                file_path.clone(),
                edit.old_string.clone(),
                edit.new_string.clone(),
            )).collect()
        )
    } else {
        None
    };
    
    AikiEvent::PostChange(AikiPostChangeEvent {
        // ... existing fields ...
        edit_details,
    })
}
```

**Why use edits array**: Cursor provides `old_string`/`new_string` pairs for each transformation, just like Claude Code. We store these to detect if user edits are mixed with AI edits. Note that Cursor may send **multiple edits per file**, unlike Claude Code which sends one.

### Phase 2: Detect User Edits in PostChange Flow

**Goal**: Compare expected AI edits with actual working copy state using fast classification.

#### 2.1 Add Detection Logic to Core Flow

```yaml
# cli/src/flows/core/flow.yaml
PostChange:
  # Classify edits (fast path: string comparison only)
  - let: detection = self.classify_edits
    on_failure: continue
  
  # Fast path: All exact matches (90% of cases)
  - if: $detection.all_exact_match == true
    then:
      - let: metadata = self.build_metadata
      - jj: metaedit --message "$metadata.message" --author "$metadata.author"
      - jj: new
      - stop
  
  # Edge case: Overlapping edits (best-effort separation)
  - if: $detection.has_overlapping_edits == true
    then:
      - log: "⚠️  Overlapping user + AI edits detected - attempting best-effort separation"
      - log: "   Attribution may be imperfect. Review with 'jj log -p' and use 'jj split -i' if needed"
      - let: metadata = self.build_metadata
      - let: sep = self.separate_edits
        args:
          ai_message: $metadata.message
          ai_author: $metadata.author
          extra_files: $detection.extra_files
        on_failure:
          - log: "Failed to separate overlapping edits"
          - jj: metaedit --message "$metadata.message" --author "$metadata.author"
          - jj: new
      - stop
  
  # Edge case: Additive user edits (auto-separate)
  - if: $detection.has_additive_edits == true
    then:
      - log: "Detected additive user edits, separating..."
      - let: metadata = self.build_metadata
      - let: sep = self.separate_edits
        args:
          ai_message: $metadata.message
          ai_author: $metadata.author
          extra_files: $detection.extra_files
        on_failure:
          - log: "Failed to separate edits automatically"
          - continue
      - stop
```

#### 2.2 Implement classify_edits Function

```rust
// cli/src/flows/core/classify_edits.rs
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub enum EditClassification {
    ExactMatch,
    AdditiveUserEdits,  // User added content around AI's edits
    OverlappingEdits,   // User modified AI's edits
}

pub fn classify_edits(state: &AikiState) -> Result<Value> {
    let event = state.event.as_post_change()?;
    let cwd = &event.cwd;
    
    // Get edit details from event
    let edit_details = match &event.edit_details {
        Some(details) => details,
        None => {
            // Can't classify without edit details - skip separation
            eprintln!("No edit_details for PostChange; skipping user/AI separation");
            return Ok(json!({
                "all_exact_match": false,
                "has_additive_edits": false,
                "has_overlapping_edits": false,
                "classification_skipped": true,
            }));
        }
    };
    
    // Group edits by file to avoid redundant JJ calls (Cursor has multiple edits per file)
    let mut edits_by_file: HashMap<String, Vec<&EditDetail>> = HashMap::new();
    for edit in edit_details {
        edits_by_file.entry(edit.file_path.clone())
            .or_insert_with(Vec::new)
            .push(edit);
    }
    
    // Classify each file (only once per file, even if multiple edits)
    let mut all_exact = true;
    let mut has_additive = false;
    let mut has_overlapping = false;
    
    for (file_path_str, file_edits) in edits_by_file {
        let file_path = cwd.join(&file_path_str);
        
        // Handle non-UTF8 files gracefully
        let actual = match std::fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(_) => {
                // Binary file or read error - skip classification for this file
                eprintln!(
                    "Skipping classification for {}: not UTF-8 or unreadable. \
                     File will be attributed to AI (no auto-separation).",
                    file_path_str
                );
                continue;  // Treat as ExactMatch (don't mark as additive/overlapping)
            }
        };
        
        // Classify all edits for this file
        let mut file_classification = EditClassification::ExactMatch;
        
        for edit in &file_edits {
            let classification = classify_edit(edit, &actual);
            
            // Aggregate: if any edit is overlapping/additive, that's the file's classification
            match classification {
                EditClassification::OverlappingEdits => {
                    file_classification = EditClassification::OverlappingEdits;
                    break;  // Worst case, no need to check other edits
                }
                EditClassification::AdditiveUserEdits => {
                    file_classification = EditClassification::AdditiveUserEdits;
                    // Don't break - might find overlapping later
                }
                EditClassification::ExactMatch => {
                    // Keep current classification
                }
            }
        }
        
        // For substring-based edits, if tentatively ExactMatch, check with JJ parent
        if matches!(file_classification, EditClassification::ExactMatch) {
            // Detect if any edit is substring (not full content)
            let has_substring = file_edits.iter()
                .any(|e| e.old_string.len() < 100 && !e.old_string.contains('\n'));
            
            if has_substring {
                // Reconstruct AI-only result using shared helper (ONCE per file)
                if let Ok(ai_only) = reconstruct_ai_only_content(edit_details, &file_path_str, cwd) {
                    if actual != ai_only {
                        // File has AI's changes PLUS other differences
                        file_classification = EditClassification::AdditiveUserEdits;
                    }
                }
                // If reconstruction fails, keep tentative ExactMatch classification
            }
        }
        
        // Apply file classification to aggregate result
        match file_classification {
            EditClassification::ExactMatch => {
                // All good, continue
            }
            EditClassification::AdditiveUserEdits => {
                all_exact = false;
                has_additive = true;
            }
            EditClassification::OverlappingEdits => {
                all_exact = false;
                has_overlapping = true;
            }
        }
    }
    
    // Check for extra files only if needed (optimization)
    let extra_files = if all_exact && !has_additive && !has_overlapping {
        // Only pay the cost of jj diff when AI files all match exactly
        check_extra_files(cwd, &event.file_paths)?
    } else {
        Vec::new()  // Already know we need separation/warning
    };
    
    if !extra_files.is_empty() {
        all_exact = false;
        has_additive = true;
    }
    
    Ok(json!({
        "all_exact_match": all_exact,
        "has_additive_edits": has_additive,
        "has_overlapping_edits": has_overlapping,
        "extra_files": extra_files,  // Pass to separate_edits
    }))
}

fn classify_edit(edit: &EditDetail, actual: &str) -> EditClassification {
    // Handle two semantic cases:
    // 1. ACP: old_string/new_string are full file contents
    // 2. Claude/Cursor: old_string/new_string are search/replace substrings
    
    // Check if this is a full-file edit (ACP style)
    // Heuristic: if old_string is long and contains newlines, likely full content
    let is_full_content = edit.old_string.len() > 100 || edit.old_string.contains('\n');
    
    if is_full_content {
        // ACP case: compare full content
        if actual == edit.new_string {
            EditClassification::ExactMatch
        } else if actual.contains(&edit.new_string) {
            EditClassification::AdditiveUserEdits
        } else {
            EditClassification::OverlappingEdits
        }
    } else {
        // Claude/Cursor case: check if transformation was applied
        let ai_change_present = actual.contains(&edit.new_string);
        let old_content_gone = !actual.contains(&edit.old_string);
        
        if !ai_change_present {
            // AI's new_string not present - user modified or deleted it
            return EditClassification::OverlappingEdits;
        }
        
        if !old_content_gone {
            // Both old and new present - ambiguous
            // Could be: user reverted, or old_string appears multiple times
            return EditClassification::OverlappingEdits;
        }
        
        // AI's transformation applied (new present, old gone)
        // But are there other changes? Need to check with parent
        // This is handled in classify_edits() which gets JJ parent
        EditClassification::ExactMatch  // Tentative - refined in classify_edits
    }
}

/// Check for files changed in working copy that AI didn't claim to edit
fn check_extra_files(cwd: &Path, ai_files: &[String]) -> Result<Vec<String>> {
    let output = Command::new("jj")
        .args(["diff", "--name-only"])
        .current_dir(cwd)
        .output()?;
    
    if !output.status.success() {
        return Ok(Vec::new());
    }
    
    let all_files: HashSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    
    let ai_files: HashSet<String> = ai_files.iter().cloned().collect();
    
    Ok(all_files.difference(&ai_files).cloned().collect())
}
```

**Performance characteristics:**
- **ACP hot path (full content)**: File read + string equality check (~1-2ms per file)
- **Claude/Cursor hot path (substring)**: File read + contains check + JJ parent read (~11-52ms per file)
  - File read: ~1-2ms
  - Contains checks: ~0.1ms
  - JJ subprocess: ~10-50ms
- **Optimization**: Edits grouped by file - only one JJ call per unique file
  - Cursor with 5 edits to same file: 1 JJ call, not 5
- **Total overhead**: 10-50ms per unique file for substring-based edits (Claude/Cursor)
- **Trade-off**: Slower than pure file-read, but necessary to detect mixed user/AI edits accurately

**Correctness trade-offs:**
- ✅ Exact match: 100% accurate
- ✅ Contains check: ~95% accurate (may misclassify "user changed 1 char" as overlapping)
- ⚠️ Edge case: `actual = "foo baz bar"`, `expected = "foo bar"` → classified as additive (arguably overlapping)
  - Decision: This is acceptable. Worst case is we separate when we could've warned, or vice versa.

#### 2.3 Implement Helper: reconstruct_ai_only_content

```rust
// cli/src/flows/core/separate_edits.rs

/// Reconstruct what the file should contain if ONLY the AI edited it
/// 
/// Handles two cases:
/// - Full-content edits (ACP): Return new_string directly
/// - Substring edits (Claude/Cursor): Apply transformations to JJ parent
fn reconstruct_ai_only_content(
    edit_details: &[EditDetail],
    file_path: &str,
    cwd: &Path
) -> Result<String> {
    let file_edits: Vec<_> = edit_details.iter()
        .filter(|e| e.file_path == file_path)
        .collect();
    
    if file_edits.is_empty() {
        return Err(anyhow::anyhow!("No edits found for file: {}", file_path));
    }
    
    // Check if any edit is full-content (ACP style)
    let has_full_content = file_edits.iter()
        .any(|e| e.old_string.len() > 100 || e.old_string.contains('\n'));
    
    if has_full_content {
        // ACP case: new_string is complete file content
        // Just use the first edit's new_string (ACP only has one edit per file)
        Ok(file_edits[0].new_string.clone())
    } else {
        // Claude/Cursor case: apply substring transformations to parent
        let parent_result = Command::new("jj")
            .args(["file", "show", "--revision", "@-", file_path])
            .current_dir(cwd)
            .output()?;
        
        if !parent_result.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to read parent content for {}: {}",
                file_path,
                String::from_utf8_lossy(&parent_result.stderr)
            ));
        }
        
        let parent_content = String::from_utf8_lossy(&parent_result.stdout);
        let mut result = parent_content.to_string();
        
        // Apply all edits sequentially (for Cursor with multiple edits)
        for edit in file_edits {
            result = result.replace(&edit.old_string, &edit.new_string);
        }
        
        Ok(result)
    }
}

#### 2.4 Implement separate_edits Function (Universal)

```rust
// cli/src/flows/core/separate_edits.rs

pub fn separate_edits(state: &AikiState) -> Result<Value> {
    let event = state.event.as_post_change()?;
    let cwd = &event.cwd;
    let edit_details = event.edit_details.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No edit details available"))?;
    
    // Extract args from state (passed by flow.yaml)
    let ai_message = state.get_string("ai_message")?;
    let ai_author = state.get_string("ai_author")?;
    let extra_files: Vec<String> = state.get_array("extra_files")
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    
    // Capture combined content (AI + user edits) before we overwrite
    let mut combined_contents = Vec::new();
    
    // Get unique files from edit_details (Cursor may have multiple edits per file)
    let unique_files: HashSet<&str> = edit_details.iter()
        .map(|e| e.file_path.as_str())
        .collect();
    
    // 1. Capture AI-touched files
    for file_path in &unique_files {
        let full_path = cwd.join(file_path);
        if let Ok(combined) = std::fs::read_to_string(&full_path) {
            combined_contents.push((file_path.to_string(), combined));
        }
    }
    
    // 2. Capture extra files (user-only edits)
    for file in &extra_files {
        let file_path = cwd.join(file);
        if let Ok(combined) = std::fs::read_to_string(&file_path) {
            combined_contents.push((file.clone(), combined));
        }
    }
    
    // Step 1: Build AI-only working copy
    //   a. Write AI's expected content for AI-touched files
    for file_path in unique_files {
        let full_path = cwd.join(file_path);
        
        // Reconstruct what AI intended (handles both full-content and substring edits)
        let ai_content = reconstruct_ai_only_content(edit_details, file_path, cwd)?;
        
        std::fs::write(&full_path, ai_content)?;
    }
    
    //   b. Reset user-only files to parent (remove user's changes)
    if !extra_files.is_empty() {
        let file_args: Vec<&str> = extra_files.iter().map(AsRef::as_ref).collect();
        Command::new("jj")
            .arg("restore")
            .arg("--from")
            .arg("@-")
            .arg("--")
            .args(&file_args)
            .current_dir(cwd)
            .status()?;
    }
    
    // Step 2: Commit AI-only state with Aiki metadata
    Command::new("jj")
        .args(["describe", "-m", &ai_message])
        .current_dir(cwd)
        .status()?;
    
    Command::new("jj")
        .args(["metaedit", "--author", &ai_author])
        .current_dir(cwd)
        .status()?;
    
    // Step 3: Create new change for user edits
    Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .status()?;
    
    // Step 4: Restore combined content to working copy
    for (file_path, combined_content) in &combined_contents {
        let full_path = cwd.join(file_path);
        std::fs::write(&full_path, combined_content)?;
    }
    
    // Step 5: Restore AI's version as baseline (so jj sees diff = user-only changes)
    let mut all_files: Vec<String> = event.file_paths.clone();
    all_files.extend(extra_files.clone());
    let file_args: Vec<&str> = all_files.iter().map(AsRef::as_ref).collect();
    
    Command::new("jj")
        .arg("restore")
        .arg("--from")
        .arg("@-")
        .arg("--")
        .args(&file_args)
        .current_dir(cwd)
        .status()?;
    
    // Step 6: Describe user change
    Command::new("jj")
        .args(["describe", "-m", "User edits during AI session"])
        .current_dir(cwd)
        .status()?;
    
    // Step 7: Create new change for next AI edit
    Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .status()?;
    
    Ok(json!({
        "separated": true,
        "method": "unified",
        "ai_files": event.file_paths.clone(),
        "user_files": extra_files,
    }))
}
```

**Why this works:**

This single function handles **all three scenarios**:

1. **Scenario 1 (Different files case)**: AI edits main.rs, user edited utils.rs
   - Step 1a writes AI's version of main.rs
   - Step 1b restores utils.rs to parent (removes user's changes from working copy)
   - Step 2 commits: AI-only change (main.rs only)
   - Step 3 creates new change
   - Step 4 restores combined content (main.rs + utils.rs both with edits)
   - Step 5 `jj restore --from @-` sets AI's version as baseline for both files
   - Result: JJ diff shows user's changes to utils.rs only ✓

2. **Scenario 2 (Same-file additive case)**: AI edited lines 50-60, user added lines at 10-20
   - Step 1a writes AI's version (lines 50-60 only)
   - Step 1b skipped (no extra_files)
   - Step 2 commits: AI-only change
   - Step 3 creates new change
   - Step 4 writes combined version (lines 10-20 + 50-60)
   - Step 5 `jj restore --from @-` sets AI's version as baseline
   - Result: JJ diff shows user's additions at lines 10-20 only ✓

3. **Scenario 3 (Overlapping edits)**: Filtered out by classification, never reaches this function

**Key insight**: We leverage JJ's native diff engine instead of manually computing "user-only" content. JJ handles all line-shifting complexity internally. The `extra_files` parameter allows us to handle user-only file edits by resetting them to parent before committing the AI change.

### Phase 3: Testing & Validation

#### 3.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_classify_edit_exact_match() {
        // Full-content edit (ACP style)
        let edit = EditDetail::new("test.rs", "foo\nbar", "foo\nbar\nbaz");
        let actual = "foo\nbar\nbaz";
        assert!(matches!(
            classify_edit(&edit, actual),
            EditClassification::ExactMatch
        ));
    }
    
    #[test]
    fn test_classify_edit_additive() {
        // Full-content edit where user added to AI's result
        let edit = EditDetail::new("test.rs", "bar\nbaz", "bar\nbaz");
        let actual = "foo\nbar\nbaz";  // User added "foo\n" at start
        assert!(matches!(
            classify_edit(&edit, actual),
            EditClassification::AdditiveUserEdits
        ));
    }
    
    #[test]
    fn test_classify_edit_overlapping() {
        // Substring edit where AI's new_string is missing
        let edit = EditDetail::new("test.rs", "return 1", "return 2");
        let actual = "def foo():\n    return 3\n";  // User changed to 3, not 2
        assert!(matches!(
            classify_edit(&edit, actual),
            EditClassification::OverlappingEdits
        ));
        
        // Substring edit where old_string is still present
        let edit = EditDetail::new("test.rs", "return 1", "return 2");
        let actual = "def foo():\n    return 1\n";  // AI's change didn't apply
        assert!(matches!(
            classify_edit(&edit, actual),
            EditClassification::OverlappingEdits
        ));
    }
    
    #[test]
    fn test_classify_edit_substring_present() {
        // Substring edit where transformation is cleanly applied
        let edit = EditDetail::new("test.rs", "return 1", "return 2");
        let actual = "def foo():\n    return 2\n";
        // This would be ExactMatch tentatively, then checked with JJ parent
        assert!(matches!(
            classify_edit(&edit, actual),
            EditClassification::ExactMatch
        ));
    }
}
```

#### 3.2 Integration Tests

```rust
#[test]
fn test_end_to_end_different_files() {
    // 1. Start session
    // 2. User manually edits file A
    // 3. AI edits file B
    // 4. PostChange fires
    // 5. Assert: File A in separate change with "User edits" message
    // 6. Assert: File B in AI change with [aiki] metadata
}

#[test]
fn test_end_to_end_same_file_additive() {
    // 1. Start session
    // 2. AI edits lines 50-60 of main.rs
    // 3. User adds lines 10-20 of main.rs
    // 4. PostChange fires
    // 5. Assert: Two changes created
    // 6. Assert: AI change has lines 50-60 with [aiki] metadata
    // 7. Assert: User change has lines 10-20 with "User edits" message
}

#[test]
fn test_end_to_end_overlapping() {
    // 1. Start session
    // 2. AI edits lines 50-60 of main.rs
    // 3. User modifies lines 55-65 of main.rs (overlaps)
    // 4. PostChange fires
    // 5. Assert: Warning logged about overlapping edits
    // 6. Assert: Single change created with [aiki] metadata
    // 7. Assert: User told to run `jj split -i`
}
```

## Implementation Plan

### Step 1: Event Structure (1 hour)
- [ ] Add `edit_details: Option<Vec<EditDetail>>` to `AikiPostChangeEvent`
- [ ] Add `EditDetail` struct with `file_path` and `new_text` fields
- [ ] Add `EditDetail::new()` constructor
- [ ] Update event creation in all vendors (ACP, Claude Code, Cursor)
- [ ] Add debug logging to verify edit details are captured

### Step 2: Classification Logic (2 hours)
- [ ] Create `cli/src/flows/core/classify_edits.rs`
- [ ] Implement `EditClassification` enum (ExactMatch, AdditiveUserEdits, OverlappingEdits)
- [ ] Implement `classify_edit()` function (string comparison only)
- [ ] Implement `classify_edits()` function (iterate over files)
- [ ] Implement `check_extra_files()` helper (optional `jj diff --name-only`)
- [ ] Add to module exports

### Step 3: Separation Logic (3 hours)
- [ ] Create `cli/src/flows/core/separate_edits.rs`
- [ ] Implement `reconstruct_ai_only_content()` helper function
  - [ ] Handle full-content edits (ACP)
  - [ ] Handle substring edits with JJ parent reconstruction (Claude/Cursor)
  - [ ] Apply multiple edits sequentially for Cursor
- [ ] Implement `separate_edits()` universal separation function
  - [ ] Use `reconstruct_ai_only_content()` to build AI-only working copy
  - [ ] Deduplicate files before processing (Cursor multiple edits case)
- [ ] Test with real jj commands (all scenarios)
- [ ] Add error handling for file I/O and jj command failures

### Step 4: Flow Integration (1 hour)
- [ ] Update `cli/src/flows/core/flow.yaml`
- [ ] Add `classify_edits` step
- [ ] Add fast-path branch for `all_exact_match`
- [ ] Add overlapping-edits warning branch
- [ ] Add additive-edits separation branch

### Step 5: Testing (3-4 hours)
- [ ] Write unit tests for `reconstruct_ai_only_content()`
  - [ ] Test full-content reconstruction (ACP)
  - [ ] Test substring reconstruction (Claude/Cursor)
  - [ ] Test multiple edits per file (Cursor)
- [ ] Write unit tests for `classify_edit()` (exact, additive, overlapping)
- [ ] Write integration tests (different files, same-file additive, overlapping)
- [ ] Manual testing with each integration (ACP, Claude, Cursor)
- [ ] Test edge cases (empty files, large files, binary files, JJ parent read failures)

### Step 6: Documentation (30 minutes)
- [ ] Update ROADMAP.md with completion status
- [ ] Document classification algorithm and trade-offs
- [ ] Add troubleshooting guide for overlapping edits

**Total Estimated Time: 9-12 hours**

## Success Criteria

1. ✅ Different-file user edits are automatically separated into distinct changes
2. ✅ Same-file additive user edits are automatically separated
3. ✅ Same-file overlapping edits are automatically separated with best-effort (warns if imperfect)
4. ✅ AI provenance is only recorded for AI-edited content
5. ✅ User edits get separate change with clear description
6. ✅ Works across all three integrations (ACP, Claude Code, Cursor)
7. ✅ Fast path (exact match) for ACP takes <5ms per file
8. ✅ No regressions in existing functionality

## Limitations & Future Work

### Known Limitations

1. **Substring replacement fragility**: Using `.replace()` to reconstruct AI-only content has edge cases:
   - If `old_string` appears multiple times, replaces all occurrences (may not match AI's intent)
   - Multiple edits (Cursor) applied sequentially may interact unexpectedly
   - Order of edits matters - wrong order gives wrong reconstruction
   - **Impact**: ~5-10% false positives/negatives in additive vs overlapping classification
   - **For overlapping edits**: Best-effort separation may attribute some user changes to AI or vice versa. User should review with `jj log -p` and manually fix with `jj split -i` if needed.

2. **No line-level precision in error messages**: When overlapping edits are detected, we can't tell user "overlap at line 42". They need to inspect diff manually.

3. **Binary file handling**: Non-UTF8 files are skipped during classification and treated as ExactMatch. If user and AI both edit a binary file, no separation occurs.

4. **JJ parent assumption**: For Claude/Cursor, we assume `@-` is the state before AI edited. This breaks if:
   - User runs `jj new` between SessionStart and PostChange
   - Multiple tools edit same file in sequence (parent is previous AI edit, not pre-session state)
   - **Impact**: May incorrectly attribute previous AI edits as user edits

### Future Enhancements

1. **Fuzzy matching for additive detection**: Use similarity threshold (e.g., "95% of AI's edit is present") to catch "user changed 1 typo in 1000-line file" case. Would reduce false negatives from ~5% to ~1%.

2. **Progressive disclosure for debugging**: Add `aiki debug diff --file src/main.rs` command that uses structured diff (with `similar` crate) to show line-by-line comparison. Only used when user needs to debug separation failures.

3. **Confidence scoring**: Show attribution confidence (High/Medium/Low) based on classification type:
   - ExactMatch → High (100%)
   - AdditiveUserEdits → Medium (95%)
   - OverlappingEdits → Low (requires manual review)

4. **Interactive separation UI**: When overlapping edits detected, provide a UI to help user separate them (similar to `jj split -i` but with context about what AI vs user changed).

## Design Decisions

### Why Simple String Comparison Instead of Structured Diffs?

After prototyping both approaches, we chose simple string comparison (`contains()`) over structured diff parsing (using `similar` crate) for these reasons:

**Performance:**
- **String comparison**: ~1-2ms per file (hot path)
- **Structured diff**: ~5-10ms per file (parsing + comparison)
- **Impact**: 10-100x faster on hot path (90% of PostChange events)

**Complexity:**
- **String comparison**: ~50 lines of code, easy to reason about
- **Structured diff**: ~300 lines of code, complex line-range arithmetic
- **Maintenance**: Simple code has fewer bugs, easier to debug

**Correctness:**
- **String comparison**: ~95% accurate (misses some edge cases)
- **Structured diff**: ~98% accurate (precise line-level detection)
- **Impact**: ~5% false negatives (won't auto-separate when we could)

**Trade-off decision**: The 3% accuracy loss is acceptable given the massive simplification. Users can always fall back to `jj split --interactive` for edge cases.

### Why Single Separation Function?

Originally designed two separate functions: `separate_user_edits()` for different files and `separate_same_file_edits()` for same-file edits. But both use the same "universal JJ trick":

1. Write AI-only content
2. Commit with metadata
3. Create new change
4. Restore combined content
5. `jj restore --from @-` to set AI baseline
6. JJ computes diff automatically

**Benefits of unification:**
- Single code path to maintain and test
- Fewer conditional branches in flow.yaml
- Consistent behavior across scenarios
- Simpler mental model

### Why Not Use jj-lib Directly?

We could use jj-lib to compare tree objects instead of file I/O:

```rust
let ai_tree = /* construct from edit_details */;
let wc_tree = repo.working_copy().tree();
let diff = ai_tree.diff(&wc_tree);
```

**Why we didn't:**
- **Complexity**: Requires constructing tree objects, managing jj-lib workspace
- **Performance**: Tree construction + diff is ~5-20ms (not faster than file reads)
- **Storage**: Creates persistent tree objects in .jj/repo/store (disk I/O)
- **Dependencies**: Couples flow logic tightly to jj-lib internals

File I/O + string comparison is simpler and just as fast.

## Related Work

- **Phase 0**: JJ initialization with `jj new` on SessionStart (guards against pre-existing edits)
- **Phase 1**: Claude Code provenance tracking
- **Phase 2**: Cursor support
- **Phase 6**: ACP protocol support with ToolCallContent
- **Phase 8** (planned): Confidence scoring and attribution quality metrics

## References

- ACP Protocol: https://agentclientprotocol.com/protocol/schema
- Claude Code Hooks: https://docs.claude.com/claude-code/hooks
- Cursor Hooks: https://cursor.com/docs/agent/hooks#afterfileedit
- JJ Commands: `jj restore`, `jj split`, `jj describe`
- Myers diff algorithm: https://en.wikipedia.org/wiki/Diff#Algorithm

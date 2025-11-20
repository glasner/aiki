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
    pub old_text: String,
    pub new_text: String,
    pub line_range: Option<(usize, usize)>, // Optional line numbers from ACP
}
```

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
    // Extract from ToolCallContent::Diff variant
    // Map oldText/newText to EditDetail structs
}
```

#### 1.3 Populate from Claude Code Hook

```rust
// cli/src/vendors/claude_code.rs
"PostToolUse" => {
    let tool_input = payload.tool_input
        .ok_or_else(|| anyhow::anyhow!("PostToolUse requires tool_input"))?;
    
    // NEW: Create edit detail from old_string/new_string
    let edit_details = Some(vec![EditDetail {
        file_path: tool_input.file_path.clone(),
        old_text: tool_input.old_string,
        new_text: tool_input.new_string,
        line_range: None,
    }]);
    
    AikiEvent::PostChange(AikiPostChangeEvent {
        // ... existing fields ...
        edit_details,
    })
}
```

#### 1.4 Populate from Cursor Hook

```rust
// cli/src/vendors/cursor.rs
"afterFileEdit" => {
    let file_path = /* ... */;
    
    // NEW: Map Cursor edits array to EditDetail
    let edit_details = if !payload.edits.is_empty() {
        Some(
            payload.edits.iter().map(|edit| EditDetail {
                file_path: file_path.clone(),
                old_text: edit.old_string.clone(),
                new_text: edit.new_string.clone(),
                line_range: None,
            }).collect()
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

### Phase 2: Detect User Edits in PostChange Flow

**Goal**: Compare expected AI edits with actual working copy state.

#### 2.1 Add Detection Logic to Core Flow

```yaml
# cli/src/flows/core/flow.yaml
PostChange:
  # Check for unexpected changes (user edits)
  - let: user_edit_check = self.check_for_user_edits
    on_failure: continue
  
  # If user edits detected in DIFFERENT files, separate them
  - if: $user_edit_check.has_different_files == true
    then:
      - log: "Detected user edits to different files, separating..."
      - let: result = self.separate_user_edits
      on_failure: continue
  
  # If user edits detected in SAME files, warn
  - if: $user_edit_check.has_same_file_edits == true
    then:
      - log: "⚠️  User edited same files as AI - cannot auto-separate"
      - log: "   Run 'jj split --interactive' to manually separate"
  
  # Record AI provenance (on isolated change if separation happened)
  - let: metadata = self.build_metadata
    on_failure: stop
  - jj: metaedit --message "$metadata.message" --author "$metadata.author"
  - jj: new
```

#### 2.2 Implement check_for_user_edits Function

```rust
// cli/src/flows/core/check_user_edits.rs
pub fn check_for_user_edits(state: &AikiState) -> Result<Value> {
    let event = state.event.as_post_change()?;
    let cwd = &event.cwd;
    
    // Get all changed files in working copy
    let output = Command::new("jj")
        .args(["diff", "--name-only"])
        .current_dir(cwd)
        .output()?;
    let all_files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    
    // Files AI claimed to edit
    let ai_files: HashSet<String> = event.file_paths.iter().cloned().collect();
    
    // Files that were changed but AI didn't claim
    let different_files: Vec<String> = all_files.iter()
        .filter(|f| !ai_files.contains(*f))
        .cloned()
        .collect();
    
    // Check if AI files have unexpected content (user edited same file)
    let mut same_file_edits = false;
    if let Some(edit_details) = &event.edit_details {
        same_file_edits = has_unexpected_changes(cwd, edit_details)?;
    }
    
    Ok(json!({
        "has_different_files": !different_files.is_empty(),
        "different_files": different_files,
        "has_same_file_edits": same_file_edits,
    }))
}

fn has_unexpected_changes(cwd: &Path, expected_edits: &[EditDetail]) -> Result<bool> {
    for edit in expected_edits {
        // Get actual diff for this file
        let output = Command::new("jj")
            .args(["diff", &edit.file_path])
            .current_dir(cwd)
            .output()?;
        let actual_diff = String::from_utf8_lossy(&output.stdout);
        
        // Check if actual diff contains more than expected edit
        // This is heuristic - we look for the expected old->new change
        if !actual_diff.contains(&edit.old_text) || 
           !actual_diff.contains(&edit.new_text) {
            // Diff doesn't match expected - user likely edited
            return Ok(true);
        }
        
        // Count total changed lines vs expected
        let actual_lines = count_changed_lines(&actual_diff);
        let expected_lines = count_changed_lines_in_edit(edit);
        
        if actual_lines > expected_lines * 2 {
            // Significantly more changes than expected
            return Ok(true);
        }
    }
    
    Ok(false)
}
```

#### 2.3 Implement separate_user_edits Function

```rust
// cli/src/flows/core/separate_user_edits.rs
pub fn separate_user_edits(state: &AikiState) -> Result<Value> {
    let event = state.event.as_post_change()?;
    let cwd = &event.cwd;
    let user_files = /* extract from $user_edit_check.different_files */;
    
    if user_files.is_empty() {
        return Ok(json!({ "separated": false }));
    }
    
    // Use jj restore to separate user files from AI files
    // 1. Restore user files from parent (removes them from working copy)
    let file_args: Vec<String> = user_files.iter()
        .map(|f| f.to_string())
        .collect();
    
    Command::new("jj")
        .arg("restore")
        .arg("--from")
        .arg("@-")
        .args(&file_args)
        .current_dir(cwd)
        .status()?;
    
    // Now @ has only AI edits
    // 2. Record AI metadata (caller will do this)
    
    // 3. Create new change for user edits
    Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .status()?;
    
    // 4. Restore user files in this new change
    Command::new("jj")
        .arg("restore")
        .arg("--from")
        .arg("@--")
        .args(&file_args)
        .current_dir(cwd)
        .status()?;
    
    // 5. Describe user change
    Command::new("jj")
        .args(["describe", "-m", "User edits during AI session"])
        .current_dir(cwd)
        .status()?;
    
    // 6. Create new change for next AI edit
    Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .status()?;
    
    Ok(json!({
        "separated": true,
        "user_files": user_files,
    }))
}
```

### Phase 3: Testing & Validation

#### 3.1 Unit Tests

```rust
#[test]
fn test_check_for_user_edits_different_files() {
    // Setup: AI edits main.rs, user edits utils.rs
    // Assert: has_different_files = true
}

#[test]
fn test_check_for_user_edits_same_file() {
    // Setup: AI edits lines 50-60, user edits lines 10-20 of same file
    // Assert: has_same_file_edits = true
}

#[test]
fn test_separate_user_edits() {
    // Setup: Working copy has main.rs (AI) + utils.rs (user)
    // Execute: separate_user_edits
    // Assert: Two changes created, properly separated
}
```

#### 3.2 Integration Tests

```rust
#[test]
fn test_end_to_end_user_edit_separation() {
    // 1. Start session
    // 2. User manually edits file A
    // 3. AI edits file B
    // 4. PostChange fires
    // 5. Assert: File A in separate change with "User edits" message
    // 6. Assert: File B in AI change with [aiki] metadata
}
```

## Implementation Plan

### Step 1: Event Structure (1-2 hours)
- [ ] Add `edit_details` field to `AikiPostChangeEvent`
- [ ] Add `EditDetail` struct
- [ ] Update event creation in all vendors (ACP, Claude Code, Cursor)
- [ ] Add debug logging to verify edit details are captured

### Step 2: Detection Logic (2-3 hours)
- [ ] Create `cli/src/flows/core/check_user_edits.rs`
- [ ] Implement `check_for_user_edits()` function
- [ ] Implement `has_unexpected_changes()` helper
- [ ] Add to build metadata module exports

### Step 3: Separation Logic (2-3 hours)
- [ ] Create `cli/src/flows/core/separate_user_edits.rs`
- [ ] Implement `separate_user_edits()` function
- [ ] Test with real jj commands

### Step 4: Flow Integration (1 hour)
- [ ] Update `cli/src/flows/core/flow.yaml`
- [ ] Add user edit detection step
- [ ] Add separation step with conditionals
- [ ] Add warning for same-file edits

### Step 5: Testing (2-3 hours)
- [ ] Write unit tests for detection logic
- [ ] Write unit tests for separation logic
- [ ] Write integration test for end-to-end flow
- [ ] Manual testing with each integration (ACP, Claude, Cursor)

### Step 6: Documentation (1 hour)
- [ ] Update ROADMAP.md with completion status
- [ ] Document limitations (same-file edits require manual split)
- [ ] Add troubleshooting guide for users

**Total Estimated Time: 9-13 hours**

## Success Criteria

1. ✅ Different-file user edits are automatically separated into distinct changes
2. ✅ Same-file user edits are detected and warned about
3. ✅ AI provenance is only recorded for AI-edited files
4. ✅ User edits get separate change with clear description
5. ✅ Works across all three integrations (ACP, Claude Code, Cursor)
6. ✅ All tests pass
7. ✅ No regressions in existing functionality

## Limitations & Future Work

### Known Limitations

1. **Same-file concurrent edits**: Cannot automatically separate when user and AI edit the same file. Requires manual `jj split --interactive`.

2. **Heuristic detection**: Comparison logic is heuristic-based (line counts, content matching). May have false positives/negatives.

3. **Performance**: Adds `jj diff` calls which may slow down PostChange for large files.

### Future Enhancements

1. **Line-level diff comparison**: Parse diffs more precisely to detect exact line overlaps
2. **Smart merging**: Attempt to merge non-overlapping same-file edits automatically
3. **Performance optimization**: Cache diffs, use incremental checks
4. **Visual indicators**: Show confidence level of attribution (High/Medium/Low based on detection)

## Related Work

- **Phase 0**: JJ initialization with `jj new` on SessionStart (guards against pre-existing edits)
- **Phase 1**: Claude Code provenance tracking
- **Phase 2**: Cursor support
- **Phase 6**: ACP protocol support with ToolCallContent

## References

- ACP Protocol: https://agentclientprotocol.com/protocol/schema
- Claude Code Hooks: https://docs.claude.com/claude-code/hooks
- Cursor Hooks: https://cursor.com/docs/agent/hooks#afterfileedit
- JJ Commands: `jj restore`, `jj split`, `jj describe`

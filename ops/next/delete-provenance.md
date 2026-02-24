---
status: draft
---

# Delete Operation Provenance Tracking

## Problem Statement

Currently, when an AI agent deletes a file, we lose all provenance information about:
1. Who originally created/authored the file
2. What AI agents contributed to it before deletion
3. When and why it was deleted

The file-operation-refactor plan includes a `delete.completed` event and basic metadata recording, but this doesn't address the deeper question: **Should we track delete provenance, and if so, how?**

## Current State (After File Operation Refactor)

From the file-operation-refactor plan:
```yaml
delete.completed:
  - let: metadata = self.build_delete_metadata
    on_failure:
      - stop: "Failed to build delete metadata"
  - with_author_and_message: $metadata
    jj: metaedit --message "$message"
  - jj: new
```

This creates a JJ change with metadata like:
```
[aiki]
author=claude
author_type=agent
session=session-123
tool=Bash
operation=delete
files=["src/old_file.rs"]
[/aiki]
```

**What this captures:**
- Who deleted the file (AI agent)
- When it was deleted
- Which files were deleted

**What this DOESN'T capture:**
- Who originally created the file
- AI agents that previously modified it
- File's lineage before deletion

## Use Cases

### Use Case 1: Audit Trail
**Scenario:** User wants to know "who deleted this file and why?"

**Current:** Can find deletion change in `jj log`, see AI agent as author
**Missing:** Can't easily see who originally created it or its history

### Use Case 2: Restoration Context
**Scenario:** User wants to restore a deleted file and needs to know who to attribute it to

**Current:** Would need to manually search history before deletion
**Missing:** No easy way to get "last known author" information

### Use Case 3: Co-author Credit
**Scenario:** File was collaboratively developed by human + AI, then deleted. User wants Git commit to credit all contributors.

**Current:** Only deletion is tracked, not previous authorship
**Missing:** No co-author information preserved through deletion

### Use Case 4: Blame After Restoration
**Scenario:** File is deleted then restored. User runs `aiki blame` on it.

**Current:** Would show history from before deletion (if restored via `jj restore`)
**Missing:** No indication in blame output that it was deleted and restored

## Proposed Solutions

### Option 1: Minimal (Current Plan)
**What:** Just record the deletion event in JJ change description
**Pros:** Simple, already designed in file-operation-refactor
**Cons:** Loses file lineage information

**Implementation:** Already planned in Phase 7 of file-operation-refactor

### Option 2: Capture Last Known Metadata
**What:** Before deletion, read the file's last JJ change and extract `[aiki]` metadata, then include it in deletion metadata

**Example:**
```
[aiki]
author=claude
author_type=agent
session=session-123
tool=Bash
operation=delete
files=["src/old_file.rs"]
previous_author=Alice <alice@example.com>
previous_session=session-100
[/aiki]
```

**Pros:**
- Preserves who last modified the file
- Easy to implement (just read previous change description)
- Useful for restoration decisions

**Cons:**
- Only captures last author, not full history
- Requires file to have been tracked in JJ (won't work for untracked files)

**Implementation:**
```rust
pub fn build_delete_metadata(
    event: &AikiDeleteCompletedPayload,
    context: Option<&AikiState>,
) -> Result<ActionResult> {
    // 1. For each deleted file, get its last JJ change
    // 2. Extract [aiki] metadata from that change
    // 3. Include as previous_author, previous_session in deletion record
    
    let mut provenance = ProvenanceRecord {
        agent: event.session.agent_type().to_string().to_lowercase(),
        author_type: "agent".to_string(),
        session: event.session.external_id().to_string(),
        tool: Some(event.tool_name.clone()),
        operation: Some("delete".to_string()),
        files: Some(event.file_paths.clone()),
        timestamp: Some(event.timestamp),
        coauthor: None, // Could populate with previous_author
    };
    
    // Try to get previous author info
    for file_path in &event.file_paths {
        if let Ok(prev_metadata) = get_last_metadata_for_file(&event.cwd, file_path) {
            provenance.coauthor = prev_metadata.author;
            break; // For now, just use first file's metadata
        }
    }
    
    // ... rest of implementation
}

fn get_last_metadata_for_file(cwd: &Path, file_path: &str) -> Result<ProvenanceRecord> {
    // Run: jj log -r "file($file_path)" -n 1 --no-graph -T description
    // Parse [aiki] block from output
    // Return ProvenanceRecord
}
```

### Option 3: Full Change History Snapshot
**What:** Capture complete file history (all changes that touched this file) in deletion metadata

**Example:**
```
[aiki]
author=claude
author_type=agent
session=session-123
tool=Bash
operation=delete
files=["src/old_file.rs"]
file_history=[
  {"change_id": "abc123", "author": "Alice <alice@example.com>", "timestamp": "..."},
  {"change_id": "def456", "author": "claude", "timestamp": "..."},
  {"change_id": "ghi789", "author": "Bob <bob@example.com>", "timestamp": "..."}
]
[/aiki]
```

**Pros:**
- Complete audit trail
- Can reconstruct full file lineage
- Useful for blame after restoration

**Cons:**
- Complex to implement
- Potentially large metadata blocks for files with many changes
- Redundant (JJ already has this info via `jj log`)

**Implementation:** Not recommended - use JJ's native history instead

### Option 4: Delete Provenance Namespace
**What:** Store deletion metadata in a separate namespace that's preserved even after file is gone

**Example:**
```bash
# Store in .jj/repo/deletions/
.jj/repo/deletions/src__old_file.rs.json
```

**Pros:**
- Metadata survives even if JJ change is abandoned
- Can build tools to query all deletions
- Doesn't clutter change descriptions

**Cons:**
- New storage mechanism (not using JJ's native data model)
- Need migration path, backup/restore, etc.
- Deviates from "JJ change description is source of truth" principle

**Implementation:** Not recommended - violates architectural principle of using JJ as single source of truth

## Recommendation

**Choose Option 2: Capture Last Known Metadata**

### Rationale
1. **Balances simplicity and utility** - Captures enough context for restoration decisions without over-engineering
2. **Uses existing architecture** - Stores in JJ change description like all other provenance
3. **Easy to implement** - Can be added to `build_delete_metadata` function
4. **Covers most common use cases** - Knowing who last worked on a file is usually sufficient

### What This Looks Like

**Before deletion - file has this metadata:**
```
[aiki]
author=claude
author_type=agent
session=session-100
tool=Edit
[/aiki]
```

**After deletion - deletion change has this metadata:**
```
[aiki]
author=claude
author_type=agent
session=session-123
tool=Bash
operation=delete
files=["src/old_file.rs"]
coauthor=Claude <noreply@anthropic.com>
previous_session=session-100
[/aiki]
```

**If file had human author before deletion:**
```
[aiki]
author=claude
author_type=agent
session=session-123
tool=Bash
operation=delete
files=["src/old_file.rs"]
coauthor=Alice <alice@example.com>
previous_session=session-100
previous_author_type=human
[/aiki]
```

## Implementation Plan

### Phase 1: Extend ProvenanceRecord
**File:** `src/provenance.rs`

Add fields for previous authorship:
```rust
pub struct ProvenanceRecord {
    pub agent: String,
    pub author_type: String,
    pub session: String,
    pub tool: Option<String>,
    pub operation: Option<String>,
    pub files: Option<Vec<String>>,
    pub timestamp: Option<DateTime<Utc>>,
    pub coauthor: Option<String>,
    
    // New fields for delete provenance
    pub previous_session: Option<String>,
    pub previous_author_type: Option<String>,
}
```

### Phase 2: Implement File History Lookup
**File:** `src/flows/core/functions.rs`

Add helper function:
```rust
/// Get the last provenance metadata for a file
///
/// Returns None if:
/// - File was never tracked in JJ
/// - File has no [aiki] metadata in its history
fn get_last_metadata_for_file(cwd: &Path, file_path: &str) -> Option<ProvenanceRecord> {
    // 1. Run: jj log -r "file($file_path)" -n 1 --no-graph -T description
    // 2. Parse output to extract description
    // 3. Parse [aiki] block from description
    // 4. Return ProvenanceRecord
    
    let output = Command::new("jj")
        .current_dir(cwd)
        .args(["log", "-r", &format!("file({})", file_path), "-n", "1", "--no-graph", "-T", "description"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let description = String::from_utf8(output.stdout).ok()?;
    ProvenanceRecord::from_description(&description).ok().flatten()
}
```

### Phase 3: Update build_delete_metadata
**File:** `src/flows/core/functions.rs`

Enhance to capture previous metadata:
```rust
pub fn build_delete_metadata(
    event: &AikiDeleteCompletedPayload,
    context: Option<&AikiState>,
) -> Result<ActionResult> {
    let mut provenance = ProvenanceRecord {
        agent: event.session.agent_type().to_string().to_lowercase(),
        author_type: "agent".to_string(),
        session: event.session.external_id().to_string(),
        tool: Some(event.tool_name.clone()),
        operation: Some("delete".to_string()),
        files: Some(event.file_paths.clone()),
        timestamp: Some(event.timestamp),
        coauthor: None,
        previous_session: None,
        previous_author_type: None,
    };

    // Try to capture previous authorship for the first file
    // (If deleting multiple files, just use first one's metadata)
    if let Some(file_path) = event.file_paths.first() {
        if let Some(prev) = get_last_metadata_for_file(&event.cwd, file_path) {
            // Determine coauthor based on previous author type
            provenance.coauthor = match prev.author_type.as_str() {
                "human" => Some(prev.agent.clone()), // Was human, credit them
                "agent" => {
                    // Was AI, get agent's git author
                    match prev.agent.as_str() {
                        "claude" => Some("Claude <noreply@anthropic.com>".to_string()),
                        "cursor" => Some("Cursor <noreply@cursor.com>".to_string()),
                        _ => None,
                    }
                }
                _ => None,
            };
            
            provenance.previous_session = Some(prev.session);
            provenance.previous_author_type = Some(prev.author_type);
        }
    }

    let message = provenance.to_description();
    let author = event.session.agent_type().git_author();

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
```

### Phase 4: Update to_description Serialization
**File:** `src/provenance.rs`

Add new fields to serialization:
```rust
impl ProvenanceRecord {
    pub fn to_description(&self) -> String {
        let mut lines = vec![
            "[aiki]".to_string(),
            format!("author={}", self.agent),
            format!("author_type={}", self.author_type),
            format!("session={}", self.session),
        ];

        if let Some(ref tool) = self.tool {
            lines.push(format!("tool={}", tool));
        }
        
        if let Some(ref operation) = self.operation {
            lines.push(format!("operation={}", operation));
        }
        
        if let Some(ref files) = self.files {
            lines.push(format!("files={}", serde_json::to_string(files).unwrap_or_default()));
        }

        if let Some(ref timestamp) = self.timestamp {
            lines.push(format!("timestamp={}", timestamp.to_rfc3339()));
        }

        if let Some(ref coauthor) = self.coauthor {
            lines.push(format!("coauthor={}", coauthor));
        }
        
        // New fields
        if let Some(ref prev_session) = self.previous_session {
            lines.push(format!("previous_session={}", prev_session));
        }
        
        if let Some(ref prev_author_type) = self.previous_author_type {
            lines.push(format!("previous_author_type={}", prev_author_type));
        }

        lines.push("[/aiki]".to_string());
        lines.join("\n")
    }
}
```

### Phase 5: Update from_description Parsing
**File:** `src/provenance.rs`

Parse new fields:
```rust
// In from_description method, add:
"previous_session" => record.previous_session = Some(value.to_string()),
"previous_author_type" => record.previous_author_type = Some(value.to_string()),
```

## Testing Plan

### Unit Tests
```rust
#[test]
fn test_build_delete_metadata_captures_previous_author() {
    // Setup: Create temp repo with file that has [aiki] metadata
    // Delete the file
    // Assert: deletion metadata includes previous_session and coauthor
}

#[test]
fn test_delete_untracked_file() {
    // Delete a file that was never in JJ
    // Assert: deletion metadata has no previous_session (graceful degradation)
}

#[test]
fn test_delete_file_without_aiki_metadata() {
    // Delete a file tracked in JJ but with no [aiki] block
    // Assert: deletion metadata has no previous_session
}
```

### Integration Tests
```rust
#[test]
fn test_delete_after_human_edit() {
    // 1. Human creates file (author_type=human)
    // 2. AI deletes it
    // Assert: coauthor is human's name
}

#[test]
fn test_delete_after_ai_edit() {
    // 1. AI creates file
    // 2. Different AI session deletes it
    // Assert: coauthor is previous AI agent
}
```

## Migration Notes

This is **additive** - no breaking changes:
- New fields are optional in `ProvenanceRecord`
- Existing deletion metadata without these fields will parse correctly
- Old deletions won't have previous authorship info, but that's expected

## Future Enhancements

### 1. Multi-file Delete Handling
Currently we only capture metadata from the first deleted file. Could enhance to:
- Capture metadata for each file separately
- Store as array in deletion metadata
- More complex but covers edge case of deleting files with different authors

### 2. Deletion in Git Commits
When user runs `git commit` after AI deletion, the `commit.message_started` handler could:
- Detect deleted files in staging area
- Extract coauthor from deletion change
- Add to Git commit trailers

### 3. Restoration Helper
New command: `aiki restore <file>` that:
- Finds last deletion of the file
- Shows previous authorship info
- Restores with proper attribution

## Open Questions

1. **What if multiple files are deleted with different authors?**
   - Current: Use first file's metadata
   - Alternative: Store array of {file, previous_author} tuples

2. **Should we track renames as special delete+create?**
   - `mv old.rs new.rs` could be tracked as delete(old.rs) + create(new.rs)
   - Preserves lineage through rename
   - Requires detecting renames in shell command parsing

3. **How far back in history should we look?**
   - Current: Just the immediate previous change
   - Alternative: Walk back until we find a human author
   - Trade-off: Complexity vs. usefulness

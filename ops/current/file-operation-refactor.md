# File Operation Event Refactor

## Problem Statement

The current `file.*` event model requires checking `$event.operation` in flow YAML to distinguish between read, write, and delete operations:

```yaml
file.completed:
  - if: $event.operation == "write"
    then:
      # Only handle writes here...
```

This is verbose and error-prone. The `aiki/core` flow has nested conditionals that make the logic hard to follow.

## Proposed Solution

Replace `file.permission_asked` and `file.completed` with operation-specific events:

| Current | New |
|---------|-----|
| `file.permission_asked` (operation: read) | `read.permission_asked` |
| `file.permission_asked` (operation: write) | `write.permission_asked` |
| `file.permission_asked` (operation: delete) | `delete.permission_asked` |
| `file.completed` (operation: read) | `read.completed` |
| `file.completed` (operation: write) | `write.completed` |
| `file.completed` (operation: delete) | `delete.completed` |

## Benefits

1. **Cleaner YAML** - No conditional checks needed:
   ```yaml
   write.completed:
     - if: self.classify_edits == "AdditiveUserEdits"
       then:
         # Handle user edits...
   ```

2. **Self-documenting** - Event handlers clearly indicate what they handle

3. **Smaller payloads** - No need for `operation` field (it's implicit in the event type)

4. **Better flow separation** - Different handlers for different operations without nesting

## Implementation Plan

### Phase 1: Add New Event Types and Tool Classification

**Files to modify:**
- `src/events/mod.rs` - Add new event variants
- `src/tools.rs` - Keep `FileOperation` enum (used for internal classification)
- Create new payload files:
  - `src/events/read_permission_asked.rs`
  - `src/events/read_completed.rs`
  - `src/events/write_permission_asked.rs`
  - `src/events/write_completed.rs`
  - `src/events/delete_permission_asked.rs`
  - `src/events/delete_completed.rs`

**New event variants:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    // Read operations
    #[serde(rename = "read.permission_asked")]
    ReadPermissionAsked(AikiReadPermissionAskedPayload),
    #[serde(rename = "read.completed")]
    ReadCompleted(AikiReadCompletedPayload),

    // Write operations
    #[serde(rename = "write.permission_asked")]
    WritePermissionAsked(AikiWritePermissionAskedPayload),
    #[serde(rename = "write.completed")]
    WriteCompleted(AikiWriteCompletedPayload),

    // Delete operations
    #[serde(rename = "delete.permission_asked")]
    DeletePermissionAsked(AikiDeletePermissionAskedPayload),
    #[serde(rename = "delete.completed")]
    DeleteCompleted(AikiDeleteCompletedPayload),

    // ... existing events
}
```

**Payload structures:**

```rust
// Write events get all the edit tracking fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWriteCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub file_paths: Vec<String>,
    pub success: Option<bool>,
    pub edit_details: Vec<EditDetail>,
}

// Read events are simpler - no edit details needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiReadCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub file_paths: Vec<String>,
    pub success: Option<bool>,
}

// Delete events - similar to read
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiDeleteCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub file_paths: Vec<String>,
    pub success: Option<bool>,
}
```

**Keep `FileOperation` enum for internal use:**
```rust
// In src/tools.rs - KEEP THIS, don't delete
/// File operation type (used for internal tool classification)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
    Read,
    Write,
    Delete,
}
```

**Tool classification already exists:**
The `ClaudeTool::file_operation()` method already exists in `src/vendors/claude_code/tools.rs`:
```rust
impl ClaudeTool {
    pub fn file_operation(&self) -> Option<FileOperation> {
        match self {
            ClaudeTool::Edit(_) | ClaudeTool::Write(_) 
            | ClaudeTool::NotebookEdit(_) | ClaudeTool::MultiEdit(_) => {
                Some(FileOperation::Write)
            }
            ClaudeTool::Read(_) | ClaudeTool::Glob(_) 
            | ClaudeTool::Grep(_) | ClaudeTool::LS(_) => {
                Some(FileOperation::Read)
            }
            _ => None,
        }
    }
}
```

### Phase 2: Update Flow Type

**File:** `src/flows/types.rs`

Add new handler fields to `Flow`:
```rust
pub struct Flow {
    // ... existing fields

    // Read operations
    #[serde(rename = "read.permission_asked", default)]
    pub read_permission_asked: Vec<FlowStatement>,
    #[serde(rename = "read.completed", default)]
    pub read_completed: Vec<FlowStatement>,

    // Write operations
    #[serde(rename = "write.permission_asked", default)]
    pub write_permission_asked: Vec<FlowStatement>,
    #[serde(rename = "write.completed", default)]
    pub write_completed: Vec<FlowStatement>,

    // Delete operations
    #[serde(rename = "delete.permission_asked", default)]
    pub delete_permission_asked: Vec<FlowStatement>,
    #[serde(rename = "delete.completed", default)]
    pub delete_completed: Vec<FlowStatement>,
}
```

### Phase 3: Update Event Bus

**File:** `src/event_bus.rs`

Add dispatch handlers for new events:
```rust
AikiEvent::ReadPermissionAsked(_) => {
    self.execute_handlers(&flow.read_permission_asked, state)?;
}
AikiEvent::ReadCompleted(_) => {
    self.execute_handlers(&flow.read_completed, state)?;
}
AikiEvent::WritePermissionAsked(_) => {
    self.execute_handlers(&flow.write_permission_asked, state)?;
}
AikiEvent::WriteCompleted(_) => {
    self.execute_handlers(&flow.write_completed, state)?;
}
AikiEvent::DeletePermissionAsked(_) => {
    self.execute_handlers(&flow.delete_permission_asked, state)?;
}
AikiEvent::DeleteCompleted(_) => {
    self.execute_handlers(&flow.delete_completed, state)?;
}
```

### Phase 4: Update Vendors

**Files:**
- `src/vendors/claude_code/events.rs`
- `src/vendors/cursor/events.rs`

Use existing `file_operation()` method to construct the appropriate event variant:

```rust
// In vendor event construction
use crate::tools::FileOperation;

match tool.file_operation() {
    Some(FileOperation::Write) => {
        AikiEvent::WriteCompleted(AikiWriteCompletedPayload { 
            session: session.clone(),
            cwd: cwd.clone(),
            timestamp: Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: extract_file_paths(&tool),
            success: Some(true),
            edit_details: extract_edit_details(&tool),
        })
    }
    Some(FileOperation::Read) => {
        AikiEvent::ReadCompleted(AikiReadCompletedPayload { 
            session: session.clone(),
            cwd: cwd.clone(),
            timestamp: Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: extract_file_paths(&tool),
            success: Some(true),
        })
    }
    Some(FileOperation::Delete) => {
        AikiEvent::DeleteCompleted(AikiDeleteCompletedPayload { 
            session: session.clone(),
            cwd: cwd.clone(),
            timestamp: Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: paths.clone(),
            success: Some(true),
        })
    }
    None => return Ok(None), // Not a file operation
}
```

This keeps the tool knowledge centralized while vendors just use it to pick the right event variant.

### Phase 5: Update Core Functions

**File:** `src/flows/core/functions.rs`

Update function signatures to accept operation-specific payloads. These functions currently accept `&AikiFileCompletedPayload` but only make sense for write operations:

**Functions to update:**
1. `build_metadata` - Only for writes (generates provenance metadata)
2. `classify_edits` - Only for writes (compares AI edits vs file state)
3. `prepare_separation` - Only for writes (reconstructs AI-only content)
4. `write_ai_files` - Only for writes (writes reconstructed content)
5. `restore_original_files` - Only for writes (restores after split)
6. `separate_edits` - Only for writes (splits AI from user changes)

**New signatures:**
```rust
pub fn build_metadata(
    event: &AikiWriteCompletedPayload,  // Changed from AikiFileCompletedPayload
    context: Option<&AikiState>,
) -> Result<ActionResult>

pub fn classify_edits(
    event: &AikiWriteCompletedPayload,  // Changed
) -> Result<ActionResult>

pub fn prepare_separation(
    event: &AikiWriteCompletedPayload,  // Changed
) -> Result<ActionResult>

pub fn write_ai_files(
    event: &AikiWriteCompletedPayload,  // Changed (needed for cwd and edit_details)
    context: Option<&AikiState>,
) -> Result<ActionResult>

pub fn restore_original_files(
    event: &AikiWriteCompletedPayload,  // Changed (needed for cwd)
    context: Option<&AikiState>,
) -> Result<ActionResult>

pub fn separate_edits(
    event: &AikiWriteCompletedPayload,  // Changed
) -> Result<ActionResult>
```

**Functions for permission_asked (both read and write):**
```rust
// These accept the permission_asked payloads
pub fn build_human_metadata(
    event: &AikiWritePermissionAskedPayload,  // For write.permission_asked
    context: Option<&AikiState>,
) -> Result<ActionResult>

// Or create a version that works with both:
pub fn build_human_metadata_write(
    event: &AikiWritePermissionAskedPayload,
    context: Option<&AikiState>,
) -> Result<ActionResult>
```

**Note on delete operations:**
Delete provenance tracking is intentionally minimal in this refactor. Full delete provenance (capturing previous authorship, file lineage, etc.) is tracked in a separate plan: `ops/current/delete-provenance.md`.

For this refactor, delete operations will simply create a new change without metadata, keeping the implementation simple.

### Phase 6: Update Flow Engine

**File:** `src/flows/engine.rs`

Update function dispatch to match on the correct event types:

```rust
("core", "build_metadata") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "build_metadata can only be called for write.completed events"
        )));
    };
    crate::flows::core::build_metadata(event, Some(state))
}

("core", "classify_edits") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "classify_edits can only be called for write.completed events"
        )));
    };
    crate::flows::core::classify_edits(event)
}

("core", "prepare_separation") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "prepare_separation can only be called for write.completed events"
        )));
    };
    crate::flows::core::prepare_separation(event)
}

("core", "write_ai_files") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "write_ai_files can only be called for write.completed events"
        )));
    };
    crate::flows::core::write_ai_files(event, Some(state))
}

("core", "restore_original_files") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "restore_original_files can only be called for write.completed events"
        )));
    };
    crate::flows::core::restore_original_files(event, Some(state))
}

("core", "separate_edits") => {
    let AikiEvent::WriteCompleted(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "separate_edits can only be called for write.completed events"
        )));
    };
    crate::flows::core::separate_edits(event)
}

("core", "build_human_metadata") => {
    let AikiEvent::WritePermissionAsked(event) = &state.event else {
        return Err(AikiError::Other(anyhow::anyhow!(
            "build_human_metadata can only be called for write.permission_asked events"
        )));
    };
    crate::flows::core::build_human_metadata(event, Some(state))
}
```

### Phase 7: Update Core Flow YAML

**File:** `src/flows/core/flow.yaml`

Simplify from:
```yaml
file.permission_asked:
  - if: $event.operation == "write"
    then:
      - jj: diff -r @ --name-only
        alias: changed_files
      - if: $changed_files
        then:
          - with_author_and_message: self.build_human_metadata
            jj: describe --message "$message"
          - jj: new

file.completed:
  - if: $event.operation == "write"
    then:
      - if: self.classify_edits == "AdditiveUserEdits"
        # ... nested logic
```

To:
```yaml
# Write operations - stash user changes before AI edits
write.permission_asked:
  - jj: diff -r @ --name-only
    alias: changed_files
  - if: $changed_files
    then:
      - with_author_and_message: self.build_human_metadata
        jj: describe --message "$message"
      - jj: new

# Write operations - record provenance after AI edits
write.completed:
  - if: self.classify_edits == "AdditiveUserEdits"
    then:
      # User added more changes - separate them
      - let: prep = self.prepare_separation
      - self: write_ai_files
      - with_author: $prep.ai_author
        jj: split --message "$prep.ai_message" $prep.file_list
      - self: restore_original_files
      - with_author_and_message: self.build_human_metadata
        jj: metaedit --message "$message"
    else:
      # ExactMatch or OverlappingUserEdits - set AI metadata
      - let: metadata = self.build_metadata
        on_failure:
          - stop: "Failed to build metadata"
      - with_author_and_message: $metadata
        jj: metaedit --message "$message"
  - jj: new

# Delete operations - basic tracking (see delete-provenance.md for full plan)
delete.completed:
  # For now, just create a new change to separate delete from other operations
  # Full provenance tracking for deletes is tracked in separate plan
  - jj: new

# Read operations - no provenance needed (reads don't modify repo)
read.permission_asked:
  # Empty - could gate sensitive file reads in the future

read.completed:
  # Empty - reads don't need provenance tracking
```

### Phase 8: Update Variable Resolution in Event Engine

**File:** `src/flows/engine.rs`

Update the existing variable resolution match statement to handle new event types. The current implementation uses `resolver.add_var()` calls in a match statement - we just need to replace the old `FilePermissionAsked` and `FileCompleted` arms with operation-specific ones.

**Remove these old arms:**
```rust
crate::events::AikiEvent::FilePermissionAsked(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.operation", e.operation.to_string());  // ← Remove this
    if let Some(ref path) = e.path {
        resolver.add_var("event.path", path.clone());
    }
    if let Some(ref pattern) = e.pattern {
        resolver.add_var("event.pattern", pattern.clone());
    }
}

crate::events::AikiEvent::FileCompleted(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.operation", e.operation.to_string());  // ← Remove this
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
    resolver.add_var("event.file_count", e.file_paths.len().to_string());
    if let Some(success) = e.success {
        resolver.add_var("event.success", success.to_string());
    }
}
```

**Add these new arms:**
```rust
crate::events::AikiEvent::ReadPermissionAsked(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
}

crate::events::AikiEvent::ReadCompleted(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
    resolver.add_var("event.file_count", e.file_paths.len().to_string());
    if let Some(success) = e.success {
        resolver.add_var("event.success", success.to_string());
    }
}

crate::events::AikiEvent::WritePermissionAsked(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
}

crate::events::AikiEvent::WriteCompleted(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
    resolver.add_var("event.file_count", e.file_paths.len().to_string());
    if let Some(success) = e.success {
        resolver.add_var("event.success", success.to_string());
    }
    // Note: edit_details are available in the payload but not exposed as variables
    // Flows should use self.classify_edits to analyze edits instead
}

crate::events::AikiEvent::DeletePermissionAsked(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
}

crate::events::AikiEvent::DeleteCompleted(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.file_paths", e.file_paths.join(" "));
    resolver.add_var("event.file_count", e.file_paths.len().to_string());
    if let Some(success) = e.success {
        resolver.add_var("event.success", success.to_string());
    }
}
```

**Key changes:**
1. **No more `$event.operation` variable** - The operation type is now implicit in the event name
2. **Consistent field set** - All operation types expose the same core variables (session_id, tool_name, file_paths, success)
3. **Simplified permission_asked events** - No longer need to check operation type conditionally

### Phase 9: Remove Old file.* Events

Once the new events are working:

1. **Remove event variants from `AikiEvent` enum:**
   - Remove `FilePermissionAsked` variant
   - Remove `FileCompleted` variant

2. **Delete payload files:**
   - Delete `src/events/file_permission_asked.rs`
   - Delete `src/events/file_completed.rs`

3. **Remove from Flow type:**
   - Remove `file_permission_asked: Vec<FlowStatement>` field
   - Remove `file_completed: Vec<FlowStatement>` field

4. **Keep `FileOperation` enum:**
   - **DO NOT DELETE** `FileOperation` from `src/tools.rs`
   - It's still needed for:
     - Internal tool classification (`ClaudeTool::file_operation()`)
     - Shell command parsing (`parse_file_operation_from_shell_command()`)
   - Remove only from event payload structures (no longer a field)

5. **Update tests:**
   - Update any tests using old `file.*` events
   - Add tests for new operation-specific events

6. **Update documentation:**
   - CLAUDE.md event naming conventions
   - Flow YAML examples
   - Migration guide for custom flows

## Migration Notes

### Backwards Compatibility

This is a **breaking change** for:
- Custom flow YAML files using `file.completed` or `file.permission_asked`
- Any code directly constructing `AikiFileCompletedPayload`

### Migration Path for Custom Flows

**Old flow:**
```yaml
file.completed:
  - if: $event.operation == "write"
    then:
      - log: "File written: $event.file_paths"
  - if: $event.operation == "read"
    then:
      - log: "File read: $event.file_paths"
```

**New flow:**
```yaml
write.completed:
  - log: "File written: $event.file_paths"

read.completed:
  - log: "File read: $event.file_paths"
```

### Documentation Updates

- Update CLAUDE.md event naming conventions
- Update any flow YAML examples
- Add migration guide in ops/migrations/
- Update ACP protocol documentation if applicable

## Testing Checklist

### Unit Tests
- [ ] All existing tests pass after refactor
- [ ] Event payload serialization/deserialization for all 6 new events
- [ ] `FileOperation` enum still works for tool classification
- [ ] Shell command parsing still detects delete operations

### Integration Tests
- [ ] `write.completed` handler runs only for write operations
- [ ] `read.completed` handler runs only for read operations
- [ ] `delete.completed` handler runs only for delete operations
- [ ] Core functions work with new payload types
- [ ] Variable resolution works for all new event types
- [ ] Bundled core flow works correctly

### Vendor Tests
- [ ] Claude Code vendor emits correct events
  - [ ] Edit tool → `write.completed`
  - [ ] Read tool → `read.completed`
  - [ ] Bash with `rm` → `delete.completed`
- [ ] Cursor vendor emits correct events

### End-to-End Tests
- [ ] Full session with read/write/delete operations
- [ ] User edits during AI session are properly separated
- [ ] Provenance metadata is correctly recorded for all operation types
- [ ] Git commit includes correct co-authors

### Error Handling Tests
- [ ] Calling `build_metadata` on non-write event fails with clear error
- [ ] Flow YAML parsing fails gracefully for old `file.*` events (after Phase 9)
- [ ] Missing event handlers (e.g., no `delete.completed` defined) don't crash

## Design Decisions

### 1. No Backwards Compatibility Aliases

Clean break, no `file.*` aliases. This simplifies the codebase and makes the event model clearer.

**Rationale:** Supporting both old and new event names would complicate the code and make migration unclear. Better to have a clear cutover with good documentation.

### 2. One Event Per Tool Call

Each tool call emits one event based on its primary operation:
- **Edit tool** → `write.completed` (even though it reads first to find old_string)
- **Read tool** → `read.completed`
- **Bash with `rm`** → `delete.completed`

**Rationale:** The semantic operation is what matters. Edit's purpose is to modify a file (write), so it emits a write event. The fact that it reads first is an implementation detail.

**Example:** `Edit` tool workflow:
1. Reads file to find `old_string` (implementation detail)
2. Replaces with `new_string` (semantic operation: write)
3. Emits `write.completed` event (based on semantic operation)

### 3. Keep All permission_asked Events

Including `read.permission_asked` and `delete.permission_asked` for consistency and future use cases:
- **read.permission_asked** - Could block reads of sensitive files (secrets, .env, etc.)
- **delete.permission_asked** - Could block deletion of important files
- **write.permission_asked** - Current use: stash user changes before AI edits

**Rationale:** Even if some handlers are empty now, having the event structure allows users to add custom gating logic without refactoring the event model.

### 4. Keep FileOperation Enum (Internal Use Only)

The `FileOperation` enum stays but is no longer in event payloads:
- **Internal classification:** Tool → operation mapping (`ClaudeTool::file_operation()`)
- **Shell parsing:** Detecting `rm`/`rmdir` commands
- **NOT in payloads:** Operation type is implicit in event name

**Rationale:** The enum is useful for centralizing the "what operation is this tool?" logic, even though we don't need it in the event payload anymore.

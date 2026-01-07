# Unified Change Event Model

## Motivation

Align the event model with jj's "change" concept:
- Every file mutation (write, delete, move) creates a jj change
- Reads are fundamentally different - they don't modify the repository
- The current `write.*` / `delete.*` split forces duplication when you want "handle any mutation"

### Why Unifying Mutations Isn't Problematic

**Concern:** "I need different logic for writes vs deletes"
**Answer:** You still can. Computed properties + the `?` truthiness operator let you branch when needed:

```yaml
change.completed:
  - if: $event.write?     # true if $event.write is "true"
    then: [write-specific logic...]
  - if: $event.delete?    # true if $event.delete is "true"
    then: [delete-specific logic...]
```

**Concern:** "My flow only handles writes - now I'll accidentally run on deletes"
**Answer:** Guard with `$event.write?`. But in practice, most mutation handling is identical (stash, provenance, `jj new`). Operation-specific logic is the exception, not the rule.

**Concern:** "Deletes are destructive - I want separate gating"
**Answer:** Use `$event.delete?` in `change.permission_asked` to add delete-specific gates. The unified event doesn't remove granularity.

The key insight: **combining makes differentiation opt-in rather than forced**. You don't lose the ability to distinguish operations - you just stop being forced to duplicate common logic across `write.*` / `delete.*` handlers.

### Why Reads Stay Separate

Reads are fundamentally different from mutations:

| Aspect | Mutations (write/delete/move) | Reads |
|--------|------------------------------|-------|
| Creates jj change? | Yes | No |
| Needs provenance? | Yes (who changed what) | No (nothing changed) |
| Pre-event purpose | Stash user work | Gate sensitive files |
| Post-event purpose | Record metadata, `jj new` | Logging only |

If reads were unified into `change.*`, every handler would need guards:
```yaml
change.completed:
  - if: $event.read?
    then: []  # Skip everything for reads
  - else:
    then: [actual mutation logic...]
```

This is worse than having `read.*` separate. The split reflects a real semantic boundary: observations vs. mutations.

## Design Goals

1. **JJ alignment** - Events map to jj's change concept
2. **Reads are separate** - `read.*` stays distinct (no jj change created)
3. **Mutations unified** - `change.*` covers write/delete/move
4. **Operation-specific logic** - Use `$event.write?`, `$event.delete?`, `$event.move?` for branching
5. **Move as first-class** - Single event with source + destination paths
6. **General truthiness** - `?` suffix works on any variable, not just operation checks

## Event Model

### Before (current)
```
read.permission_asked / read.completed
write.permission_asked / write.completed
delete.permission_asked / delete.completed
```

### After (proposed)
```
read.permission_asked / read.completed      # Unchanged
change.permission_asked / change.completed  # Unified mutations
```

## Type Definitions

### ChangeOperation Enum (Tagged Union)

```rust
// In src/events/mod.rs

/// The type of file mutation - each variant contains operation-specific data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "lowercase")]
pub enum ChangeOperation {
    /// File content created or modified
    Write(WriteOperation),
    /// File removed
    Delete(DeleteOperation),
    /// File relocated (source deleted, destination created)
    Move(MoveOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteOperation {
    /// Files that were created or modified
    pub file_paths: Vec<String>,
    /// Edit details (old_string -> new_string) for permission_asked and completed events
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edit_details: Vec<EditDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteOperation {
    /// Files that were removed
    pub file_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveOperation {
    /// Original file paths before the move
    pub source_paths: Vec<String>,
    /// New file paths after the move
    pub destination_paths: Vec<String>,
}

impl ChangeOperation {
    /// Get the operation name as a string
    pub fn operation_name(&self) -> &str {
        match self {
            Self::Write(_) => "write",
            Self::Delete(_) => "delete",
            Self::Move(_) => "move",
        }
    }

    /// Computed property: returns "true" if this is a Write operation, "" otherwise
    /// Enables `$event.write?` via general truthiness check
    pub fn is_write(&self) -> &str {
        match self {
            Self::Write(_) => "true",
            _ => "",
        }
    }

    /// Computed property: returns "true" if this is a Delete operation, "" otherwise
    pub fn is_delete(&self) -> &str {
        match self {
            Self::Delete(_) => "true",
            _ => "",
        }
    }

    /// Computed property: returns "true" if this is a Move operation, "" otherwise
    pub fn is_move(&self) -> &str {
        match self {
            Self::Move(_) => "true",
            _ => "",
        }
    }

    /// Get all file paths affected by this operation (for unified access)
    pub fn file_paths(&self) -> Vec<String> {
        match self {
            Self::Write(op) => op.file_paths.clone(),
            Self::Delete(op) => op.file_paths.clone(),
            Self::Move(op) => op.destination_paths.clone(),
        }
    }
}
```

### JSON Serialization Examples

With `#[serde(flatten)]`, the operation data is merged into the parent struct:

**Write operation:**
```json
{
  "event": "change.completed",
  "session": { "external_id": "session-123", "agent_type": "claude-code" },
  "cwd": "/path/to/project",
  "timestamp": "2025-12-17T10:30:00Z",
  "tool_name": "Edit",
  "success": true,
  "operation": "write",
  "file_paths": ["src/main.rs", "src/lib.rs"],
  "edit_details": [
    { "old_string": "foo", "new_string": "bar" }
  ]
}
```

**Delete operation:**
```json
{
  "event": "change.completed",
  "session": { "external_id": "session-123", "agent_type": "claude-code" },
  "cwd": "/path/to/project",
  "timestamp": "2025-12-17T10:30:00Z",
  "tool_name": "Bash",
  "success": true,
  "operation": "delete",
  "file_paths": ["old_file.txt"]
}
```

**Move operation:**
```json
{
  "event": "change.completed",
  "session": { "external_id": "session-123", "agent_type": "claude-code" },
  "cwd": "/path/to/project",
  "timestamp": "2025-12-17T10:30:00Z",
  "tool_name": "Bash",
  "success": true,
  "operation": "move",
  "file_paths": ["new_location.txt"],
  "source_paths": ["old_location.txt"],
  "destination_paths": ["new_location.txt"]
}
```

### AikiChangePermissionAskedPayload

```rust
// In src/events/change_permission_asked.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiChangePermissionAskedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,

    /// The tool requesting permission (e.g., "Edit", "Write", "Delete", "Move")
    pub tool_name: String,

    /// The specific operation being requested (contains operation-specific fields)
    #[serde(flatten)]
    pub operation: ChangeOperation,
}
```

### AikiChangeCompletedPayload

```rust
// In src/events/change_completed.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiChangeCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,

    /// The tool that made the change (e.g., "Edit", "Write", "Delete", "Move", "Bash")
    pub tool_name: String,

    /// Whether the operation succeeded
    pub success: bool,

    /// The specific operation that occurred (contains operation-specific fields)
    #[serde(flatten)]
    pub operation: ChangeOperation,
}
```

### AikiEvent Enum Update

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    // ... session events ...

    // Read operations (unchanged)
    #[serde(rename = "read.permission_asked")]
    ReadPermissionAsked(AikiReadPermissionAskedPayload),
    #[serde(rename = "read.completed")]
    ReadCompleted(AikiReadCompletedPayload),

    // Change operations (NEW - replaces write.* and delete.*)
    #[serde(rename = "change.permission_asked")]
    ChangePermissionAsked(AikiChangePermissionAskedPayload),
    #[serde(rename = "change.completed")]
    ChangeCompleted(AikiChangeCompletedPayload),

    // ... shell, web, mcp events ...
}
```

## Flow Type Update

### Before
```rust
pub struct Flow {
    // ...
    pub read_permission_asked: Vec<FlowStatement>,
    pub read_completed: Vec<FlowStatement>,
    pub write_permission_asked: Vec<FlowStatement>,
    pub write_completed: Vec<FlowStatement>,
    pub delete_permission_asked: Vec<FlowStatement>,
    pub delete_completed: Vec<FlowStatement>,
    // ...
}
```

### After
```rust
pub struct Flow {
    // ...
    #[serde(rename = "read.permission_asked", default)]
    pub read_permission_asked: Vec<FlowStatement>,
    #[serde(rename = "read.completed", default)]
    pub read_completed: Vec<FlowStatement>,

    #[serde(rename = "change.permission_asked", default)]
    pub change_permission_asked: Vec<FlowStatement>,
    #[serde(rename = "change.completed", default)]
    pub change_completed: Vec<FlowStatement>,
    // ...
}
```

## Variable Truthiness (`?` suffix)

The `?` suffix is a **general language feature** for truthiness checks on any variable:

```rust
// In resolver.rs
fn resolve(&self, var: &str) -> Option<String> {
    if let Some(base) = var.strip_suffix('?') {
        let value = self.resolve_inner(base).unwrap_or_default();
        let truthy = !value.is_empty() && value != "false";
        return Some(truthy.to_string());
    }
    self.resolve_inner(var)
}
```

**Rules:**
- `$var?` returns `"true"` if `$var` is non-empty AND not `"false"`
- `$var?` returns `"false"` otherwise

**Examples:**
```yaml
# Operation type checks (via computed properties)
- if: $event.write?      # true if $event.write is "true"
- if: $event.delete?     # true if $event.delete is "true"

# Presence checks on any field
- if: $event.edit_details?   # true if edit_details is non-empty
- if: $context.review_result?  # true if review_result is set
- if: $changed_files?        # true if changed_files has content

# Success/failure checks
- if: $event.success?        # true if success is "true"
```

This enables `$event.write?` to work naturally: the computed property `$event.write` returns `"true"` or `""`, and the `?` suffix converts that to a boolean string.

## Variable Resolution

Variables available in `change.*` handlers:

```rust
// In engine.rs create_resolver()

AikiEvent::ChangePermissionAsked(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.operation", e.operation.operation_name());
    resolver.add_var("event.tool_name", e.tool_name.clone());

    // Computed properties for operation type checks
    // These return "true" or "" - use with ? suffix for boolean conditionals
    resolver.add_var("event.write", e.operation.is_write());
    resolver.add_var("event.delete", e.operation.is_delete());
    resolver.add_var("event.move", e.operation.is_move());

    // Add operation-specific variables
    match &e.operation {
        ChangeOperation::Write(op) => {
            resolver.add_var("event.file_paths", op.file_paths.join(" "));
            resolver.add_var("event.file_count", op.file_paths.len().to_string());
            resolver.add_var("event.edit_details", format_edit_details(&op.edit_details));
        }
        ChangeOperation::Delete(op) => {
            resolver.add_var("event.file_paths", op.file_paths.join(" "));
            resolver.add_var("event.file_count", op.file_paths.len().to_string());
        }
        ChangeOperation::Move(op) => {
            // file_paths points to destinations for consistency with Write/Delete
            resolver.add_var("event.file_paths", op.destination_paths.join(" "));
            resolver.add_var("event.source_paths", op.source_paths.join(" "));
            resolver.add_var("event.destination_paths", op.destination_paths.join(" "));
            resolver.add_var("event.file_count", op.destination_paths.len().to_string());
        }
    }
}

AikiEvent::ChangeCompleted(e) => {
    resolver.add_var("event.session_id", e.session.external_id());
    resolver.add_var("event.operation", e.operation.operation_name());
    resolver.add_var("event.tool_name", e.tool_name.clone());
    resolver.add_var("event.success", e.success.to_string());

    // Computed properties for operation type checks
    resolver.add_var("event.write", e.operation.is_write());
    resolver.add_var("event.delete", e.operation.is_delete());
    resolver.add_var("event.move", e.operation.is_move());

    // Add operation-specific variables
    match &e.operation {
        ChangeOperation::Write(op) => {
            resolver.add_var("event.file_paths", op.file_paths.join(" "));
            resolver.add_var("event.file_count", op.file_paths.len().to_string());
            resolver.add_var("event.edit_details", format_edit_details(&op.edit_details));
        }
        ChangeOperation::Delete(op) => {
            resolver.add_var("event.file_paths", op.file_paths.join(" "));
            resolver.add_var("event.file_count", op.file_paths.len().to_string());
        }
        ChangeOperation::Move(op) => {
            // file_paths points to destinations for consistency with Write/Delete
            resolver.add_var("event.file_paths", op.destination_paths.join(" "));
            resolver.add_var("event.source_paths", op.source_paths.join(" "));
            resolver.add_var("event.destination_paths", op.destination_paths.join(" "));
            resolver.add_var("event.file_count", op.destination_paths.len().to_string());
        }
    }
}
```

### Variable Summary

| Variable | Write | Delete | Move | Notes |
|----------|-------|--------|------|-------|
| `$event.operation` | "write" | "delete" | "move" | String name |
| `$event.write` | "true" | "" | "" | Computed property |
| `$event.delete` | "" | "true" | "" | Computed property |
| `$event.move` | "" | "" | "true" | Computed property |
| `$event.file_paths` | written files | deleted files | destination files | Always present |
| `$event.file_count` | count | count | count | |
| `$event.source_paths` | "" | "" | original files | Move only |
| `$event.destination_paths` | "" | "" | new locations | Move only |
| `$event.edit_details` | JSON | "" | "" | Write only |

**Using `?` suffix for conditionals:**
```yaml
- if: $event.write?          # Checks if $event.write is truthy ("true")
- if: $event.edit_details?   # Checks if edit_details is non-empty
- if: $event.source_paths?   # Checks if source_paths is present (Move only)
```

**Design rationale:**
- Computed properties (`$event.write`, `$event.delete`, `$event.move`) return `"true"` or `""`
- The `?` suffix is a general truthiness operator, not special syntax for these fields
- This enables `$event.write?` to work AND enables presence checks like `$event.edit_details?`
- `$event.file_paths` is always available and points to the "result" files:
  - Write: files that were created/modified
  - Delete: files that were removed
  - Move: files at their new locations (destinations)

This enables flows to use `$event.file_paths` generically without checking operation type.

## Core Flow YAML

```yaml
name: "Aiki Core"
description: "Core flow for jj change tracking and provenance"
version: "1"

session.started:
  - jj: new
  - shell: aiki init --quiet
    on_failure:
      - stop: "Failed to initialize Aiki repository"
  - jj: diff -r @ --name-only
    alias: init_changes
  - if: $init_changes
    then:
      - with_author: "Aiki <noreply@aiki.dev>"
        jj: describe --message "[aiki]\nauthor=aiki\nauthor_type=agent\nsession=$event.session_id\n[/aiki]"
      - jj: new

# Stash user changes before ANY mutation (write, delete, or move)
change.permission_asked:
  - jj: diff -r @ --name-only
    alias: changed_files
  - if: $changed_files
    then:
      - with_author_and_message: self.build_human_metadata
        jj: describe --message "$message"
      - jj: new

# Record provenance after mutation completes
change.completed:
  # Write-specific: classify edits and potentially split changes
  - if: $event.write?
    then:
      - if: self.classify_edits == "AdditiveUserEdits"
        then:
          - let: prep = self.prepare_separation
          - self: write_ai_files
          - with_author: $prep.ai_author
            jj: split --message "$prep.ai_message" $prep.file_list
          - self: restore_original_files
          - with_author_and_message: self.build_human_metadata
            jj: metaedit --message "$message"
        else:
          - let: metadata = self.build_write_metadata
            on_failure:
              - stop: "Failed to build metadata"
          - with_author_and_message: $metadata
            jj: metaedit --message "$message"

  # Delete: simpler provenance (no edit classification)
  - if: $event.delete?
    then:
      - let: metadata = self.build_delete_metadata
        on_failure:
          - stop: "Failed to build metadata"
      - with_author_and_message: $metadata
        jj: metaedit --message "$message"

  # Move: record both source and destination paths
  - if: $event.move?
    then:
      - let: metadata = self.build_move_metadata
        on_failure:
          - stop: "Failed to build metadata"
      - with_author_and_message: $metadata
        jj: metaedit --message "$message"

  # Always create new change after any mutation
  - jj: new

# Read operations - no provenance needed
read.permission_asked:
  # Can gate sensitive file reads here

read.completed:
  # Reads don't create jj changes
```

## Implementation Plan

### Phase 1: Create New Event Types

**Files to create:**
- `src/events/change_permission_asked.rs`
- `src/events/change_completed.rs`

**Files to modify:**
- `src/events/mod.rs` - Add `ChangeOperation`, new variants, From impls

**Tasks:**
1. Define `ChangeOperation` enum
2. Create `AikiChangePermissionAskedPayload`
3. Create `AikiChangeCompletedPayload`
4. Add `ChangePermissionAsked` and `ChangeCompleted` to `AikiEvent`
5. Implement `From` traits for payload-to-event conversion
6. Add `cwd()` and `agent_type()` match arms

### Phase 2: Update Flow Type

**File:** `src/flows/types.rs`

**Tasks:**
1. Add `change_permission_asked: Vec<FlowStatement>` field
2. Add `change_completed: Vec<FlowStatement>` field
3. Keep `write_*` and `delete_*` temporarily for migration

### Phase 3: Update Event Bus

**File:** `src/event_bus.rs`

**Tasks:**
1. Add dispatch handlers for `ChangePermissionAsked` and `ChangeCompleted`
2. Update shell-to-delete transformation to emit `ChangeCompleted` with `operation: Delete`

### Phase 4: Update Vendors

**Files:**
- `src/vendors/claude_code/events.rs`
- `src/vendors/cursor/events.rs`

**Tasks:**
1. Update `build_file_permission_asked_event` to emit `ChangePermissionAsked`
2. Update `build_file_completed_event` to emit `ChangeCompleted`
3. Construct operation based on `FileOperation` enum:
   - `FileOperation::Write` → `ChangeOperation::Write(WriteOperation { file_paths, edit_details })`
   - `FileOperation::Delete` → `ChangeOperation::Delete(DeleteOperation { file_paths })`
4. Move operations come from shell commands (Phase 3)

### Phase 5: Update ACP Vendor

**File:** `src/commands/acp.rs`

**Tasks:**
1. Update `record_post_change_events` to emit `ChangeCompleted`
2. Update `fire_pre_file_change_event` to emit `ChangePermissionAsked`
3. Construct operation based on `ToolKind`:
   - `ToolKind::Edit` / `ToolKind::Write` → `ChangeOperation::Write(WriteOperation { file_paths, edit_details })`
   - `ToolKind::Delete` → `ChangeOperation::Delete(DeleteOperation { file_paths })`
   - `ToolKind::Move` → `ChangeOperation::Move(MoveOperation { source_paths, destination_paths })`

### Phase 6: Update Core Functions

**File:** `src/flows/core/functions.rs`

**Approach:** Create separate `build_*_metadata` functions for each operation type instead of one generic function with a large match statement. This provides:
- Single Responsibility - each function handles one operation type
- Type Safety - pattern matching ensures correct operation data
- Clearer Intent - flow YAML is explicit about which builder to use
- Easier Testing - test each operation type independently
- Better Error Messages - wrong builder gives clear error

**Tasks:**
1. Create `build_write_metadata(&AikiChangeCompletedPayload) -> Result<ActionResult>`:
   - Validate operation is `Write` using pattern matching
   - Generate provenance with write-specific fields (edit_details, file_paths)
   - Return JSON with `{"author": "...", "message": "..."}`
   
2. Create `build_delete_metadata(&AikiChangeCompletedPayload) -> Result<ActionResult>`:
   - Validate operation is `Delete`
   - Generate provenance with delete-specific fields (file_paths only, no edits)
   - Return JSON with `{"author": "...", "message": "..."}`
   
3. Create `build_move_metadata(&AikiChangeCompletedPayload) -> Result<ActionResult>`:
   - Validate operation is `Move`
   - Generate provenance with move-specific fields (source_paths, destination_paths)
   - Return JSON with `{"author": "...", "message": "..."}`

4. Update `classify_edits` to accept `&AikiChangeCompletedPayload`:
   - Add validation that operation is `Write` (error if called on delete/move)
   - Extract `WriteOperation` data for edit classification

5. Update `prepare_separation` to accept `&AikiChangeCompletedPayload`:
   - Add validation that operation is `Write`
   - Extract `WriteOperation` data for separation logic

6. Update `write_ai_files` to accept `&AikiChangeCompletedPayload`:
   - Add validation that operation is `Write`
   - Extract `WriteOperation` data

7. Update `restore_original_files` to accept `&AikiChangeCompletedPayload`:
   - Add validation that operation is `Write`
   - Extract `WriteOperation` data

8. Update `build_human_metadata` to accept `&AikiChangePermissionAskedPayload`:
   - Works for any operation type (doesn't need operation-specific logic)

### Phase 7: Update Flow Engine and Variable Resolver

**Files:** `src/flows/engine.rs`, `src/flows/resolver.rs`

**Tasks:**
1. Add variable resolution for `ChangePermissionAsked` and `ChangeCompleted`
2. Update function dispatch to extract `ChangeCompleted` payloads
3. Add operation validation (e.g., error if `classify_edits` called on delete)
4. Implement general `?` suffix truthiness in resolver:
   ```rust
   fn resolve(&self, var: &str) -> Option<String> {
       if let Some(base) = var.strip_suffix('?') {
           let value = self.resolve_inner(base).unwrap_or_default();
           let truthy = !value.is_empty() && value != "false";
           return Some(truthy.to_string());
       }
       self.resolve_inner(var)
   }
   ```
5. Add computed properties for operation type checks (`event.write`, `event.delete`, `event.move`)

### Phase 8: Update Core Flow YAML

**File:** `src/flows/core/flow.yaml`

**Tasks:**
1. Replace `write.permission_asked` with `change.permission_asked`
2. Replace `write.completed` with `change.completed`
3. Add `$event.operation` conditionals for write-specific logic
4. Remove `delete.permission_asked` and `delete.completed` handlers

### Phase 9: Remove Deprecated Events

**Files to delete:**
- `src/events/write_permission_asked.rs`
- `src/events/write_completed.rs`
- `src/events/delete_permission_asked.rs`
- `src/events/delete_completed.rs`

**Files to modify:**
- `src/events/mod.rs` - Remove old variants, imports, From impls
- `src/flows/types.rs` - Remove old handler fields
- `src/event_bus.rs` - Remove old dispatch handlers

### Phase 10: Update Tests

**Tasks:**
1. Update all tests using `WriteCompleted` to use `ChangeCompleted`
2. Update all tests using `DeleteCompleted` to use `ChangeCompleted`
3. Add tests for `ChangeOperation` serialization
4. Add tests for Move operations with `source_paths`
5. Update YAML parsing tests

## Move Operation Handling

### From Claude Code / Cursor (hooks)

Operations come through shell commands or tool calls:

```rust
// In event_bus.rs
AikiEvent::ShellCompleted(e) => {
    let (file_op, paths) = parse_file_operation_from_shell_command(&e.command);
    
    let operation = match file_op {
        Some(FileOperation::Delete) => {
            ChangeOperation::Delete(DeleteOperation {
                file_paths: paths,
            })
        }
        Some(FileOperation::Move) => {
            let (sources, destinations) = parse_move_paths(&e.command);
            ChangeOperation::Move(MoveOperation {
                source_paths: sources,
                destination_paths: destinations,
            })
        }
        _ => return Ok(()), // Not a file operation
    };
    
    return dispatch(AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
        session: e.session,
        cwd: e.cwd,
        timestamp: e.timestamp,
        tool_name: "Bash".to_string(),
        success: e.success,
        operation,
    }));
}
```

### From Vendors (Edit/Write/Delete tools)

```rust
// In vendors/claude_code/events.rs or vendors/cursor/events.rs
pub fn build_file_completed_event(/* ... */) -> AikiEvent {
    let operation = match file_operation {
        FileOperation::Write => {
            ChangeOperation::Write(WriteOperation {
                file_paths,
                edit_details,
            })
        }
        FileOperation::Delete => {
            ChangeOperation::Delete(DeleteOperation {
                file_paths,
            })
        }
    };
    
    AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
        session,
        cwd,
        timestamp: Utc::now(),
        tool_name,
        success: true,
        operation,
    })
}
```

### From ACP

```rust
// In acp.rs
match tool_kind {
    ToolKind::Edit | ToolKind::Write => {
        let operation = ChangeOperation::Write(WriteOperation {
            file_paths: context.paths,
            edit_details: extract_edit_details(&context),
        });
        
        dispatch(AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
            session,
            cwd: working_dir,
            timestamp: Utc::now(),
            tool_name: tool_kind.to_string(),
            success: true,
            operation,
        }))?;
    }
    ToolKind::Delete => {
        let operation = ChangeOperation::Delete(DeleteOperation {
            file_paths: context.paths,
        });
        
        dispatch(AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
            session,
            cwd: working_dir,
            timestamp: Utc::now(),
            tool_name: "Delete".to_string(),
            success: true,
            operation,
        }))?;
    }
    ToolKind::Move => {
        let (sources, destinations) = split_move_paths(&context.paths);
        let operation = ChangeOperation::Move(MoveOperation {
            source_paths: sources,
            destination_paths: destinations,
        });
        
        dispatch(AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
            session,
            cwd: working_dir,
            timestamp: Utc::now(),
            tool_name: "Move".to_string(),
            success: true,
            operation,
        }))?;
    }
}
```

## Testing Checklist

### Unit Tests
- [ ] `ChangeOperation` serializes to lowercase strings
- [ ] `ChangeCompleted` with `operation: Write` includes edit_details
- [ ] `ChangeCompleted` with `operation: Delete` has empty edit_details
- [ ] `ChangeCompleted` with `operation: Move` has source_paths populated
- [ ] Variable resolution exposes `$event.operation`
- [ ] Variable resolution exposes `$event.file_paths` for all operations (Write, Delete, Move)
- [ ] Move `$event.file_paths` equals `$event.destination_paths`

### Computed Property Tests
- [ ] `$event.write` returns "true" for Write, "" for Delete/Move
- [ ] `$event.delete` returns "true" for Delete, "" for Write/Move
- [ ] `$event.move` returns "true" for Move, "" for Write/Delete

### General Truthiness (`?` suffix) Tests
- [ ] `$var?` returns "true" when `$var` is non-empty string
- [ ] `$var?` returns "false" when `$var` is empty string
- [ ] `$var?` returns "false" when `$var` is literal "false"
- [ ] `$var?` returns "true" when `$var` is literal "true"
- [ ] `$event.write?` returns "true" for Write operations (via computed property)
- [ ] `$event.edit_details?` returns "true" when edit_details is non-empty
- [ ] `$event.source_paths?` returns "false" for Write/Delete (empty string)
- [ ] `$undefined_var?` returns "false" (missing variable treated as empty)

### Integration Tests
- [ ] `change.permission_asked` fires before any mutation
- [ ] `change.completed` fires after write operations
- [ ] `change.completed` fires after delete operations
- [ ] `change.completed` fires after move operations
- [ ] Flow conditionals on `$event.operation` work correctly
- [ ] Core functions work with unified payload

### Vendor Tests
- [ ] Claude Code Edit → `ChangeCompleted { operation: Write }`
- [ ] Claude Code Bash `rm` → `ChangeCompleted { operation: Delete }`
- [ ] Claude Code Bash `mv` → `ChangeCompleted { operation: Move }`
- [ ] ACP Edit → `ChangeCompleted { operation: Write }`
- [ ] ACP Delete → `ChangeCompleted { operation: Delete }`
- [ ] ACP Move → `ChangeCompleted { operation: Move }`

### Move-Specific Tests
- [ ] Move with single file: `mv a.txt b.txt`
- [ ] Move with directory destination: `mv file.txt dir/`
- [ ] Move detection fails gracefully for `mv *.txt dir/` (complex glob)
- [ ] `$event.source_paths` and `$event.destination_paths` have matching counts
- [ ] Move provenance records both source and destination in [aiki] block

## Migration Notes

### Breaking Changes

1. **Flow YAML** - `write.*` and `delete.*` handlers no longer work
2. **Custom flows** - Must migrate to `change.*` handlers
3. **Payload types** - `AikiWriteCompletedPayload` and `AikiDeleteCompletedPayload` removed

### Migration Path

**Old flow:**
```yaml
write.completed:
  - log: "File written"

delete.completed:
  - log: "File deleted"
```

**New flow (using computed properties + `?` truthiness):**
```yaml
change.completed:
  - if: $event.write?      # $event.write is "true" → $event.write? is "true"
    then:
      - log: "File written"
  - if: $event.delete?     # $event.delete is "true" → $event.delete? is "true"
    then:
      - log: "File deleted"
```

**Or using string comparison:**
```yaml
change.completed:
  - if: $event.operation == "write"
    then:
      - log: "File written"
```

**Or check for presence of operation-specific data:**
```yaml
change.completed:
  - if: $event.edit_details?   # Non-empty only for writes
    then:
      - log: "Write with $event.edit_count edits"
  - if: $event.source_paths?   # Non-empty only for moves
    then:
      - log: "Moved from $event.source_paths"
```

**Or if same behavior for all mutations:**
```yaml
change.completed:
  - log: "File changed ($event.operation)"
```

## Open Questions

1. ~~**Provenance for Move**~~ - **Resolved:** Treat move as a single atomic operation. Avoid splitting into delete+write - that loses the semantic meaning of "this file was relocated."
   ```
   [aiki]
   operation=move
   source_files=old/file.txt
   destination_files=new/file.txt
   tool=Bash
   [/aiki]
   ```

2. ~~**`build_metadata` for non-writes**~~ - **Resolved:** Update function to be operation-aware (see Phase 6).

3. **Shell move detection** - Need to implement `parse_move_paths()` for `mv` command parsing. Similar to existing delete detection.

## Definition of Done

- [ ] `ChangeOperation` enum with Write, Delete, Move
- [ ] `ChangeOperation` computed properties: `is_write()`, `is_delete()`, `is_move()`
- [ ] `AikiChangePermissionAskedPayload` with operation, source_paths
- [ ] `AikiChangeCompletedPayload` with operation, source_paths, edit_details
- [ ] Flow type has `change_permission_asked` and `change_completed`
- [ ] All vendors emit `change.*` events
- [ ] Core flow uses `change.*` handlers
- [ ] Variable resolution exposes `$event.operation`, `$event.write`, `$event.delete`, `$event.move`
- [ ] General `?` suffix truthiness implemented in resolver
- [ ] Old `write.*` and `delete.*` events removed
- [ ] All tests pass
- [ ] Documentation updated

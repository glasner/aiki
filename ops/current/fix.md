# Fix: ACP Vendor Operation-Specific Events

## Dependencies

We use the `agent_client_protocol` crate (v0.7) which provides:
- `ToolKind` enum: `Edit`, `Delete`, `Move`, `Read`, `Other`
- `ToolCall` struct: full tool call with `kind`, `locations`, `content`, `status`
- `ToolCallUpdate` struct: incremental updates with optional fields
- `ToolCallLocation` struct: `path: PathBuf`, `line: Option<...>`

Our wrapper at `cli/src/acp/protocol.rs` re-exports `SessionNotification` and provides `JsonRpcMessage`.

## Problem Statement

The ACP vendor (`cli/src/commands/acp.rs`) always emits `write.*` events regardless of the actual tool operation:

1. **`record_post_change_events()`** (line 1217): Checks for `Edit`, `Delete`, and `Move` tool kinds, but always creates `AikiEvent::WriteCompleted`.

2. **`fire_pre_file_change_event()`** (line 1669): Always creates `AikiEvent::WritePermissionAsked` without tool kind information.

This means delete operations never surface `delete.*` events, and Move operations don't track source file deletion.

## ACP Protocol Data Flow Analysis

Understanding when data becomes available:

### Permission Request (`session/request_permission`)
**Available:**
- `sessionId`
- `toolCallId`
- `kind` or `toolKind` (edit/delete/move) - via params, checked by `is_file_modifying_permission_request()`

**NOT Available:**
- File paths (arrive later via `ToolCall`/`ToolCallUpdate`)

### Tool Call (`ToolCall` via `session/update`)
**Available:**
- `kind: ToolKind` - the operation type
- `locations: Vec<ToolCallLocation>` - file paths with `path: PathBuf`
- `title: String` - human-readable description (e.g., "Reading file", "Edit /path/to/file")
- `content: Vec<ToolCallContent>` - diff content for edits
- `status: ToolCallStatus`

### Tool Call Update (`ToolCallUpdate` via `session/update`)
**Available (incrementally):**
- `fields.kind: Option<ToolKind>`
- `fields.locations: Option<Vec<ToolCallLocation>>`
- `fields.content: Option<Vec<ToolCallContent>>`
- `fields.status: Option<ToolCallStatus>`
- `fields.title: Option<String>`

### Key Constraints

1. **Pre-events cannot have file paths** - Permission requests arrive before tool call details populate `ToolCallContext`.

2. **Post-events have full context** - By completion, `ToolCallContext` has kind, paths, and content.

3. **Move operations have multiple paths** - `locations` contains both source and destination.

## Implementation Plan

### Phase 1: Fix Tool Name Handling

**Current (wrong):**
```rust
let tool_name = format!("{:?}", context.kind); // "Edit", "Delete", "Move"
```

**Better approach:** Use the `ToolKind` enum's canonical names, preserving what the agent actually invoked:

```rust
fn tool_kind_to_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Edit => "Edit",
        ToolKind::Delete => "Delete",
        ToolKind::Move => "Move",
        ToolKind::Read => "Read",
        ToolKind::Other => "Other",
    }
}
```

This is explicit rather than relying on Debug formatting.

### Phase 2: Update `record_post_change_events()` for Operation-Specific Events

**File:** `cli/src/commands/acp.rs` (line 1217)

```rust
fn record_post_change_events(
    session_id: &str,
    agent_type: &AgentType,
    client_info: &Option<ClientInfo>,
    agent_info: &Option<AgentInfo>,
    cwd: &Option<PathBuf>,
    context: ToolCallContext,
) -> Result<()> {
    if context.paths.is_empty() {
        return Ok(());
    }

    let working_dir = cwd
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    let agent_version = agent_info.as_ref().and_then(|a| a.version.as_deref());
    let session = create_session(*agent_type, session_id.to_string(), agent_version)
        .with_client_info(
            client_info.as_ref().map(|c| c.name.as_str()),
            client_info.as_ref().and_then(|c| c.version.as_deref()),
        );

    let tool_name = tool_kind_to_name(context.kind);
    let file_paths: Vec<String> = context.paths.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    match context.kind {
        ToolKind::Edit => {
            let edit_details = extract_edit_details(&context);
            let event = AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths,
                success: true,
                edit_details,
            });
            dispatch_event(event)?;
        }

        ToolKind::Delete => {
            let event = AikiEvent::DeleteCompleted(AikiDeleteCompletedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths,
                success: Some(true),
            });
            dispatch_event(event)?;
        }

        ToolKind::Move => {
            // Move = delete source + write destination
            // ACP provides locations as [source, destination] (needs verification)
            emit_move_events(session.clone(), working_dir, tool_name, &context)?;
        }

        ToolKind::Read => {
            let event = AikiEvent::ReadCompleted(AikiReadCompletedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths,
                success: true,
            });
            dispatch_event(event)?;
        }

        ToolKind::Other => {
            // Unknown tool kind - skip event emission
            return Ok(());
        }
    }

    Ok(())
}

/// Emit events for Move operations (delete source + write destination)
fn emit_move_events(
    session: AikiSession,
    cwd: PathBuf,
    tool_name: &str,
    context: &ToolCallContext,
) -> Result<()> {
    // Move operations should have at least 2 paths: [source, destination]
    // If only 1 path, treat as simple write (destination only)
    let paths: Vec<String> = context.paths.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    if paths.len() >= 2 {
        // First path(s) are sources, last is destination
        let (sources, destination) = paths.split_at(paths.len() - 1);

        // Emit delete for source files
        let delete_event = AikiEvent::DeleteCompleted(AikiDeleteCompletedPayload {
            session: session.clone(),
            cwd: cwd.clone(),
            timestamp: chrono::Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: sources.to_vec(),
            success: Some(true),
        });
        dispatch_event(delete_event)?;

        // Emit write for destination
        let write_event = AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
            session,
            cwd,
            timestamp: chrono::Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: destination.to_vec(),
            success: true,
            edit_details: vec![], // Move doesn't have edit details
        });
        dispatch_event(write_event)?;
    } else {
        // Single path - treat as write only
        let event = AikiEvent::WriteCompleted(AikiWriteCompletedPayload {
            session,
            cwd,
            timestamp: chrono::Utc::now(),
            tool_name: tool_name.to_string(),
            file_paths: paths,
            success: true,
            edit_details: vec![],
        });
        dispatch_event(event)?;
    }

    Ok(())
}

fn dispatch_event(event: AikiEvent) -> Result<()> {
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("Warning: Event bus dispatch failed: {}", e);
    }
    Ok(())
}
```

**Required imports:**
```rust
use crate::events::{
    AikiDeleteCompletedPayload, AikiDeletePermissionAskedPayload,
    AikiReadCompletedPayload, AikiReadPermissionAskedPayload,
    AikiWriteCompletedPayload, AikiWritePermissionAskedPayload,
};
```

### Phase 3: Update Pre-Event Handling

**Problem:** Pre-events fire from `session/request_permission` before file paths are known.

**Options:**

#### Option A: Emit pre-events without file paths (Recommended for MVP)
Pre-events can still provide value for flows that don't need paths:
- Stashing user changes before AI edits
- Logging that an operation is about to happen
- Rate limiting by operation type

```rust
fn fire_pre_file_change_event(
    session_id: &str,
    agent_type: &AgentType,
    cwd: &Option<PathBuf>,
    tool_kind: Option<ToolKind>, // Pass from is_file_modifying_permission_request
) -> Result<()> {
    let working_dir = cwd.as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    let session = create_session(*agent_type, session_id.to_string(), None::<&str>);
    let tool_kind = tool_kind.unwrap_or(ToolKind::Edit); // Default to Edit if unknown
    let tool_name = tool_kind_to_name(tool_kind);

    match tool_kind {
        ToolKind::Edit | ToolKind::Move => {
            // Move treated as write for pre-event (destination creation)
            let event = AikiEvent::WritePermissionAsked(AikiWritePermissionAskedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths: vec![], // Paths not available at permission request time
            });
            dispatch_event(event)?;
        }
        ToolKind::Delete => {
            let event = AikiEvent::DeletePermissionAsked(AikiDeletePermissionAskedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths: vec![], // Paths not available at permission request time
            });
            dispatch_event(event)?;
        }
        ToolKind::Read => {
            let event = AikiEvent::ReadPermissionAsked(AikiReadPermissionAskedPayload {
                session,
                cwd: working_dir,
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                file_paths: vec![],
                pattern: None,
            });
            dispatch_event(event)?;
        }
        ToolKind::Other => {
            // Don't emit pre-events for unknown tool kinds
        }
    }

    Ok(())
}
```

**Update `is_file_modifying_permission_request` to return tool kind:**

```rust
fn get_permission_request_tool_kind(msg: &JsonRpcMessage) -> Option<ToolKind> {
    let params = msg.params.as_ref()?;
    let kind_val = params.get("kind").or_else(|| params.get("toolKind"))?;
    let kind_str = kind_val.as_str()?;

    match kind_str {
        "edit" => Some(ToolKind::Edit),
        "delete" => Some(ToolKind::Delete),
        "move" => Some(ToolKind::Move),
        "read" => Some(ToolKind::Read),
        _ => None,
    }
}

fn is_file_modifying_permission_request(msg: &JsonRpcMessage) -> bool {
    get_permission_request_tool_kind(msg)
        .map(|k| matches!(k, ToolKind::Edit | ToolKind::Delete | ToolKind::Move))
        .unwrap_or(false)
}
```

**Update call site:**
```rust
if method == "session/request_permission" {
    let tool_kind = get_permission_request_tool_kind(&msg);
    if tool_kind.map(|k| matches!(k, ToolKind::Edit | ToolKind::Delete | ToolKind::Move)).unwrap_or(false) {
        if let Some(params) = &msg.params {
            if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                fire_pre_file_change_event(session_id, &validated_agent_type, &cwd, tool_kind)?;
            }
        }
    }
}
```

#### Option B: Fire pre-events from ToolCall status changes (Future enhancement)
Fire `permission_asked` when `ToolCallStatus::Pending` is first seen, after paths are populated.

**Trade-off:** Hooks-based integrations fire pre-events BEFORE the tool runs. ACP would fire them slightly later but WITH paths.

### Phase 4: Verify Move Operation Path Structure

Before implementing Move handling, verify with actual ACP traffic:
1. Confirm `locations` contains `[source, destination]` for moves
2. Check if there's a different field for source/destination
3. Update `emit_move_events()` based on actual structure

**Test approach:**
```rust
#[cfg(test)]
fn test_move_locations_structure() {
    // Capture actual ACP Move tool call and verify structure
    // May need to check ToolCallContent for source/dest info
}
```

## Testing Checklist

### Unit Tests
- [ ] `tool_kind_to_name` returns correct strings
- [ ] `record_post_change_events` emits `WriteCompleted` for `ToolKind::Edit`
- [ ] `record_post_change_events` emits `DeleteCompleted` for `ToolKind::Delete`
- [ ] `record_post_change_events` emits both `DeleteCompleted` and `WriteCompleted` for `ToolKind::Move`
- [ ] `record_post_change_events` emits `ReadCompleted` for `ToolKind::Read`
- [ ] `fire_pre_file_change_event` emits `WritePermissionAsked` for Edit
- [ ] `fire_pre_file_change_event` emits `DeletePermissionAsked` for Delete
- [ ] Pre-events work correctly with empty file_paths

### Integration Tests
- [ ] ACP delete operation triggers `delete.completed` flow handler
- [ ] ACP edit operation triggers `write.completed` flow handler
- [ ] Flow can gate ACP delete operations via `delete.permission_asked`

## Files to Modify

| File | Changes |
|------|---------|
| `cli/src/acp/protocol.rs` | Add `tool_kind_to_name()` helper, re-export `ToolKind` from `agent_client_protocol` |
| `cli/src/commands/acp.rs` | Update `record_post_change_events()`, `fire_pre_file_change_event()`, add `get_permission_request_tool_kind()`, `emit_move_events()`, use protocol helpers |

### Suggested additions to `cli/src/acp/protocol.rs`

```rust
// Re-export ToolKind for use in event handling
pub use agent_client_protocol::ToolKind;

/// Convert ToolKind to canonical tool name string
pub fn tool_kind_to_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Edit => "Edit",
        ToolKind::Delete => "Delete",
        ToolKind::Move => "Move",
        ToolKind::Read => "Read",
        ToolKind::Other => "Other",
    }
}

/// Check if a ToolKind represents a file-modifying operation
pub fn is_file_modifying_kind(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Edit | ToolKind::Delete | ToolKind::Move)
}
```

## Open Questions

1. **Move operation path structure:** Need to verify that `locations = [source, destination]`. May require inspecting actual ACP traffic from Codex/Claude.

2. **Pre-event value without paths:** Is `write.permission_asked` with empty `file_paths` useful enough? The hook-based vendors have paths available.

3. **Read event coverage:** Should ACP emit read events? Need to check if clients send `ToolKind::Read`.

## Definition of Done

- [ ] `ToolKind::Delete` emits `delete.*` events with correct tool name
- [ ] `ToolKind::Edit` emits `write.*` events with correct tool name
- [ ] `ToolKind::Move` emits both `delete.*` (source) and `write.*` (destination)
- [ ] `ToolKind::Read` emits `read.*` events if supported
- [ ] Pre-events emit operation-specific types based on tool kind
- [ ] All existing tests pass
- [ ] New tests cover operation-specific event emission

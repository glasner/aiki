# PostChange Event Batch Refactoring Plan

## Overview

Refactor the `AikiPostChangeEvent` to handle batches of files instead of single files. This aligns with the ACP protocol's ability to report multiple file changes in a single tool call.

## Current State

### Event Structure
```rust
// cli/src/events.rs:17-29
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostChangeEvent {
    pub agent_type: AgentType,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub agent_version: Option<String>,
    pub session_id: String,
    pub tool_name: String,
    pub file_path: String,  // ⚠️ SINGLE FILE
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub detection_method: DetectionMethod,
}
```

### Current Behavior
- **ACP Handler** (`cli/src/commands/acp.rs:506-563`): Loops through `context.paths` (Vec<PathBuf>) and creates **one event per file**
- **Hook Vendors**: Only receive single files from vendor-specific hooks (Claude Code, Cursor)
- **Flow Variables**: Exposes `$event.file_path` as a string
- **Success Message**: `"✅ Provenance recorded for {file_path}"`

## Design Decisions

Based on user requirements:

1. **Variable Naming**: Only expose `$event.file_paths` (breaking change, no backward compat)
2. **Success Message**: Simple count format: `"✅ Provenance recorded for N files"`
3. **Backward Compatibility**: None - this is early enough in Aiki's lifecycle
4. **Batching Strategy**: Always use `Vec<String>` even for single files

## Implementation Plan

### 1. Update Event Structure

**File**: `cli/src/events.rs` (lines 17-29)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPostChangeEvent {
    pub agent_type: AgentType,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub agent_version: Option<String>,
    pub session_id: String,
    pub tool_name: String,
    pub file_paths: Vec<String>,  // ✅ CHANGED: Now a vector
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub detection_method: DetectionMethod,
}
```

### 2. Refactor ACP Handler (Primary Change)

**File**: `cli/src/commands/acp.rs` (lines 506-563)

**Before**:
```rust
fn record_post_change_events(
    session_id: &str,
    agent_type: &AgentType,
    // ... other params
    context: ToolCallContext,
) -> Result<()> {
    // Creates N events for N files
    for path in context.paths {
        let event = AikiEvent::PostChange(AikiPostChangeEvent {
            // ... metadata fields
            file_path: path.to_string_lossy().to_string(),
            // ...
        });
        event_bus::dispatch(event)?;
    }
    Ok(())
}
```

**After**:
```rust
fn record_post_change_events(
    session_id: &str,
    agent_type: &AgentType,
    // ... other params
    context: ToolCallContext,
) -> Result<()> {
    // Create ONE event with ALL files
    let file_paths: Vec<String> = context.paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    
    let event = AikiEvent::PostChange(AikiPostChangeEvent {
        // ... metadata fields
        file_paths,  // ✅ Single batch
        // ...
    });
    
    event_bus::dispatch(event)?;  // ✅ Single dispatch
    Ok(())
}
```

### 3. Update Event Handler

**File**: `cli/src/handlers.rs` (lines 143-187)

**Change**:
```rust
pub fn handle_post_change(event: AikiPostChangeEvent) -> Result<HookResponse> {
    // ... existing flow execution logic ...
    
    // Update success message
    Ok(HookResponse::success_with_message(format!(
        "✅ Provenance recorded for {} files",
        event.file_paths.len()  // ✅ Use count
    )))
}
```

### 4. Update Flow Variable Resolver

**File**: `cli/src/flows/executor.rs` (lines 62-66)

**Change**:
```rust
fn create_resolver(context: &AikiState) -> VariableResolver {
    let mut resolver = VariableResolver::new();
    
    match &context.event {
        AikiEvent::PostChange(e) => {
            resolver.add_var("event.tool_name".to_string(), e.tool_name.clone());
            
            // ✅ CHANGED: Expose as space-separated list for shell iteration
            resolver.add_var(
                "event.file_paths".to_string(),
                e.file_paths.join(" ")
            );
            
            // Optional: Add file count variable
            resolver.add_var(
                "event.file_count".to_string(),
                e.file_paths.len().to_string()
            );
            
            resolver.add_var("event.session_id".to_string(), e.session_id.clone());
        }
        // ...
    }
}
```

**Note**: Consider using newline-separated (`join("\n")`) if flows need to iterate over files individually.

### 5. Update Hook Vendors (Wrap Single Files)

**File**: `cli/src/vendors/claude_code.rs` (lines 77-91)

**Change**:
```rust
let event = AikiEvent::PostChange(AikiPostChangeEvent {
    agent_type: AgentType::ClaudeCode,
    // ... other fields ...
    file_paths: vec![tool_input.file_path.clone()],  // ✅ Wrap in Vec
    // ...
});
```

**File**: `cli/src/vendors/cursor.rs` (lines 61-73)

**Change**:
```rust
let event = AikiEvent::PostChange(AikiPostChangeEvent {
    agent_type: AgentType::Cursor,
    // ... other fields ...
    file_paths: vec![payload.edited_file.clone()],  // ✅ Wrap in Vec
    // ...
});
```

### 6. Update Test Files

All test files creating `AikiPostChangeEvent` must change:

**Pattern**:
```rust
// Before
AikiPostChangeEvent {
    // ...
    file_path: "test.rs".to_string(),
    // ...
}

// After
AikiPostChangeEvent {
    // ...
    file_paths: vec!["test.rs".to_string()],
    // ...
}
```

**Affected Files**:
- `cli/src/test_must_use.rs` (lines 32-42, 64-74)
- `cli/src/flows/executor.rs` (lines 793-810) - test helper
- `cli/src/flows/state.rs` (line 137) - test helper
- `cli/src/flows/core/build_description.rs` (line 65) - test

### 7. Files That Don't Need Changes

✅ **`cli/src/provenance.rs`**
- Already doesn't store file paths (JJ tracks them via diffs)

✅ **`cli/src/flows/core/flow.yaml`**
- Doesn't directly reference `$event.file_path`

✅ **`cli/src/flows/core/build_description.rs`**
- Only uses metadata fields from the event, not file paths

## Migration Impact

### Breaking Changes

1. **Flow Variables**: Any custom flows using `$event.file_path` will break
   - **Mitigation**: Aiki is early in development, assume no external flows exist

2. **Event Structure**: Any code directly constructing `AikiPostChangeEvent` will fail to compile
   - **Mitigation**: All usages are in this codebase and will be updated

3. **Success Message Format**: Tools parsing the success message will see different format
   - **Mitigation**: Unlikely to have external parsers at this stage

### Non-Breaking Changes

- The `[aiki]...[/aiki]` metadata format in change descriptions is unchanged
- The provenance system continues to not store file paths (JJ handles that)
- The ACP protocol integration becomes more efficient (one event vs many)

## Testing Strategy

### Unit Tests
```bash
cargo test
```
- Compiler will catch all `file_path` → `file_paths` migration issues
- Verify all test files updated correctly

### Integration Tests

1. **ACP Handler with Multiple Files**:
   ```bash
   # Send ACP notification with multiple file changes
   # Verify single PostChange event created
   ```

2. **Hook Vendors with Single Files**:
   ```bash
   # Trigger Claude Code hook
   # Verify single-file vec works correctly
   ```

3. **Flow Variable Resolution**:
   ```bash
   # Run core flow
   # Verify $event.file_paths resolves correctly
   ```

### Manual Testing
- Run `aiki doctor` to verify hook installation still works
- Test ACP integration with multi-file tool calls
- Verify provenance recording via `jj log`

## Performance Improvements

**Before**: For N files → N events → N flow executions → N `jj describe` calls
**After**: For N files → 1 event → 1 flow execution → 1 `jj describe` call

**Benefits**:
- Reduced event bus overhead
- Single JJ operation instead of N operations
- More accurate "atomic" view of multi-file changes

## Future Considerations

### Flow Variable Enhancement
Consider adding helper variables in the future:
```rust
// Potential additions to variable resolver
resolver.add_var("event.first_file", e.file_paths.first().unwrap_or(&String::new()));
resolver.add_var("event.last_file", e.file_paths.last().unwrap_or(&String::new()));
resolver.add_var("event.file_extensions", get_unique_extensions(&e.file_paths).join(" "));
```

### Success Message Enhancement
Could make messages more informative:
```rust
// Option: List first few files
if event.file_paths.len() <= 3 {
    format!("✅ Provenance recorded for: {}", event.file_paths.join(", "))
} else {
    format!("✅ Provenance recorded for {} files", event.file_paths.len())
}
```

## Rollout Plan

1. ✅ **Phase 1**: Update event structure and all creation sites
2. ✅ **Phase 2**: Update event consumption (handlers, flows)
3. ✅ **Phase 3**: Update all test files
4. ✅ **Phase 4**: Run full test suite
5. ✅ **Phase 5**: Manual integration testing

All phases can be done in a single PR since this is a breaking change.

## Success Criteria

- ✅ `cargo build` succeeds with no warnings
- ✅ `cargo test` passes all tests
- ✅ ACP handler creates single event for multi-file tool calls
- ✅ Hook vendors still work with single-file events
- ✅ Flow variables resolve `$event.file_paths` correctly
- ✅ Success messages show file count accurately
- ✅ Provenance still records correctly in JJ change descriptions

## References

- **ACP Protocol Schema**: https://agentclientprotocol.com/protocol/schema
- **ToolCall.locations**: Array of file paths changed by a tool invocation
- **JJ Change Model**: Changes are mutable, commit IDs are transient
- **Project Guidelines**: `/Users/glasner/code/aiki/CLAUDE.md` (Change-centric terminology)

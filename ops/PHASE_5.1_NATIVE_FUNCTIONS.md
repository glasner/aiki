# Phase 5.1 Enhancement: Aiki Built-in Functions

**Date**: 2025-11-15  
**Status**: ✅ Complete  
**Enhancement Type**: Architecture improvement

## Summary

Added support for calling Rust functions directly from flow YAML files using the `aiki:` action type. This moves logic from handlers into flows, making the system more declarative and maintainable.

## What Changed

### Before: Handler-Built Variables

The handler built the provenance description in Rust and passed it as a variable:

**`handlers.rs`:**
```rust
let provenance = ProvenanceRecord {
    agent: AgentInfo { ... },
    session_id: session_id.to_string(),
    tool_name: tool_name.to_string(),
};

context.event_vars.insert(
    "aiki_provenance_description",
    provenance.to_description(),
);
```

**`provenance.yaml`:**
```yaml
PostChange:
  - jj: describe -m "$aiki_provenance_description"
```

### After: Flow-Called Functions

The flow calls the Rust function directly:

**`handlers.rs`:**
```rust
// Just pass event variables - the flow handles the rest
context.event_vars.insert("agent", format!("{:?}", event.agent));
context.event_vars.insert("session_id", session_id.to_string());
context.event_vars.insert("tool_name", tool_name.to_string());
```

**`provenance.yaml`:**
```yaml
PostChange:
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
    on_failure: fail

  - jj: describe -m "$build_provenance_description_output"
```

## Benefits

### 1. **Self-Contained Flows**
Flows now describe their complete behavior. You can read `provenance.yaml` and understand exactly what happens without checking the handler code.

### 2. **Reusable Functions**
Built-in Aiki functions can be called from any flow:
```yaml
MyFlow:
  PostChange:
    - aiki: build_provenance_description
      args:
        agent: "$event.agent"
        session_id: "$event.session_id"
        tool_name: "$event.tool_name"
```

### 3. **Cleaner Handlers**
Handlers are now thin wrappers that just set up context and execute flows. All logic lives in flows.

### 4. **Step Output Capture**
Aiki functions can return values that subsequent steps can use:
```yaml
PostChange:
  - aiki: some_function  # Returns output
  - shell: echo "$some_function_output"  # Uses output
```

### 5. **Testable in Isolation**
Aiki functions can be tested independently from flows, and flows can be tested independently from handlers.

## Implementation Details

### New Action Type: `aiki`

```rust
pub struct AikiAction {
    pub aiki: String,                    // Function name
    pub args: HashMap<String, String>,   // Arguments with variable interpolation
    pub on_failure: FailureMode,         // Error handling
}
```

### Function Registry

Functions are registered in `executor.rs`:

```rust
fn execute_aiki(action: &AikiAction, context: &ExecutionContext) -> Result<ActionResult> {
    match action.aiki.as_str() {
        "build_provenance_description" => {
            Self::aiki_build_provenance_description(&resolved_args, context)
        }
        _ => anyhow::bail!("Unknown aiki function: {}", action.aiki),
    }
}
```

### Output Capture

When an `aiki` action completes, its output is stored as a variable:

```rust
if let Action::Aiki(aiki_action) = action {
    if !result.stdout.is_empty() {
        let var_name = format!("{}_output", aiki_action.aiki);
        context.event_vars.insert(var_name, result.stdout.clone());
    }
}
```

## Available Aiki Functions

### `build_provenance_description`

**Purpose**: Generate provenance metadata in the `[aiki]...[/aiki]` format

**Arguments**:
- `agent`: Agent type (ClaudeCode, Cursor, Unknown)
- `session_id`: Session identifier
- `tool_name`: Tool that made the change

**Returns**: Formatted provenance description

**Example**:
```yaml
- aiki: build_provenance_description
  args:
    agent: "$event.agent"
    session_id: "$event.session_id"
    tool_name: "$event.tool_name"
```

**Output** (stored in `$build_provenance_description_output`):
```
[aiki]
agent=claude-code
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]
```

## Future Aiki Functions

Potential built-in functions to add in Phase 5.2+:

### Code Analysis
```yaml
- aiki: calculate_complexity
  args:
    file_path: "$event.file_path"
# Output: $calculate_complexity_output = "10"
```

### Git Integration
```yaml
- aiki: get_staged_files
# Output: $get_staged_files_output = "file1.rs,file2.rs"
```

### Change Queries
```yaml
- aiki: query_changes
  args:
    revset: "description(agent=claude-code)"
# Output: $query_changes_output = JSON array of changes
```

### Signing
```yaml
- aiki: sign_change
  args:
    change_id: "$event.change_id"
    key: "$signing_key"
```

## Migration Guide

### For Existing Flows

No changes needed - existing flows continue to work.

### For New Functions

To add a new Aiki function:

1. **Define the function in `executor.rs`**:
```rust
fn aiki_my_function(
    args: &HashMap<String, String>,
    context: &ExecutionContext,
) -> Result<ActionResult> {
    // Your logic here
    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "result".to_string(),
        stderr: String::new(),
    })
}
```

2. **Register it in the match statement**:
```rust
match action.aiki.as_str() {
    "build_provenance_description" => { ... }
    "my_function" => Self::aiki_my_function(&resolved_args, context),
    _ => anyhow::bail!("Unknown aiki function: {}", action.aiki),
}
```

3. **Use it in flows**:
```yaml
PostChange:
  - aiki: my_function
    args:
      key: "$value"
```

## Testing

All 102 tests pass with this enhancement:
- ✅ All existing flow tests
- ✅ All provenance tests  
- ✅ All integration tests
- ✅ Backward compatibility maintained

## Files Modified

1. `cli/src/flows/types.rs` - Added `AikiAction` type
2. `cli/src/flows/executor.rs` - Added `execute_aiki()` and `aiki_build_provenance_description()`
3. `cli/flows/provenance.yaml` - Updated to use `aiki:` action
4. `cli/src/handlers.rs` - Simplified to remove provenance building

**Total Changes**: ~150 lines added, ~30 lines removed

## Performance Impact

No performance degradation - the function is called the same way, just from the flow instead of the handler.

## Documentation

This pattern enables:
- **Declarative flows**: Logic in YAML, not Rust
- **Composable functions**: Reusable across flows
- **Clear separation**: Handlers set up context, flows execute logic
- **Extensibility**: Easy to add new functions

## References

- Phase 5.1 Complete: [`ops/PHASE_5.1_COMPLETE.md`](PHASE_5.1_COMPLETE.md)
- Flow Examples: [`ops/examples/flow.yaml`](examples/flow.yaml)
- Design Doc: [`ops/phase-5.md`](phase-5.md)

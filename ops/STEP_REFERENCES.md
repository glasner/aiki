# Step References in Flows

**Date**: 2025-11-15  
**Status**: ✅ Implemented (Phase 5.1)

## Overview

Actions in flows can be referenced by subsequent steps using a consistent variable syntax. This enables flows to use outputs from previous steps.

## How It Works

### Automatic Step Variables

When an action completes, the executor automatically creates variables:

```yaml
PostChange:
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
  
  # Reference the output using the function name
  - jj: describe -m "$build_provenance_description.output"
```

### Available Step Variables

For any `aiki` action (using the function name as identifier):

| Variable | Description | Example Value |
|----------|-------------|---------------|
| `$name.output` | stdout from the action | `"[aiki]\nagent=claude-code\n..."` |
| `$name.exit_code` | Exit code | `"0"` or `"1"` |
| `$name.failed` | Boolean failed status | `"true"` or `"false"` |
| `$name.result` | Text result status | `"success"` or `"failed"` |

### Example: Using Step Results

```yaml
PostChange:
  # Step 1: Call aiki function
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
    on_failure: stop

  # Step 2: Use the output
  - jj: describe -m "$build_provenance_description.output"
  
  # Step 3: Conditional based on result
  - shell: echo "Provenance: $build_provenance_description.output"
```

### Example: Checking Failures

```yaml
PreCommit:
  - aiki: run_tests
    args:
      test_path: "tests/"
  
  # Only run if tests failed
  - shell: notify-send "Tests failed!"
    # Note: Conditionals (when:) are Phase 5.2+, but this shows the pattern
```

## Implementation Details

### Storing Step Results

The executor calls `store_step_result()` after each action:

```rust
fn store_step_result(context: &mut ExecutionContext, step_name: &str, result: &ActionResult) {
    // Store output
    context.event_vars.insert(format!("{}.output", step_name), result.stdout);
    
    // Store exit code
    context.event_vars.insert(format!("{}.exit_code", step_name), exit_code.to_string());
    
    // Store failed status
    context.event_vars.insert(format!("{}.failed", step_name), (!result.success).to_string());
    
    // Store result status
    context.event_vars.insert(
        format!("{}.result", step_name),
        if result.success { "success" } else { "failed" }
    );
}
```

### Variable Resolution

The variable resolver handles nested properties:

```rust
let mut resolver = VariableResolver::new();
resolver.add_event_vars(&context.event_vars);

// Given: context.event_vars["build_provenance_description.output"] = "[aiki]..."
// Input: "describe -m '$build_provenance_description.output'"
// Output: "describe -m '[aiki]...'"
let result = resolver.resolve(input);
```

## Step Naming Convention

### Aiki Functions

Use the function name directly:

```yaml
- aiki: build_provenance_description
  # Referenced as: $build_provenance_description.*
  
- aiki: calculate_complexity
  # Referenced as: $calculate_complexity.*
```

### Future: Other Action Types (Phase 5.2+)

Shell and JJ commands will use auto-generated names or aliases:

```yaml
# Auto-generated from command (sanitized)
- shell: ruff check .
  # Could be: $ruff_check.*

# Or explicit alias
- shell: ruff check .
  alias: lint
  # Referenced as: $lint.*
```

## Phase 5.1 Scope

**Currently Supported:**
- ✅ `aiki` actions are referenceable
- ✅ Step variables: `.output`, `.exit_code`, `.failed`, `.result`
- ✅ Automatic variable storage
- ✅ Variable interpolation in subsequent steps

**Not Yet Supported (Phase 5.2+):**
- ❌ `shell`, `jj`, `log` step references (only `aiki` for now)
- ❌ `alias:` property for custom names
- ❌ `$previous_step.*` for last step
- ❌ Conditionals using step results (`when:`, `if/then/else`)

## Real-World Example

From `flows/provenance.yaml`:

```yaml
name: "Aiki Provenance Recording"
version: "1"

PostChange:
  # Step 1: Build provenance description
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
    on_failure: stop

  # Step 2: Use the output from step 1
  - jj: describe -m "$build_provenance_description.output"
    on_failure: continue

  # Step 3: Create new change
  - jj: new
    on_failure: continue

  # Step 4: Log success
  - log: "Recorded change by $event.agent (session: $event.session_id)"
```

## Benefits

1. **Self-documenting**: Clear data flow between steps
2. **Composable**: Outputs become inputs naturally
3. **Debuggable**: Can log step outputs to see intermediate values
4. **Consistent**: Same pattern for all action types (in future phases)
5. **Type-safe**: Variables follow predictable naming convention

## Testing

All step reference functionality is tested:
- ✅ Output storage and retrieval
- ✅ Variable interpolation with step results
- ✅ Integration with full flow execution

## Future Enhancements (Phase 5.2+)

### Aliases

```yaml
- aiki: build_provenance_description
  alias: prov
  # Use as: $prov.output instead of $build_provenance_description.output
```

### Previous Step Reference

```yaml
- shell: echo "test"
- log: "Last step output: $previous_step.output"
```

### Conditionals

```yaml
- aiki: run_tests
- shell: echo "Tests passed!"
  when: $run_tests.result == "success"
```

## References

- Phase 5 Design: [`ops/phase-5.md#step-references`](phase-5.md#step-references)
- Native Functions: [`ops/PHASE_5.1_NATIVE_FUNCTIONS.md`](PHASE_5.1_NATIVE_FUNCTIONS.md)
- Phase 5.1 Complete: [`ops/PHASE_5.1_COMPLETE.md`](PHASE_5.1_COMPLETE.md)

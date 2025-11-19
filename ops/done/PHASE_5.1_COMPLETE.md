# Phase 5.1 Complete: Let Syntax Migration

## Status: ✅ IMPLEMENTED

This milestone completes the migration from implicit variable naming with `aiki:` actions to explicit variable binding with `let:` syntax.

---

## What Changed

### Before: Implicit Variable Naming

```yaml
PostChange:
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
    on_failure: stop

  - jj: describe -m "$build_provenance_description.output"
```

**Problems:**
- Variable name derived from function name (`build_provenance_description`)
- Output accessed via `.output` suffix
- Requires `args:` section with explicit parameter passing
- Verbose and less intuitive

### After: Explicit Variable Binding (No Args Needed!)

```yaml
PostChange:
  - let: description = aiki/provenance.build_description
    on_failure: stop

  - jj: describe -m "$description"
```

**Benefits:**
- Explicit variable name chosen by user (`description`)
- Direct variable reference (no `.output` suffix)
- No `args:` section needed - functions read from event context
- Cleaner, more intuitive syntax
- Function path uses namespace convention (`aiki/module.function`)

---

## Implementation Summary

### 1. New Type Definitions (`cli/src/flows/types.rs`)

```rust
/// Let binding action (function call or variable aliasing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetAction {
    /// The let binding in format "variable = expression"
    #[serde(rename = "let")]
    pub let_: String,

    /// What to do when the action fails
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

pub enum Action {
    Shell(ShellAction),
    Jj(JjAction),
    Log(LogAction),
    Let(LetAction),        // NEW
    Aiki(AikiAction),      // Deprecated
}
```

### 2. ExecutionContext Enhanced

```rust
pub struct ExecutionContext {
    pub cwd: std::path::PathBuf,
    pub event_vars: HashMap<String, String>,
    pub env_vars: HashMap<String, String>,
    pub variable_metadata: HashMap<String, ActionResult>,  // NEW
}
```

### 3. Core Implementation (`cli/src/flows/executor.rs`)

**Key Functions:**
- `execute_let()` - Main dispatcher for let actions
- `is_valid_variable_name()` - Validates variable names
- `execute_let_alias()` - Handles `let x = $y` aliasing
- `execute_let_function()` - Handles `let x = aiki/module.function` calls
- `fn_build_provenance_description()` - Built-in function implementation
- `store_action_result()` - Stores variables with structured metadata

### 4. Updated System Flow (`cli/flows/provenance.yaml`)

**Version:** 2 (bumped from 1)

**Changes:**
- Replaced `aiki: build_provenance_description` with `let: description = aiki/provenance.build_description`
- Removed `args:` section
- Changed `$build_provenance_description.output` to `$description`

### 5. Comprehensive Tests

Added 13 new tests covering:
- Variable name validation
- Function call syntax
- Variable aliasing syntax
- Invalid syntax detection
- Whitespace trimming
- Variable storage and retrieval
- Structured metadata storage
- Alias functionality for Shell/Jj/Log actions
- Backward compatibility with dotted properties

**Test Results:** ✅ All 20 tests passing

---

## Two Modes of Operation

### Mode 1: Function Call

```yaml
- let: description = aiki/provenance.build_description
  on_failure: stop
```

**Behavior:**
1. Parses function path: `aiki/provenance.build_description`
2. Routes to built-in function implementation
3. Function reads from `$event.*` variables automatically
4. Returns result as `$description`

### Mode 2: Variable Aliasing

```yaml
- let: file = $event.file_path
- let: desc = $description
```

**Behavior:**
1. Resolves the right-hand side variable
2. Creates a copy with the new name
3. Preserves structured metadata

---

## Built-in Functions

### `aiki/provenance.build_description`

**Purpose:** Builds provenance metadata for JJ change descriptions

**Input (from event context):**
- `$event.agent` - Agent type (e.g., "ClaudeCode", "Cursor")
- `$event.session_id` - Session identifier
- `$event.tool_name` - Tool that made the change

**Output:**
```
[aiki]
agent=ClaudeCode
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
detected_at=2025-11-16T12:34:56Z
[/aiki]
```

**Usage:**
```yaml
- let: description = aiki/provenance.build_description
  on_failure: fail
- jj: describe -m "$description"
```

---

## Variable Storage and Metadata

### Direct Variable Access

```yaml
- let: description = aiki/provenance.build_description

# Access directly:
- log: "Description: $description"
```

### Structured Metadata

All let-bound variables also create dotted properties for backward compatibility:

```yaml
- let: result = aiki/some.function

# Available variables:
# $result               - The main output (stdout)
# $result.output        - Same as $result
# $result.exit_code     - Exit code (if applicable)
# $result.failed        - "true" or "false"
```

### Alias Support

Shell, JJ, and Log actions can also store results with `alias:`:

```yaml
- shell: git status
  alias: git_status

- log: "Git status: $git_status"
```

---

## Variable Naming Rules

**Valid variable names:**
- Must start with letter or underscore: `description`, `_temp`
- Can contain letters, numbers, underscores: `var123`, `my_var`, `CamelCase`

**Invalid variable names:**
- Cannot start with number: `123var` ❌
- Cannot contain hyphens: `my-var` ❌
- Cannot contain dots: `my.var` ❌
- Cannot contain spaces: `my var` ❌
- Cannot start with `$`: `$var` ❌

**Examples:**
```yaml
- let: description = aiki/provenance.build_description  ✅
- let: desc = $description                               ✅
- let: _private = $event.agent                          ✅
- let: my_file_123 = $event.file_path                   ✅

- let: 123var = value      ❌ starts with number
- let: my-var = value      ❌ contains hyphen
```

---

## Migration Guide for Users

### Old Syntax (Deprecated)

```yaml
PostChange:
  - aiki: some_function
    args:
      key: value
  - shell: echo "$some_function.output"
```

### New Syntax (Recommended)

```yaml
PostChange:
  - let: result = aiki/namespace.some_function
  - shell: echo "$result"
```

### Key Differences

| Aspect | Old (`aiki:`) | New (`let:`) |
|--------|---------------|--------------|
| Function format | `some_function` | `aiki/namespace.function` |
| Parameters | `args:` section | Read from context |
| Variable name | Implicit (from function name) | Explicit (user chooses) |
| Output access | `$function.output` | `$variable` |
| Namespace | Flat | Hierarchical |

---

## Breaking Changes

⚠️ **The `aiki:` action with `args:` is now deprecated**

While the old syntax still works for backward compatibility, it is recommended to migrate to `let:` syntax.

**What breaks:**
- Old flows using `aiki: build_provenance_description` with `args:` will continue to work
- However, new built-in functions will ONLY be available via `let:` syntax

**Migration path:**
1. Replace `aiki: function_name` with `let: var = aiki/module.function`
2. Remove `args:` section (functions read from context)
3. Update variable references from `$function_name.output` to `$var`

---

## Future Compatibility

### Phase 8: External WASM Functions

The `let:` syntax is designed to support external WASM functions in Phase 8:

```yaml
# Built-in function
- let: description = aiki/provenance.build_description

# External vendor function (Phase 8)
- let: complexity = vendor/analyzer.analyze_complexity
```

The namespace-based function path (`vendor/analyzer.analyze_complexity`) will route to WASM modules in Phase 8.

---

## Files Changed

### Core Implementation
- ✅ `cli/src/flows/types.rs` - Added `LetAction` type
- ✅ `cli/src/flows/executor.rs` - Implemented let execution logic
- ✅ `cli/flows/provenance.yaml` - Updated to use let syntax (v2)

### Documentation
- ✅ `ops/phase-5.md` - Added Let Binding section
- ✅ `ops/PHASE_5.1_COMPLETE.md` - This file

### Tests
- ✅ `cli/src/flows/executor.rs` - Added 13 new tests

---

## Success Criteria

✅ **All criteria met:**

1. ✅ `let:` syntax parses correctly in YAML flows
2. ✅ Function calls route to built-in implementations
3. ✅ Variable aliasing works (`let x = $y`)
4. ✅ Variables are stored and accessible in subsequent actions
5. ✅ Structured metadata is preserved
6. ✅ Invalid variable names are rejected with clear errors
7. ✅ `aiki/provenance.build_description` function implemented
8. ✅ `provenance.yaml` migrated to new syntax
9. ✅ All tests passing (20/20)
10. ✅ Documentation updated

---

## Next Steps

### Immediate (Phase 5.2+)
- Implement additional built-in functions as needed
- Add more comprehensive error handling for function failures
- Consider adding function parameter support for future use cases

### Future (Phase 8)
- Add WASM-based external functions
- Support `vendor/*` namespace for third-party functions
- Optional native compilation for performance-critical functions

---

## Notes

**Why namespace-based function paths?**

The `aiki/module.function` syntax:
1. Provides clear organization of functions
2. Prevents naming conflicts
3. Enables future vendor/external functions
4. Makes function origins explicit
5. Scales better than flat naming

**Why no `args:` section?**

Functions now read directly from the execution context (`$event.*` variables), which:
1. Reduces verbosity
2. Makes flows more readable
3. Simplifies function implementations
4. Aligns with the event-driven architecture

**Why both function calls AND variable aliasing?**

- Function calls: `let x = aiki/module.function` for computations
- Variable aliasing: `let x = $y` for readability and convenience
- Both modes use the same syntax, keeping it simple

---

**Implementation Date:** 2025-11-16  
**Status:** ✅ Complete and tested  
**Version:** Aiki v0.1.0 (Phase 5.1)

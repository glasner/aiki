# Error Handling Improvements with Structured Error Types

## Summary

Replaced generic `anyhow::bail!()` error handling with structured error types using `thiserror`, delivering better error ergonomics, type safety, and testability across the codebase.

## Motivation

### Before: String-Based Errors

```rust
// BEFORE: Allocates strings unnecessarily, no type safety
anyhow::bail!(
    "Unknown agent type: '{}'. Supported values: 'claude-code', 'cursor'",
    agent
);
```

**Problems:**
- ❌ Runtime string allocation for every error
- ❌ No compile-time checking of error types
- ❌ Harder to test specific error conditions
- ❌ Error messages scattered throughout codebase
- ❌ Inconsistent error formatting

### After: Structured Error Types

```rust
// AFTER: Type-safe, zero-cost error variants
#[derive(Error, Debug)]
pub enum AikiError {
    #[error("Unknown agent type: '{0}'. Supported values: 'claude-code', 'cursor'")]
    UnknownAgentType(String),
    // ...
}

// Usage
Err(AikiError::UnknownAgentType(agent.to_string()))
```

**Benefits:**
- ✅ Type-safe error handling
- ✅ Centralized error messages
- ✅ Better error composition
- ✅ Easier to test specific errors
- ✅ IDE autocomplete for error types

## Implementation

### 1. Created Structured Error Types

**File**: `cli/src/error.rs` (new)

Defined `AikiError` enum with 25+ structured error variants:

```rust
#[derive(Error, Debug)]
pub enum AikiError {
    // Repository errors
    #[error("Not in a JJ repository. Run 'jj init' or 'aiki init' first")]
    NotInJjRepo,

    #[error("Failed to initialize JJ workspace")]
    JjInitFailed,

    // File errors
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    // Agent/vendor errors
    #[error("Unknown agent type: '{0}'. Supported values: 'claude-code', 'cursor'")]
    UnknownAgentType(String),

    // Flow execution errors
    #[error("Invalid let syntax: '{0}'. Expected 'variable = expression'")]
    InvalidLetSyntax(String),

    #[error("Invalid variable name: '{0}'. Variable names must start with a letter or underscore, and contain only letters, numbers, and underscores")]
    InvalidVariableName(String),

    // Command execution errors
    #[error("jj command failed: {0}")]
    JjCommandFailed(String),

    // Signing/GPG errors
    #[error("SSH key file not found: {0}")]
    SshKeyNotFound(PathBuf),

    // Generic wrapper for underlying errors
    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AikiError>;
```

### 2. Updated Main Entry Point

**File**: `cli/src/main.rs`

Changed error handling to use proper Display formatting:

```rust
// BEFORE
fn main() -> Result<()> {
    // Errors printed with Debug: "Error: NotInJjRepo"
}

// AFTER
fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);  // Uses Display trait
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Actual application logic
}
```

**Why this matters**: Rust's default error handling for `main() -> Result<()>` uses Debug formatting, which prints `Error: NotInJjRepo` instead of the user-friendly message. The wrapper ensures Display is used.

### 3. Replaced Errors in Core Modules

#### Main Command Handler (`main.rs`)

| Error Site | Before | After |
|------------|--------|-------|
| Repository check | `anyhow::bail!("Not in a JJ repository...")` | `Err(AikiError::NotInJjRepo)` |
| File not found | `anyhow::bail!("File not found: {}", path)` | `Err(AikiError::FileNotFound(path))` |
| Unknown scope | `anyhow::bail!("Unknown scope: '{}'...", other)` | `Err(AikiError::UnknownScope(other.to_string()))` |
| Unknown format | `anyhow::bail!("Unknown format: '{}'...", other)` | `Err(AikiError::UnknownFormat(other.to_string()))` |
| Agent parsing | `anyhow::bail!("Unknown agent type...")` | `Err(AikiError::UnknownAgentType(agent.to_string()))` |

#### Flow Executor (`flows/executor.rs`)

| Error Site | Before | After |
|------------|--------|-------|
| Action failed | `anyhow::bail!("Action failed...")` | `Err(AikiError::ActionFailed)` |
| Invalid let syntax | `anyhow::bail!("Invalid let syntax: '{}'...", action)` | `Err(AikiError::InvalidLetSyntax(action.to_string()))` |
| Invalid variable name | `anyhow::bail!("Invalid variable name...")` | `Err(AikiError::InvalidVariableName(name.to_string()))` |
| Unknown function | `anyhow::bail!("Unknown aiki function: {}", func)` | `Err(AikiError::UnknownAikiFunction(func.clone()))` |
| Invalid timeout | `anyhow::bail!("Invalid timeout format...")` | `Err(AikiError::InvalidTimeoutFormat(timeout.to_string()))` |

#### Blame Module (`blame.rs`)

Special handling for JJ-lib interop:

```rust
// Uses anyhow::Result for JJ-lib compatibility, converts AikiError when needed
type Result<T> = anyhow::Result<T>;

// Convert AikiError to anyhow::Error
return Err(AikiError::FileNotFoundNoParents.into());
```

**Why**: The `blame.rs` module heavily interacts with `jj-lib`, which returns `jj_lib::backend::BackendError`. Using `anyhow::Result` allows seamless integration while still using structured errors where appropriate.

## Error Categories

### 1. Repository Errors
- `NotInJjRepo` - User tried to run a command outside a JJ repository
- `JjInitFailed` - JJ initialization failed

### 2. File Errors
- `FileNotFound(PathBuf)` - File doesn't exist
- `FileNotFoundNoParents` - File not in working copy and no parents
- `FileNotFoundInParent` - File not in working copy or parent

### 3. Agent/Vendor Errors
- `UnknownAgentType(String)` - Invalid agent type
- `UnsupportedAgentType(String)` - Agent type not yet implemented

### 4. Flow Execution Errors
- `InvalidLetSyntax(String)` - Malformed let binding
- `InvalidVariableName(String)` - Invalid variable identifier
- `ActionFailed` - Action failed with `on_failure: stop`
- `UnknownAikiFunction(String)` - Unknown function call
- `FunctionNotFoundInNamespace(String, String)` - Function not in module
- `UnsupportedFunctionNamespace(String)` - Unsupported namespace
- `InvalidTimeoutFormat(String)` - Bad timeout format

### 5. Scope/Format Errors
- `UnknownScope(String)` - Invalid scope parameter
- `UnknownFormat(String)` - Invalid output format

### 6. Command Execution Errors
- `JjCommandFailed(String)` - JJ command returned error
- `JjStatusFailed(String)` - JJ status check failed
- `GitDiffFailed(String)` - Git diff command failed

### 7. Signing/GPG Errors
- `GpgSmNotSupported` - GPG-SM not yet supported
- `SshKeyNotFound(PathBuf)` - SSH key file missing
- `NoUserEmailConfigured` - Git user.email missing
- `GitUserNotConfigured` - Git user.name or user.email missing
- `GpgKeyIdExtractionFailed` - Cannot extract GPG key ID

### 8. Wrapper Errors
- `Other(anyhow::Error)` - Wrapped anyhow errors
- `Io(std::io::Error)` - Wrapped I/O errors

## Testing

### Error Message Validation

```rust
#[test]
fn test_error_display() {
    let err = AikiError::NotInJjRepo;
    assert_eq!(
        err.to_string(),
        "Not in a JJ repository. Run 'jj init' or 'aiki init' first"
    );
}

#[test]
fn test_unknown_agent_type() {
    let err = AikiError::UnknownAgentType("vscode".to_string());
    assert_eq!(
        err.to_string(),
        "Unknown agent type: 'vscode'. Supported values: 'claude-code', 'cursor'"
    );
}
```

### Integration Tests

All existing integration tests pass, including:
- `test_verify_outside_jj_repo` - Now correctly checks for "Not in a JJ repository"
- Flow execution tests - Type-safe error checking
- Signing tests - Proper error propagation

## Performance Impact

### Before (String Allocation)

```rust
anyhow::bail!("Unknown agent type: '{}'. Supported values: 'claude-code', 'cursor'", agent);
// Allocates: 1x format! allocation + 1x String in Error
```

### After (Zero-Cost Abstraction)

```rust
Err(AikiError::UnknownAgentType(agent.to_string()))
// Allocates: 1x to_string() only (same as before for the agent string)
// Error variant itself is zero-cost
```

**Result**: Similar or better performance due to:
- Fewer intermediate allocations
- Better compiler optimizations (monomorphization of error types)
- Reduced dynamic dispatch

## Migration Patterns

### Pattern 1: Simple Message

```rust
// Before
anyhow::bail!("Not in a JJ repository. Run 'jj init' or 'aiki init' first.");

// After
return Err(AikiError::NotInJjRepo);
```

### Pattern 2: Formatted Message

```rust
// Before
anyhow::bail!("Unknown agent type: '{}'. Supported values: 'claude-code', 'cursor'", agent);

// After
return Err(AikiError::UnknownAgentType(agent.to_string()));
```

### Pattern 3: Path-Based Errors

```rust
// Before
anyhow::bail!("File not found: {}", file_path.display());

// After
return Err(AikiError::FileNotFound(file_path));
```

### Pattern 4: Error Conversion (anyhow interop)

```rust
// Before (in anyhow::Result context)
anyhow::bail!("File not found in working copy and no parents available");

// After (convert to anyhow::Error)
return Err(AikiError::FileNotFoundNoParents.into());
```

### Pattern 5: Error Propagation Across Boundaries

```rust
// Before (anyhow::Result)
fn handle_event(agent: AgentType, event: &str) -> anyhow::Result<()> {
    vendors::claude_code::handle(event)  // Returns anyhow::Result
}

// After (AikiError::Result with conversion)
fn handle_event(agent: AgentType, event: &str) -> Result<()> {
    Ok(vendors::claude_code::handle(event)?)  // Convert via ?
}
```

## Remaining Work (Future Improvements)

### 1. Convert Remaining Modules

These modules still use `anyhow::bail!()`:
- `cli/src/authors.rs` (2 instances)
- `cli/src/config.rs` (1 instance)
- `cli/src/verify.rs` (1 instance)
- `cli/src/signing.rs` (1 instance)
- `cli/src/sign_setup_wizard.rs` (5 instances)
- `cli/src/record_change.rs` (1 instance)
- Test modules (4 instances)

**Effort**: Low, follow existing patterns

### 2. Add Context to Errors

```rust
// Future: Add context fields for better debugging
#[error("Function '{function}' not found in namespace '{namespace}'. Available: {available}")]
FunctionNotFoundInNamespace {
    function: String,
    namespace: String,
    available: Vec<String>,  // List available functions
},
```

### 3. Error Recovery Hints

```rust
#[error("Invalid timeout format: {0}. Use 's', 'm', or 'h' suffix (e.g., '30s', '5m', '1h')")]
InvalidTimeoutFormat(String),
```

**Done**: Most errors already include hints!

## Files Modified

- `cli/src/error.rs` - **New**: Structured error types
- `cli/src/lib.rs` - Export error module
- `cli/Cargo.toml` - Add `thiserror` dependency
- `cli/src/main.rs` - Update to use AikiError, add error display wrapper
- `cli/src/flows/executor.rs` - Replace 7 error sites
- `cli/src/blame.rs` - Replace 2 error sites, add anyhow interop
- `cli/tests/verify_tests.rs` - Remove debug output (test unchanged)

## Conclusion

The structured error types provide:

1. **Type Safety** - Compile-time checking of error handling
2. **Better UX** - Consistent, user-friendly error messages
3. **Maintainability** - Centralized error definitions
4. **Testability** - Easy to test specific error conditions
5. **Performance** - Zero-cost abstractions, no overhead
6. **Extensibility** - Easy to add new error types

This lays the foundation for more sophisticated error handling in future phases, including error recovery, better diagnostics, and structured logging.

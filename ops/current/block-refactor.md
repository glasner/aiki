# Plan: Refactor `on_failure` to Support Nested Actions

## Problem with Current Design

Currently, `on_failure` is a simple enum with three modes:

```yaml
PostFileChange:
  - shell: "cargo test"
    on_failure: block  # ← Limited: can only specify behavior, not actions
```

**Limitations:**
1. **No customization**: Can't emit custom messages when blocking
2. **No conditional logic**: Can't run different actions based on error type
3. **Inconsistent with event model**: Everything else uses actions, but failure handling uses an enum

## Proposed Design: `on_failure` with Nested Actions

Allow `on_failure` to accept a list of actions that run when the action fails:

```yaml
PostFileChange:
  - shell: "cargo test"
    on_failure:
      - error: "Tests failed: ${SHELL.stderr}"
      - block: "Fix the tests before committing"
```

### New `block` Action

Add a new action type that triggers blocking behavior:

```rust
/// Block action - stops the hook and returns exit code 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAction {
    /// The blocking message (goes into Decision::Block)
    pub block: String,
}
```

**Usage:**

```yaml
# Simple block
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "Prompt is too long (${event.prompt.len()} chars)"
      - block: "Reduce prompt to under 10000 characters"

# Block with context
PrePrompt:
  - shell: "validate-prompt.sh '${event.prompt}'"
    on_failure:
      - warning: "Prompt validation failed"
      - info: "Error: ${SHELL.stderr}"
      - block: "Fix validation errors above"
```

## Migration Path

### Phase 1: Support Both Syntaxes

Allow both the old enum syntax and the new action syntax:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFailure {
    /// Legacy: simple mode (continue, stop, block)
    Mode(FailureMode),
    
    /// New: list of actions to execute on failure
    Actions(Vec<Action>),
}
```

**YAML examples:**

```yaml
# Old syntax (still works)
- shell: "test"
  on_failure: block

# New syntax
- shell: "test"
  on_failure:
    - error: "Test failed"
    - block: "Fix the test"
```

### Phase 2: Update Actions to Use OnFailure

Change all action structs:

```rust
// Before:
pub struct ShellAction {
    pub shell: String,
    pub on_failure: FailureMode,  // ❌ Old
}

// After:
pub struct ShellAction {
    pub shell: String,
    #[serde(default = "default_on_failure")]
    pub on_failure: OnFailure,  // ✅ New
}

fn default_on_failure() -> OnFailure {
    OnFailure::Mode(FailureMode::Continue)
}
```

### Phase 3: Update Engine to Execute Failure Actions

Change `execute_actions` to handle the new format:

```rust
// In FlowEngine::execute_actions
for action in actions {
    let result = Self::execute_action(action, context)?;
    
    if !result.success {
        let on_failure = get_on_failure(action);
        
        match on_failure {
            OnFailure::Mode(FailureMode::Continue) => {
                // Log and continue (existing behavior)
                continue_failure_errors.push(error_msg);
            }
            OnFailure::Mode(FailureMode::Stop) => {
                // Stop silently (existing behavior)
                return Ok((FlowResult::FailedStop(error_msg), timing));
            }
            OnFailure::Mode(FailureMode::Block) => {
                // Block operation (existing behavior)
                return Ok((FlowResult::FailedBlock(error_msg), timing));
            }
            OnFailure::Actions(failure_actions) => {
                // Execute the failure actions
                let (failure_result, _) = Self::execute_actions(&failure_actions, context)?;
                
                match failure_result {
                    FlowResult::Success => {
                        // Failure actions succeeded - continue
                        continue;
                    }
                    FlowResult::FailedContinue(msg) => {
                        // Failure actions had warnings - continue
                        continue_failure_errors.push(msg);
                    }
                    FlowResult::FailedStop(msg) => {
                        // Failure actions stopped - propagate
                        return Ok((FlowResult::FailedStop(msg), timing));
                    }
                    FlowResult::FailedBlock(msg) => {
                        // Failure actions blocked - propagate
                        return Ok((FlowResult::FailedBlock(msg), timing));
                    }
                }
            }
        }
    }
}
```

## Implementation of `block` Action

### In types.rs

```rust
/// Block action - stops the hook and returns exit code 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAction {
    /// The blocking message (shown to user and agent)
    pub block: String,
}

// Add to Action enum
pub enum Action {
    // ... existing actions ...
    Block(BlockAction),
}
```

### In engine.rs

```rust
fn execute_block(action: &BlockAction, context: &mut AikiState) -> Result<ActionResult> {
    // Create variable resolver
    let mut resolver = Self::create_resolver(context);
    
    // Resolve variables in message
    let message = resolver.resolve(&action.block);
    
    // Return a special failure that triggers FailedBlock
    Ok(ActionResult {
        success: false,
        exit_code: Some(2),
        stdout: String::new(),
        stderr: message,
    })
}
```

**Key insight:** The `block` action just returns a failure with exit code 2. The engine's failure handling logic sees this and returns `FlowResult::FailedBlock`.

## Benefits

### 1. Composable Failure Handling

```yaml
PostFileChange:
  - shell: "cargo test"
    on_failure:
      - let: test_output = "$SHELL.stderr"
      - if: "$test_output contains 'permission denied'"
        then:
          - error: "Permission error running tests"
          - info: "Try: chmod +x test script"
        else:
          - error: "Tests failed: ${test_output}"
      - block: "Fix the test failures above"
```

### 2. Consistent Mental Model

Everything is actions now:
- Event handlers run actions
- Failure handlers run actions
- Control flow (if/switch) runs actions
- No special cases

### 3. Better Error Messages

```yaml
PrePrompt:
  - shell: "validate-prompt.sh '${event.prompt}'"
    on_failure:
      - warning: "Prompt validation failed"
      - info: "Validator output: ${SHELL.stderr}"
      - info: "Prompt length: ${event.prompt.len()}"
      - block: "Fix validation errors (see above for details)"
```

All four messages appear to the user, giving complete context.

### 4. No Need for `on_failure: block` Validation

The validation problem disappears:
- `block` is just another action
- It can be used anywhere, like `info`, `warning`, `error`
- Event handlers decide whether to respect `Decision::Block`
- No need for YAML-level validation

## Migration Strategy

### Step 1: Add `BlockAction` and `OnFailure` enum
- Add types to `types.rs`
- Keep existing `FailureMode` for backwards compatibility

### Step 2: Update all action structs
- Change `on_failure: FailureMode` to `on_failure: OnFailure`
- Use `#[serde(default)]` to maintain backwards compatibility

### Step 3: Update engine
- Add `execute_block()` function
- Update failure handling to execute `OnFailure::Actions`
- Keep legacy behavior for `OnFailure::Mode`

### Step 4: Update core flow
- Gradually migrate to new syntax
- Show examples of both styles

### Step 5: Deprecation (future)
- Eventually remove `OnFailure::Mode` variant
- All flows use action-based failure handling

## Backwards Compatibility

The `#[serde(untagged)]` enum ensures old YAML still works:

```yaml
# Old syntax - still works
- shell: "test"
  on_failure: block

# Deserialized as: OnFailure::Mode(FailureMode::Block)

# New syntax
- shell: "test"
  on_failure:
    - block: "Tests failed"

# Deserialized as: OnFailure::Actions(vec![Action::Block(...)])
```

## Testing

```rust
#[test]
fn test_on_failure_legacy_block() {
    let yaml = r#"
shell: "false"
on_failure: block
"#;
    let action: ShellAction = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(action.on_failure, OnFailure::Mode(FailureMode::Block)));
}

#[test]
fn test_on_failure_actions() {
    let yaml = r#"
shell: "false"
on_failure:
  - error: "Command failed"
  - block: "Fix the error"
"#;
    let action: ShellAction = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(action.on_failure, OnFailure::Actions(_)));
}

#[test]
fn test_block_action_execution() {
    let mut state = AikiState::new(test_event());
    let action = BlockAction {
        block: "Operation blocked".to_string(),
    };
    
    let result = FlowEngine::execute_block(&action, &mut state).unwrap();
    assert!(!result.success);
    assert_eq!(result.exit_code, Some(2));
    assert_eq!(result.stderr, "Operation blocked");
}
```

## Documentation

Update flow documentation to show new patterns:

```yaml
# Simple blocking with custom message
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "Prompt too long: ${event.prompt.len()} characters"
      - block: "Reduce prompt to under 10000 characters"

# Failure handling with conditional logic
PostFileChange:
  - shell: "cargo test"
    on_failure:
      - if: "$SHELL.exit_code == 101"
        then:
          - error: "Test panicked"
          - info: "Check for unwrap() or panic!() calls"
        else:
          - error: "Tests failed"
          - info: "Run 'cargo test' locally to debug"
      - block: "Fix test failures before committing"

# Multiple error checks
PrepareCommitMessage:
  - shell: "check-commit-msg.sh '${event.commit_msg_file}'"
    on_failure:
      - warning: "Commit message validation failed"
      - info: "${SHELL.stdout}"
      - block: "Follow commit message conventions"
```

## Open Questions

1. **Should `block` have `on_failure`?** 
   - Probably not - it's the terminal action
   - Could allow it for consistency, but would be confusing

2. **What if failure actions themselves fail?**
   - Current design: propagate the failure result
   - Alternative: treat all failure action errors as `FailedStop`

3. **Variable access in failure actions?**
   - Make `$SHELL.exit_code`, `$SHELL.stdout`, `$SHELL.stderr` available
   - Already supported via variable resolver

4. **Should we keep `FailureMode` enum?**
   - Yes for backwards compatibility
   - Can deprecate in future major version

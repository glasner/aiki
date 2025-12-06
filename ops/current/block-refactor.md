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
    /// Error message shown when blocking
    /// Will be emitted as Message::Error if non-empty
    #[serde(rename = "block")]
    pub error: String,
}
```

**Important:** The `block` action triggers blocking, but the actual message shown to users comes from `error`/`warning`/`info` actions:

```yaml
# Simple block
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "Prompt is too long (${event.prompt.len()} chars)"
      - block: ""  # Just triggers blocking

# Better: Include explanation in the error
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "Prompt too long: ${event.prompt.len()} chars (max 10000)"
      - info: "Reduce prompt length before continuing"
      - block: ""

# Or use block's message field for convenience (both approaches work)
PrePrompt:
  - shell: "validate-prompt.sh '${event.prompt}'"
    on_failure:
      - error: "Prompt validation failed"
      - info: "Validator output: ${SHELL.stderr}"
      - block: "Fix validation errors"  # This becomes an info message
```

**Design Decision:** The `block` field could be:
1. **Empty string**: `block: ""` - just triggers blocking, messages come from other actions
2. **Error message**: `block: "message"` - convenience wrapper that emits error + triggers blocking

For simplicity, we'll implement option 2: `block: "message"` emits an error message and triggers blocking.

### Update Decision Enum

The `Decision` enum should be simplified to not carry a message:

```rust
// Before:
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block(String),  // ❌ Message is redundant
}

// After:
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block,  // ✅ No message - it's in HookResponse.messages
}
```

This means handlers no longer need to extract the block message:

```rust
// Before:
FlowResult::FailedBlock(msg) => {
    Ok(HookResponse {
        context: None,
        decision: Decision::Block(msg),  // ❌ msg goes in two places
        messages,
    })
}

// After:
FlowResult::FailedBlock(_) => {
    Ok(HookResponse {
        context: None,
        decision: Decision::Block,  // ✅ Simple flag
        messages,  // ✅ All messages here (including block reason)
    })
}
```

## Implementation

### Step 1: Update Action Structs

Change all action structs to use `Vec<Action>` for `on_failure`:

```rust
// Before:
pub struct ShellAction {
    pub shell: String,
    pub on_failure: FailureMode,  // ❌ Old
}

// After:
pub struct ShellAction {
    pub shell: String,
    #[serde(default)]
    pub on_failure: Vec<Action>,  // ✅ New - empty vec means "continue"
}
```

**Default behavior:** Empty vec = continue execution (no special failure handling)

### Step 2: Update Engine to Execute Failure Actions

Change `execute_actions` to execute failure actions:

```rust
// In FlowEngine::execute_actions
for action in actions {
    let result = Self::execute_action(action, context)?;
    
    if !result.success {
        let on_failure = get_on_failure(action);
        
        if on_failure.is_empty() {
            // No failure actions - default to continue
            let error_msg = if !result.stderr.is_empty() {
                result.stderr.clone()
            } else {
                "Action failed".to_string()
            };
            eprintln!("[aiki] Action failed but continuing: {}", error_msg);
            continue_failure_errors.push(error_msg);
        } else {
            // Execute the failure actions
            let (failure_result, _) = Self::execute_actions(&on_failure, context)?;
            
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
```

## Implementation of `block` Action

### In types.rs

```rust
/// Block action - stops the hook and returns exit code 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAction {
    /// Error message shown when blocking
    #[serde(rename = "block")]
    pub error: String,
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
    
    // Resolve variables in error message
    let error = resolver.resolve(&action.error);
    
    // If error message is non-empty, emit it
    if !error.is_empty() {
        context.add_message(crate::handlers::Message::Error(error));
    }
    
    // Return a special failure that triggers FailedBlock
    // The exit code signals blocking, but no message in stderr
    Ok(ActionResult {
        success: false,
        exit_code: Some(2),
        stdout: String::new(),
        stderr: String::new(),  // ← No message here, it's in context.messages
    })
}
```

**Key insight:** 
- The `block` action emits an optional error message (if provided)
- It returns a failure with exit code 2 to signal blocking
- The actual messages shown to users are in `context.messages`, not in the failure result

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

## Implementation Steps

### Step 1: Update Decision enum in handlers.rs
- Change `Decision::Block(String)` to `Decision::Block`
- Remove message parameter from Block variant
- Update `block_message()` helper method (or remove it)

### Step 2: Add `BlockAction` and `StopAction` to types.rs
- Add `BlockAction` struct
- Add `StopAction` struct (for silent stop behavior)
- Add `Block(BlockAction)` and `Stop(StopAction)` variants to `Action` enum
- Remove `FailureMode` enum (no longer needed)

### Step 3: Update all action structs
- Change `on_failure: FailureMode` to `on_failure: Vec<Action>`
- Update all 12 action types (If, Switch, Shell, Jj, Let, Self, Context, Autoreply, CommitMessage, Info, Warning, Error)
- Remove `default_on_failure()` function

### Step 4: Update engine.rs
- Add `execute_block()` function
- Add `execute_stop()` function
- Update failure handling in `execute_actions()` to execute failure action lists
- Remove old failure mode match logic

### Step 5: Update handlers.rs
- Update all handlers to use `Decision::Block` without message
- Handlers now just check `decision.is_block()` and don't extract message

### Step 6: Update core flow
- Replace all `on_failure: continue/stop/block` with action-based syntax
- Add examples showing the new patterns

### Step 7: Update tests
- Update all existing tests to use new syntax
- Add tests for `block` and `stop` actions
- Add tests for complex failure handling patterns
- Verify all 182+ tests still pass

## Testing

```rust
#[test]
fn test_on_failure_default_empty() {
    let yaml = r#"
shell: "echo test"
"#;
    let action: ShellAction = serde_yaml::from_str(yaml).unwrap();
    assert!(action.on_failure.is_empty());
}

#[test]
fn test_on_failure_with_actions() {
    let yaml = r#"
shell: "false"
on_failure:
  - error: "Command failed"
  - block: "Fix the error"
"#;
    let action: ShellAction = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(action.on_failure.len(), 2);
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

#[test]
fn test_failure_actions_trigger_block() {
    let yaml = r#"
name: test
PostFileChange:
  - shell: "false"
    on_failure:
      - error: "Command failed"
      - block: "Fix the error"
"#;
    let flow: Flow = serde_yaml::from_str(yaml).unwrap();
    let mut state = AikiState::new(test_event());
    
    let (result, _) = FlowEngine::execute_actions(&flow.post_file_change, &mut state).unwrap();
    assert!(matches!(result, FlowResult::FailedBlock(_)));
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
   - No - `block` doesn't have `on_failure` since it's the terminal action
   - Keeps the API simple and clear

2. **What if failure actions themselves fail?**
   - Design: propagate the failure result
   - If a failure action uses `block`, the whole flow blocks
   - If a failure action fails without `block`, it continues (via FailedContinue)

3. **Variable access in failure actions?**
   - Need to make the failed action's result available as variables
   - Options:
     - `$FAILED.exit_code`, `$FAILED.stdout`, `$FAILED.stderr`
     - Or use the action's alias: `$my_action.exit_code` if it had one
   - Decision: Use the action's alias if available, otherwise no special variables

4. **How to implement "stop" behavior?**
   - Old: `on_failure: stop` (silent failure)
   - New: `on_failure: []` (empty list = continue, but silent)
   - Or add a `stop` action that returns `FailedStop`?
   - **Decision**: Empty `on_failure` means "continue", add `stop` action for silent stop

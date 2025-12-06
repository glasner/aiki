# Plan: Validate `on_failure: block` in YAML Parsing

## Problem

Currently, `on_failure: block` is allowed on any action in any event, but semantically it only makes sense for certain events:

| Event | Can Block? | Reason |
|-------|-----------|--------|
| **SessionStart** | ✅ | Can prevent session from starting |
| **PrePrompt** | ✅ | Can block prompt from being sent to agent |
| **PrepareCommitMessage** | ✅ | Can prevent Git commit |
| **PreFileChange** | ❌ | Too late - change request already received |
| **PostFileChange** | ❌ | Too late - files already modified |
| **PostResponse** | ❌ | Too late - agent response already sent |

Currently, if you write:

```yaml
PostFileChange:
  - jj: "describe -m '[aiki]metadata'"
    on_failure: block  # ❌ Doesn't make sense - files already changed!
```

The flow loads successfully, but `FailedBlock` is silently treated as `Allow` in the handler. This is confusing.

## Solution: Validate During YAML Parsing with Serde

Use `#[serde(deserialize_with = "...")]` to validate during deserialization, giving users immediate feedback with line numbers.

### Implementation

**Step 1: Add validation helper functions**

```rust
// In cli/src/flows/types.rs

/// Check if an action has on_failure: block
fn has_block_failure_mode(action: &Action) -> bool {
    match action {
        Action::If(a) => a.on_failure == FailureMode::Block,
        Action::Switch(a) => a.on_failure == FailureMode::Block,
        Action::Shell(a) => a.on_failure == FailureMode::Block,
        Action::Jj(a) => a.on_failure == FailureMode::Block,
        Action::Let(a) => a.on_failure == FailureMode::Block,
        Action::Self_(a) => a.on_failure == FailureMode::Block,
        Action::Context(a) => a.on_failure == FailureMode::Block,
        Action::Autoreply(a) => a.on_failure == FailureMode::Block,
        Action::CommitMessage(a) => a.on_failure == FailureMode::Block,
        Action::Info(a) => a.on_failure == FailureMode::Block,
        Action::Warning(a) => a.on_failure == FailureMode::Block,
        Action::Error(a) => a.on_failure == FailureMode::Block,
        Action::Log(_) => false, // Log doesn't have on_failure
    }
}

/// Deserialize actions that cannot use on_failure: block
fn deserialize_non_blocking_actions<'de, D>(
    deserializer: D,
) -> Result<Vec<Action>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let actions = Vec::<Action>::deserialize(deserializer)?;
    
    for (idx, action) in actions.iter().enumerate() {
        if has_block_failure_mode(action) {
            return Err(serde::de::Error::custom(format!(
                "on_failure: block not allowed in this event (action {}). \
                 Use 'continue' or 'stop' instead.",
                idx + 1
            )));
        }
    }
    
    Ok(actions)
}
```

**Step 2: Apply to non-blocking event fields**

```rust
// In cli/src/flows/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    
    #[serde(default)]
    pub description: Option<String>,
    
    #[serde(default = "default_version")]
    pub version: String,
    
    // Can block ✅
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<Action>,
    
    // Can block ✅
    #[serde(rename = "PrePrompt", default)]
    pub pre_prompt: Vec<Action>,
    
    // Cannot block ❌
    #[serde(
        rename = "PreFileChange",
        default,
        deserialize_with = "deserialize_non_blocking_actions"
    )]
    pub pre_file_change: Vec<Action>,
    
    // Cannot block ❌
    #[serde(
        rename = "PostFileChange",
        default,
        deserialize_with = "deserialize_non_blocking_actions"
    )]
    pub post_file_change: Vec<Action>,
    
    // Cannot block ❌
    #[serde(
        rename = "PostResponse",
        default,
        deserialize_with = "deserialize_non_blocking_actions"
    )]
    pub post_response: Vec<Action>,
    
    // Can block ✅
    #[serde(rename = "PrepareCommitMessage", default)]
    pub prepare_commit_message: Vec<Action>,
    
    // Can block ✅ (for cleanup/shutdown operations)
    #[serde(rename = "Stop", default)]
    pub stop: Vec<Action>,
}
```

## Benefits

### 1. Immediate Feedback with Line Numbers

**Before:**
```
# Flow loads successfully, fails silently at runtime
```

**After:**
```
Error: on_failure: block not allowed in this event (action 2). Use 'continue' or 'stop' instead.
  --> flows/core.yaml:42:15
   |
42 |       on_failure: block
   |                   ^^^^^
```

### 2. Type-Level Constraints

The constraint is encoded in the type definition - you can't deserialize an invalid Flow.

### 3. Automatic Validation

Happens automatically whenever a Flow is loaded from YAML. No need to remember to call a separate validation function.

### 4. Future-Proof

If we add new event types, we just add `deserialize_with` to non-blocking ones.

## Testing

Add tests to verify validation works:

```rust
#[test]
fn test_block_not_allowed_in_post_file_change() {
    let yaml = r#"
name: test
PostFileChange:
  - shell: "echo test"
    on_failure: block
"#;
    
    let result: Result<Flow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("on_failure: block not allowed"));
}

#[test]
fn test_block_allowed_in_pre_prompt() {
    let yaml = r#"
name: test
PrePrompt:
  - shell: "echo test"
    on_failure: block
"#;
    
    let result: Result<Flow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_ok());
}

#[test]
fn test_stop_allowed_in_all_events() {
    let yaml = r#"
name: test
PostFileChange:
  - shell: "echo test"
    on_failure: stop
"#;
    
    let result: Result<Flow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_ok());
}
```

## Implementation Checklist

- [ ] Add `has_block_failure_mode()` helper function in `cli/src/flows/types.rs`
- [ ] Add `deserialize_non_blocking_actions()` function in `cli/src/flows/types.rs`
- [ ] Update `Flow` struct to use `deserialize_with` for non-blocking events:
  - [ ] `pre_file_change`
  - [ ] `post_file_change`
  - [ ] `post_response`
- [ ] Add unit tests for validation
- [ ] Update documentation to clarify which events can block
- [ ] Run all tests to ensure no regressions

## Documentation Updates

Update `flows/core.yaml` comments to clarify blocking behavior:

```yaml
# SessionStart - can use on_failure: block to prevent session from starting
SessionStart: []

# PrePrompt - can use on_failure: block to prevent prompt from being sent
PrePrompt: []

# PreFileChange - cannot block (use 'stop' or 'continue' instead)
PreFileChange: []

# PostFileChange - cannot block (use 'stop' or 'continue' instead)
PostFileChange: []

# PostResponse - cannot block (use 'stop' or 'continue' instead)
PostResponse: []

# PrepareCommitMessage - can use on_failure: block to prevent commit
PrepareCommitMessage: []
```

## Edge Cases

1. **Nested actions (if/switch)**: The validation only checks top-level actions. Nested actions inside `if` or `switch` branches aren't validated. This is acceptable because:
   - The outer action's `on_failure` controls the overall behavior
   - Inner actions inherit the context from the outer action
   - Adding deep validation would be complex and likely unnecessary

2. **Empty action lists**: Default empty vectors are fine (no actions = nothing to validate)

3. **Custom flows**: User-defined flows in `~/.config/aiki/flows/` get the same validation automatically

## Future Enhancements

If we want to validate nested actions in the future, we could add a recursive helper:

```rust
fn validate_action_tree(action: &Action, can_block: bool) -> Result<(), String> {
    if !can_block && has_block_failure_mode(action) {
        return Err("on_failure: block not allowed".to_string());
    }
    
    match action {
        Action::If(if_action) => {
            for a in &if_action.then {
                validate_action_tree(a, can_block)?;
            }
            if let Some(else_actions) = &if_action.else_ {
                for a in else_actions {
                    validate_action_tree(a, can_block)?;
                }
            }
        }
        Action::Switch(switch_action) => {
            for actions in switch_action.cases.values() {
                for a in actions {
                    validate_action_tree(a, can_block)?;
                }
            }
            // ... validate default case too
        }
        _ => {}
    }
    
    Ok(())
}
```

But this is likely overkill for now.

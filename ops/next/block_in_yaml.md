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

/// Recursively check if an action or any nested actions use on_failure: block
fn has_block_failure_mode(action: &Action) -> bool {
    // Check this action's on_failure
    let this_has_block = match action {
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
    };
    
    if this_has_block {
        return true;
    }
    
    // Check nested actions
    match action {
        Action::If(if_action) => {
            // Check then branch
            if if_action.then.iter().any(has_block_failure_mode) {
                return true;
            }
            // Check else branch
            if let Some(else_actions) = &if_action.else_ {
                if else_actions.iter().any(has_block_failure_mode) {
                    return true;
                }
            }
        }
        Action::Switch(switch_action) => {
            // Check all case branches
            for actions in switch_action.cases.values() {
                if actions.iter().any(has_block_failure_mode) {
                    return true;
                }
            }
            // Check default branch
            if let Some(default_actions) = &switch_action.default {
                if default_actions.iter().any(has_block_failure_mode) {
                    return true;
                }
            }
        }
        _ => {}
    }
    
    false
}

/// Deserialize actions that cannot use on_failure: block (validates recursively)
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
                "on_failure: block not allowed in this event (found in action {} or its nested actions). \
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
fn test_block_not_allowed_in_nested_actions() {
    let yaml = r#"
name: test
PostFileChange:
  - if: "$test_result.exit_code != 0"
    then:
      - warning: "Tests failed"
        on_failure: block  # ← Should be caught!
    on_failure: continue
"#;
    
    let result: Result<Flow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("on_failure: block not allowed"));
}

#[test]
fn test_block_not_allowed_in_switch_cases() {
    let yaml = r#"
name: test
PostResponse:
  - switch: "$detection.classification"
    cases:
      "exact_match":
        - info: "Exact match detected"
          on_failure: block  # ← Should be caught!
    on_failure: continue
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
fn test_block_allowed_in_nested_actions_of_blocking_events() {
    let yaml = r#"
name: test
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "Prompt too long"
        on_failure: block  # ← OK in PrePrompt
    on_failure: continue
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

1. **Nested actions (if/switch)**: The validation recursively checks nested actions in `if` branches (then/else) and `switch` branches (cases/default). This catches errors like:
   ```yaml
   PostFileChange:
     - if: "$test_failed"
       then:
         - error: "Tests failed"
           on_failure: block  # ← Caught by recursive validation!
   ```

2. **Empty action lists**: Default empty vectors are fine (no actions = nothing to validate)

3. **Custom flows**: User-defined flows in `~/.config/aiki/flows/` get the same validation automatically

4. **Error messages**: When nested actions fail validation, the error message indicates which top-level action contains the problem:
   ```
   Error: on_failure: block not allowed in this event (found in action 2 or its nested actions)
   ```
   This could be improved in the future to show the exact path (e.g., "action 2 → then branch → action 1"), but the current message is sufficient to locate the issue.

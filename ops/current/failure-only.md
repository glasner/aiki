# Failure-Only Message Refactoring

## Overview

Simplify the message action system by removing `info`, `warning`, and `error` actions, keeping only `FailureMessage` for reporting problems. Informational messages should use `context` instead.

## Current State

**Message Types in YAML:**
- `info:` - general informational messages → `Message::Info`
- `warning:` - potential issues → `Message::Warning`
- `error:` - things that went wrong → `Message::Error`
- `continue:` - flow control with warning
- `stop:` - flow control with warning
- `block:` - flow control with error

**Problems:**
1. The distinction between `info`, `warning`, and `error` is subjective and unclear
2. Writers have to make judgment calls about severity that don't really matter
3. Most "info" messages are really just context about what happened
4. The engine has to handle 4 different message types when it really just needs to know: "did this fail?"

## Proposed State

**Single Message Type:**
- `Message::Failure(String)` - the only message type for reporting problems

**How failures are created:**

### 1. Default behavior (implicit `on_failure: "continue"`)
```yaml
- shell: "some-command"
  # If command fails, adds stderr as FailureMessage and continues
```

### 2. Explicit on_failure handlers
```yaml
- shell: "some-command"
  on_failure: "continue"  # Adds stderr as FailureMessage, continues (default)

- shell: "some-command"
  on_failure: "stop"      # Adds stderr as FailureMessage, stops flow

- shell: "some-command"
  on_failure: "block"     # Adds stderr as FailureMessage, blocks operation
```

### 3. Explicit flow control with custom messages
```yaml
- continue: "Ignoring this issue"  # Adds custom message as FailureMessage, continues
- stop: "User cancelled"           # Adds custom message as FailureMessage, stops
- block: "Invalid format"          # Adds custom message as FailureMessage, blocks
```

### 4. Informational messages
```yaml
# OLD (removed):
- info: "Running validation checks"
- warning: "File may need manual review"

# NEW:
- context: "Running validation checks. File may need manual review."
```

## Implementation Plan

### 1. Replace `Message` enum with `Failure` struct in `handlers.rs`

**Before:**
```rust
pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
}

pub struct HookResponse {
    pub context: Option<String>,
    pub decision: Decision,
    pub messages: Vec<Message>,
}
```

**After:**
```rust
pub struct Failure(pub String);

pub struct HookResponse {
    pub context: Option<String>,
    pub decision: Decision,
    pub failures: Vec<Failure>,
}
```

**Impact:**
- Update `HookResponse::format_messages()` to only format failures
- Remove `with_info()`, `with_warning()`, `with_error()` methods
- Add `with_failure()` method
- Update all constructors that create messages
- Update `is_success()` to check `failures.is_empty()`

### 2. Update `types.rs` action types

**Remove these action types:**
```rust
pub struct InfoAction { ... }
pub struct WarningAction { ... }
pub struct ErrorAction { ... }
```

**Update these action types:**
```rust
// Continue: generates Failure, continues
pub struct ContinueAction {
    #[serde(rename = "continue")]
    pub failure: String,  // Renamed from 'warning'
}

// Stop: generates Failure, stops
pub struct StopAction {
    #[serde(rename = "stop")]
    pub failure: String,  // Renamed from 'warning'
}

// Block: generates Failure, blocks
pub struct BlockAction {
    #[serde(rename = "block")]
    pub failure: String,  // Renamed from 'error'
}
```

**Remove from `Action` enum:**
```rust
Info(InfoAction),
Warning(WarningAction),
Error(ErrorAction),
```

### 3. Update `engine.rs` execution

**Remove these methods:**
```rust
fn execute_info() -> Result<ActionResult>
fn execute_warning() -> Result<ActionResult>
fn execute_error() -> Result<ActionResult>
```

**Update these methods:**
```rust
fn execute_continue(action: &ContinueAction, state: &mut AikiState) -> Result<ActionResult> {
    let mut resolver = Self::create_resolver(state);
    let failure = resolver.resolve(&action.failure);
    
    // Add failure to state only if non-empty
    if !failure.is_empty() {
        state.add_failure(Failure(failure));
    }
    
    Ok(ActionResult::success())
}

fn execute_stop(action: &StopAction, state: &mut AikiState) -> Result<ActionResult> {
    let mut resolver = Self::create_resolver(state);
    let failure = resolver.resolve(&action.failure);
    
    // Add failure to state only if non-empty
    if !failure.is_empty() {
        state.add_failure(Failure(failure.clone()));
    }
    
    // Return failure to trigger stop behavior
    Ok(ActionResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: failure,
    })
}

fn execute_block(action: &BlockAction, state: &mut AikiState) -> Result<ActionResult> {
    let mut resolver = Self::create_resolver(state);
    let failure = resolver.resolve(&action.failure);
    
    // Add failure to state only if non-empty
    if !failure.is_empty() {
        state.add_failure(Failure(failure.clone()));
    }
    
    // Return failure to trigger block behavior
    Ok(ActionResult {
        success: false,
        exit_code: Some(2),
        stdout: String::new(),
        stderr: failure,
    })
}
```

**Update on_failure handling:**

Currently, the default on_failure behavior is to log an error and continue:
```rust
// If no callbacks, default to continue behavior
if on_failure_callbacks.is_empty() {
    let failure_text = if !result.stderr.is_empty() {
        result.stderr.clone()
    } else {
        "Action failed".to_string()
    };
    eprintln!("[aiki] Action failed but continuing: {}", failure_text);
    state.add_failure(Failure(failure_text.clone()));
    continue_failure_errors.push(failure_text);
    continue;
}
```

**Support string-based on_failure:**

Add support for `on_failure: "stop"` and `on_failure: "block"` as shortcuts:

```rust
// Parse on_failure - can be either actions or a string shortcut
enum OnFailureBehavior {
    Actions(Vec<Action>),
    Stop,
    Block,
    Continue, // Explicit continue (same as default)
}

fn parse_on_failure(callbacks: &[Action]) -> OnFailureBehavior {
    // If callbacks is a single Continue/Stop/Block action, extract it
    if callbacks.len() == 1 {
        match &callbacks[0] {
            Action::Continue(_) => return OnFailureBehavior::Continue,
            Action::Stop(_) => return OnFailureBehavior::Stop,
            Action::Block(_) => return OnFailureBehavior::Block,
            _ => {}
        }
    }
    OnFailureBehavior::Actions(callbacks.to_vec())
}
```

Wait, actually the YAML would be:
```yaml
on_failure: "stop"  # String
```

But our current structure is:
```rust
pub on_failure: Vec<Action>
```

So we need to change the deserialization to support both string shortcuts and action arrays.

Let's add a new type:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFailure {
    /// Shortcut: "continue", "stop", or "block"
    Shortcut(String),
    /// Full action list
    Actions(Vec<Action>),
}

impl Default for OnFailure {
    fn default() -> Self {
        OnFailure::Shortcut("continue".to_string())
    }
}
```

Then update all action types:
```rust
pub struct ShellAction {
    pub shell: String,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub on_failure: OnFailure,  // Changed from Vec<Action>
    #[serde(default)]
    pub alias: Option<String>,
}
```

### 4. Update `state.rs`

**Before:**
```rust
pub fn add_message(&mut self, message: Message) {
    self.messages.push(message);
}
```

**After:**
```rust
pub fn add_failure(&mut self, failure: Failure) {
    self.failures.push(failure);
}
```

Update the field type as well:
```rust
pub struct AikiState {
    // ...
    pub failures: Vec<Failure>,  // Was: messages: Vec<Message>
}
```

### 5. Update format_messages in handlers.rs

**Before:**
```rust
pub fn format_messages(&self) -> String {
    let mut parts = vec![];
    for msg in &self.messages {
        match msg {
            Message::Info(s) => parts.push(format!("ℹ️ {}", s)),
            Message::Warning(s) => parts.push(format!("⚠️ {}", s)),
            Message::Error(s) => parts.push(format!("❌ {}", s)),
        }
    }
    parts.join("\n\n")
}
```

**After:**
```rust
pub fn format_messages(&self) -> String {
    self.failures
        .iter()
        .map(|Failure(s)| format!("❌ {}", s))
        .collect::<Vec<_>>()
        .join("\n\n")
}
```

### 6. Update example flows and tests

Search for:
- `info:` actions
- `warning:` actions  
- `error:` actions

Replace with appropriate `context:` or flow control actions.

## Benefits

1. **Clearer semantics** - `Message::Failure` means "something went wrong", everything else is context
2. **Simpler mental model** - No more debating if something is a warning vs error
3. **Better separation of concerns** - Context goes in context, failures indicate problems
4. **Easier to write** - Less decision fatigue when authoring flows
5. **Consistent with action-based model** - `continue`/`stop`/`block` all generate failures, just with different control flow

## Migration Guide

**Old:**
```yaml
- info: "Starting validation"
- warning: "File may need review"
- error: "Validation failed"
- shell: "some-command"
  on_failure:
    - error: "Command failed"
```

**New:**
```yaml
- context: "Starting validation. File may need review."
- shell: "some-command"
  on_failure: "stop"  # Uses stderr as failure text
# Or:
- shell: "some-command"
  on_failure:
    - stop: "Custom failure text"
```

## Implementation Order

1. ✅ Document plan (this file)
2. Add `OnFailure` enum to `types.rs`
3. Update all action types to use `OnFailure` instead of `Vec<Action>`
4. Update `Message` enum to only have `Failure` variant
5. Update `HookResponse` methods
6. Remove `InfoAction`, `WarningAction`, `ErrorAction` from `types.rs`
7. Remove from `Action` enum
8. Update `engine.rs` to handle `OnFailure` enum
9. Remove `execute_info`, `execute_warning`, `execute_error`
10. Update `execute_continue`, `execute_stop`, `execute_block`
11. Update default on_failure behavior to add Failure
12. Update example flows
13. Update tests

## Critical Review

### User Experience: Before vs After

**BEFORE - Multiple message types create confusion:**
```yaml
PostFileChange:
  - info: "Validating changes..."        # What does this do?
  - warning: "Missing test coverage"     # Is this blocking?
  - error: "Validation failed"           # Does this stop execution?
  - shell: "run-tests"
    on_failure:
      - error: "Tests failed"            # Redundant with stderr
```

User sees:
```
ℹ️ Validating changes...
⚠️ Missing test coverage
❌ Validation failed
❌ Tests failed
```

**Issues:**
1. Unclear which messages block vs continue
2. `info`/`warning`/`error` don't control flow - just visual noise
3. Redundant with stderr output
4. Users confused about severity vs behavior
5. No clear mental model for when to use which

**AFTER - Clear flow control with failures:**
```yaml
PostFileChange:
  - context: "Validating changes..."
  - shell: "check-coverage"
    on_failure: "continue"  # Default: logs stderr as failure, continues
  - shell: "run-tests"
    on_failure: "block"     # Logs stderr as failure, blocks operation
```

User sees (if coverage check fails):
```
❌ Coverage below 80%
❌ Tests failed
[Operation blocked]
```

Or with custom messages:
```yaml
PostFileChange:
  - shell: "check-coverage"
    on_failure:
      - continue: "Coverage low but acceptable"
  - shell: "run-tests"
    on_failure:
      - block: "Tests must pass before committing"
```

**Improvements:**
1. ✅ Clear flow control: `continue`/`stop`/`block` do what they say
2. ✅ Single failure type - no confusion about severity
3. ✅ Context for informational messages (doesn't imply failure)
4. ✅ Failures always mean "something went wrong"
5. ✅ Simple mental model: did it fail? → Failure. Need info? → Context.

### API Clarity

**BEFORE:**
```rust
state.add_message(Message::Info("..."));     // Doesn't indicate failure
state.add_message(Message::Warning("..."));  // Might be failure?
state.add_message(Message::Error("..."));    // Definitely failure?
```

Unclear: Does `Error` stop execution? What's the difference between `Warning` and `Error`?

**AFTER:**
```rust
state.add_failure(Failure("..."));  // Always means: something failed
```

Clear: A failure was recorded. Flow control is separate (continue/stop/block).

### Breaking Changes

**Impact: HIGH - This is a breaking change**

All existing flows using `info:`, `warning:`, or `error:` actions will need updates:

1. **Replace informational messages:**
   ```yaml
   # OLD
   - info: "Processing files..."
   # NEW
   - context: "Processing files..."
   # Or just remove if not user-facing
   ```

2. **Replace warnings/errors that continue:**
   ```yaml
   # OLD
   - warning: "Missing documentation"
   # NEW
   - continue: "Missing documentation"
   ```

3. **Replace errors that should block:**
   ```yaml
   # OLD
   - error: "Invalid format"
   # NEW
   - block: "Invalid format"
   ```

**Migration complexity: MEDIUM**
- Pattern is clear and mechanical
- Can provide migration script
- Most flows probably don't use these actions yet (early stage)

### Potential Issues

1. **Loss of semantic information**
   - BEFORE: Three severity levels (info/warning/error)
   - AFTER: One failure type
   - **Assessment:** Not a real loss - severity never controlled behavior anyway
   - **Mitigation:** Failures always mean "something went wrong", context provides info

2. **No visual distinction for "soft" vs "hard" failures**
   - BEFORE: ⚠️ warnings vs ❌ errors
   - AFTER: ❌ all failures
   - **Assessment:** Visual distinction was misleading - didn't match behavior
   - **Mitigation:** Flow control keywords (continue/stop/block) make behavior explicit

3. **Verbosity for custom failure messages**
   - BEFORE: `- error: "message"`
   - AFTER: `on_failure: [{stop: "message"}]` (array form)
   - **Assessment:** String shortcut helps: `on_failure: "stop"`
   - **Mitigation:** Common case (use stderr) is one word

### Strengths of This Design

1. **Single responsibility:** Failures = things that went wrong
2. **Flow control is explicit:** continue/stop/block are keywords, not message types
3. **Less cognitive load:** One decision instead of three (info vs warning vs error)
4. **Aligns with behavior:** Message types now match what they do
5. **Simpler implementation:** One message type, less code
6. **Better error model:** stderr → Failure is clear and automatic

### Weaknesses

1. **No severity levels:** Can't distinguish "minor issue" from "critical failure"
   - **Counter:** We can add severity later to `Failure` if needed: `Failure { text: String, severity: Severity }`
   - **Counter:** Current severity (info/warning/error) doesn't affect behavior, so removing it is honest

2. **Breaking change:** Existing flows need updates
   - **Counter:** Aiki is pre-1.0, breaking changes are expected
   - **Counter:** Migration is mechanical and can be scripted

3. **All failures look the same:** ❌ for everything
   - **Counter:** User can distinguish by reading the text
   - **Counter:** We could add emoji to custom messages if needed

### Recommendation

**✅ PROCEED with this design**

**Rationale:**
1. The current info/warning/error system is confusing and doesn't match behavior
2. This simplification makes flow control explicit and predictable
3. The migration path is clear
4. We can add severity to `Failure` later if truly needed (YAGNI for now)
5. Single Failure type is the honest representation of what these messages mean

**Follow-up considerations:**
- Monitor real-world usage to see if severity is actually needed
- Consider adding `Failure { text: String, severity: Option<Severity> }` if users need it
- Could add emoji shorthand in failure text: `"⚠️ Coverage low"` vs `"❌ Tests failed"`

## Questions / Decisions

1. **Should `continue:` with no failure text be allowed?**
   - Proposal: Yes, empty string is fine (won't add a failure)
   
2. **Should we keep emoji prefixes?**
   - Proposal: Yes, keep ❌ for all failures
   - Alternative: Let users add emoji in their text if they want distinction

3. **What about informational output?**
   - Use `context:` for user-facing information
   - Or just use `log:` for debugging

4. **Should we version this change?**
   - Proposal: No, we're pre-1.0, just document migration in changelog
   - Update flow version from "1" to "2" and reject old formats

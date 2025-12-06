# Exit Code Refactoring Plan

## Problem Statement

Currently, we use numeric exit codes (0, 2) throughout the system to represent semantic meanings:
- `exit_code: 0` = success or non-blocking error
- `exit_code: 2` = blocking error

This is problematic because:

1. **Semantic meaning is lost** - Code uses magic numbers instead of clear intent
2. **Vendor-specific concerns leak into core** - Exit codes are a vendor protocol detail, not a domain concept
3. **Confusing dual meaning** - Exit code 0 means both "success" and "non-blocking failure"
4. **Hard to extend** - Adding new response types requires picking new magic numbers
5. **Type safety lost** - No compiler help distinguishing response types

## Proposed Solution

**Key Insight:** `FlowResult` already models the decision semantics we need!

```rust
pub enum FlowResult {
    Success,                  // → Allow operation
    FailedContinue(String),   // → Allow with errors (logged)
    FailedStop(String),       // → Allow (silent failure)
    FailedBlock(String),      // → Block operation
}
```

Instead of creating a new enum, **use `FlowResult` directly in `HookResponse`**.

### Why FlowResult Is Perfect

1. **Already exists** - No new types needed
2. **Semantic names** - Clear intent (Success, FailedBlock, etc.)
3. **Carries error messages** - String payloads for Failed* variants
4. **Type-safe** - Compiler enforces exhaustive matching
5. **Domain-appropriate** - Represents flow validation results

### Flow Through System

```
FlowResult (flows/engine.rs) - semantic decision from flow validation
    ↓
HookResponse { result: FlowResult } (handlers.rs) - carries decision + context/messages
    ↓
event_bus::dispatch() - passes through unchanged
    ↓
Vendor Translators (vendors/*.rs)
    ↓
Match on FlowResult variants → vendor-specific format + exit code
```

## Implementation Plan

### Phase 1: Update HookResponse to Use FlowResult

No new enum needed! We'll use the existing `FlowResult` from `flows/engine.rs`.

### Phase 2: Update HookResponse Structure

**File:** `cli/src/handlers.rs`

**Before:**
```rust
pub struct HookResponse {
    pub context: Option<String>,
    pub exit_code: i32,  // ← Remove this
    pub messages: Vec<Message>,
}
```

**After:**
```rust
use crate::flows::FlowResult;

pub struct HookResponse {
    pub context: Option<String>,
    pub result: FlowResult,  // ← Add this (use existing FlowResult)
    pub messages: Vec<Message>,
}

impl HookResponse {
    // Update constructors
    pub fn success() -> Self {
        Self {
            context: None,
            result: FlowResult::Success,
            messages: vec![],
        }
    }
    
    pub fn blocking_failure(user_msg: String, agent_msg: Option<String>) -> Self {
        Self {
            context: agent_msg,
            result: FlowResult::FailedBlock(user_msg.clone()),
            messages: vec![Message::Error(user_msg)],
        }
    }
    
    pub fn failure(user_msg: String, agent_msg: Option<String>) -> Self {
        Self {
            context: agent_msg,
            result: FlowResult::FailedContinue(user_msg.clone()),
            messages: vec![Message::Error(user_msg)],
        }
    }
    
    // Update helper methods
    pub fn is_blocking(&self) -> bool {
        matches!(self.result, FlowResult::FailedBlock(_))
    }
    
    pub fn is_success(&self) -> bool {
        matches!(self.result, FlowResult::Success)
    }
}
```

### Phase 3: Update Handlers to Pass FlowResult Through

**File:** `cli/src/handlers.rs`

Update all handler functions to pass `FlowResult` directly into `HookResponse`:

```rust
// In handle_pre_prompt, handle_pre_file_change, etc.
// Now much simpler - just pass the FlowResult through!
match flow_result {
    FlowResult::Success => HookResponse {
        result: FlowResult::Success,
        context: response.context,
        messages: response.messages,
    },
    
    FlowResult::FailedBlock(msg) => HookResponse {
        result: FlowResult::FailedBlock(msg.clone()),
        context: Some("Fix the validation error before continuing.".to_string()),
        messages: vec![Message::Error(format!("❌ Prompt blocked: {}", msg))],
    },
    
    FlowResult::FailedContinue(msg) => HookResponse {
        result: FlowResult::FailedContinue(msg.clone()),
        context: None,
        messages: vec![Message::Warning(msg)],
    },
    
    FlowResult::FailedStop(msg) => HookResponse {
        result: FlowResult::FailedStop(msg),
        context: None,
        messages: vec![],  // Silent failure
    },
}
```

**Note:** We can now handle all four `FlowResult` variants properly. Previously, `FailedStop` was mapped to exit code 0 with no distinction from Success.

### Phase 4: Update Vendor Translators

**File:** `cli/src/vendors/claude_code.rs`

Match on `FlowResult` variants to translate to Claude Code protocol:

```rust
fn translate_user_prompt_submit(response: &HookResponse) -> ClaudeCodeResponse {
    match &response.result {
        FlowResult::FailedBlock(msg) => {
            // Block the prompt
            let reason = response.format_messages();
            let mut json_value = json!({
                "decision": "block",
                "reason": reason
            });
            
            // Add hookSpecificOutput if there's context
            if let Some(ref ctx) = response.context {
                json_value["hookSpecificOutput"] = json!({
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": ctx
                });
            }
            
            ClaudeCodeResponse {
                json_value: Some(json_value),
                exit_code: 0,  // Claude Code always exits 0
            }
        }
        
        FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => {
            // Allow with optional context
            // All non-blocking results allow the operation to proceed
            let combined = response.combined_output();
            let json_value = if let Some(ctx) = combined {
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "UserPromptSubmit",
                        "additionalContext": ctx
                    }
                })
            } else {
                json!({})
            };
            
            ClaudeCodeResponse {
                json_value: Some(json_value),
                exit_code: 0,
            }
        }
    }
}
```

**File:** `cli/src/vendors/cursor.rs`

Match on `FlowResult` variants to translate to Cursor protocol:

```rust
fn translate_before_submit_prompt(response: &HookResponse) -> CursorResponse {
    match &response.result {
        FlowResult::FailedBlock(msg) => {
            // Blocking - combine messages and context for user
            let combined = response.combined_output();
            let user_message = combined.unwrap_or_default();

            CursorResponse {
                json_value: Some(json!({
                    "continue": false,
                    "user_message": user_message
                })),
                exit_code: 2,  // Cursor uses exit code 2 for blocking
            }
        }
        
        FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => {
            // Non-blocking - allow prompt to continue
            // All non-blocking results use exit code 0
            CursorResponse {
                json_value: Some(json!({
                    "continue": true
                })),
                exit_code: 0,
            }
        }
    }
}
```

### Phase 5: Update Tests

Update all tests that check `exit_code` to match on `FlowResult` variants instead:

**Before:**
```rust
assert_eq!(response.exit_code, 2);
assert_eq!(response.exit_code, 0);
```

**After:**
```rust
// More expressive - test the semantic meaning
assert!(matches!(response.result, FlowResult::FailedBlock(_)));
assert!(matches!(response.result, FlowResult::Success));

// Or use helper methods
assert!(response.is_blocking());
assert!(response.is_success());

// Can also extract and test the error message
if let FlowResult::FailedBlock(msg) = &response.result {
    assert!(msg.contains("validation error"));
}
```

## Migration Strategy

1. ✅ **Add `result: FlowResult` field to `HookResponse`** alongside `exit_code` (both exist temporarily)
2. ✅ **Update all constructors** to set both fields (for backward compatibility)
3. ✅ **Update handlers** to populate `result` with `FlowResult` (still setting `exit_code` for compatibility)
4. ✅ **Update vendor translators** to match on `response.result` instead of `response.exit_code`
5. ✅ **Update tests** to match on `FlowResult` variants
6. ✅ **Remove `exit_code` field** once all references are updated
7. ✅ **Clean up compatibility code**

**Note:** This migration is simpler than creating a new enum because we're reusing existing types!

## Benefits

### 1. **Semantic Clarity**
```rust
// Before - unclear what 2 means
if response.exit_code == 2 { ... }

// After - crystal clear intent
if matches!(response.result, FlowResult::FailedBlock(_)) { ... }
// Or even simpler with helper:
if response.is_blocking() { ... }
```

### 2. **Type Safety**
```rust
// Compiler enforces exhaustive matching
match &response.result {
    FlowResult::Success => { ... },
    FlowResult::FailedBlock(msg) => { ... },
    FlowResult::FailedContinue(msg) => { ... },
    FlowResult::FailedStop(msg) => { ... },
    // Compiler warns if we forget a variant!
}
```

### 3. **Access to Error Messages**
```rust
// Before - error message lost, stored separately
let exit_code = 2;
let messages = vec![Message::Error("validation failed")];

// After - error message embedded in the result
match &response.result {
    FlowResult::FailedBlock(msg) => {
        // Can use the message directly!
        eprintln!("Blocked: {}", msg);
    }
    _ => {}
}
```

### 4. **Vendor Isolation**
Exit codes are now purely a vendor translation concern, not a domain concept:

```rust
// Core domain - uses semantic FlowResult
pub struct HookResponse {
    pub result: FlowResult,  // Domain concept
    ...
}

// Vendor translator - maps to vendor protocol
match &response.result {
    FlowResult::FailedBlock(_) => exit_code = 2,  // Cursor-specific
    _ => exit_code = 0,
}
```

### 5. **No New Types Needed**
We're reusing `FlowResult` which already exists and is well-understood in the codebase. No need to learn a new enum or maintain duplicate semantics.

### 6. **Proper Handling of All Variants**
Previously, `FailedStop` was indistinguishable from `Success` (both exit code 0). Now vendors can handle it correctly:

```rust
match &response.result {
    FlowResult::Success => { /* true success */ },
    FlowResult::FailedStop(_) => { /* silent failure - log but don't show user */ },
    // These are now distinct!
}
```

## Risks & Mitigations

### Risk 1: Large Refactoring
**Impact:** Many files touched, potential for bugs

**Mitigation:** 
- Migrate incrementally (keep both fields temporarily)
- Run full test suite after each phase
- Use compiler to find all references

### Risk 2: Breaking Existing Flows
**Impact:** Custom flows might depend on exit_code

**Mitigation:**
- Provide compatibility methods during migration
- Add deprecation warnings
- Document migration guide

### Risk 3: Confusion During Transition
**Impact:** Code has both `exit_code` and `decision`

**Mitigation:**
- Complete migration quickly (single PR)
- Clear comments marking deprecated code
- Remove old code as soon as possible

## Success Criteria

- ✅ No references to `exit_code` in core handlers or `HookResponse`
- ✅ `FlowResult` used throughout domain code (already exists!)
- ✅ Exit codes only appear in vendor translators (final translation step)
- ✅ All four `FlowResult` variants handled properly (Success, FailedBlock, FailedContinue, FailedStop)
- ✅ All tests passing
- ✅ More readable, maintainable code
- ✅ Type-safe response handling with exhaustive pattern matching

## Files to Modify

1. **cli/src/handlers.rs** - Update `HookResponse` to use `result: FlowResult`
2. **cli/src/event_bus.rs** - No changes needed (already vendor-agnostic, just passes through)
3. **cli/src/vendors/claude_code.rs** - Match on `response.result` variants → Claude Code format
4. **cli/src/vendors/cursor.rs** - Match on `response.result` variants → Cursor format
5. **cli/src/flows/engine.rs** - Simplify `FlowResult` → `HookResponse` mapping (direct pass-through)
6. **cli/src/commands/acp.rs** - Update ACP proxy if it uses exit_code
7. **tests/** - Update all assertions to match on `FlowResult` variants

## Timeline

- **Phase 1-2:** 20 min (Update `HookResponse` struct - no new enum to create!)
- **Phase 3:** 45 min (Update handlers to pass FlowResult through)
- **Phase 4:** 1 hour (Update vendor translators to match on FlowResult)
- **Phase 5:** 30 min (Update tests)
- **Total:** ~2.5 hours (faster than originally estimated due to reusing FlowResult)

## Open Questions

1. **What about ACP proxy?**
   - Does it use exit_code internally?
   - Check if it needs migration
   - Decision: Review during implementation

2. **Should vendors differentiate between FailedContinue and FailedStop?**
   - Currently both would exit 0
   - FailedStop is meant to be silent, FailedContinue logs errors
   - Cursor/Claude Code don't have different protocols for these
   - Decision: Both exit 0, but we can log FailedContinue to stderr if AIKI_DEBUG is set

## Alternative Considered: Keep exit_code as Implementation Detail

**Approach:** Keep `exit_code` but make it a computed property:

```rust
impl HookResponse {
    pub fn exit_code(&self) -> i32 {
        match &self.result {
            FlowResult::Success | FlowResult::FailedContinue(_) | FlowResult::FailedStop(_) => 0,
            FlowResult::FailedBlock(_) => 2,
        }
    }
}
```

**Rejected because:**
- Still mixes vendor concerns into domain model (exit code 2 is Cursor-specific)
- Doesn't fix semantic clarity issue in core code
- Exit code 2 is Cursor-specific (Claude Code doesn't use it for blocking)
- Loses vendor-specific flexibility (what if a vendor needs different exit codes?)
- Better to translate at vendor boundary where protocol details belong

## Conclusion

This refactoring will make the codebase more maintainable and type-safe by:

1. **Using existing domain types** - `FlowResult` instead of magic exit codes
2. **Eliminating duplication** - No need to create a new enum when `FlowResult` already models the semantics
3. **Vendor isolation** - Exit codes become a pure translation concern at vendor boundaries
4. **Type safety** - Compiler-enforced exhaustive pattern matching on `FlowResult` variants
5. **Error message access** - Failed variants carry their error messages directly
6. **Proper distinction** - All four `FlowResult` variants handled correctly (previously `FailedStop` was indistinguishable from `Success`)

**Key Insight:** By recognizing that `FlowResult` already models our needs, we avoid creating unnecessary abstractions and keep the codebase simpler.

The migration is straightforward with low risk when done incrementally, and is actually **simpler and faster** than originally planned since we don't need to create and maintain a new enum.

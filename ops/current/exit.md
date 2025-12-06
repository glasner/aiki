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

Create a `Decision` enum to represent hook response semantics, decoupled from flow engine internals.

```rust
/// Decision about how to respond to a hook event
pub enum Decision {
    /// Allow the operation to proceed
    Allow,
    /// Block the operation with an error message
    Block(String),
}
```

**Why a separate `Decision` enum?**

`FlowResult` represents **flow execution policies** (on_failure: continue/stop/block):
```rust
pub enum FlowResult {
    Success,                  // Flow succeeded
    FailedContinue(String),   // on_failure: continue
    FailedStop(String),       // on_failure: stop
    FailedBlock(String),      // on_failure: block
}
```

These are **flow engine implementation details**, not general hook response semantics.

### Why Separate Decision from FlowResult

1. **Decoupling** - Hook responses don't depend on flow engine internals
2. **Simpler semantics** - Hook responses only need Allow/Block, not flow policies
3. **Future flexibility** - Flow engine can change without affecting hook API
4. **Clear separation** - Flow execution != Hook response decision
5. **Type safety** - Can't accidentally leak flow engine concepts to vendors

### Flow Through System

```
FlowResult (flows/engine.rs) - flow execution result
    ↓
Map to Decision - handlers convert flow policies to hook decisions
    ↓
HookResponse { decision: Decision } (handlers.rs) - carries decision + context/messages
    ↓
event_bus::dispatch() - passes through unchanged
    ↓
Vendor Translators (vendors/*.rs)
    ↓
Match on Decision variants → vendor-specific format + exit code
```

## Implementation Plan

### Phase 1: Add Decision Enum

**File:** `cli/src/handlers.rs`

```rust
/// Decision about how to respond to a hook event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the operation to proceed
    Allow,
    
    /// Block the operation with an error message
    Block(String),
}
```

**Rationale:** Only two variants needed since:
- `Success`, `FailedContinue`, `FailedStop` all → `Allow` (operation proceeds)
- `FailedBlock` → `Block` (operation is blocked)

The `messages` field on `HookResponse` handles warnings/info, so we don't need `AllowWithWarning`.

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
pub struct HookResponse {
    pub context: Option<String>,
    pub decision: Decision,  // ← Add this
    pub messages: Vec<Message>,
}

impl HookResponse {
    // Update constructors
    pub fn success() -> Self {
        Self {
            context: None,
            decision: Decision::Allow,
            messages: vec![],
        }
    }
    
    pub fn blocking_failure(user_msg: String, agent_msg: Option<String>) -> Self {
        Self {
            context: agent_msg,
            decision: Decision::Block(user_msg.clone()),
            messages: vec![Message::Error(user_msg)],
        }
    }
    
    pub fn failure(user_msg: String, agent_msg: Option<String>) -> Self {
        Self {
            context: agent_msg,
            decision: Decision::Allow,  // Non-blocking - allow operation
            messages: vec![Message::Error(user_msg)],
        }
    }
    
    // Update helper methods
    pub fn is_blocking(&self) -> bool {
        matches!(self.decision, Decision::Block(_))
    }
    
    pub fn is_success(&self) -> bool {
        matches!(self.decision, Decision::Allow) && self.messages.is_empty()
    }
}
```

### Phase 3: Update Handlers to Map FlowResult to Decision

**File:** `cli/src/handlers.rs`

Update all handler functions to convert `FlowResult` → `Decision`:

```rust
// In handle_pre_prompt, handle_pre_file_change, etc.
match flow_result {
    FlowResult::Success => HookResponse {
        decision: Decision::Allow,
        context: response.context,
        messages: response.messages,
    },
    
    FlowResult::FailedBlock(msg) => HookResponse {
        decision: Decision::Block(msg.clone()),
        context: Some("Fix the validation error before continuing.".to_string()),
        messages: vec![Message::Error(format!("❌ Prompt blocked: {}", msg))],
    },
    
    // All non-blocking failures map to Allow
    FlowResult::FailedContinue(msg) => HookResponse {
        decision: Decision::Allow,
        context: None,
        messages: vec![Message::Warning(msg)],
    },
    
    FlowResult::FailedStop(msg) => HookResponse {
        decision: Decision::Allow,
        context: None,
        messages: vec![],  // Silent failure - allow but no messages
    },
}
```

**Mapping:**
- `Success` → `Decision::Allow` (clean success)
- `FailedBlock` → `Decision::Block(msg)` (block operation)
- `FailedContinue` → `Decision::Allow` (allow with warning messages)
- `FailedStop` → `Decision::Allow` (allow silently)

### Phase 4: Update Vendor Translators

**File:** `cli/src/vendors/claude_code.rs`

Match on `Decision` variants to translate to Claude Code protocol:

```rust
fn translate_user_prompt_submit(response: &HookResponse) -> ClaudeCodeResponse {
    match &response.decision {
        Decision::Block(msg) => {
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
        
        Decision::Allow => {
            // Allow with optional context
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

**Note:** Much simpler pattern matching now - only two cases instead of four!

**File:** `cli/src/vendors/cursor.rs`

Match on `Decision` variants to translate to Cursor protocol:

```rust
fn translate_before_submit_prompt(response: &HookResponse) -> CursorResponse {
    match &response.decision {
        Decision::Block(msg) => {
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
        
        Decision::Allow => {
            // Allow prompt to continue
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

Update all tests that check `exit_code` to match on `Decision` variants instead:

**Before:**
```rust
assert_eq!(response.exit_code, 2);
assert_eq!(response.exit_code, 0);
```

**After:**
```rust
// More expressive - test the semantic meaning
assert!(matches!(response.decision, Decision::Block(_)));
assert!(matches!(response.decision, Decision::Allow));

// Or use helper methods
assert!(response.is_blocking());
assert!(response.is_success());

// Can also extract and test the error message
if let Decision::Block(msg) = &response.decision {
    assert!(msg.contains("validation error"));
}
```

## Migration Strategy

1. ✅ **Add `Decision` enum** to `cli/src/handlers.rs` (new code, no breaking changes)
2. ✅ **Add `decision: Decision` field to `HookResponse`** alongside `exit_code` (both exist temporarily)
3. ✅ **Update all constructors** to set both fields (for backward compatibility)
4. ✅ **Update handlers** to map `FlowResult` → `Decision` (still setting `exit_code` for compatibility)
5. ✅ **Update vendor translators** to match on `response.decision` instead of `response.exit_code`
6. ✅ **Update tests** to match on `Decision` variants
7. ✅ **Remove `exit_code` field** once all references are updated
8. ✅ **Clean up compatibility code**

## Benefits

### 1. **Semantic Clarity**
```rust
// Before - unclear what 2 means
if response.exit_code == 2 { ... }

// After - crystal clear intent
if matches!(response.decision, Decision::Block(_)) { ... }
// Or even simpler with helper:
if response.is_blocking() { ... }
```

### 2. **Type Safety**
```rust
// Compiler enforces exhaustive matching
match &response.decision {
    Decision::Allow => { ... },
    Decision::Block(msg) => { ... },
    // Compiler warns if we forget a variant!
}
```

### 3. **Simpler API**
```rust
// Only two variants instead of four
// Easier to understand and use
pub enum Decision {
    Allow,
    Block(String),
}

// vs. FlowResult with four variants tied to flow engine
```

### 4. **Access to Error Messages**
```rust
// Before - error message lost, stored separately
let exit_code = 2;
let messages = vec![Message::Error("validation failed")];

// After - error message embedded in the decision
match &response.decision {
    Decision::Block(msg) => {
        // Can use the message directly!
        eprintln!("Blocked: {}", msg);
    }
    _ => {}
}
```

### 5. **Vendor Isolation**
Exit codes are now purely a vendor translation concern, not a domain concept:

```rust
// Core domain - uses semantic Decision
pub struct HookResponse {
    pub decision: Decision,  // Domain concept
    ...
}

// Vendor translator - maps to vendor protocol
match &response.decision {
    Decision::Block(_) => exit_code = 2,  // Cursor-specific
    Decision::Allow => exit_code = 0,
}
```

### 6. **Decoupling from Flow Engine**
```rust
// HookResponse doesn't depend on flow engine internals
// Flow policies (on_failure: continue/stop/block) are mapped to decisions
// Future flow engine changes won't affect hook response API
```

### 7. **Clear Separation of Concerns**
```rust
// Flow engine: Executes actions and returns FlowResult
FlowResult::FailedContinue("action failed") // Flow execution detail

// Handler: Converts to hook decision
Decision::Allow // Hook response decision

// The flow policy is an implementation detail
// The hook decision is the public API
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
- ✅ `Decision` enum used throughout domain code
- ✅ Exit codes only appear in vendor translators (final translation step)
- ✅ Clear mapping from `FlowResult` → `Decision` in handlers
- ✅ All tests passing
- ✅ More readable, maintainable code
- ✅ Type-safe response handling with exhaustive pattern matching
- ✅ `HookResponse` decoupled from flow engine internals

## Files to Modify

1. **cli/src/handlers.rs** - Add `Decision` enum, update `HookResponse` to use `decision: Decision`
2. **cli/src/event_bus.rs** - No changes needed (already vendor-agnostic, just passes through)
3. **cli/src/vendors/claude_code.rs** - Match on `response.decision` variants → Claude Code format
4. **cli/src/vendors/cursor.rs** - Match on `response.decision` variants → Cursor format
5. **cli/src/flows/engine.rs** - No changes needed (returns FlowResult, handlers do the mapping)
6. **cli/src/commands/acp.rs** - Update ACP proxy if it uses exit_code
7. **tests/** - Update all assertions to match on `Decision` variants

## Timeline

- **Phase 1:** 15 min (Add `Decision` enum - simple two-variant enum)
- **Phase 2:** 20 min (Update `HookResponse` struct and constructors)
- **Phase 3:** 45 min (Update handlers to map FlowResult → Decision)
- **Phase 4:** 1 hour (Update vendor translators to match on Decision)
- **Phase 5:** 30 min (Update tests)
- **Total:** ~2.5 hours

## Open Questions

1. **What about ACP proxy?**
   - Does it use exit_code internally?
   - Check if it needs migration
   - Decision: Review during implementation

2. **Should we preserve flow failure reasons in HookResponse.messages?**
   - When mapping `FailedContinue(msg)` → `Decision::Allow`, should we add msg to messages?
   - Currently handlers create new messages with emojis
   - Decision: Yes, map flow errors to HookResponse.messages for consistency

## Alternatives Considered

### Alternative 1: Use FlowResult Directly in HookResponse

**Approach:** Skip creating `Decision` and use `FlowResult` directly:

```rust
pub struct HookResponse {
    pub result: FlowResult,
    ...
}
```

**Rejected because:**
- Couples `HookResponse` to flow engine internals
- Exposes flow execution policies (on_failure: continue/stop/block) in hook API
- Makes it harder to change flow engine implementation
- Vendors need to understand flow-specific semantics
- Hook responses conceptually only need Allow/Block, not four flow variants

### Alternative 2: Keep exit_code as Computed Property

**Approach:** Keep `exit_code` but make it a computed property:

```rust
impl HookResponse {
    pub fn exit_code(&self) -> i32 {
        match &self.decision {
            Decision::Allow => 0,
            Decision::Block(_) => 2,
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

1. **Creating proper abstraction** - `Decision` enum for hook responses, decoupled from flow engine
2. **Simpler semantics** - Only two variants (Allow/Block) instead of four flow-specific ones
3. **Vendor isolation** - Exit codes become a pure translation concern at vendor boundaries
4. **Type safety** - Compiler-enforced exhaustive pattern matching on `Decision` variants
5. **Error message access** - Block variant carries error message directly
6. **Clear separation** - Flow execution policies separated from hook response decisions
7. **Future-proof** - Flow engine can evolve without affecting hook API

**Key Insight:** While `FlowResult` might seem like a good fit at first, it represents **flow execution policies** (on_failure modes), not **hook response semantics**. Creating a dedicated `Decision` enum provides proper separation of concerns and a cleaner API.

**Design Principle:** Avoid coupling domain concepts to implementation details. `HookResponse` is a public API for hook responses; `FlowResult` is an internal flow engine implementation detail. Keep them separate.

The migration is straightforward with low risk when done incrementally.

# Claude Code Hook Response Refactoring Plan

## Current State

The `translate_response` function in `cli/src/vendors/claude_code.rs` (lines 157-293) has several issues:

**Problems:**
1. **Complex branching** - Matches on exit code first, then event type, leading to nested conditionals
2. **Duplicate logic** - Message formatting and context combination repeated across branches
3. **Manual JSON building** - Using `Map<String, Value>` with string keys (error-prone)
4. **Hard to test** - No clear structure for what each event type should return
5. **Exit code 2 handling** - Shouldn't be used for Claude Code (should always exit 0)
6. **Mixed concerns** - Single function handles all events with different structures

**Current code structure:**
```rust
match exit_code {
    2 => {
        if is_post_tool_use { /* ... */ }
        else { /* ... */ }
    }
    0 => {
        if !formatted_messages.is_empty() {
            if is_post_tool_use { /* ... */ }
            else { /* ... */ }
        }
        if let Some(ref ctx) = response.context {
            if event_type == "UserPromptSubmit" { /* ... */ }
            else if event_type == "Stop" { /* ... */ }
        }
    }
    _ => { /* stderr fallback */ }
}
```

---

## Proposed Architecture: Type-Safe Response Enums with Serde Magic

Create enum types for each event with variants representing the possible decisions. Use serde attributes to handle serialization automatically—**no manual Serialize implementations needed!**

### Key Serde Attributes

- `#[serde(untagged)]` - Serializes variant contents directly (no enum tag)
- `#[serde(flatten)]` - Merges nested struct fields into parent
- `#[serde(skip_serializing_if = "Option::is_none")]` - Omits None fields
- `#[serde(rename = "...")]` - Renames fields in JSON output

### Response Type Definitions

```rust
// Helper type that always serializes to "block"
#[derive(Debug, Clone, Copy)]
struct BlockDecision;

impl Default for BlockDecision {
    fn default() -> Self {
        BlockDecision
    }
}

impl Serialize for BlockDecision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("block")
    }
}

// SessionStart output
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStartOutput {
    hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_context: Option<String>,
}

// SessionStart: Cannot block, only add context
#[derive(Serialize)]
#[serde(untagged)]
enum SessionStartResponse {
    Allow {
        #[serde(skip_serializing_if = "Option::is_none")]
        system_message: Option<String>,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        hook_specific_output: Option<SessionStartOutput>,
    },
}

// UserPromptSubmit output
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserPromptSubmitOutput {
    hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_context: Option<String>,
}

// UserPromptSubmit: Can block or allow with context
#[derive(Serialize)]
#[serde(untagged)]
enum UserPromptSubmitResponse {
    Block {
        #[serde(default)]
        decision: BlockDecision,
        reason: String,
    },
    Allow {
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        hook_specific_output: Option<UserPromptSubmitOutput>,
    },
}

// PreToolUse output
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseOutput {
    hook_event_name: String,
    permission_decision: String,  // "allow" | "deny" | "ask"
    #[serde(skip_serializing_if = "Option::is_none")]
    permission_decision_reason: Option<String>,
}

// PreToolUse: Three permission decisions
#[derive(Serialize)]
#[serde(untagged, rename_all = "camelCase")]
enum PreToolUseResponse {
    Allow {
        hook_specific_output: PreToolUseOutput,
    },
    Deny {
        hook_specific_output: PreToolUseOutput,
    },
    Ask {
        hook_specific_output: PreToolUseOutput,
    },
}

// PostToolUse output
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostToolUseOutput {
    hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_context: Option<String>,
}

// PostToolUse: Can block (autoreply) or allow with context
#[derive(Serialize)]
#[serde(untagged)]
enum PostToolUseResponse {
    Block {
        #[serde(default)]
        decision: BlockDecision,
        reason: String,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        hook_specific_output: Option<PostToolUseOutput>,
    },
    Allow {
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        hook_specific_output: Option<PostToolUseOutput>,
    },
}

// Stop: Can block (autoreply) or allow
#[derive(Serialize)]
#[serde(untagged)]
enum StopResponse {
    Block {
        #[serde(default)]
        decision: BlockDecision,
        reason: String,
    },
    Allow {},  // Empty object: {}
}
```

### Example Serialization

```rust
// UserPromptSubmitResponse::Block
UserPromptSubmitResponse::Block { 
    decision: "block".to_string(), 
    reason: "Too long".to_string() 
}
// → { "decision": "block", "reason": "Too long" }

// UserPromptSubmitResponse::Allow with context
UserPromptSubmitResponse::Allow { 
    hook_specific_output: Some(UserPromptSubmitOutput {
        hook_event_name: "UserPromptSubmit".to_string(),
        additional_context: Some("Repo: /path".to_string()),
    })
}
// → { "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": "Repo: /path" } }

// StopResponse::Allow (empty)
StopResponse::Allow {}
// → {}
```

### Refactored Translation Logic

```rust
fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
    // Claude Code hooks always return exit 0
    let json_output = match event_type {
        "SessionStart" => translate_session_start(response),
        "UserPromptSubmit" => translate_user_prompt_submit(response),
        "PreToolUse" => translate_pre_tool_use(response),
        "PostToolUse" => translate_post_tool_use(response),
        "Stop" => translate_stop(response),
        _ => {
            eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
            return (None, 0);
        }
    };
    
    (json_output, 0)
}

fn translate_session_start(response: HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(&response);
    
    let resp = SessionStartResponse::Allow {
        system_message: None,  // Could add warnings here if needed
        hook_specific_output: combined.map(|ctx| SessionStartOutput {
            hook_event_name: "SessionStart".to_string(),
            additional_context: Some(ctx),
        }),
    };
    
    serde_json::to_string(&resp).ok()
}

fn translate_user_prompt_submit(response: HookResponse) -> Option<String> {
    let resp = if response.is_blocking() {
        // Block the prompt
        let formatted_messages = crate::handlers::format_messages(&response);
        let reason = if !formatted_messages.is_empty() {
            formatted_messages
        } else {
            "Prompt validation failed".to_string()
        };
        
        UserPromptSubmitResponse::Block {
            decision: BlockDecision,
            reason,
        }
    } else {
        // Allow with optional context
        let combined = combine_messages_and_context(&response);
        
        UserPromptSubmitResponse::Allow {
            hook_specific_output: combined.map(|ctx| UserPromptSubmitOutput {
                hook_event_name: "UserPromptSubmit".to_string(),
                additional_context: Some(ctx),
            }),
        }
    };
    
    serde_json::to_string(&resp).ok()
}

fn translate_pre_tool_use(response: HookResponse) -> Option<String> {
    let formatted_messages = crate::handlers::format_messages(&response);
    
    // Determine permission decision from response
    // For now, default to "allow" unless blocked
    let resp = if response.is_blocking() {
        PreToolUseResponse::Deny {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: "deny".to_string(),
                permission_decision_reason: Some(formatted_messages),
            }
        }
    } else {
        PreToolUseResponse::Allow {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: "allow".to_string(),
                permission_decision_reason: if !formatted_messages.is_empty() {
                    Some(formatted_messages)
                } else {
                    None
                },
            }
        }
    };
    
    serde_json::to_string(&resp).ok()
}

fn translate_post_tool_use(response: HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(&response);
    
    let resp = if response.is_blocking() {
        // Block (autoreply with reason)
        let formatted_messages = crate::handlers::format_messages(&response);
        let reason = if !formatted_messages.is_empty() {
            formatted_messages
        } else {
            "Tool execution requires attention".to_string()
        };
        
        PostToolUseResponse::Block {
            decision: BlockDecision,
            reason,
            hook_specific_output: response.context.as_ref().map(|ctx| PostToolUseOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: Some(ctx.clone()),
            }),
        }
    } else {
        // Allow with optional context
        PostToolUseResponse::Allow {
            hook_specific_output: combined.map(|ctx| PostToolUseOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: Some(ctx),
            }),
        }
    };
    
    serde_json::to_string(&resp).ok()
}

fn translate_stop(response: HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(&response);
    
    let resp = if let Some(reason_text) = combined {
        // Block (autoreply/force continuation)
        StopResponse::Block {
            decision: BlockDecision,
            reason: reason_text,
        }
    } else {
        // Allow normal stop
        StopResponse::Allow
    };
    
    serde_json::to_string(&resp).ok()
}

// Helper function to combine messages + context (Phase 8 logic)
fn combine_messages_and_context(response: &HookResponse) -> Option<String> {
    let formatted_messages = crate::handlers::format_messages(response);
    let context = response.context.as_deref().unwrap_or("");
    
    match (!formatted_messages.is_empty(), !context.is_empty()) {
        (true, true) => Some(format!("{}\n\n{}", formatted_messages, context)),
        (true, false) => Some(formatted_messages),
        (false, true) => Some(context.to_string()),
        (false, false) => None,
    }
}
```

---

## Implementation Plan

### Phase 1: Add Response Types (Non-Breaking)

**File:** `cli/src/vendors/claude_code.rs`

1. Add enum definitions for all event types:
   - `SessionStartResponse`
   - `UserPromptSubmitResponse`
   - `PreToolUseResponse`
   - `PostToolUseResponse`
   - `StopResponse`

2. Add shared struct:
   - `HookSpecificOutput`

3. Implement custom `Serialize` for each enum
   - Handle flattening to correct JSON structure
   - Use `serde::ser::SerializeMap` for control

**Testing:**
- Add unit tests for each response type's serialization
- Verify JSON output matches Claude Code's expected format

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Add types at top of file

**Estimated effort:** 2-3 hours

---

### Phase 2: Add Event-Specific Translation Functions (Non-Breaking)

**File:** `cli/src/vendors/claude_code.rs`

1. Add helper function:
   - `fn combine_messages_and_context(response: &HookResponse) -> Option<String>`

2. Add event-specific translators:
   - `fn translate_session_start(response: HookResponse) -> Option<String>`
   - `fn translate_user_prompt_submit(response: HookResponse) -> Option<String>`
   - `fn translate_pre_tool_use(response: HookResponse) -> Option<String>`
   - `fn translate_post_tool_use(response: HookResponse) -> Option<String>`
   - `fn translate_stop(response: HookResponse) -> Option<String>`

**Testing:**
- Unit test each translator function
- Compare output with current `translate_response` for same inputs
- Verify Phase 8 combination logic (messages + context)

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Add functions after type definitions

**Estimated effort:** 3-4 hours

---

### Phase 3: Replace Main Translation Function

**File:** `cli/src/vendors/claude_code.rs`

1. Replace `translate_response` body:
   ```rust
   fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
       let json_output = match event_type {
           "SessionStart" => translate_session_start(response),
           "UserPromptSubmit" => translate_user_prompt_submit(response),
           "PreToolUse" => translate_pre_tool_use(response),
           "PostToolUse" => translate_post_tool_use(response),
           "Stop" => translate_stop(response),
           _ => {
               eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
               return (None, 0);
           }
       };
       
       (json_output, 0)  // Always exit 0 for Claude Code
   }
   ```

2. Remove old exit code branching logic

**Testing:**
- Run full test suite
- Integration test with actual Claude Code hooks (if available)
- Verify all events produce correct JSON

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Replace `translate_response` body (lines 157-293)

**Estimated effort:** 1-2 hours

---

### Phase 4: Clean Up and Documentation

1. Remove exit code 2 handling (no longer used)
2. Add documentation comments to all types and functions
3. Update `translator-requirements.md` references if needed
4. Add examples to docstrings

**Testing:**
- Documentation tests (if applicable)
- Final full test suite run

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Documentation
- `ops/current/translator-requirements.md` - Update if needed

**Estimated effort:** 1 hour

---

## Benefits

**Code Quality:**
- ✅ Type-safe JSON generation (compiler prevents field name typos)
- ✅ Clear separation of concerns (one function per event type)
- ✅ Exhaustive pattern matching (compiler ensures all cases handled)
- ✅ Self-documenting code (response types show what's possible)

**Correctness:**
- ✅ Always returns exit 0 for Claude Code (no more exit code 2 handling)
- ✅ Consistent Phase 8 combination logic (messages + context)
- ✅ Correct JSON structure per event (enforced by types)

**Maintainability:**
- ✅ Easy to add new events (add enum + translator function)
- ✅ Easy to modify event behavior (change one translator)
- ✅ Easy to test (each translator is independent)
- ✅ Easy to understand (match by event type, not exit code)

---

## Testing Strategy

### Unit Tests

Add tests for each component:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_start_response_serialization() {
        let resp = SessionStartResponse::Success {
            system_message: None,
            hook_specific_output: Some(HookSpecificOutput {
                hook_event_name: "SessionStart".to_string(),
                additional_context: Some("Context here".to_string()),
                permission_decision: None,
                permission_decision_reason: None,
            }),
        };
        
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"hookEventName\":\"SessionStart\""));
        assert!(json.contains("\"additionalContext\":\"Context here\""));
    }
    
    #[test]
    fn test_user_prompt_submit_block() {
        let resp = UserPromptSubmitResponse::Block {
            decision: "block".to_string(),
            reason: "Too long".to_string(),
        };
        
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"decision\":\"block\""));
        assert!(json.contains("\"reason\":\"Too long\""));
    }
    
    #[test]
    fn test_combine_messages_and_context() {
        let response = HookResponse {
            context: Some("Context text".to_string()),
            messages: vec![Message::Warning("Warning text".to_string())],
            exit_code: 0,
        };
        
        let combined = combine_messages_and_context(&response);
        assert_eq!(combined, Some("⚠️ Warning text\n\nContext text".to_string()));
    }
    
    #[test]
    fn test_translate_session_start() {
        let response = HookResponse {
            context: Some("Repo: /path".to_string()),
            messages: vec![],
            exit_code: 0,
        };
        
        let json = translate_session_start(response);
        assert!(json.is_some());
        
        let parsed: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            "Repo: /path"
        );
    }
    
    // Add tests for all other translators...
}
```

### Integration Tests

Test with actual Claude Code hook scenarios:

```rust
#[test]
fn test_full_preprompt_flow() {
    // Simulate PrePrompt with validation warning + context
    let event = AikiPrePromptEvent { /* ... */ };
    let response = handlers::handle_pre_prompt(event).unwrap();
    let (json, exit_code) = translate_response(response, "UserPromptSubmit");
    
    assert_eq!(exit_code, 0);
    assert!(json.is_some());
    
    // Parse and verify JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
    assert!(parsed.get("hookSpecificOutput").is_some());
}
```

---

## Migration Checklist

- [ ] Phase 1: Add response types
  - [ ] Define all enum types
  - [ ] Add `HookSpecificOutput` struct
  - [ ] Implement `Serialize` for each enum
  - [ ] Unit test serialization
  
- [ ] Phase 2: Add translator functions
  - [ ] Add `combine_messages_and_context` helper
  - [ ] Add `translate_session_start`
  - [ ] Add `translate_user_prompt_submit`
  - [ ] Add `translate_pre_tool_use`
  - [ ] Add `translate_post_tool_use`
  - [ ] Add `translate_stop`
  - [ ] Unit test each translator
  
- [ ] Phase 3: Replace main function
  - [ ] Update `translate_response` to use event-based match
  - [ ] Remove exit code branching
  - [ ] Run full test suite
  
- [ ] Phase 4: Clean up
  - [ ] Add documentation
  - [ ] Remove unused code
  - [ ] Update related documentation
  - [ ] Final test run

---

## Risk Assessment

**Low Risk:**
- All changes are internal to `claude_code.rs` translator
- Existing tests will catch regressions
- Can be done incrementally (add new code alongside old)

**Potential Issues:**
- Custom `Serialize` implementation complexity
  - Mitigation: Extensive unit tests for each response type
- Subtle differences in JSON output
  - Mitigation: Compare old vs new output in tests
- Missing edge cases
  - Mitigation: Test with all combinations of messages/context/blocking

---

## Alternative Approaches Considered

### 1. Keep current structure, just clean up
**Pros:** Minimal changes
**Cons:** Still hard to maintain, error-prone

### 2. Use builder pattern instead of enums
**Pros:** More flexible
**Cons:** Less type-safe, can build invalid states

### 3. Use serde-flattened structs
**Pros:** Less custom serialization code
**Cons:** Harder to ensure correct structure per event

**Decision:** Proceed with enum approach for maximum type safety and clarity.

# Claude Code Hook Response Refactoring Plan (Simpler Approach)

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

## Proposed Solution: Event-Based Dispatch with Helper Functions

**Key changes:**
1. Match on event type first (not exit code)
2. Extract event-specific translator functions
3. Use `serde_json::json!` macro for type-safe JSON construction
4. Share common logic via helper function

### Main Translation Function

```rust
fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
    // Claude Code hooks always return exit 0
    let json_output = match event_type {
        "SessionStart" => translate_session_start(&response),
        "UserPromptSubmit" => translate_user_prompt_submit(&response),
        "PreToolUse" => translate_pre_tool_use(&response),
        "PostToolUse" => translate_post_tool_use(&response),
        "Stop" => translate_stop(&response),
        _ => {
            eprintln!("Warning: Unknown Claude Code event type: {}", event_type);
            return (None, 0);
        }
    };
    
    (json_output, 0)
}
```

### Event-Specific Translator Functions

```rust
fn translate_session_start(response: &HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(response);
    
    let json = if let Some(ctx) = combined {
        json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": ctx
            }
        })
    } else {
        json!({})
    };
    
    serde_json::to_string(&json).ok()
}

fn translate_user_prompt_submit(response: &HookResponse) -> Option<String> {
    if response.is_blocking() {
        // Block the prompt
        let reason = format_messages(response);
        let json = json!({
            "decision": "block",
            "reason": reason
        });
        serde_json::to_string(&json).ok()
    } else {
        // Allow with optional context
        let combined = combine_messages_and_context(response);
        let json = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        serde_json::to_string(&json).ok()
    }
}

fn translate_pre_tool_use(response: &HookResponse) -> Option<String> {
    let formatted_messages = format_messages(response);
    
    // Determine permission decision from response
    // For now, default to "allow" unless blocked
    let (permission_decision, reason) = if response.is_blocking() {
        ("deny", Some(formatted_messages))
    } else {
        ("allow", if !formatted_messages.is_empty() {
            Some(formatted_messages)
        } else {
            None
        })
    };
    
    let mut output = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": permission_decision
        }
    });
    
    // Add reason if present
    if let Some(reason_text) = reason {
        output["hookSpecificOutput"]["permissionDecisionReason"] = json!(reason_text);
    }
    
    serde_json::to_string(&output).ok()
}

fn translate_post_tool_use(response: &HookResponse) -> Option<String> {
    if response.is_blocking() {
        // Block (autoreply with reason)
        let reason = format_messages(response);
        let reason_text = if !reason.is_empty() {
            reason
        } else {
            "Tool execution requires attention".to_string()
        };
        
        let mut json = json!({
            "decision": "block",
            "reason": reason_text
        });
        
        // Add optional context
        if let Some(ref ctx) = response.context {
            json["hookSpecificOutput"] = json!({
                "hookEventName": "PostToolUse",
                "additionalContext": ctx
            });
        }
        
        serde_json::to_string(&json).ok()
    } else {
        // Allow with optional context
        let combined = combine_messages_and_context(response);
        let json = if let Some(ctx) = combined {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": ctx
                }
            })
        } else {
            json!({})
        };
        serde_json::to_string(&json).ok()
    }
}

fn translate_stop(response: &HookResponse) -> Option<String> {
    let combined = combine_messages_and_context(response);
    
    let json = if let Some(reason_text) = combined {
        // Block (autoreply/force continuation)
        json!({
            "decision": "block",
            "reason": reason_text
        })
    } else {
        // Allow normal stop
        json!({})
    };
    
    serde_json::to_string(&json).ok()
}
```

### Helper Function

```rust
/// Combine formatted messages and context according to Phase 8 architecture
fn combine_messages_and_context(response: &HookResponse) -> Option<String> {
    let formatted_messages = format_messages(response);
    let context = response.context.as_deref().unwrap_or("");
    
    match (!formatted_messages.is_empty(), !context.is_empty()) {
        (true, true) => Some(format!("{}\n\n{}", formatted_messages, context)),
        (true, false) => Some(formatted_messages),
        (false, true) => Some(context.to_string()),
        (false, false) => None,
    }
}

/// Format HookResponse messages with emoji prefixes
fn format_messages(response: &HookResponse) -> String {
    let mut parts = vec![];
    for msg in &response.messages {
        match msg {
            Message::Info(s) => parts.push(format!("ℹ️ {}", s)),
            Message::Warning(s) => parts.push(format!("⚠️ {}", s)),
            Message::Error(s) => parts.push(format!("❌ {}", s)),
        }
    }
    parts.join("\n\n")
}
```

---

## Implementation Plan

### Phase 1: Add Helper Functions (Non-Breaking)

**File:** `cli/src/vendors/claude_code.rs`

1. Add `combine_messages_and_context` helper function
2. Add `format_messages` helper function (or use existing `crate::handlers::format_messages`)

**Testing:**
- Unit test `combine_messages_and_context` with all combinations:
  - Both messages and context
  - Only messages
  - Only context
  - Neither

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Add helpers after existing code

**Estimated effort:** 30 minutes

---

### Phase 2: Add Event-Specific Translation Functions (Non-Breaking)

**File:** `cli/src/vendors/claude_code.rs`

1. Add `translate_session_start`
2. Add `translate_user_prompt_submit`
3. Add `translate_pre_tool_use`
4. Add `translate_post_tool_use`
5. Add `translate_stop`

**Testing:**
- Unit test each translator function
- Compare output with current `translate_response` for same inputs
- Verify all JSON structures match expected Claude Code format

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Add functions after helpers

**Estimated effort:** 2 hours

---

### Phase 3: Replace Main Translation Function

**File:** `cli/src/vendors/claude_code.rs`

1. Replace `translate_response` body with event-based dispatch
2. Remove old exit code branching logic
3. Remove old JSON building code

**Testing:**
- Run full test suite
- Integration test with actual Claude Code hooks (if available)
- Verify all events produce correct JSON

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Replace `translate_response` body (lines 157-293)

**Estimated effort:** 1 hour

---

### Phase 4: Clean Up

1. Remove any unused variables or imports
2. Add documentation comments to new functions
3. Update `translator-requirements.md` if needed

**Testing:**
- Final full test suite run
- Lint check

**Files to modify:**
- `cli/src/vendors/claude_code.rs` - Documentation
- `ops/current/translator-requirements.md` - Update if needed

**Estimated effort:** 30 minutes

---

## Benefits

**Code Quality:**
- ✅ Clear separation of concerns (one function per event type)
- ✅ Event-based dispatch (easier to understand flow)
- ✅ Shared logic extracted to helpers (DRY principle)
- ✅ Simpler code structure (no complex types needed)

**Correctness:**
- ✅ Always returns exit 0 for Claude Code
- ✅ Consistent Phase 8 combination logic (messages + context)
- ✅ `json!` macro provides compile-time field checking

**Maintainability:**
- ✅ Easy to add new events (add new translator function)
- ✅ Easy to modify event behavior (change one function)
- ✅ Easy to test (each translator is independent)
- ✅ Familiar `json!` macro (no custom types to learn)

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_combine_messages_and_context() {
        // Both messages and context
        let response = HookResponse {
            context: Some("Context text".to_string()),
            messages: vec![Message::Warning("Warning text".to_string())],
            exit_code: 0,
        };
        let combined = combine_messages_and_context(&response);
        assert_eq!(combined, Some("⚠️ Warning text\n\nContext text".to_string()));
        
        // Only messages
        let response = HookResponse {
            context: None,
            messages: vec![Message::Info("Info text".to_string())],
            exit_code: 0,
        };
        let combined = combine_messages_and_context(&response);
        assert_eq!(combined, Some("ℹ️ Info text".to_string()));
        
        // Only context
        let response = HookResponse {
            context: Some("Context only".to_string()),
            messages: vec![],
            exit_code: 0,
        };
        let combined = combine_messages_and_context(&response);
        assert_eq!(combined, Some("Context only".to_string()));
        
        // Neither
        let response = HookResponse {
            context: None,
            messages: vec![],
            exit_code: 0,
        };
        let combined = combine_messages_and_context(&response);
        assert_eq!(combined, None);
    }
    
    #[test]
    fn test_translate_session_start() {
        let response = HookResponse {
            context: Some("Repo: /path".to_string()),
            messages: vec![],
            exit_code: 0,
        };
        
        let json_str = translate_session_start(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(
            parsed["hookSpecificOutput"]["hookEventName"],
            "SessionStart"
        );
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            "Repo: /path"
        );
    }
    
    #[test]
    fn test_translate_user_prompt_submit_block() {
        let response = HookResponse {
            context: None,
            messages: vec![Message::Error("Too long".to_string())],
            exit_code: 2,
        };
        
        let json_str = translate_user_prompt_submit(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(parsed["decision"], "block");
        assert_eq!(parsed["reason"], "❌ Too long");
    }
    
    #[test]
    fn test_translate_user_prompt_submit_allow() {
        let response = HookResponse {
            context: Some("Working dir: /path".to_string()),
            messages: vec![],
            exit_code: 0,
        };
        
        let json_str = translate_user_prompt_submit(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            "Working dir: /path"
        );
    }
    
    #[test]
    fn test_translate_pre_tool_use_deny() {
        let response = HookResponse {
            context: None,
            messages: vec![Message::Error("Policy violation".to_string())],
            exit_code: 2,
        };
        
        let json_str = translate_pre_tool_use(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(
            parsed["hookSpecificOutput"]["permissionDecision"],
            "deny"
        );
        assert_eq!(
            parsed["hookSpecificOutput"]["permissionDecisionReason"],
            "❌ Policy violation"
        );
    }
    
    #[test]
    fn test_translate_post_tool_use_block() {
        let response = HookResponse {
            context: Some("Additional context".to_string()),
            messages: vec![Message::Error("Provenance failed".to_string())],
            exit_code: 2,
        };
        
        let json_str = translate_post_tool_use(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(parsed["decision"], "block");
        assert_eq!(parsed["reason"], "❌ Provenance failed");
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            "Additional context"
        );
    }
    
    #[test]
    fn test_translate_stop_block() {
        let response = HookResponse {
            context: Some("Tests failed".to_string()),
            messages: vec![],
            exit_code: 0,
        };
        
        let json_str = translate_stop(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(parsed["decision"], "block");
        assert_eq!(parsed["reason"], "Tests failed");
    }
    
    #[test]
    fn test_translate_stop_allow() {
        let response = HookResponse {
            context: None,
            messages: vec![],
            exit_code: 0,
        };
        
        let json_str = translate_stop(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        
        // Should be empty object
        assert_eq!(parsed, json!({}));
    }
}
```

### Integration Tests

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

- [ ] Phase 1: Add helper functions
  - [ ] Add `combine_messages_and_context`
  - [ ] Add `format_messages` (or reuse existing)
  - [ ] Unit test helpers
  
- [ ] Phase 2: Add translator functions
  - [ ] Add `translate_session_start`
  - [ ] Add `translate_user_prompt_submit`
  - [ ] Add `translate_pre_tool_use`
  - [ ] Add `translate_post_tool_use`
  - [ ] Add `translate_stop`
  - [ ] Unit test each translator
  
- [ ] Phase 3: Replace main function
  - [ ] Update `translate_response` to use event-based dispatch
  - [ ] Remove exit code branching
  - [ ] Run full test suite
  
- [ ] Phase 4: Clean up
  - [ ] Remove unused code
  - [ ] Add documentation
  - [ ] Update related documentation
  - [ ] Final test run

---

## Risk Assessment

**Low Risk:**
- All changes are internal to `claude_code.rs` translator
- Existing tests will catch regressions
- Can be done incrementally (add new code alongside old)
- Uses familiar `json!` macro (no new dependencies)

**Potential Issues:**
- Subtle differences in JSON output (extra fields, field ordering)
  - Mitigation: Compare old vs new output in tests
- Missing edge cases
  - Mitigation: Test with all combinations of messages/context/blocking

---

## Comparison with Type-Safe Approach

### Simpler Approach (This Document)

**Pros:**
- Less code to write and maintain
- No new types to learn
- Familiar `json!` macro
- Easier to make quick changes

**Cons:**
- JSON structure not enforced by type system
- Can accidentally set wrong fields
- Runtime errors for typos (caught by tests, not compiler)

### Type-Safe Approach (hook-responses.md)

**Pros:**
- Compile-time guarantees on JSON structure
- Can't set invalid fields
- Self-documenting types
- Better IDE autocomplete

**Cons:**
- More boilerplate (structs, enums, impls)
- Steeper learning curve
- Harder to modify structures

**Recommendation:** Start with the simpler approach. If we find ourselves making repeated mistakes with JSON structure, we can always refactor to the type-safe approach later.

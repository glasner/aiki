# Event Bus Testing Plan

## Problem Statement

The ACP proxy's `handle_session_prompt()` and `handle_post_response()` functions call `event_bus::dispatch()` to fire PrePrompt and PostResponse events. These functions cannot be easily unit tested because:

1. `event_bus::dispatch()` executes real flows on the filesystem
2. No way to inject mock responses for testing
3. Tests would require setting up actual .aiki/flows/ directories
4. Side effects (file I/O, subprocess execution) make tests non-deterministic

**Current code pattern** (`cli/src/commands/acp.rs:1287-1290`):
```rust
let response = event_bus::dispatch(event)?;

// Extract modified_prompt from metadata
let modified_prompt = response.metadata
    .iter()
    .find(|(k, _)| k == "modified_prompt")
    .map(|(_, v)| v.clone())
    .unwrap_or_else(|| original_text.clone());
```

**What we need to test**:
- PrePrompt event fires with correct data
- Modified prompt is extracted from metadata
- Fallback to original prompt when flow fails
- PostResponse event fires only on `end_turn`
- Autoreply is extracted from metadata
- MAX_AUTOREPLIES enforcement works

---

## Solution Approaches

### Option 1: Dependency Injection (Trait-Based)

**Concept**: Define a trait for event dispatching and inject different implementations.

**Implementation**:

```rust
// Define trait in cli/src/event_bus.rs
pub trait EventDispatcher {
    fn dispatch(&self, event: AikiEvent) -> Result<HookResponse>;
}

// Real implementation
pub struct FlowEventDispatcher;

impl EventDispatcher for FlowEventDispatcher {
    fn dispatch(&self, event: AikiEvent) -> Result<HookResponse> {
        // Current event_bus::dispatch() implementation
        crate::event_bus::dispatch_internal(event)
    }
}

// Mock implementation for tests
#[cfg(test)]
pub struct MockEventDispatcher {
    responses: HashMap<String, HookResponse>,
}

#[cfg(test)]
impl MockEventDispatcher {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }
    
    pub fn with_response(mut self, event_type: &str, response: HookResponse) -> Self {
        self.responses.insert(event_type.to_string(), response);
        self
    }
}

#[cfg(test)]
impl EventDispatcher for MockEventDispatcher {
    fn dispatch(&self, event: AikiEvent) -> Result<HookResponse> {
        let event_type = match &event {
            AikiEvent::PrePrompt(_) => "PrePrompt",
            AikiEvent::PostResponse(_) => "PostResponse",
            _ => "Other",
        };
        
        Ok(self.responses
            .get(event_type)
            .cloned()
            .unwrap_or_else(HookResponse::success))
    }
}
```

**Refactor functions to accept dispatcher**:

```rust
// Before
pub fn handle_session_prompt(msg: &JsonRpcMessage, ...) -> Result<JsonRpcMessage> {
    let response = event_bus::dispatch(event)?;
}

// After
pub fn handle_session_prompt<D: EventDispatcher>(
    msg: &JsonRpcMessage,
    dispatcher: &D,
    ...
) -> Result<JsonRpcMessage> {
    let response = dispatcher.dispatch(event)?;
}
```

**Test usage**:

```rust
#[test]
fn test_handle_session_prompt_modified_prompt() {
    let dispatcher = MockEventDispatcher::new()
        .with_response("PrePrompt", HookResponse {
            success: true,
            metadata: vec![("modified_prompt".to_string(), "MODIFIED".to_string())],
            ..Default::default()
        });
    
    let result = handle_session_prompt(&msg, &dispatcher, ...)?;
    
    // Verify modified prompt was applied
    assert!(result.params.as_ref().unwrap()
        .get("prompt").unwrap()
        .as_array().unwrap()[0]
        .get("text").unwrap()
        .as_str().unwrap()
        .contains("MODIFIED"));
}
```

**Pros**:
- ✅ Clean abstraction
- ✅ Type-safe
- ✅ Easy to test different scenarios
- ✅ No runtime overhead (trait is zero-cost)

**Cons**:
- ❌ Requires refactoring all call sites
- ❌ Generic parameters make function signatures more complex
- ❌ Need to thread dispatcher through many layers

---

### Option 2: Environment Variable Flag

**Concept**: Use an environment variable to enable test mode with mock responses.

**Implementation**:

```rust
// In cli/src/event_bus.rs
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    // Check for test mode
    if let Ok(test_mode) = std::env::var("AIKI_TEST_MODE") {
        return dispatch_test_mode(&test_mode, event);
    }
    
    // Normal dispatch
    dispatch_internal(event)
}

#[cfg(test)]
fn dispatch_test_mode(mode: &str, event: AikiEvent) -> Result<HookResponse> {
    match mode {
        "preprompt_modified" => {
            // Return mock modified_prompt
            Ok(HookResponse {
                success: true,
                metadata: vec![("modified_prompt".to_string(), "MODIFIED".to_string())],
                ..Default::default()
            })
        }
        "postresponse_autoreply" => {
            // Return mock autoreply
            Ok(HookResponse {
                success: true,
                metadata: vec![("autoreply".to_string(), "Fix the errors".to_string())],
                ..Default::default()
            })
        }
        "error" => {
            Err(AikiError::ActionFailed)
        }
        _ => Ok(HookResponse::success()),
    }
}
```

**Test usage**:

```rust
#[test]
fn test_handle_session_prompt_modified_prompt() {
    std::env::set_var("AIKI_TEST_MODE", "preprompt_modified");
    
    let result = handle_session_prompt(&msg, ...)?;
    
    // Verify modified prompt was applied
    assert!(/* ... */);
    
    std::env::remove_var("AIKI_TEST_MODE");
}
```

**Pros**:
- ✅ Minimal code changes
- ✅ No function signature changes
- ✅ Easy to enable/disable

**Cons**:
- ❌ Global state (env vars)
- ❌ Not type-safe
- ❌ Tests can interfere with each other
- ❌ Limited control over responses

---

### Option 3: Conditional Compilation with Test Module

**Concept**: Use `#[cfg(test)]` to replace `event_bus::dispatch()` in tests.

**Implementation**:

```rust
// In cli/src/event_bus.rs

#[cfg(not(test))]
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    dispatch_internal(event)
}

#[cfg(test)]
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    // Call test-specific dispatcher
    crate::tests::mock_event_bus::dispatch(event)
}

// In cli/src/tests/mock_event_bus.rs
use std::sync::Mutex;
use once_cell::sync::Lazy;

static MOCK_RESPONSES: Lazy<Mutex<HashMap<String, HookResponse>>> = 
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn set_response(event_type: &str, response: HookResponse) {
    MOCK_RESPONSES.lock().unwrap()
        .insert(event_type.to_string(), response);
}

pub fn clear_responses() {
    MOCK_RESPONSES.lock().unwrap().clear();
}

pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    let event_type = match &event {
        AikiEvent::PrePrompt(_) => "PrePrompt",
        AikiEvent::PostResponse(_) => "PostResponse",
        _ => "Other",
    };
    
    Ok(MOCK_RESPONSES.lock().unwrap()
        .get(event_type)
        .cloned()
        .unwrap_or_else(HookResponse::success))
}
```

**Test usage**:

```rust
#[test]
fn test_handle_session_prompt_modified_prompt() {
    mock_event_bus::set_response("PrePrompt", HookResponse {
        success: true,
        metadata: vec![("modified_prompt".to_string(), "MODIFIED".to_string())],
        ..Default::default()
    });
    
    let result = handle_session_prompt(&msg, ...)?;
    
    // Verify modified prompt was applied
    assert!(/* ... */);
    
    mock_event_bus::clear_responses();
}
```

**Pros**:
- ✅ No production code changes (only test code)
- ✅ No function signature changes
- ✅ Clean separation of test/prod code

**Cons**:
- ❌ Global mutable state (Mutex)
- ❌ Tests can interfere if responses not cleared
- ❌ Harder to debug (dispatch behavior changes in tests)
- ❌ Requires `once_cell` dependency

---

### Option 4: Extract Testable Pure Functions

**Concept**: Split functions into pure (testable) and impure (side-effecting) parts.

**Implementation**:

```rust
// Pure function - easily testable
pub fn build_modified_prompt(
    original_prompt: &[serde_json::Value],
    modified_text: &str,
) -> Vec<serde_json::Value> {
    let mut new_prompt = vec![json!({
        "type": "text",
        "text": modified_text
    })];
    
    // Preserve non-text resources
    for item in original_prompt {
        if item.get("type").and_then(|v| v.as_str()) != Some("text") {
            new_prompt.push(item.clone());
        }
    }
    
    new_prompt
}

// Pure function - easily testable
pub fn extract_modified_prompt(
    response: &HookResponse,
    original_text: &str,
) -> String {
    response.metadata
        .iter()
        .find(|(k, _)| k == "modified_prompt")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| original_text.to_string())
}

// Impure wrapper - harder to test but minimal logic
pub fn handle_session_prompt(msg: &JsonRpcMessage, ...) -> Result<JsonRpcMessage> {
    let original_text = /* extract text */;
    
    // Side effect: dispatch event
    let response = event_bus::dispatch(event)?;
    
    // Pure logic (tested separately)
    let modified_text = extract_modified_prompt(&response, &original_text);
    let new_prompt = build_modified_prompt(&prompt_array, &modified_text);
    
    // Apply changes
    /* ... */
}
```

**Test usage**:

```rust
#[test]
fn test_extract_modified_prompt_with_metadata() {
    let response = HookResponse {
        success: true,
        metadata: vec![("modified_prompt".to_string(), "MODIFIED".to_string())],
        ..Default::default()
    };
    
    let result = extract_modified_prompt(&response, "original");
    assert_eq!(result, "MODIFIED");
}

#[test]
fn test_extract_modified_prompt_fallback() {
    let response = HookResponse::success();
    
    let result = extract_modified_prompt(&response, "original");
    assert_eq!(result, "original");
}

#[test]
fn test_build_modified_prompt_preserves_images() {
    let original = vec![
        json!({"type": "text", "text": "old"}),
        json!({"type": "image", "data": "base64"}),
    ];
    
    let result = build_modified_prompt(&original, "new");
    
    assert_eq!(result.len(), 2);
    assert_eq!(result[0]["text"], "new");
    assert_eq!(result[1]["type"], "image");
}
```

**Pros**:
- ✅ No mocking needed
- ✅ Pure functions are easy to test
- ✅ No production code changes
- ✅ Tests are fast and deterministic

**Cons**:
- ❌ Doesn't test the full integration
- ❌ Still need integration tests for end-to-end flows
- ❌ Event dispatch logic itself remains untested

---

## Recommendation: Hybrid Approach (Option 4 + Integration Tests)

### Phase 1: Extract Pure Functions (Option 4)

Immediately extract and test pure functions:

1. **Text extraction logic**:
   - `extract_text_from_prompt_array()`
   - `concatenate_text_chunks()`

2. **Prompt modification logic**:
   - `build_modified_prompt()`
   - `extract_modified_prompt_from_metadata()`

3. **Autoreply logic**:
   - `extract_autoreply_from_metadata()`
   - `check_autoreply_limit()`
   - `increment_autoreply_counter()`

**Advantages**:
- ✅ Can be done incrementally
- ✅ No breaking changes
- ✅ Tests are fast and deterministic
- ✅ Immediate value (covers ~70% of the logic)

### Phase 2: Add Integration Tests

Add end-to-end integration tests in `cli/tests/` that:

1. Spin up a real agent process
2. Set up actual .aiki/flows/ with test flows
3. Send JSON-RPC messages through the proxy
4. Verify expected behavior

**Example** (`cli/tests/test_preprompt_event.rs`):

```rust
#[test]
fn test_preprompt_event_modifies_prompt() {
    // Setup: Create temp directory with flow
    let temp_dir = TempDir::new().unwrap();
    create_test_flow(&temp_dir, "preprompt", r#"
        #!/bin/bash
        echo "modified_prompt=MODIFIED"
    "#);
    
    // Start proxy with test agent
    let mut proxy = start_test_proxy(&temp_dir);
    
    // Send session/prompt
    proxy.send_json(json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "session/prompt",
        "params": {
            "sessionId": "test",
            "prompt": [{"type": "text", "text": "original"}]
        }
    }));
    
    // Verify agent received modified prompt
    let agent_received = proxy.read_agent_stdin();
    assert!(agent_received.contains("MODIFIED"));
}
```

**Advantages**:
- ✅ Tests real behavior end-to-end
- ✅ Catches integration issues
- ✅ Validates event_bus::dispatch() works correctly

**Disadvantages**:
- ❌ Slower than unit tests
- ❌ More complex setup
- ❌ Harder to debug failures

---

## Implementation Plan

### Step 1: Extract Pure Functions (Week 1)

**Files to create/modify**:
- `cli/src/commands/acp/prompt.rs` - Prompt manipulation logic
- `cli/src/commands/acp/autoreply.rs` - Autoreply logic
- `cli/src/commands/acp/metadata.rs` - Metadata extraction

**Tests to add** (~15-20 tests):
- `test_extract_text_from_prompt_array`
- `test_concatenate_text_chunks_with_separators`
- `test_build_modified_prompt_single_text`
- `test_build_modified_prompt_preserves_images`
- `test_extract_modified_prompt_with_metadata`
- `test_extract_modified_prompt_fallback_to_original`
- `test_extract_autoreply_from_metadata`
- `test_extract_autoreply_missing_returns_none`
- `test_extract_autoreply_empty_returns_none`
- `test_check_autoreply_limit_under_max`
- `test_check_autoreply_limit_at_max`
- `test_check_autoreply_limit_over_max`
- `test_increment_autoreply_counter_first_time`
- `test_increment_autoreply_counter_existing`

### Step 2: Add Integration Tests (Week 2)

**Files to create**:
- `cli/tests/test_preprompt_integration.rs`
- `cli/tests/test_postresponse_integration.rs`
- `cli/tests/test_autoreply_integration.rs`
- `cli/tests/helpers/test_proxy.rs` - Test proxy helper

**Tests to add** (~5-8 tests):
- `test_preprompt_event_fires_and_modifies_prompt`
- `test_preprompt_event_fallback_on_flow_failure`
- `test_postresponse_event_fires_on_end_turn`
- `test_postresponse_event_skips_on_max_tokens`
- `test_autoreply_sends_to_agent`
- `test_autoreply_max_limit_enforced`
- `test_autoreply_counter_resets_per_turn`

### Step 3: Consider Dependency Injection (Future)

If we need more fine-grained control, implement Option 1 (trait-based DI) for specific functions.

**When to do this**:
- After Phase 1+2 prove insufficient
- When we need to test error paths that are hard to trigger
- When we need to test timing/race conditions

---

## Success Criteria

After implementing this plan, we should have:

1. ✅ Pure functions with 100% unit test coverage
2. ✅ Integration tests covering main event flows
3. ✅ Fast test suite (unit tests < 1s, integration tests < 10s)
4. ✅ Clear separation between tested logic and I/O
5. ✅ Ability to add new tests easily

**Test count target**:
- Current: 252 tests
- After pure function extraction: ~270 tests (+18)
- After integration tests: ~280 tests (+10)
- Total increase: +28 tests (+11%)

---

## Open Questions

1. **Should we extract pure functions into separate modules?**
   - Pro: Better organization, clearer ownership
   - Con: More files to navigate

2. **Should integration tests use a mock agent or real Claude?**
   - Mock: Faster, more reliable, easier to control
   - Real: More realistic, catches real issues

3. **How do we handle StateMessage channel in tests?**
   - Current: Tests can't verify StateMessage was sent
   - Solution: Extract send operations into testable functions?

4. **Should we add property-based testing (proptest)?**
   - Could catch edge cases in text concatenation
   - Would increase test complexity

---

## Next Steps

1. Review this plan with team
2. Get consensus on hybrid approach
3. Start with Step 1 (extract pure functions)
4. Measure test coverage improvement
5. Proceed to Step 2 if needed

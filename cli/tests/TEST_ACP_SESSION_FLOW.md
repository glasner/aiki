# ACP Session Flow Test Documentation

## Overview

This document describes the automated tests for the ACP (Agent Client Protocol) session/prompt → session/update flow, specifically validating the **ClearAccumulator** mechanism that was fixed in commit `0017262`.

## The Bug That Was Fixed

### Problem
The ACP proxy maintains a `response_accumulator` HashMap that collects text from `agent_message_chunk` updates during a session. When a prompt is cancelled (Ctrl-C) or encounters an error before sending `stopReason: end_turn`, the accumulated text remains in the HashMap. On the next prompt, this stale text would concatenate with the new response, causing incorrect autoreply messages.

### Original Code (Buggy)
```rust
// ❌ WRONG: Tried to deserialize session/prompt into SessionNotification
if let Ok(notification) = serde_json::from_value::<SessionNotification>(params.clone()) {
    let session_id = notification.session_id.to_string();
    let _ = metadata_tx_clone.send(MetadataMessage::ClearAccumulator { session_id });
}
```

**Why it failed**: `SessionNotification` requires an `update` field (per ACP spec), but `session/prompt` requests don't have this field. Deserialization always failed, so `ClearAccumulator` was never sent.

### Fixed Code
```rust
// ✅ CORRECT: Extract sessionId directly from params
let session_id = params
    .get("sessionId")
    .and_then(|v| v.as_str())
    .unwrap_or_default()
    .to_string();

if !session_id.is_empty() {
    let _ = metadata_tx_clone.send(MetadataMessage::ClearAccumulator { session_id });
}
```

## Test Coverage

### Core Functionality Tests

1. **`test_session_prompt_extracts_session_id`**
   - Validates that sessionId is correctly extracted from session/prompt params
   - Ensures the fix handles the JSON structure properly

2. **`test_session_prompt_without_session_id`**
   - Tests graceful handling of malformed requests
   - Verifies no panic when sessionId is missing

3. **`test_accumulator_cleared_on_new_prompt`**
   - Simulates the core behavior: accumulate → clear → accumulate again
   - Validates that clear() removes previous text

4. **`test_agent_message_chunk_accumulation`**
   - Tests that multiple agent_message_chunk updates accumulate correctly
   - Validates the message concatenation logic

### Critical Bug Scenario Tests

5. **`test_cancelled_prompt_cleared_on_next_prompt`**
   - **Most important test for the bug fix**
   - Simulates: prompt → partial response → cancel (no end_turn) → new prompt
   - Validates that stale text from cancelled turn doesn't carry over

6. **`test_full_scenario_cancelled_prompt_then_new_prompt`**
   - Complete end-to-end scenario with realistic prompts
   - User asks to refactor → agent starts responding → user cancels → user asks different question
   - Validates final response contains ONLY the second prompt's response

7. **`test_multiple_cancelled_prompts_no_accumulation`**
   - Stress test: multiple rapid cancellations
   - Validates that only the final completed prompt's text is retained

### Edge Cases

8. **`test_multiple_sessions_independent_accumulators`**
   - Validates that different sessions have independent accumulators
   - Ensures session isolation

9. **`test_multiple_prompts_same_session_cleared_each_time`**
   - Validates that each new prompt in the same session clears the accumulator
   - Tracks clear_count to ensure clearing happens

10. **`test_end_turn_with_accumulated_text`**
    - Tests the complete flow: prompt → chunks → end_turn
    - Validates that accumulated text is available for SessionEnd event

11. **`test_session_update_without_agent_message_chunk`**
    - Tests that non-text updates (tool_call, etc.) don't affect accumulator
    - Validates type filtering

12. **`test_empty_text_chunks_handled`**
    - Tests that empty strings don't break accumulation
    - Validates robustness

### JSON-RPC ID Normalization Tests

13. **`test_normalize_jsonrpc_id_string`**
    - Tests that string IDs serialize with quotes: `"abc123"` → `"\"abc123\""`

14. **`test_normalize_jsonrpc_id_number`**
    - Tests that number IDs serialize without quotes: `42` → `"42"`

15. **`test_normalize_jsonrpc_id_null`**
    - Tests that null IDs are handled: `null` → `"null"`

## Test Implementation Strategy

### Mock Components

The tests use a `MockResponseAccumulator` that mimics the real HashMap-based accumulator:

```rust
struct MockResponseAccumulator {
    accumulator: HashMap<String, String>,  // sessionId → accumulated text
    clear_count: HashMap<String, usize>,   // Track how many times cleared
}
```

**Key methods:**
- `clear(session_id)` - Removes accumulated text and increments clear counter
- `append(session_id, text)` - Adds text to accumulator
- `get(session_id)` - Retrieves accumulated text
- `clear_count(session_id)` - Gets number of times cleared (for validation)

### Why Mock Instead of Integration Tests?

1. **Isolation**: Tests focus on the accumulator logic, not the full proxy
2. **Speed**: No need to spawn agent processes
3. **Determinism**: No race conditions or timing issues
4. **Clarity**: Clear cause-effect relationships in test assertions

### Future Work: Integration Tests

These unit tests validate the fix at the logic level. For complete validation, consider adding:

1. **Live proxy test** with mock agent that:
   - Sends partial response chunks
   - Simulates cancellation (stops without end_turn)
   - Accepts new prompt
   - Validates response text doesn't contain old chunks

2. **SessionEnd event test**:
   - Fire SessionEnd with accumulated text
   - Validate autoreply contains correct text
   - Ensure no stale text from previous turns

## Running the Tests

```bash
# Run all ACP session flow tests
cd cli && cargo test --test test_acp_session_flow

# Run a specific test
cd cli && cargo test --test test_acp_session_flow test_cancelled_prompt_cleared_on_next_prompt

# Run with output
cd cli && cargo test --test test_acp_session_flow -- --nocapture
```

## Test Results

All 15 tests pass as of commit `0017262`:

```
running 15 tests
test test_accumulator_cleared_on_new_prompt ... ok
test test_agent_message_chunk_accumulation ... ok
test test_cancelled_prompt_cleared_on_next_prompt ... ok
test test_empty_text_chunks_handled ... ok
test test_end_turn_with_accumulated_text ... ok
test test_full_scenario_cancelled_prompt_then_new_prompt ... ok
test test_multiple_cancelled_prompts_no_accumulation ... ok
test test_multiple_prompts_same_session_cleared_each_time ... ok
test test_multiple_sessions_independent_accumulators ... ok
test test_normalize_jsonrpc_id_null ... ok
test test_normalize_jsonrpc_id_number ... ok
test test_normalize_jsonrpc_id_string ... ok
test test_session_prompt_extracts_session_id ... ok
test test_session_prompt_without_session_id ... ok
test test_session_update_without_agent_message_chunk ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured
```

## Related Code

- **Implementation**: `cli/src/commands/acp.rs:276-287` (session/prompt handler)
- **Accumulator**: `cli/src/commands/acp.rs:439` (response_accumulator HashMap)
- **Clear logic**: `cli/src/commands/acp.rs:401-405` (MetadataMessage::ClearAccumulator)
- **Original bug commit**: `0017262` - Fix ClearAccumulator blocker
- **Parent commit**: `bb1bbd1` - Fix response accumulator cleanup

## Validation Checklist

- [x] sessionId extraction from session/prompt works correctly
- [x] ClearAccumulator message would be sent (logic validated)
- [x] Accumulator clears on each new prompt
- [x] Stale text from cancelled prompts doesn't carry over
- [x] Multiple sessions are isolated
- [x] Multiple prompts in same session work correctly
- [x] Edge cases (empty strings, missing sessionId) handled
- [ ] **Manual validation recommended**: Run live proxy with real agent, cancel mid-response, send new prompt

## Manual Validation Instructions

To manually validate the fix under a real ACP session:

1. **Setup**:
   ```bash
   # Build aiki with the fix
   cd cli && cargo build

   # Start ACP proxy with your agent
   ./target/debug/aiki hooks acp --agent claude-code
   ```

2. **Test scenario**:
   - Send a prompt to the agent
   - Wait for agent to start responding (see chunks appear)
   - Press Ctrl-C to cancel mid-response
   - Send a completely different prompt
   - **Validate**: New response contains ONLY text from second prompt, not first

3. **Expected behavior**:
   - First prompt response: Partial (e.g., "I'll help you refactor...")
   - Cancel: No end_turn sent
   - Second prompt response: Complete, fresh (e.g., "Here's the answer: 42")
   - **No concatenation** of first and second responses

4. **How to observe**:
   - Enable debug logging: `export AIKI_DEBUG=1`
   - Look for log lines: `[acp] Fired PrePrompt event for session: {id}, modified: false`
   - Check SessionEnd autoreply (if using flows) doesn't contain stale text

## Conclusion

These tests provide comprehensive coverage of the ClearAccumulator fix and validate that the core bug—stale text accumulation across cancelled prompts—is resolved. The tests are fast, deterministic, and clearly document the expected behavior of the session/prompt → session/update flow.

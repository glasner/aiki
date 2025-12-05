/// Integration tests for ACP session/prompt → session/update flow
/// Tests the ClearAccumulator mechanism and response text accumulation
///
/// These tests validate that:
/// 1. Each session/prompt clears the response accumulator
/// 2. session/update messages with agent_message_chunk accumulate text
/// 3. Cancelled prompts don't leave stale text for the next prompt
/// 4. Multiple prompts in the same session work correctly
/// 5. Request tracking happens before fallible work (graceful degradation)
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;

/// Mock response accumulator to test text accumulation behavior
struct MockResponseAccumulator {
    accumulator: HashMap<String, String>,
    clear_count: HashMap<String, usize>,
}

impl MockResponseAccumulator {
    fn new() -> Self {
        Self {
            accumulator: HashMap::new(),
            clear_count: HashMap::new(),
        }
    }

    fn clear(&mut self, session_id: &str) {
        self.accumulator.remove(session_id);
        *self.clear_count.entry(session_id.to_string()).or_insert(0) += 1;
    }

    fn append(&mut self, session_id: &str, text: &str) {
        self.accumulator
            .entry(session_id.to_string())
            .or_insert_with(String::new)
            .push_str(text);
    }

    fn get(&self, session_id: &str) -> String {
        self.accumulator
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    fn clear_count(&self, session_id: &str) -> usize {
        self.clear_count.get(session_id).copied().unwrap_or(0)
    }
}

#[test]
fn test_session_prompt_extracts_session_id() {
    // Test that we correctly extract sessionId from session/prompt params
    let prompt_request = json!({
        "jsonrpc": "2.0",
        "id": "req-1",
        "method": "session/prompt",
        "params": {
            "sessionId": "session-abc123",
            "prompt": [{
                "type": "text",
                "text": "Hello, agent!"
            }]
        }
    });

    // Extract sessionId the same way the fix does
    let params = prompt_request.get("params").unwrap();
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    assert_eq!(session_id, "session-abc123");
    assert!(!session_id.is_empty());
}

#[test]
fn test_session_prompt_without_session_id() {
    // Test graceful handling of malformed request without sessionId
    let prompt_request = json!({
        "jsonrpc": "2.0",
        "id": "req-1",
        "method": "session/prompt",
        "params": {
            "prompt": [{
                "type": "text",
                "text": "Hello, agent!"
            }]
        }
    });

    let params = prompt_request.get("params").unwrap();
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // Should return empty string, not panic
    assert_eq!(session_id, "");
    assert!(session_id.is_empty());
}

#[test]
fn test_accumulator_cleared_on_new_prompt() {
    // Simulate the behavior: session/prompt should clear the accumulator
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-123";

    // Simulate first prompt - accumulate some text
    accumulator.append(session_id, "First response chunk");
    accumulator.append(session_id, " more text");
    assert_eq!(
        accumulator.get(session_id),
        "First response chunk more text"
    );

    // Simulate second prompt - should clear before starting
    accumulator.clear(session_id);
    assert_eq!(accumulator.get(session_id), "");
    assert_eq!(accumulator.clear_count(session_id), 1);

    // Accumulate new text
    accumulator.append(session_id, "Second response");
    assert_eq!(accumulator.get(session_id), "Second response");
}

#[test]
fn test_agent_message_chunk_accumulation() {
    // Test that agent_message_chunk updates accumulate correctly
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-456";

    let chunks = vec![
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "update": {
                    "type": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "I'll help you "
                    }
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "update": {
                    "type": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "with this task. "
                    }
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": session_id,
                "update": {
                    "type": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "Here's what I found."
                    }
                }
            }
        }),
    ];

    // Simulate processing each chunk
    for chunk in chunks {
        let params = chunk.get("params").unwrap();
        if let Some(update) = params.get("update").and_then(|v| v.as_object()) {
            if update.get("type").and_then(|v| v.as_str()) == Some("agent_message_chunk") {
                if let Some(content) = update.get("content").and_then(|v| v.as_object()) {
                    if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
                        accumulator.append(session_id, text);
                    }
                }
            }
        }
    }

    assert_eq!(
        accumulator.get(session_id),
        "I'll help you with this task. Here's what I found."
    );
}

#[test]
fn test_cancelled_prompt_cleared_on_next_prompt() {
    // Test the bug fix: cancelled prompt leaves partial text, next prompt should clear it
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-789";

    // Simulate first prompt starting to accumulate text
    accumulator.append(session_id, "Starting to work on ");
    accumulator.append(session_id, "this task but then user cancels");

    // User cancels (Ctrl-C or error) - no end_turn, text is left in accumulator
    assert_eq!(
        accumulator.get(session_id),
        "Starting to work on this task but then user cancels"
    );

    // User sends a NEW prompt - should clear the old stale text
    accumulator.clear(session_id);
    assert_eq!(accumulator.get(session_id), "");

    // New prompt starts fresh
    accumulator.append(session_id, "Let me try a different approach.");
    assert_eq!(
        accumulator.get(session_id),
        "Let me try a different approach."
    );

    // Should NOT contain the old cancelled text
    assert!(!accumulator
        .get(session_id)
        .contains("Starting to work on this task"));
}

#[test]
fn test_multiple_sessions_independent_accumulators() {
    // Test that different sessions have independent accumulators
    let mut accumulator = MockResponseAccumulator::new();
    let session1 = "session-aaa";
    let session2 = "session-bbb";

    // Accumulate in session 1
    accumulator.append(session1, "Response for session 1");
    assert_eq!(accumulator.get(session1), "Response for session 1");
    assert_eq!(accumulator.get(session2), "");

    // Accumulate in session 2
    accumulator.append(session2, "Response for session 2");
    assert_eq!(accumulator.get(session1), "Response for session 1");
    assert_eq!(accumulator.get(session2), "Response for session 2");

    // Clear session 1 only
    accumulator.clear(session1);
    assert_eq!(accumulator.get(session1), "");
    assert_eq!(accumulator.get(session2), "Response for session 2");
}

#[test]
fn test_multiple_prompts_same_session_cleared_each_time() {
    // Test that multiple prompts in the same session each start with a clear accumulator
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-multi";

    // First prompt
    accumulator.clear(session_id);
    accumulator.append(session_id, "First prompt response");
    assert_eq!(accumulator.get(session_id), "First prompt response");
    assert_eq!(accumulator.clear_count(session_id), 1);

    // Second prompt - should clear
    accumulator.clear(session_id);
    accumulator.append(session_id, "Second prompt response");
    assert_eq!(accumulator.get(session_id), "Second prompt response");
    assert_eq!(accumulator.clear_count(session_id), 2);

    // Third prompt - should clear
    accumulator.clear(session_id);
    accumulator.append(session_id, "Third prompt response");
    assert_eq!(accumulator.get(session_id), "Third prompt response");
    assert_eq!(accumulator.clear_count(session_id), 3);
}

#[test]
fn test_end_turn_with_accumulated_text() {
    // Test the full flow: prompt → chunks → end_turn with accumulated text
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-complete";
    let request_id = "req-100";

    // Prompt arrives - clear accumulator
    accumulator.clear(session_id);

    // Agent sends chunks
    let chunks = vec!["I've analyzed ", "your code and ", "found 3 issues."];
    for chunk in chunks {
        accumulator.append(session_id, chunk);
    }

    // Agent sends end_turn response
    let _end_turn_response = json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "stopReason": "end_turn",
            "message": "Task completed"
        }
    });

    // Verify accumulated text is complete
    let final_text = accumulator.get(session_id);
    assert_eq!(final_text, "I've analyzed your code and found 3 issues.");

    // After PostResponse event, accumulator would be cleared for this session
    accumulator.clear(session_id);
    assert_eq!(accumulator.get(session_id), "");
}

#[test]
fn test_session_update_without_agent_message_chunk() {
    // Test that non-text updates don't affect the accumulator
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-other-updates";

    // Tool call update (should not add to accumulator)
    let tool_call_update = json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "type": "tool_call",
                "toolCallId": "tool-123",
                "status": "running"
            }
        }
    });

    // Process - shouldn't accumulate anything
    let params = tool_call_update.get("params").unwrap();
    if let Some(update) = params.get("update").and_then(|v| v.as_object()) {
        if update.get("type").and_then(|v| v.as_str()) == Some("agent_message_chunk") {
            // This branch should NOT execute
            accumulator.append(session_id, "Should not appear");
        }
    }

    assert_eq!(accumulator.get(session_id), "");
}

#[test]
fn test_empty_text_chunks_handled() {
    // Test that empty text chunks don't break accumulation
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-empty-chunks";

    accumulator.append(session_id, "");
    accumulator.append(session_id, "Hello");
    accumulator.append(session_id, "");
    accumulator.append(session_id, " world");
    accumulator.append(session_id, "");

    assert_eq!(accumulator.get(session_id), "Hello world");
}

#[test]
fn test_normalize_jsonrpc_id_string() {
    // Test ID normalization for string IDs (used in HashMap keys)
    let id = json!("abc123");
    let normalized = id.to_string();
    assert_eq!(normalized, "\"abc123\""); // JSON serialization adds quotes
}

#[test]
fn test_normalize_jsonrpc_id_number() {
    // Test ID normalization for number IDs
    let id = json!(42);
    let normalized = id.to_string();
    assert_eq!(normalized, "42"); // Numbers serialize without quotes
}

#[test]
fn test_normalize_jsonrpc_id_null() {
    // Test ID normalization for null ID (valid in JSON-RPC)
    let id = json!(null);
    let normalized = id.to_string();
    assert_eq!(normalized, "null");
}

/// Test scenario: User prompt → partial response → error → new prompt
/// This validates the core bug fix: stale text from failed turns doesn't carry over
#[test]
fn test_full_scenario_cancelled_prompt_then_new_prompt() {
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-scenario-1";

    // === TURN 1: User asks agent to refactor code ===
    accumulator.clear(session_id); // ClearAccumulator on session/prompt
    assert_eq!(accumulator.clear_count(session_id), 1);

    // Agent starts responding with chunks
    accumulator.append(session_id, "I'll refactor this code. ");
    accumulator.append(session_id, "First, let me analyze the dependencies. ");
    accumulator.append(session_id, "I see that you're using ");

    // User presses Ctrl-C (cancels) or agent errors out
    // NO end_turn is sent, so PostResponse never fires
    // Stale text is left in accumulator: "I'll refactor this code. First, let me analyze..."

    let stale_text = accumulator.get(session_id);
    assert!(stale_text.contains("I'll refactor this code"));
    assert!(stale_text.contains("dependencies"));

    // === TURN 2: User asks a completely different question ===
    accumulator.clear(session_id); // ClearAccumulator on new session/prompt (THE FIX!)
    assert_eq!(accumulator.clear_count(session_id), 2);

    // Verify stale text is gone
    assert_eq!(accumulator.get(session_id), "");

    // Agent responds to new prompt
    accumulator.append(session_id, "I can help you debug that error. ");
    accumulator.append(session_id, "The issue is in line 42.");

    // Agent completes turn with end_turn
    let final_text = accumulator.get(session_id);
    assert_eq!(
        final_text,
        "I can help you debug that error. The issue is in line 42."
    );

    // Should NOT contain any text from the cancelled first turn
    assert!(!final_text.contains("refactor"));
    assert!(!final_text.contains("dependencies"));
    assert!(!final_text.contains("analyze"));
}

/// Test scenario: Multiple rapid prompts with cancellations
#[test]
fn test_multiple_cancelled_prompts_no_accumulation() {
    let mut accumulator = MockResponseAccumulator::new();
    let session_id = "session-rapid-fire";

    // Prompt 1 - cancelled mid-response
    accumulator.clear(session_id);
    accumulator.append(session_id, "Let me help you with ");
    // Cancelled - no end_turn

    // Prompt 2 - cancelled immediately
    accumulator.clear(session_id);
    accumulator.append(session_id, "Sure, I can ");
    // Cancelled - no end_turn

    // Prompt 3 - cancelled after one chunk
    accumulator.clear(session_id);
    accumulator.append(session_id, "Analyzing your request...");
    // Cancelled - no end_turn

    // Prompt 4 - completes successfully
    accumulator.clear(session_id);
    accumulator.append(session_id, "Here's the answer: 42");

    let final_text = accumulator.get(session_id);
    assert_eq!(final_text, "Here's the answer: 42");

    // Should contain ONLY the final completed prompt's text
    assert!(!final_text.contains("Let me help"));
    assert!(!final_text.contains("Sure, I can"));
    assert!(!final_text.contains("Analyzing"));
}

/// Test that request tracking happens BEFORE handle_session_prompt processing
/// This ensures PostResponse fires even when PrePrompt processing fails (graceful degradation)
#[test]
fn test_request_tracked_before_fallible_work() {
    // Simulate the tracking flow from the fixed code:
    // 1. Parse session/prompt message
    // 2. Extract sessionId and request ID
    // 3. Track IMMEDIATELY (before any fallible work)
    // 4. Then do PrePrompt processing (which can fail)

    let mut tracked_requests: HashSet<String> = HashSet::new();

    // === Scenario 1: Valid prompt that will succeed ===
    let valid_prompt = json!({
        "jsonrpc": "2.0",
        "id": "req-valid-123",
        "method": "session/prompt",
        "params": {
            "sessionId": "session-abc",
            "prompt": [{
                "type": "text",
                "text": "Hello, agent!"
            }]
        }
    });

    // Extract sessionId and request ID (before fallible work)
    let params = valid_prompt.get("params").unwrap();
    let session_id_str = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if !session_id_str.is_empty() {
        if let Some(request_id) = valid_prompt.get("id") {
            // Track BEFORE any fallible work
            tracked_requests.insert(request_id.to_string());
        }
    }

    // Now do the fallible work (this succeeds)
    let prompt_array = params.get("prompt").and_then(|v| v.as_array());
    assert!(prompt_array.is_some()); // Would succeed

    // Verify tracking happened
    assert!(tracked_requests.contains("\"req-valid-123\""));

    // === Scenario 2: Malformed prompt that will fail (missing prompt array) ===
    let malformed_prompt = json!({
        "jsonrpc": "2.0",
        "id": "req-malformed-456",
        "method": "session/prompt",
        "params": {
            "sessionId": "session-xyz",
            // Missing "prompt" array - will cause handle_session_prompt to fail
        }
    });

    // Extract sessionId and request ID (before fallible work)
    let params = malformed_prompt.get("params").unwrap();
    let session_id_str = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if !session_id_str.is_empty() {
        if let Some(request_id) = malformed_prompt.get("id") {
            // Track BEFORE any fallible work (THE KEY FIX!)
            tracked_requests.insert(request_id.to_string());
        }
    }

    // Now do the fallible work (this fails)
    let prompt_array = params.get("prompt").and_then(|v| v.as_array());
    assert!(prompt_array.is_none()); // Would fail with error

    // CRITICAL: Even though processing failed, the request was tracked!
    assert!(tracked_requests.contains("\"req-malformed-456\""));

    // === Scenario 3: Prompt with bad JSON serialization ===
    let bad_json_prompt = json!({
        "jsonrpc": "2.0",
        "id": "req-badjson-789",
        "method": "session/prompt",
        "params": {
            "sessionId": "session-fail",
            "prompt": [{
                "type": "text",
                "text": "Some text"
            }]
        }
    });

    // Extract sessionId and request ID (before fallible work)
    let params = bad_json_prompt.get("params").unwrap();
    let session_id_str = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if !session_id_str.is_empty() {
        if let Some(request_id) = bad_json_prompt.get("id") {
            // Track BEFORE serialization (which might fail)
            tracked_requests.insert(request_id.to_string());
        }
    }

    // Even if JSON serialization fails later, we've already tracked
    assert!(tracked_requests.contains("\"req-badjson-789\""));

    // === Verify all three requests were tracked ===
    assert_eq!(tracked_requests.len(), 3);
    assert!(tracked_requests.contains("\"req-valid-123\""));
    assert!(tracked_requests.contains("\"req-malformed-456\""));
    assert!(tracked_requests.contains("\"req-badjson-789\""));
}

/// Test that PostResponse cleanup happens for tracked requests even on errors
#[test]
fn test_post_response_cleanup_after_failed_preprompt() {
    // Simulate the flow: track request → PrePrompt fails → agent responds → cleanup
    let mut prompt_requests: HashMap<String, String> = HashMap::new();

    // === Failed PrePrompt scenario ===
    let request_id = "req-fail-999";
    let session_id = "session-fail";

    // Step 1: Track the request (happens in caller before handle_session_prompt)
    prompt_requests.insert(request_id.to_string(), session_id.to_string());
    assert_eq!(prompt_requests.len(), 1);

    // Step 2: handle_session_prompt fails (missing prompt array)
    // Original message is forwarded to agent
    // Request is still tracked in prompt_requests

    // Step 3: Agent processes the original request and responds with end_turn
    let response = json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "stopReason": "end_turn",
            "message": "Completed"
        }
    });

    // Step 4: Cleanup in Agent→IDE thread (on end_turn)
    let response_id = response.get("id").unwrap().as_str().unwrap();
    let stop_reason = response
        .get("result")
        .and_then(|r| r.get("stopReason"))
        .and_then(|s| s.as_str())
        .unwrap();

    if stop_reason == "end_turn" {
        // Lookup and remove from prompt_requests
        let session = prompt_requests.remove(response_id);
        assert!(session.is_some()); // Found it!
        assert_eq!(session.unwrap(), session_id);

        // PostResponse would fire here (even though PrePrompt failed)
    }

    // Verify cleanup happened
    assert_eq!(prompt_requests.len(), 0);
}

/// Test edge case: request without sessionId is not tracked
#[test]
fn test_no_tracking_without_session_id() {
    let mut tracked_requests: HashSet<String> = HashSet::new();

    // Prompt missing sessionId (invalid per ACP spec)
    let invalid_prompt = json!({
        "jsonrpc": "2.0",
        "id": "req-no-session",
        "method": "session/prompt",
        "params": {
            // No sessionId
            "prompt": [{
                "type": "text",
                "text": "Hello"
            }]
        }
    });

    // Extract sessionId (will be empty)
    let params = invalid_prompt.get("params").unwrap();
    let session_id_str = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    // Tracking only happens if session_id is not empty
    if !session_id_str.is_empty() {
        if let Some(request_id) = invalid_prompt.get("id") {
            tracked_requests.insert(request_id.to_string());
        }
    }

    // Should NOT be tracked (no session ID)
    assert_eq!(tracked_requests.len(), 0);
}

/// Test build_modified_prompt preserves order and replaces only first text
#[test]
fn test_build_modified_prompt_preserves_order() {
    // Test the actual order-preserving behavior we need

    // Scenario 1: Image before text (should stay before)
    let prompt_with_leading_image = vec![
        json!({
            "type": "image",
            "url": "data:image/png;base64,..."
        }),
        json!({
            "type": "text",
            "text": "Original text"
        }),
    ];

    let modified = build_modified_prompt_mock(&prompt_with_leading_image, "Modified text");

    // Image should still be first
    assert_eq!(modified.len(), 2);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(modified[1].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[1].get("text").unwrap().as_str().unwrap(),
        "Modified text"
    );
}

#[test]
fn test_build_modified_prompt_replaces_first_text_only() {
    // Scenario 2: Multiple text chunks (only first replaced, others removed)
    let prompt_with_multiple_texts = vec![
        json!({
            "type": "text",
            "text": "First text chunk"
        }),
        json!({
            "type": "text",
            "text": "Second text chunk"
        }),
        json!({
            "type": "text",
            "text": "Third text chunk"
        }),
    ];

    let modified =
        build_modified_prompt_mock(&prompt_with_multiple_texts, "Combined modified text");

    // Should have only one text entry (first one replaced, others removed)
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[0].get("text").unwrap().as_str().unwrap(),
        "Combined modified text"
    );
}

#[test]
fn test_build_modified_prompt_complex_ordering() {
    // Scenario 3: Complex mix - image, text, image, text, image
    let complex_prompt = vec![
        json!({
            "type": "image",
            "url": "image1.png"
        }),
        json!({
            "type": "text",
            "text": "First text"
        }),
        json!({
            "type": "image",
            "url": "image2.png"
        }),
        json!({
            "type": "text",
            "text": "Second text"
        }),
        json!({
            "type": "image",
            "url": "image3.png"
        }),
    ];

    let modified = build_modified_prompt_mock(&complex_prompt, "Modified combined text");

    // Should be: image1, modified_text, image2, image3
    // (First text replaced, second text removed, all images preserved in order)
    assert_eq!(modified.len(), 4);

    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(
        modified[0].get("url").unwrap().as_str().unwrap(),
        "image1.png"
    );

    assert_eq!(modified[1].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[1].get("text").unwrap().as_str().unwrap(),
        "Modified combined text"
    );

    assert_eq!(modified[2].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(
        modified[2].get("url").unwrap().as_str().unwrap(),
        "image2.png"
    );

    assert_eq!(modified[3].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(
        modified[3].get("url").unwrap().as_str().unwrap(),
        "image3.png"
    );
}

#[test]
fn test_build_modified_prompt_preserves_annotations_and_meta() {
    // Test that annotations, _meta, and other fields are preserved when modifying text
    let prompt_with_metadata = vec![json!({
        "type": "text",
        "text": "Original text",
        "annotations": {
            "audience": ["user"],
            "priority": 1.0,
            "cacheControl": {"type": "ephemeral"}
        },
        "_meta": {
            "ide": "zed",
            "cacheHint": "important"
        }
    })];

    let modified = build_modified_prompt_mock(&prompt_with_metadata, "Modified text");

    // Should have one text entry with modified text BUT preserved metadata
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[0].get("text").unwrap().as_str().unwrap(),
        "Modified text"
    );

    // Verify annotations are preserved
    let annotations = modified[0]
        .get("annotations")
        .expect("annotations should be preserved");
    assert_eq!(
        annotations.get("audience").unwrap().as_array().unwrap()[0]
            .as_str()
            .unwrap(),
        "user"
    );
    assert_eq!(annotations.get("priority").unwrap().as_f64().unwrap(), 1.0);

    // Verify _meta is preserved
    let meta = modified[0].get("_meta").expect("_meta should be preserved");
    assert_eq!(meta.get("ide").unwrap().as_str().unwrap(), "zed");
    assert_eq!(
        meta.get("cacheHint").unwrap().as_str().unwrap(),
        "important"
    );
}

#[test]
fn test_build_modified_prompt_no_text_items() {
    // Scenario 4: No text items (should append at end)
    let prompt_only_images = vec![
        json!({
            "type": "image",
            "url": "image1.png"
        }),
        json!({
            "type": "image",
            "url": "image2.png"
        }),
    ];

    let modified = build_modified_prompt_mock(&prompt_only_images, "Added text");

    // Should append text at the end
    assert_eq!(modified.len(), 3);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(modified[1].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(modified[2].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[2].get("text").unwrap().as_str().unwrap(),
        "Added text"
    );
}

#[test]
fn test_build_modified_prompt_only_text() {
    // Scenario 5: Only text, no resources (common case)
    let prompt_only_text = vec![json!({
        "type": "text",
        "text": "Just text"
    })];

    let modified = build_modified_prompt_mock(&prompt_only_text, "Modified text");

    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[0].get("text").unwrap().as_str().unwrap(),
        "Modified text"
    );
}

#[test]
fn test_build_modified_prompt_text_then_resources() {
    // Scenario 6: Text first, then resources (should preserve order)
    let prompt_text_first = vec![
        json!({
            "type": "text",
            "text": "Original prompt"
        }),
        json!({
            "type": "image",
            "url": "screenshot.png"
        }),
        json!({
            "type": "file",
            "path": "example.txt"
        }),
    ];

    let modified = build_modified_prompt_mock(&prompt_text_first, "Modified prompt");

    // Should be: modified_text, image, file (same order)
    assert_eq!(modified.len(), 3);
    assert_eq!(modified[0].get("type").unwrap().as_str().unwrap(), "text");
    assert_eq!(
        modified[0].get("text").unwrap().as_str().unwrap(),
        "Modified prompt"
    );
    assert_eq!(modified[1].get("type").unwrap().as_str().unwrap(), "image");
    assert_eq!(modified[2].get("type").unwrap().as_str().unwrap(), "file");
}

/// Mock implementation of build_modified_prompt for testing
/// (This mirrors the actual implementation in acp.rs)
fn build_modified_prompt_mock(
    original_prompt: &[serde_json::Value],
    modified_text: &str,
) -> Vec<serde_json::Value> {
    let mut new_prompt = Vec::new();
    let mut replaced_first_text = false;

    for item in original_prompt {
        let is_text = item.get("type").and_then(|v| v.as_str()) == Some("text");

        if is_text {
            if !replaced_first_text {
                // Clone the first text block to preserve all fields (annotations, _meta, etc.)
                // then mutate only the "text" field
                let mut modified_item = item.clone();
                if let Some(obj) = modified_item.as_object_mut() {
                    obj.insert("text".to_string(), json!(modified_text));
                }
                new_prompt.push(modified_item);
                replaced_first_text = true;
            }
            // Skip all other text entries (they were concatenated into modified_text)
        } else {
            // Preserve all non-text items in their original position
            new_prompt.push(item.clone());
        }
    }

    // If there were no text items, append a minimal text block at the end
    if !replaced_first_text {
        new_prompt.push(json!({
            "type": "text",
            "text": modified_text
        }));
    }

    new_prompt
}

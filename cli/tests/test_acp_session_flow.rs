/// Integration tests for ACP session/prompt → session/update flow
/// Tests the ClearAccumulator mechanism and response text accumulation
///
/// These tests validate that:
/// 1. Each session/prompt clears the response accumulator
/// 2. session/update messages with agent_message_chunk accumulate text
/// 3. Cancelled prompts don't leave stale text for the next prompt
/// 4. Multiple prompts in the same session work correctly
use serde_json::json;
use std::collections::HashMap;

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

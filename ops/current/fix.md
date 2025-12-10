# SessionEnd Event Bug - Never Emitted

**Status**: Identified - Ready to Fix  
**Date**: 2025-12-10  
**Priority**: High (causes resource leak and dead code)

## The Problem

**`AikiEvent::SessionEnd` is never emitted**, causing two critical failures:

1. **Session file leak**: `.aiki/sessions/*` files accumulate and are never cleaned up
2. **Dead code**: SessionEnd flows never execute (user-defined session cleanup/validation workflows)

## Root Causes

1. **Missing event type**: No `AikiSessionEndEvent` struct or `AikiEvent::SessionEnd` variant exists in the codebase
2. **Naming confusion**: The `handle_session_end()` function in `acp.rs` (line 1522) is **misnamed** - it actually handles turn completion (`stopReason: end_turn`) and fires `PostResponse`, not a SessionEnd event
3. **Incomplete implementation**: The session cleanup function `session::end_session()` exists but is never called outside of tests

## PostResponse vs SessionEnd Distinction

Based on the code and documentation:

**PostResponse**: 
- Fires after *each agent response* during a turn
- Used for response validation and autoreplies
- Can fire multiple times per session (for streaming responses)
- **Currently implemented**: The ACP proxy's `handle_session_end` function (line 1522) dispatches `AikiEvent::PostResponse` when `stopReason == "end_turn"`

**SessionEnd** (intended but not implemented):
- Should fire *once when the session actually ends* (agent/IDE disconnects)
- Should trigger session file cleanup (delete `.aiki/sessions/{uuid}`)
- Should execute the `SessionEnd:` flow section from `flow.yaml`
- **Not implemented**: There is no `AikiEvent::SessionEnd` variant, no `AikiSessionEndEvent` struct, and no handler in the event bus

## Current State Analysis

### 1. Events Module (`cli/src/events.rs`)
- ❌ No `AikiSessionEndEvent` struct exists
- ❌ No `AikiEvent::SessionEnd` variant exists
- ✅ Only has: `SessionStart`, `PrePrompt`, `PreFileChange`, `PostFileChange`, `PostResponse`, `PrepareCommitMessage`, `Unsupported`

### 2. Event Bus (`cli/src/event_bus.rs`)
- ❌ No handler for `AikiEvent::SessionEnd`
- The dispatch function only routes 7 event types (none for SessionEnd)

### 3. Handlers Module (`cli/src/handlers.rs`)
- ❌ No `handle_session_end` function exists in handlers.rs
- There's a `handle_session_end` function in `acp.rs` but it's **misnamed** - it actually fires `PostResponse`, not a SessionEnd event

### 4. ACP Proxy (`cli/src/commands/acp.rs`)
- ✅ Lines 720-769: Detects `stopReason == "end_turn"` 
- ❌ Calls `handle_session_end()` function (line 1522) which **only dispatches PostResponse**
- ❌ Never cleans up session files
- ❌ Never emits `AikiEvent::SessionEnd`

### 5. Vendor Hooks (Cursor/Claude Code)
- ✅ Both have "Stop" events
- ❌ Both build `PostResponse` events (cursor.rs:206, claude_code.rs:267)
- ❌ Neither emits SessionEnd events
- ❌ Neither cleans up session files

### 6. Flow Engine (`cli/src/flows/types.rs`)
- ✅ Line 112: `session_end: Vec<FlowStatement>` field exists
- ✅ Flow.yaml can have a `SessionEnd:` section
- ❌ This section is **dead code** - never executed because no SessionEnd events are dispatched

### 7. Core Flow (`cli/src/flows/core/flow.yaml`)
- ❌ No `SessionEnd:` section defined
- The test flow (`cli/tests/test_flow_yaml.yaml`) has a SessionEnd section, but it's never tested

### 8. Session Cleanup (`cli/src/session.rs`)
- ✅ Line 351: `end_session()` function exists
- ✅ Line 363: Calls `session.file(&repo_path).delete()`
- ❌ **This function is never called** - only used in unit tests

## What Cleanup/Flows Are Not Running?

**Session file cleanup:**
- `.aiki/sessions/{uuid}` files are created on SessionStart
- They're **never deleted** because `session::end_session()` is never called
- This causes `.aiki/sessions/` to accumulate stale session files over time

**SessionEnd flows:**
- Users can define `SessionEnd:` sections in their flow.yaml
- The flow engine supports parsing this section (`Flow.session_end`)
- **These flows never execute** because no SessionEnd events are dispatched
- Example use cases that don't work:
  - Session-level cleanup actions
  - Final validation workflows
  - Metrics/logging aggregation
  - Resource cleanup

## Implementation Plan

### Phase 1: Define SessionEnd Event Type

1. **Add `AikiSessionEndEvent` struct** (`cli/src/events.rs`)
   ```rust
   pub struct AikiSessionEndEvent {
       pub session: AikiSession,
       pub cwd: PathBuf,
       pub timestamp: DateTime<Utc>,
   }
   ```

2. **Add `SessionEnd` variant to `AikiEvent` enum** (`cli/src/events.rs`)
   ```rust
   pub enum AikiEvent {
       // ... existing variants ...
       SessionEnd(AikiSessionEndEvent),
   }
   ```

3. **Update `impl AikiState<T>`** to handle `AikiSessionEndEvent`

### Phase 2: Add SessionEnd Handler

4. **Create `handle_session_end()` in handlers.rs** (`cli/src/handlers.rs`)
   ```rust
   pub fn handle_session_end(event: AikiSessionEndEvent) -> Result<HookResponse> {
       if std::env::var("AIKI_DEBUG").is_ok() {
           eprintln!("[aiki] Session ended by {:?}", event.session.agent_type());
       }

       // Load core flow
       let core_flow = crate::flows::load_core_flow()?;
       
       // Build execution state from event
       let mut state = AikiState::new(event.clone());
       
       // Set flow name for self.* function resolution
       state.flow_name = Some("aiki/core".to_string());
       
       // Execute SessionEnd statements from the core flow
       let (flow_result, _timing) =
           FlowEngine::execute_statements(&core_flow.session_end, &mut state)?;
       
       // Clean up session file
       crate::session::end_session(
           &event.cwd,
           event.session.agent_type(),
           event.session.external_id(),
           event.session.detection_method().clone(),
       )?;
       
       // Extract failures from state
       let failures = state.take_failures();
       
       // Translate FlowResult to HookResponse
       match flow_result {
           FlowResult::Success | FlowResult::FailedContinue | FlowResult::FailedStop => {
               Ok(HookResponse {
                   context: None,
                   decision: Decision::Allow,
                   failures,
               })
           }
           FlowResult::FailedBlock => Ok(HookResponse {
               context: None,
               decision: Decision::Block,
               failures,
           }),
       }
   }
   ```

5. **Update event bus dispatch** (`cli/src/event_bus.rs`)
   - Add SessionEnd arm to dispatch match statement
   - Route to new `handle_session_end()` handler

### Phase 3: Emit SessionEnd Events

6. **Event Bus** (`cli/src/event_bus.rs`) - **Primary SessionEnd trigger**
   - After `handle_post_response()` returns, check if `response.context` contains an autoreply
   - If no autoreply exists, fire `SessionEnd` event automatically
   - This provides automatic session cleanup when conversation naturally ends
   - Implementation:
     ```rust
     AikiEvent::PostResponse(e) => {
         let response = handlers::handle_post_response(e.clone())?;
         
         // If PostResponse didn't produce an autoreply, the session is done
         if response.context.is_none() {
             let session_end_event = AikiEvent::SessionEnd(AikiSessionEndEvent {
                 session: e.session,
                 cwd: e.cwd,
                 timestamp: chrono::Utc::now(),
             });
             handlers::handle_session_end(session_end_event)?;
         }
         
         Ok(response)
     }
     ```


### Phase 4: Add Flow Definition

9. **Add SessionEnd section to core flow.yaml** (`cli/src/flows/core/flow.yaml`)
   ```yaml
   # SessionEnd: Handle actions when agent session ends
   SessionEnd:
     # Session file cleanup is handled automatically by the event handler
     # This section is for user-defined cleanup actions
     # (empty by default, users can override in their flows)
   ```

### Phase 5: Test & Verify

10. **Verify compilation** - `cargo build`
11. **Test session cleanup** - Start and end a session, verify `.aiki/sessions/` file is deleted
12. **Test SessionEnd flows** - Add test flow with SessionEnd actions and verify execution
13. **Update tests** - Fix `test_session_end.rs` to actually test SessionEnd events

## Key Insight: Turn End vs Session End

The current `handle_session_end()` function (acp.rs:1522) fires on `stopReason: end_turn`, which means:

- ✅ It correctly handles autoreplies after agent responses
- ✅ It correctly fires PostResponse events
- ❌ And session cleanup never happens because there's no actual session-end detection


## Related Files

- `cli/src/events.rs` - Define event types
- `cli/src/handlers.rs` - Add SessionEnd handler
- `cli/src/event_bus.rs` - Route SessionEnd events
- `cli/src/commands/acp.rs` - Emit SessionEnd on disconnect, rename `handle_session_end`
- `cli/src/vendors/cursor.rs` - Change Stop hook to SessionEnd
- `cli/src/vendors/claude_code.rs` - Change Stop hook to SessionEnd
- `cli/src/flows/core/flow.yaml` - Define SessionEnd flow section
- `cli/src/session.rs` - `end_session()` cleanup function (already exists)
- `cli/tests/test_session_end.rs` - Update tests to actually test SessionEnd

## References

- Milestone document: `ops/current/milestone-1.2-session-end.md` (shows this was planned but incomplete)
- Test file: `cli/tests/test_session_end.rs` (tests PostResponse, not SessionEnd)
- Test flow: `cli/tests/test_flow_yaml.yaml` (has SessionEnd section that's never executed)

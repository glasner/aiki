# SessionEnd Implementation Fix Plan

## Executive Summary

The SessionEnd event implementation has **three critical bugs** that prevent it from working as designed:

1. **PostResponse always fills context** → SessionEnd never fires (context is `Some("")`, not `None`)
2. **SessionEnd errors are silently discarded** → failures go unnoticed (direct call, not dispatched)
3. **Tests don't verify intended behavior** → regressions slip through (no session file lifecycle tests)

**Note:** The vendors (ACP, Cursor, Claude Code) are actually correct - they emit PostResponse, and the event bus is supposed to automatically trigger SessionEnd when there's no autoreply. The bug is in the event bus logic (Bug #1 and #2), not the vendors.

**Impact:** Session files persist indefinitely, SessionEnd flows never execute, users cannot react to session completion.

## Root Cause Analysis

### Bug 1: PostResponse Always Fills Context

**Location:** `cli/src/handlers.rs:360-379` (`handle_post_response`)

**Problem:**
```rust
// PostResponse never blocks - always allow
Ok(HookResponse {
    context: state.build_context(),  // ❌ Always Some(String), never None
    decision: Decision::Allow,
    failures,
})
```

**Why it's wrong:**
- `state.build_context()` returns `Some(String)` even when empty (`Some("")`)
- Dispatcher checks `response.context.is_none()` to fire SessionEnd (`cli/src/event_bus.rs:50`)
- Condition is **never true**, so SessionEnd never fires

**Evidence from tests:** `cli/tests/test_session_end.rs:30-53` confirms context is `Some("")` with no actions.

### Bug 2: SessionEnd Errors Are Discarded

**Location:** `cli/src/event_bus.rs:60-62`

**Problem:**
```rust
// Dispatch SessionEnd event (ignore failures to preserve PostResponse result)
let _ = handlers::handle_session_end(session_end_event);
```

**Why it's wrong:**
- Comment says "ignore failures" but **discards ALL output**
- Plan (`ops/current/plan.md:1250-1305`) requires dispatching through `event_bus::dispatch` and merging failures
- User-defined SessionEnd flows' failures are silently lost
- Session file deletion errors are silently lost

**Correct design:** Dispatch through event bus, merge failures into PostResponse response.

### Bug 3: Actually Not a Bug - Vendors Are Correct

**Locations:**
- `cli/src/commands/acp.rs:1517-1546` - ACP `handle_session_end`
- `cli/src/vendors/cursor.rs:204-213` - Cursor `stop` hook
- `cli/src/vendors/claude_code.rs:262-273` - Claude Code `stop` hook

**What they do:** All three emit `PostResponse` events when the agent session ends.

**This is CORRECT design:**
- ✅ Vendors emit PostResponse (simple, consistent)
- ✅ Event bus handles PostResponse → SessionEnd transition automatically
- ✅ SessionEnd fires when PostResponse has no autoreply
- ✅ Clean separation of concerns

**Why SessionEnd doesn't work:**
- Not because vendors are wrong
- Because Bug #1 (context check) prevents the transition from happening
- Fix Bug #1 and #2, and vendors will work correctly

### Bug 4: Tests Don't Cover Intended Behavior

**Location:** `cli/tests/test_session_end.rs:1-160`

**Problem:** Tests build `AikiPostResponseEvent` and assert autoreply assembly, but **never verify:**
- PostResponse without autoreply removes session file
- PostResponse with autoreply keeps session file
- SessionEnd flows execute
- SessionEnd can block/fail

**What plan requires:** `ops/current/plan.md:1330-1381` specifies exactly these checks.

## Fix Strategy

### Principle: Minimal, Targeted Changes

We'll fix each bug independently with minimal disruption:

1. **Fix context check logic** - Add `has_context()` helper method
2. **Fix error propagation** - Dispatch through event bus, merge failures
3. **Add graceful degradation** - SessionEnd errors don't break PostResponse (Aiki promise)
4. **Keep vendor behavior** - Vendors emit PostResponse (not SessionEnd)
5. **Add integration tests** - Verify session file lifecycle

**Key decisions:**
- Vendors should NOT emit SessionEnd directly. The dispatcher handles the PostResponse → SessionEnd transition based on autoreply presence. This maintains clean separation of concerns.
- **Graceful degradation is required**: Per Aiki's core promise, hook failures must not block agent execution. If SessionEnd dispatch fails or session file cleanup fails, we log warnings but allow PostResponse to succeed.

## Detailed Fixes

### Fix 1: Context Check Logic

**File:** `cli/src/event_bus.rs:42-67`

**Current code:**
```rust
AikiEvent::PostResponse(e) => {
    // Extract fields we'll need for SessionEnd before consuming the event
    let session = e.session.clone();
    let cwd = e.cwd.clone();

    // Handle PostResponse and check for autoreply
    let response = handlers::handle_post_response(e)?;

    // If PostResponse didn't produce an autoreply, the session is done
    // Automatically fire SessionEnd event for cleanup
    if response.context.is_none() {  // ❌ Never true!
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[aiki] No autoreply generated - ending session automatically");
        }

        let session_end_event = crate::events::AikiSessionEndEvent {
            session,
            cwd,
            timestamp: chrono::Utc::now(),
        };

        // Dispatch SessionEnd event (ignore failures to preserve PostResponse result)
        let _ = handlers::handle_session_end(session_end_event);  // ❌ Discards errors!
    }

    Ok(response)
}
```

**Fixed code:**
```rust
AikiEvent::PostResponse(e) => {
    // Extract fields we'll need for SessionEnd before consuming the event
    let session = e.session.clone();
    let cwd = e.cwd.clone();

    // Handle PostResponse and check for autoreply
    let mut response = handlers::handle_post_response(e)?;

    // If PostResponse didn't produce an autoreply, the session is done
    // Automatically fire SessionEnd event for cleanup
    // Check if context is empty string OR None (both mean no autoreply)
    let has_autoreply = response.context
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    
    if !has_autoreply {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[aiki] No autoreply generated - ending session automatically");
        }

        let session_end_event = crate::events::AikiSessionEndEvent {
            session,
            cwd,
            timestamp: chrono::Utc::now(),
        };

        // Dispatch SessionEnd through event bus to execute flows
        // Graceful degradation: SessionEnd errors should not block PostResponse
        match dispatch(AikiEvent::SessionEnd(session_end_event)) {
            Ok(session_end_response) => {
                // Merge SessionEnd failures into PostResponse response
                response.failures.extend(session_end_response.failures);
                
                // If SessionEnd blocks, propagate that decision
                if session_end_response.decision == Decision::Block {
                    response.decision = Decision::Block;
                }
            }
            Err(e) => {
                // Log SessionEnd dispatch error but don't fail PostResponse
                eprintln!("Warning: SessionEnd dispatch failed: {}", e);
                eprintln!("PostResponse will continue (graceful degradation)");
                // Session file may not be cleaned up, but agent can continue
            }
        }
    }

    Ok(response)
}
```

**Why this works:**
- ✅ Uses `has_context()` helper for clean, self-documenting check
- ✅ Dispatches SessionEnd through event bus (executes flows)
- ✅ Merges failures (user can see SessionEnd problems)
- ✅ Respects blocking decisions (SessionEnd can block)
- ✅ **Graceful degradation**: SessionEnd errors don't break PostResponse

### Fix 2: HookResponse Context Semantics

**File:** `cli/src/handlers.rs` (struct definition)

**Current implementation:** `context: Option<String>` with `build_context()` always returning `Some(String)`

**Option A: Keep current structure, fix usage**

No changes to `HookResponse` struct. Just fix how we check for empty autoreply (done in Fix 1).

**Rationale:**
- ✅ Minimal change (no struct modifications)
- ✅ `Some("")` vs `None` is semantic distinction without practical difference
- ✅ Event bus already checks properly with Fix 1
- ✅ Avoids cascading changes to all handlers

**This is the recommended approach.**

**Option B: Add helper method (if we want cleaner semantics)** ✅ **CHOSEN**

```rust
impl HookResponse {
    /// Check if this response has non-empty context (e.g., autoreply text)
    pub fn has_context(&self) -> bool {
        self.context
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }
}
```

Then in event bus:
```rust
if !response.has_context() {
    // Fire SessionEnd (no autoreply to continue session)
}
```

**Rationale:**
- ✅ Cleaner semantics
- ✅ Self-documenting code
- ✅ Encapsulates the "what is an autoreply" logic in one place
- ✅ Simpler name: `has_context` is more general and accurate

**Decision: Use Option B with `has_context()` naming.**

### Fix 3: Integration Tests

**File:** `cli/tests/test_session_end.rs` (replace entire file)

**New test suite:**

```rust
/// Integration tests for SessionEnd event and session file lifecycle
use aiki::events::{AikiEvent, AikiPostResponseEvent};
use aiki::flows::{AikiState, Flow};
use aiki::provenance::{AgentType, DetectionMethod};
use aiki::session::AikiSession;
use aiki::{event_bus, session_tracking};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Set up a test repository with Aiki initialized
fn setup_test_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();
    
    // Create .aiki directory structure
    fs::create_dir_all(repo_path.join(".aiki/flows")).unwrap();
    fs::create_dir_all(repo_path.join(".aiki/sessions")).unwrap();
    
    // Write minimal core flow
    let core_flow = r#"
SessionEnd:
  - Log: { message: "Session ended: {event.session.uuid}" }
"#;
    fs::write(repo_path.join(".aiki/flows/core.yaml"), core_flow).unwrap();
    
    temp_dir
}

/// Write a custom flow to the test repo
fn write_flow(repo_path: &std::path::Path, content: &str) {
    fs::write(repo_path.join(".aiki/flows/core.yaml"), content).unwrap();
}

#[test]
fn test_session_file_removed_without_autoreply() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create session
    let session = AikiSession::new(
        AgentType::Cursor,
        "test-session-123".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record session (creates session file)
    session_tracking::record_session_start(
        repo_path,
        AgentType::Cursor,
        "test-session-123",
        None,
    )
    .unwrap();
    
    // Verify session file exists
    let session_file = repo_path
        .join(".aiki/sessions")
        .join(session.uuid_filename());
    assert!(session_file.exists(), "Session file should exist after recording");
    
    // Fire PostResponse with no autoreply (empty flow)
    write_flow(repo_path, "PostResponse: []");
    
    let event = AikiPostResponseEvent {
        session: session.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Task completed.".to_string(),
        modified_files: vec![],
    };
    
    let response = event_bus::dispatch(AikiEvent::PostResponse(event)).unwrap();
    
    // Verify no autoreply
    assert!(
        response.context.is_none() || response.context.as_ref().unwrap().is_empty(),
        "Should have no autoreply"
    );
    
    // Verify session file was removed (SessionEnd fired)
    assert!(
        !session_file.exists(),
        "Session file should be removed after SessionEnd"
    );
}

#[test]
fn test_session_file_kept_with_autoreply() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create session
    let session = AikiSession::new(
        AgentType::Cursor,
        "test-session-456".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record session
    session_tracking::record_session_start(
        repo_path,
        AgentType::Cursor,
        "test-session-456",
        None,
    )
    .unwrap();
    
    // Verify session file exists
    let session_file = repo_path
        .join(".aiki/sessions")
        .join(session.uuid_filename());
    assert!(session_file.exists());
    
    // Fire PostResponse with autoreply
    let flow_with_autoreply = r#"
PostResponse:
  - Autoreply: { message: "Continue working on this task?" }
"#;
    write_flow(repo_path, flow_with_autoreply);
    
    let event = AikiPostResponseEvent {
        session: session.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Task completed.".to_string(),
        modified_files: vec![],
    };
    
    let response = event_bus::dispatch(AikiEvent::PostResponse(event)).unwrap();
    
    // Verify autoreply present
    assert!(
        response.context.is_some() && !response.context.as_ref().unwrap().is_empty(),
        "Should have autoreply"
    );
    
    // Verify session file still exists (SessionEnd NOT fired)
    assert!(
        session_file.exists(),
        "Session file should remain when autoreply present"
    );
}

#[test]
fn test_session_end_flows_execute() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create session
    let session = AikiSession::new(
        AgentType::Claude,
        "test-session-789".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record session
    session_tracking::record_session_start(
        repo_path,
        AgentType::Claude,
        "test-session-789",
        None,
    )
    .unwrap();
    
    // Create flow that writes a marker file on SessionEnd
    let marker_path = repo_path.join("session_end_marker.txt");
    let flow_with_sessionend = format!(
        r#"
PostResponse: []
SessionEnd:
  - Bash:
      command: "echo 'Session ended' > {}"
"#,
        marker_path.display()
    );
    write_flow(repo_path, &flow_with_sessionend);
    
    // Fire PostResponse (should trigger SessionEnd)
    let event = AikiPostResponseEvent {
        session: session.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Done.".to_string(),
        modified_files: vec![],
    };
    
    event_bus::dispatch(AikiEvent::PostResponse(event)).unwrap();
    
    // Verify SessionEnd flow executed (marker file created)
    assert!(
        marker_path.exists(),
        "SessionEnd flow should have created marker file"
    );
    
    let content = fs::read_to_string(&marker_path).unwrap();
    assert!(
        content.contains("Session ended"),
        "Marker file should contain expected content"
    );
}

#[test]
fn test_session_end_failures_propagate() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create session
    let session = AikiSession::new(
        AgentType::Cursor,
        "test-session-fail".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record session
    session_tracking::record_session_start(
        repo_path,
        AgentType::Cursor,
        "test-session-fail",
        None,
    )
    .unwrap();
    
    // Create flow with failing SessionEnd action
    let flow_with_failure = r#"
PostResponse: []
SessionEnd:
  - Bash:
      command: "exit 1"
      on_failure: "continue"
"#;
    write_flow(repo_path, flow_with_failure);
    
    // Fire PostResponse
    let event = AikiPostResponseEvent {
        session: session.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Done.".to_string(),
        modified_files: vec![],
    };
    
    let response = event_bus::dispatch(AikiEvent::PostResponse(event)).unwrap();
    
    // Verify failure was captured
    assert!(
        !response.failures.is_empty(),
        "SessionEnd failure should propagate to PostResponse"
    );
}

#[test]
fn test_multiple_sessions_independent_cleanup() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create two sessions
    let session1 = AikiSession::new(
        AgentType::Cursor,
        "session-1".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    let session2 = AikiSession::new(
        AgentType::Cursor,
        "session-2".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record both sessions
    session_tracking::record_session_start(repo_path, AgentType::Cursor, "session-1", None)
        .unwrap();
    session_tracking::record_session_start(repo_path, AgentType::Cursor, "session-2", None)
        .unwrap();
    
    let session1_file = repo_path.join(".aiki/sessions").join(session1.uuid_filename());
    let session2_file = repo_path.join(".aiki/sessions").join(session2.uuid_filename());
    
    assert!(session1_file.exists());
    assert!(session2_file.exists());
    
    // End session 1 (no autoreply)
    write_flow(repo_path, "PostResponse: []");
    
    let event1 = AikiPostResponseEvent {
        session: session1.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Done.".to_string(),
        modified_files: vec![],
    };
    
    event_bus::dispatch(AikiEvent::PostResponse(event1)).unwrap();
    
    // Session 1 cleaned up, session 2 still active
    assert!(!session1_file.exists(), "Session 1 should be cleaned up");
    assert!(session2_file.exists(), "Session 2 should remain active");
    
    // End session 2
    let event2 = AikiPostResponseEvent {
        session: session2.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Done.".to_string(),
        modified_files: vec![],
    };
    
    event_bus::dispatch(AikiEvent::PostResponse(event2)).unwrap();
    
    // Both cleaned up
    assert!(!session1_file.exists());
    assert!(!session2_file.exists());
}

#[test]
fn test_graceful_degradation_sessionend_dispatch_error() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    
    // Create session
    let session = AikiSession::new(
        AgentType::Cursor,
        "test-session-graceful".to_string(),
        None::<&str>,
        DetectionMethod::Hook,
    )
    .unwrap();
    
    // Record session
    session_tracking::record_session_start(
        repo_path,
        AgentType::Cursor,
        "test-session-graceful",
        None,
    )
    .unwrap();
    
    // Write a malformed flow that will cause SessionEnd to fail
    let bad_flow = r#"
PostResponse: []
SessionEnd:
  - InvalidAction: { this: "will cause parse error" }
"#;
    write_flow(repo_path, bad_flow);
    
    // Fire PostResponse (no autoreply)
    let event = AikiPostResponseEvent {
        session: session.clone(),
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        response: "Done.".to_string(),
        modified_files: vec![],
    };
    
    // Should succeed despite SessionEnd dispatch error (graceful degradation)
    let result = event_bus::dispatch(AikiEvent::PostResponse(event));
    assert!(
        result.is_ok(),
        "PostResponse should succeed even if SessionEnd fails (graceful degradation)"
    );
    
    // Session file might remain (acceptable trade-off for graceful degradation)
    let session_file = repo_path
        .join(".aiki/sessions")
        .join(session.uuid_filename());
    // We don't assert on file existence - it may or may not be cleaned up
    // The important part is that PostResponse didn't fail
}
```

**Test coverage:**
- ✅ Session file removed without autoreply
- ✅ Session file kept with autoreply
- ✅ SessionEnd flows execute
- ✅ SessionEnd failures propagate
- ✅ Multiple sessions clean up independently
- ✅ **Graceful degradation**: PostResponse succeeds even if SessionEnd fails

### Fix 4: Vendor Behavior (No Changes Needed)

**Decision:** Vendors should continue emitting `PostResponse` events only. The event bus handles the PostResponse → SessionEnd transition.

**Rationale:**
- ✅ Clean separation: Vendors focus on event translation, event bus handles lifecycle
- ✅ No vendor changes needed (reduces risk)
- ✅ Consistent pattern: All lifecycle logic in one place (event_bus.rs)

**Non-changes:**
- `cli/src/commands/acp.rs` - Keep building PostResponse
- `cli/src/vendors/cursor.rs` - Keep building PostResponse
- `cli/src/vendors/claude_code.rs` - Keep building PostResponse

## Implementation Checklist

### Phase 1: Core Fixes
- [ ] Update `cli/src/event_bus.rs`
  - [ ] Fix autoreply detection logic (check for empty string)
  - [ ] Dispatch SessionEnd through event bus (not direct call)
  - [ ] Merge SessionEnd failures into PostResponse
  - [ ] Respect SessionEnd blocking decisions
- [ ] Run existing unit tests
  - [ ] Verify no regressions in other events
  - [ ] Verify event bus routing still works

### Phase 2: Integration Tests
- [ ] Replace `cli/tests/test_session_end.rs`
  - [ ] Test session file removed without autoreply
  - [ ] Test session file kept with autoreply
  - [ ] Test SessionEnd flows execute
  - [ ] Test SessionEnd failures propagate
  - [ ] Test multiple sessions clean up independently
- [ ] Run new test suite
  - [ ] All tests pass
  - [ ] No flaky behavior

### Phase 3: Validation
- [ ] Manual testing with Cursor
  - [ ] Session file created on first prompt
  - [ ] Session file removed after response (no autoreply)
  - [ ] Session file kept after response (with autoreply)
  - [ ] SessionEnd Log action visible in debug output
- [ ] Manual testing with Claude Code (if available)
  - [ ] Same lifecycle as Cursor
- [ ] Edge case testing
  - [ ] SessionEnd flow failure doesn't break PostResponse
  - [ ] Session file deletion failure logs warning
  - [ ] Missing .aiki/sessions/ directory handled gracefully

### Phase 4: Documentation
- [ ] Update `ops/current/session-tracking.md`
  - [ ] Document that vendors emit PostResponse only
  - [ ] Document event bus handles SessionEnd transition
  - [ ] Update flow diagram
- [ ] Add comments to `cli/src/event_bus.rs`
  - [ ] Explain autoreply detection logic
  - [ ] Explain why we dispatch SessionEnd (not direct call)
- [ ] Update CLAUDE.md if needed
  - [ ] Document SessionEnd event lifecycle
  - [ ] Document autoreply semantics

## Risk Assessment

### Low Risk
- ✅ Fix 1 (context check) - Simple boolean logic change
- ✅ Fix 3 (tests) - New tests, no production code changes
- ✅ Fix 4 (no vendor changes) - No changes = no risk

### Medium Risk
- ⚠️ Fix 1 (dispatching SessionEnd) - Recursive dispatch could cause issues
  - Mitigation: SessionEnd never triggers PostResponse (no cycles)
  - Mitigation: Extensive testing of event flow

### Potential Issues

**Issue 1: Recursive dispatch**
- Could SessionEnd trigger PostResponse which triggers SessionEnd?
- **No:** SessionEnd handler doesn't emit any events, just cleans up
- **Verified by:** `cli/src/handlers.rs:387-425` - no event emission

**Issue 2: Session file already deleted**
- What if SessionEnd flow takes time and file already deleted?
- **Handled by:** `session.end()` is idempotent (file.remove() returns Ok if missing)
- **Verified by:** Standard filesystem semantics

**Issue 3: Performance**
- Does extra dispatch add latency?
- **No:** SessionEnd is lightweight (flow execution + file delete)
- **Acceptable:** Session end is infrequent, latency not critical

## Testing Strategy

### Unit Tests (Existing)
- Run all existing tests
- Should pass without changes (no breaking changes)

### Integration Tests (New)
- Comprehensive session lifecycle tests
- Cover all scenarios from plan
- Use real filesystem operations

### Manual Testing
- Test with real Cursor integration
- Verify AIKI_DEBUG output shows SessionEnd
- Verify session files cleaned up

### Regression Testing
- Test other events still work (SessionStart, PrePrompt, etc.)
- Test event bus routing unchanged for non-PostResponse events

## Success Criteria

1. ✅ Session files are removed when PostResponse has no autoreply
2. ✅ Session files persist when PostResponse has autoreply
3. ✅ SessionEnd flows execute exactly once per session
4. ✅ SessionEnd failures propagate to PostResponse response
5. ✅ SessionEnd blocking decisions are respected
6. ✅ All integration tests pass
7. ✅ No regressions in other event handling
8. ✅ Manual testing confirms correct behavior

## Timeline Estimate

- **Phase 1 (Core Fixes):** 30 minutes
  - Update event_bus.rs: 15 minutes
  - Test existing suite: 15 minutes

- **Phase 2 (Integration Tests):** 45 minutes
  - Write new test suite: 30 minutes
  - Debug test failures: 15 minutes

- **Phase 3 (Validation):** 30 minutes
  - Manual testing: 20 minutes
  - Edge case testing: 10 minutes

- **Phase 4 (Documentation):** 15 minutes
  - Update docs: 10 minutes
  - Add comments: 5 minutes

**Total: ~2 hours**

## Future Improvements (Out of Scope)

These are deferred until users request them:

1. **SessionEnd metrics** - Track how often sessions end vs continue
2. **SessionEnd flow timeout** - Prevent hanging on cleanup
3. **Session file corruption handling** - Detect/fix malformed session files
4. **SessionEnd retry logic** - Retry session file cleanup on transient failures

## Conclusion

All four bugs have clear, low-risk fixes:

1. **Fix autoreply detection** - Check for empty string, not None
2. **Fix error propagation** - Dispatch through event bus, merge failures
3. **No vendor changes** - Keep current behavior (low risk)
4. **Add integration tests** - Prevent future regressions

The fixes are minimal, targeted, and maintain backward compatibility. The new test suite ensures the intended behavior works correctly.

**Recommendation:** Proceed with implementation following the checklist above.

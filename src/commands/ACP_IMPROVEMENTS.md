# ACP Proxy: Potential Improvements

## High Priority Issues

### 1. Missing SessionStart Event
**Issue**: The ACP proxy never fires `SessionStart` event, only `PrePrompt`, `PreFileChange`, `PostFileChange`, and `PostResponse`.

**Where to fix**: Should fire on:
- `session/new` request from IDE
- OR first `initialize` response from agent

**Impact**: Flows can't react to new session creation, only to prompts within sessions.

**Location**: `cli/src/commands/acp.rs` - Add handler in IDE→Agent thread

---

### 2. Panic Hook Uses Hardcoded /tmp Path
**Current code**:
```rust
if let Ok(mut file) = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open("/tmp/aiki-proxy-panic.log")
```

**Issue**: 
- Works on macOS, but `/tmp` may get cleared aggressively on some Linux systems
- Not portable to Windows

**Fix**: Use `std::env::temp_dir()` or `$HOME/.aiki/logs/proxy-panic.log`

**Location**: `cli/src/commands/acp.rs:177`

---

### 3. Thread Shutdown Relies on Subtle Drop Order
**Current code**:
```rust
// CRITICAL: Drop autoreply_tx and metadata_tx to close the channels before joining threads
// Without this, the forwarder thread blocks forever on recv() and the proxy hangs at shutdown
drop(autoreply_tx);
drop(metadata_tx);
```

**Issue**: 
- Correct but fragile
- If someone refactors and moves the drops, threads will hang
- Easy to miss in code review

**Better patterns**:
1. Wrap main loop in a scope so senders drop automatically
2. Use `Option<mpsc::Sender>` and explicitly call `.take()` to close
3. Add explicit "shutdown" message variant

**Location**: `cli/src/commands/acp.rs:554-556`

---

## Medium Priority Issues

### 4. Confusing cwd Variable Names
**Issue**: Two variables tracking working directory with different names:
- `thread_cwd` in IDE→Agent thread (line 238)
- `cwd` in Agent→IDE thread (line 333)

Both are updated via the same `StateMessage::WorkingDirectory`, but the naming suggests they might diverge.

**Fix**: Rename to match (e.g., both `cwd` or both `working_directory`) and add comment explaining they're synchronized via channel.

**Location**: `cli/src/commands/acp.rs:238, 333`

---

### 5. response_accumulator Keyed by session_id, Not request_id
**Current behavior**:
- `response_accumulator: HashMap<String, String>` keyed by `session_id`
- `prompt_requests: HashMap<JsonRpcId, String>` maps `request_id` → `session_id`
- Lookup flow: `request_id` → `session_id` → `response_text`

**Why it works**: A session only has one active request at a time (prompt/response cycle).

**Issue**: Not obvious from code. Easy to assume it should be keyed by `request_id`.

**Fix**: Add comment explaining why session-based keying is correct:
```rust
// Track response text accumulation per session (not per request)
// A session only has one active prompt at a time, so we can key by session_id
// rather than request_id. This simplifies accumulation across multiple chunks.
let mut response_accumulator: HashMap<String, String> = HashMap::new();
```

**Location**: `cli/src/commands/acp.rs:349`

---

## Refactoring Opportunities

### 6. Extract Thread Functions
The `run()` function is 529 lines. Consider extracting:
- `spawn_autoreply_forwarder_thread()`
- `spawn_ide_to_agent_thread()`
- `run_agent_to_ide_loop()` (the main Agent→IDE forwarding logic)

### 7. Add SessionStart Event Support
When adding SessionStart, need to decide:
- Fire on `session/new` (explicit session creation)
- Fire on first `initialize` response (implicit via agent startup)
- Fire on both?

Probably fire on `session/new` since that's when the IDE explicitly creates a session.

---

## Testing Gaps

### 8. No Integration Tests for:
- SessionStart event (once implemented)
- Thread shutdown scenarios
- Multiple concurrent sessions
- Error handling paths (malformed JSON-RPC, etc.)
- Panic recovery

---

## Documentation Improvements

### 9. Add Architecture Diagram
Document the thread architecture:
```
┌─────────────────────────────────────────────────────────┐
│                    ACP Proxy Process                     │
│                                                          │
│  ┌──────────────────┐  StateMessage  ┌────────────────┐ │
│  │ IDE→Agent Thread │  ───────────▶  │ Agent→IDE      │ │
│  │                  │  mpsc::channel │ Thread (OWNS   │ │
│  │ - Parse IDE msgs │                │ all state)     │ │
│  │ - Fire PrePrompt │  Autoreply     │                │ │
│  │ - Forward to     │  Message       │ - Parse agent  │ │
│  │   agent stdin    │  ◀───────────  │   responses    │ │
│  │                  │  mpsc::channel │ - Fire Post*   │ │
│  │                  │                │   events       │ │
│  └──────────────────┘                └────────────────┘ │
│         ▲                                     │          │
│         │                                     ▼          │
│    IDE stdin                            Agent stdout     │
└─────────────────────────────────────────────────────────┘
```

### 10. Document State Ownership Model
Add module-level docs explaining:
- Agent→IDE thread owns all state
- IDE→Agent thread sends updates via `StateMessage`
- Why this prevents races and simplifies logic

---

## Priority Ranking

**P0 (Fix Soon)**:
1. Panic hook temp dir (easy fix, portability issue)
2. Thread shutdown pattern (correctness, easy to break)
3. response_accumulator comment (clarify intent)

**P1 (Should Fix)**:
4. SessionStart event (missing functionality)
5. cwd naming consistency (readability)

**P2 (Nice to Have)**:
6. Extract thread functions (maintainability)
7. Architecture docs (onboarding)
8. More integration tests (confidence)

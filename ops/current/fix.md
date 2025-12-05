# Fix: PrePrompt and PostResponse HookResponse Contract Violations

## Problem Statement

The ACP proxy implementation in `cli/src/commands/acp.rs` violates the `HookResponse` contract by only consuming metadata fields while ignoring critical response signals:

### PrePrompt Handler (`handle_session_prompt`, line 1415-1441)

**Current behavior:**
- ✅ Extracts `modified_prompt` from metadata
- ❌ Ignores `success` flag
- ❌ Ignores `exit_code` (cannot block prompts)
- ❌ Ignores `user_message` (cannot show warnings to user)
- ❌ Ignores `agent_message` (cannot inject context into prompt)

**Impact:**
- Validation flows cannot block invalid prompts (e.g., prompt too long, missing context)
- Users don't see warnings/errors from PrePrompt flows
- Cannot inject agent-visible context separate from prompt modification

### PostResponse Handler (`handle_post_response`, line 1484-1556)

**Current behavior:**
- ✅ Extracts `autoreply` from metadata
- ❌ Ignores `success` flag
- ❌ Ignores `exit_code` (cannot signal non-blocking failures)
- ❌ Ignores `user_message` (cannot show validation results)
- ❌ Ignores `agent_message` (cannot explain autoreply to agent)

**Impact:**
- Validation flows cannot inform users about test failures or linting issues
- Cannot escalate errors after agent turn completes
- Users don't know why autoreplies are being sent

## HookResponse Contract (from `cli/src/handlers.rs`)

```rust
pub struct HookResponse {
    pub success: bool,              // Was the flow successful?
    pub user_message: Option<String>,    // Message for IDE UI
    pub agent_message: Option<String>,   // Message for AI agent
    pub metadata: Vec<(String, String)>, // Key-value data (modified_prompt, autoreply, etc.)
    pub exit_code: Option<i32>,          // 0=continue, 2=block
}
```

### Exit Code Semantics

- `None` or `Some(0)`: Continue normally (success or non-blocking failure)
- `Some(2)`: Block the operation (e.g., reject invalid prompt, prevent file modification)

## Architecture Requirements (from `ops/current/event-dispatch-gap-analysis.md`)

The HookResponse contract exists specifically to communicate validation results across vendor boundaries:

1. **Blocking operations** - Flows must be able to prevent prompts/responses when validation fails
2. **User feedback** - Users must see warnings/errors from validation flows
3. **Agent context** - Agents must understand why autoreplies are being sent
4. **Graceful degradation** - Non-blocking failures should log warnings but continue

## Important: `agent_message` is NOT Part of ACP

The `agent_message` field in `HookResponse` is **vendor-specific** and does not exist in the ACP protocol specification:

- ✅ **Cursor hooks** have an `agent_message` JSON field
- ✅ **Claude Code hooks** have an `additionalContext` JSON field  
- ❌ **ACP protocol** has NO mechanism for hidden agent-only messages

**In ACP proxy mode:**
- All communication flows through standard visible channels (prompts/responses)
- `agent_message` can only be injected by prepending to prompts or autoreplies
- There is no "send message to agent without user seeing" mechanism

This is by design - ACP is a transparent protocol.

## Proposed Fix

### Phase 1: Add HookResponse Translation Helpers

Create helper functions to translate `HookResponse` to ACP-compatible actions:

```rust
// cli/src/commands/acp.rs

/// Emit user_message as a log notification to stderr
/// 
/// Note: ACP protocol doesn't have a standard "notification to user" message type.
/// stderr is visible in IDE terminals and provides immediate feedback.
fn emit_user_message(message: &str) {
    eprintln!("[aiki] {}", message);
}

/// Check if operation should be blocked based on exit_code
fn should_block(response: &HookResponse) -> bool {
    response.exit_code == Some(2)
}
```

**Rationale:**
- ACP is a transparent protocol - no hidden message channels
- stderr is the most straightforward way to show user feedback
- `agent_message` can only be communicated by modifying visible content (prompts/autoreplies)

### Phase 2: Fix PrePrompt Handler

**File:** `cli/src/commands/acp.rs`, function `handle_session_prompt` (line 1415-1481)

**Changes:**

1. **Check success flag and exit_code:**
   ```rust
   let response = event_bus::dispatch(event)?;
   
   // Check if prompt should be blocked
   if should_block(&response) {
       if let Some(ref msg) = response.user_message {
           emit_user_message(&format!("❌ Prompt blocked: {}", msg));
       }
       // Return error to prevent prompt from being sent
       return Err(AikiError::Other(anyhow::anyhow!(
           "PrePrompt flow blocked prompt: {}",
           response.user_message.as_deref().unwrap_or("validation failed")
       )));
   }
   ```

2. **Emit user_message for warnings:**
   ```rust
   // Show warnings to user (non-blocking)
   if !response.success {
       if let Some(ref msg) = response.user_message {
           emit_user_message(&format!("⚠️ PrePrompt warning: {}", msg));
       }
   }
   ```

3. **Inject agent_message into prompt if present:**
   ```rust
   // Extract modified_prompt from metadata
   let mut modified_prompt = extract_modified_prompt(&response, &original_text);
   
   // Prepend agent_message if present
   // Note: In ACP, agent_message becomes part of the visible prompt (no hidden channels)
   if let Some(ref agent_msg) = response.agent_message {
       modified_prompt = format!("{}\n\n{}", agent_msg, modified_prompt);
   }
   ```

**Example flow use case:**
```yaml
PrePrompt:
  - if: "$event.original_prompt.len() > 10000"
    then:
      - log: "Prompt too long (${event.original_prompt.len()} chars), blocking"
    on_failure: block  # Sets exit_code = 2
```

**Note:** Unlike Claude Code/Cursor hooks where `agent_message` can be hidden from the user, in ACP mode it becomes part of the prompt text itself. This is acceptable because ACP is designed as a transparent protocol.

### Phase 3: Fix PostResponse Handler

**File:** `cli/src/commands/acp.rs`, function `handle_post_response` (line 1484-1585)

**Changes:**

1. **Check success flag and emit user_message:**
   ```rust
   let response = event_bus::dispatch(event)?;
   
   // Emit user feedback (PostResponse should never block, only warn)
   if let Some(ref msg) = response.user_message {
       if response.success {
           emit_user_message(&format!("ℹ️ {}", msg));
       } else {
           emit_user_message(&format!("⚠️ PostResponse validation: {}", msg));
       }
   }
   ```

2. **Check for autoreply and include agent_message:**
   ```rust
   if let Some(autoreply_text) = extract_autoreply(&response) {
       // Prepend agent_message to autoreply if present
       // Note: In ACP, agent_message becomes part of the autoreply text (no hidden channels)
       let full_autoreply = if let Some(ref agent_msg) = response.agent_message {
           format!("{}\n\n{}", agent_msg, autoreply_text)
       } else {
           autoreply_text
       };
       
       // ... existing autoreply logic with full_autoreply ...
   }
   
   // Log warning if agent_message is set without autoreply (cannot deliver)
   if response.agent_message.is_some() && extract_autoreply(&response).is_none() {
       emit_user_message("Warning: PostResponse flow set agent_message but no autoreply (cannot deliver in ACP)");
   }
   ```

**Example flow use case:**
```yaml
PostResponse:
  - shell: "cargo test --quiet"
    alias: "test_result"
    on_failure: continue  # Non-blocking failure
  
  - if: "$test_result.exit_code != 0"
    then:
      - log: "Tests failed, re-running with verbose output"  # user_message
      - autoreply: "The tests are failing. Here's the verbose output:\n\n$test_result.stderr"
```

**Note:** `agent_message` in PostResponse only makes sense when paired with an autoreply. Without an autoreply, there's no mechanism to send a message to the agent in ACP.

### Phase 4: Update Tests

**File:** `cli/tests/test_acp_session_flow.rs`

Add integration tests to verify:

1. **PrePrompt blocking:**
   ```rust
   #[test]
   fn test_preprompt_blocks_invalid_prompt() {
       // Flow returns exit_code = 2
       // Verify prompt is NOT forwarded to agent
       // Verify user_message is logged to stderr
   }
   ```

2. **PrePrompt warnings:**
   ```rust
   #[test]
   fn test_preprompt_non_blocking_warning() {
       // Flow returns success=false, exit_code=0
       // Verify prompt IS forwarded (degraded)
       // Verify user_message is logged to stderr
   }
   ```

3. **PostResponse user feedback:**
   ```rust
   #[test]
   fn test_postresponse_emits_user_message() {
       // Flow returns user_message
       // Verify message is logged to stderr
       // Verify autoreply still sent
   }
   ```

4. **PostResponse agent context:**
   ```rust
   #[test]
   fn test_postresponse_agent_message_in_autoreply() {
       // Flow returns agent_message + autoreply
       // Verify both are combined in autoreply prompt
   }
   ```

## Implementation Checklist

- [ ] Add `emit_user_message()` helper (stderr logging)
- [ ] Add `should_block()` helper (check exit_code == 2)
- [ ] Fix PrePrompt handler:
  - [ ] Check exit_code and block if needed
  - [ ] Emit user_message warnings
  - [ ] Inject agent_message into prompt
- [ ] Fix PostResponse handler:
  - [ ] Emit user_message (info or warning)
  - [ ] Prepend agent_message to autoreply
- [ ] Add integration tests for all scenarios
- [ ] Update event-dispatch-gap-analysis.md with fix status

## Edge Cases

### 1. PrePrompt blocks but metadata contains modified_prompt
**Behavior:** Ignore metadata, block the prompt entirely  
**Rationale:** exit_code=2 is an explicit rejection signal

### 2. PostResponse returns exit_code=2 (block)
**Behavior:** Log warning but do NOT block (PostResponse cannot block)  
**Rationale:** Agent turn already completed, blocking is meaningless  
**Implementation:**
```rust
if response.exit_code == Some(2) {
    eprintln!("Warning: PostResponse flow tried to block (not supported)");
    // Continue normally
}
```

### 3. agent_message without autoreply in PostResponse
**Behavior:** Log warning to stderr (cannot deliver without autoreply)  
**Rationale:** ACP has no mechanism to send message to agent mid-conversation  
**Implementation:**
```rust
if response.agent_message.is_some() && extract_autoreply(&response).is_none() {
    emit_user_message("Warning: PostResponse flow set agent_message but no autoreply (cannot deliver in ACP)");
}
```

**Why this limitation exists:** Unlike Claude Code/Cursor hooks which have vendor-specific channels for agent-only messages, ACP only allows communication through prompts and responses. Without an autoreply, there's no way to "send" the agent_message.

### 4. PrePrompt fails with error (not FailedBlock)
**Current behavior:** Graceful degradation (use original prompt)  
**Proposed change:** Also emit user_message if present  
**Implementation:**
```rust
Err(e) => {
    eprintln!("⚠️ PrePrompt flow failed: {}", e);
    if let Some(ref msg) = response.user_message {
        emit_user_message(&format!("⚠️ {}", msg));
    }
    eprintln!("Continuing with original prompt...\n");
    // ... existing fallback logic ...
}
```

## Success Criteria

✅ **PrePrompt validation flows can:**
- Block invalid prompts (exit_code=2)
- Show warnings to users (user_message with exit_code=0)
- Inject context visible only to agent (agent_message)

✅ **PostResponse validation flows can:**
- Show test/lint results to users (user_message)
- Explain autoreply rationale to agent (agent_message)
- Queue autoreplies with full context

✅ **Tests verify:**
- Blocking behavior (PrePrompt only)
- User message emission (both hooks)
- Agent message injection (both hooks)
- Graceful degradation on errors

## Non-Goals

❌ **Custom JSON-RPC notifications** - Use stderr for now, wait for ACP spec  
❌ **IDE-specific message formatting** - Keep it generic (works in all terminals)  
❌ **PostResponse blocking** - Architecturally impossible (turn already completed)

## Related Files

### Core Implementation
- `cli/src/commands/acp.rs` - ACP proxy (PrePrompt/PostResponse handlers)
- `cli/src/handlers.rs` - HookResponse contract definition
- `cli/src/event_bus.rs` - Event dispatch to flow engine

### Testing
- `cli/tests/test_acp_session_flow.rs` - Integration tests
- `.aiki/flows/aiki/core.yml` - Core flow (PrePrompt/PostResponse actions)

### Documentation
- `ops/current/event-dispatch-gap-analysis.md` - Architecture analysis
- `ops/current/review.md` - Phase-1 review notes
- `CLAUDE.md` - Error handling and architecture guidelines

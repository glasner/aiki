# HookResponse Refactor: Typed Messages

## Problem Statement

The current `HookResponse` design uses `user_message` and `agent_message` fields that are ambiguous:
- What's the difference between user vs agent messages?
- When should each be used?
- How do translators know what to do with them?

Current example:
```rust
HookResponse::success_with_message("⚠️ Session started with warnings")
    .with_agent_message("Some initialization actions failed: Could not find .jj directory")
```

**Questions:**
- Why is the warning a "user_message" and the detail an "agent_message"?
- How do translators distinguish warnings from errors?
- What if we want to show both to the user, or both to the agent?

## Proposed Design: Typed Messages by Severity

Instead of `user_message` vs `agent_message`, use **typed messages** for feedback and a **prompt field** for agent communication:

```rust
pub struct HookResponse {
    pub messages: Vec<Message>,
    pub prompt: Option<String>,  // Text to send to agent (PrePrompt: modified prompt, PostResponse: autoreply)
    pub exit_code: Option<i32>,
}

pub enum Message {
    /// Informational message (e.g., "Co-authors added", "Provenance recorded for 3 files")
    Info(String),
    
    /// Warning message (e.g., "Session started with warnings", "Tests failed")
    Warning(String),
    
    /// Error message (e.g., "Failed to initialize session: Repository not found")
    Error(String),
}

impl HookResponse {
    /// Returns true if the operation was successful (exit_code is None or 0)
    pub fn is_success(&self) -> bool {
        matches!(self.exit_code, None | Some(0))
    }
    
    /// Returns true if the operation should block (exit_code is 2)
    pub fn is_blocking(&self) -> bool {
        self.exit_code == Some(2)
    }
}
```

### Example Translations

#### SessionStart (warnings only)
**Before:**
```rust
HookResponse::success_with_message("⚠️ Session started with warnings")
    .with_agent_message("Some initialization actions failed: Could not find .jj directory")
```

**After:**
```rust
HookResponse::success()
    .with_warning("Session started with warnings: Some initialization actions failed: Could not find .jj directory")
```

#### PrePrompt (modified prompt)
**Before:**
```rust
HookResponse::success()
    .with_metadata(vec![("modified_prompt".to_string(), final_prompt)])
```

**After:**
```rust
HookResponse::success()
    .with_prompt(final_prompt)
```

#### PostResponse (autoreply)
**Before:**
```rust
HookResponse::success()
    .with_metadata(vec![("autoreply".to_string(), autoreply_text)])
```

**After:**
```rust
HookResponse::success()
    .with_prompt(autoreply_text)
```

#### Blocking Error
**Before:**
```rust
HookResponse::blocking_failure(
    "❌ Failed to initialize session: Repository not found",
    Some("Please run 'aiki init' or 'aiki doctor' to fix setup.".to_string()),
)
```

**After:**
```rust
HookResponse::blocking()
    .with_error("Failed to initialize session: Repository not found. Please run 'aiki init' or 'aiki doctor' to fix setup.")
```

---

## Translator Logic

Translators need to be **event-aware** because different events have different semantics:

| Event | messages | prompt |
|-------|----------|--------|
| **SessionStart** | Warnings/errors about initialization | N/A |
| **PrePrompt** | Validation warnings/errors | Modified prompt text |
| **PreFileChange** | Warnings about stashing | N/A |
| **PostFileChange** | Provenance recording status | N/A |
| **PostResponse** | Validation results | Autoreply text |
| **PrepareCommitMessage** | Co-author status | N/A |

### Event-Aware Translation

Translators must handle `prompt` differently based on event context:

```rust
fn translate_response(response: HookResponse, event_type: &str) -> (Option<String>, i32) {
    match event_type {
        "PrePrompt" => {
            // prompt = modified prompt (send to agent as next user message)
            // messages = validation warnings (show to user)
        }
        "PostResponse" => {
            // prompt = autoreply (send to agent as follow-up prompt)
            // messages = test/lint results (show to user)
        }
        "SessionStart" | "PostFileChange" | "PrepareCommitMessage" => {
            // prompt should be None (these events don't modify prompts)
            // messages = status/warnings (show to user)
        }
        _ => {}
    }
}
```

### Cursor Translator

**Cursor hook JSON format (consistent across all events):**

**For all events:**
- `user_message` field: Combine all messages with emoji prefixes (ℹ️/⚠️/❌), newline-separated, shown in IDE UI
- `agent_message` field: Combine all messages as plain text (no emoji), newline-separated, visible to agent
- `metadata` field: Pass through `prompt` field as metadata object (for PrePrompt/PostResponse)

**Exit code logic:**
- `is_blocking()` → exit 2 (blocks operation)
- `is_success()` → exit 0 (continues)
- Otherwise → exit 1 (non-blocking failure)

**Note:** Unlike Claude Code, Cursor uses the same JSON structure for all events. The only event-specific behavior is whether `prompt` field is populated in metadata.

### Claude Code Translator

**Claude Code hook JSON format (event-dependent):**

**For PostFileChange events:**
- Blocking (`is_blocking()`): `{"decision": "block", "reason": "<first error/warning>"}`
- Success: `{"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": "<all messages>"}}`

**For other events (SessionStart, PrePrompt, etc.):**
- Blocking (`is_blocking()`): `{"continue": false, "stopReason": "<first error/warning>", "systemMessage": "<all messages>"}`
- Success with warnings: `{"systemMessage": "<all messages>"}` (only if messages contain warnings/errors)
- Pure success: Empty JSON or no output

**Metadata handling:**
- Pass through `prompt` field in `metadata` array format for PrePrompt/PostResponse

### ACP Proxy Handler

**PrePrompt:**
1. Emit all messages to stderr with emoji prefixes for user visibility
2. If `is_blocking()` → return error to prevent prompt from being sent
3. Use `prompt` field as modified prompt (or original if None)
4. Prepend all messages to prompt text so agent sees validation context
5. Forward modified prompt to agent stdin

**PostResponse:**
1. Emit all messages to stderr with emoji prefixes for user visibility
2. Check `prompt` field for autoreply text
3. If autoreply exists, prepend messages to it and queue via autoreply channel
4. If no autoreply, just show messages to user (no agent communication)

**Other events (SessionStart, PostFileChange, etc.):**
1. Emit all messages to stderr with emoji prefixes
2. `prompt` field should be None (these events don't modify prompts)
3. No agent communication needed

---

## Migration Strategy

### Phase 1: Add New Types (Non-Breaking)

Add `Message` enum and `prompt` field alongside existing fields:

```rust
pub struct HookResponse {
    // Old (deprecated)
    #[deprecated(note = "Use exit_code instead (None or 0 = success, 2 = block)")]
    pub success: bool,
    #[deprecated(note = "Use messages field instead")]
    pub user_message: Option<String>,
    #[deprecated(note = "Use messages field instead")]
    pub agent_message: Option<String>,
    #[deprecated(note = "Use prompt field instead")]
    pub metadata: Vec<(String, String)>,
    
    // New
    pub messages: Vec<Message>,
    pub prompt: Option<String>,  // Replaces metadata["modified_prompt"] and metadata["autoreply"]
    pub exit_code: Option<i32>,
}

impl HookResponse {
    pub fn is_success(&self) -> bool {
        matches!(self.exit_code, None | Some(0))
    }
    
    pub fn is_blocking(&self) -> bool {
        self.exit_code == Some(2)
    }
}
```

### Phase 2: Update Handlers

Update all handlers to use new API:

```rust
// OLD (SessionStart)
HookResponse::success_with_message("⚠️ Session started with warnings")
    .with_agent_message("Some initialization actions failed: Could not find .jj directory")

// NEW
HookResponse::success()
    .with_warning("Session started with warnings: Some initialization actions failed: Could not find .jj directory")

// OLD (PrePrompt)
HookResponse::success()
    .with_metadata(vec![("modified_prompt".to_string(), final_prompt)])

// NEW
HookResponse::success()
    .with_prompt(final_prompt)

// OLD (PostResponse)
HookResponse::success()
    .with_metadata(vec![("autoreply".to_string(), autoreply_text)])

// NEW
HookResponse::success()
    .with_prompt(autoreply_text)
```

### Phase 3: Update Translators

Update translators to read from `messages` and `prompt` fields:

```rust
fn translate_cursor(response: HookResponse) -> (Option<String>, i32) {
    let exit_code = if response.is_blocking() {
        2
    } else if response.is_success() {
        0
    } else {
        1
    };
    
    // Try new fields first, fall back to old for compatibility
    if !response.messages.is_empty() || response.prompt.is_some() {
        // Use new typed messages and prompt
    } else {
        // Fall back to user_message/agent_message/metadata
    }
    
    (json_output, exit_code)
}
```

#### ACP Proxy Updates

```rust
// PrePrompt: Use prompt field instead of extract_modified_prompt()
let modified_prompt = response.prompt.unwrap_or(original_text);

// PostResponse: Use prompt field instead of extract_autoreply()
if let Some(autoreply) = response.prompt {
    // Send autoreply
}
```

### Phase 4: Remove Deprecated Fields

Once all handlers and translators are updated, remove old fields:

```rust
pub struct HookResponse {
    pub messages: Vec<Message>,
    pub prompt: Option<String>,
    pub exit_code: Option<i32>,
}
```

---

## Design Decisions

1. **One message type for both users and agents**
   - Simplifies the mental model: Info, Warning, Error
   - Translators format differently for each audience (emoji for users, plain text for agents)
   - No more confusion about "user_message" vs "agent_message"

2. **Emoji formatting happens in translators, not handlers**
   - Handlers: `with_warning("Session started with warnings")`
   - Translators add emoji: `"⚠️ Session started with warnings"` (user) or plain (agent)
   - Keeps handlers vendor-agnostic

3. **Single `prompt` field replaces metadata**
   - `prompt` holds the text to send to agent (modified prompt or autoreply)
   - No more generic key-value metadata
   - Clearer intent: "what should we send to the agent?"

4. **ACP proxy shows messages to both users and agents**
   - Users see via stderr: `[aiki] ⚠️ Session started with warnings`
   - Agents see via prompt injection: prepended to modified prompt

---

## Benefits

1. **Clearer Intent**: `with_warning()` is clearer than `with_message()`, `with_prompt()` is clearer than `with_metadata(vec![...])`
2. **Type Safety**: Can't accidentally mix error and info messages
3. **Flexible Translation**: Each vendor can format messages appropriately
4. **Better ACP Support**: Clear distinction between user feedback (stderr) and agent context (prompt)
5. **Easier Testing**: Can assert on message types and prompt existence, not string parsing
6. **No More Magic Keys**: `prompt` field replaces `metadata["modified_prompt"]` and `metadata["autoreply"]`

---

## Example: Complete Refactor

### Before
```rust
match flow_result {
    FlowResult::Success => Ok(HookResponse::success_with_message(
        "✅ Provenance recorded for 3 files"
    )),
    FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(
        "⚠️ Provenance partially recorded for 5 files"
    ).with_agent_message(format!("Some actions failed: {}", msg))),
    FlowResult::FailedBlock(msg) => Ok(HookResponse::failure(
        format!("⚠️ Provenance recording blocked: {}", msg),
        Some("Changes saved but provenance tracking failed.".to_string()),
    )),
}
```

### After
```rust
match flow_result {
    FlowResult::Success => Ok(HookResponse::success()
        .with_info("Provenance recorded for 3 files")),
    
    FlowResult::FailedContinue(msg) => Ok(HookResponse::success()
        .with_warning(format!("Provenance partially recorded for 5 files. Some actions failed: {}", msg))),
    
    FlowResult::FailedBlock(msg) => Ok(HookResponse::failure()
        .with_error(format!("Provenance recording blocked: {}. Changes saved but tracking failed.", msg))),
}
```

---

## Next Steps

1. ✅ Document current usage patterns
2. ⏳ Get feedback on proposed design
3. ⏳ Implement Phase 1 (non-breaking addition)
4. ⏳ Update handlers (Phase 2)
5. ⏳ Update translators (Phase 3)
6. ⏳ Remove deprecated fields (Phase 4)

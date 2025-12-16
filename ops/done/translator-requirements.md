# Translator Requirements by Vendor and Event

This document details exactly what each translator needs to do for each event type.

---

## HookResponse Structure (Phase 8 Architecture)

```rust
pub struct HookResponse {
    pub context: Option<String>,  // Agent-visible content (prompts, autoreplies, commit messages)
    pub messages: Vec<Message>,   // Validation messages (info/warning/error)
    pub exit_code: i32,           // 0 = success, 2 = blocking
}
```

**Key Principles:**
- **`context`**: Content for the agent (modified prompts, autoreply text, commit messages)
- **`messages`**: Validation feedback (warnings, errors, info) - visibility depends on event:
  - User-only (stderr): SessionStart, PreFileChange, PostFileChange
  - User + Agent: PrePrompt, PostResponse (combined with context)
- **`exit_code`**: Controls blocking behavior (0 = allow, 2 = block)
- **Combination logic**: Translators should combine `messages` + `context` when appropriate

---

## Event Mapping

### Cursor Events → Aiki Events

| Cursor Event | Aiki Event(s) | Purpose |
|--------------|---------------|---------|
| `beforeSubmitPrompt` | SessionStart + PrePrompt | Session initialization AND prompt validation |
| `beforeMCPExecution` | PreFileChange | Before file-modifying tools execute |
| `afterFileEdit` | PostFileChange | After file edits complete |

**Note:** 
- `beforeSubmitPrompt` fires for **every** user prompt, so it serves dual purpose: SessionStart (first prompt) and PrePrompt (all prompts)
- Cursor doesn't have a PostResponse hook yet

### Claude Code Events → Aiki Events

| Claude Code Event | Aiki Event | Purpose |
|-------------------|------------|---------|
| `SessionStart` | SessionStart | Session initialization |
| `UserPromptSubmit` | PrePrompt | Before prompt sent to agent |
| `PreToolUse` | PreFileChange | Before file-modifying tools execute |
| `PostToolUse` | PostFileChange | After tool execution completes |
| `Stop` | PostResponse | After agent completes response |

### ACP Events → Aiki Events

| ACP Method | Aiki Event | Purpose |
|------------|------------|---------|
| `session/new` response | SessionStart | Session initialization |
| `session/prompt` request | PrePrompt | Before prompt sent to agent |
| `session/request_permission` | PreFileChange | Before file-modifying tools execute |
| `session/update` (tool complete) | PostFileChange | After tool execution completes |
| `session/update` (stopReason) | PostResponse | After agent completes response |

---

## Cursor Translator Requirements

**Note:** Cursor's `beforeSubmitPrompt` fires on first prompt only. It should trigger SessionStart.

### Event: beforeSubmitPrompt → SessionStart

**HookResponse fields used:**
- `messages`: Warnings/errors about initialization
- `context`: Not used (Cursor doesn't support injecting context at session start)
- `exit_code`: Blocking if initialization fails

**Cursor JSON output:**
```json
{
  "continue": true
}
```

**Blocking output:**
```json
{
  "user_message": "❌ Failed to initialize session: Repository not found", // should just be messages
  "continue": false
}
```

**Exit code:**
- Blocking → exit 2
- Success → exit 0

---

### Event: beforeSubmitPrompt → PrePrompt
**Cursor documentation:** https://cursor.com/docs/agent/hooks#beforesubmitprompt

**HookResponse fields used:**
- `messages`: Prompt validation warnings/errors
- `context`: **Cannot be used** - Cursor doesn't support prompt modification
- `exit_code`: Blocking if validation fails

**Cursor JSON output (success):**
```json
{
  "continue": true
}
```

**Blocking output:**
```json
{
  "continue": false,
  "user_message": "❌ Prompt too long (10,234 chars)"
}
```

**Exit code:**
- Blocking → exit 2
- Success → exit 0

**Limitations:**
- Cursor's `beforeSubmitPrompt` **cannot modify the prompt** - it can only block or allow it
- The `context` field in HookResponse must be ignored for Cursor
- Use `messages` to show warnings/errors to the user, but prompt text cannot be changed

---

### Event: beforeShellExecution/beforeMCPExecution → PreFileChange

**Cursor documentation:** https://cursor.com/docs/agent/hooks#beforeshellexecution-beforemcpexecution

**HookResponse fields used:**
- `messages`: Warnings about stashing user edits or blocking reasons
- `context`: Should be None (not used for PreFileChange)
- `exit_code`: Can block tool execution if needed

**Cursor JSON output (success):**
```json
{
  "continue": true
}
```

**Blocking output:**
```json
{
  "continue": false,
  "agent_message": "Policy blocked editing file" // should be error messages
}
```

**Exit code:**
- Blocking → exit 2
- Success → exit 0

**Note:** Unlike our current implementation which never blocks on PreFileChange, Cursor's hook can prevent tool execution. This allows flows to block file modifications if certain conditions aren't met (e.g., dirty working directory).

---

### Event: afterFileEdit → PostFileChange

**Cursor documentation:** https://cursor.com/docs/agent/hooks#afterfileedit

**HookResponse fields used:**
- `messages`: Status about provenance recording (informational only)
- `context`: Should be None (not used for PostFileChange)
- `exit_code`: Ignored (Cursor doesn't accept responses from this hook)

**Cursor JSON output:**
- None - Cursor does not accept JSON responses from `afterFileEdit`
- The hook is **read-only** / informational

**Exit code:**
- Always exit 0

**Limitations:**
- Cursor's `afterFileEdit` hook does not accept responses - it's notification-only
- Any `messages` in the HookResponse cannot be shown to the user or agent
- This hook is useful for side effects (provenance recording, logging) but not for user feedback
- Cannot block or influence the file edit in any way

---

### Event: stop → PostResponse

**Cursor documentation:** https://cursor.com/docs/agent/hooks#stop

**HookResponse fields used:**
- `messages`: Validation results (e.g., "Tests failed", "Lint errors found")
- `context`: Follow-up instruction / autoreply text (e.g., "Please run the tests again with verbose output")
- `exit_code`: Should never block (agent loop already finished)

**Building followup_message:**
Combine `messages` and `context` into a single string for the agent:
- If both exist: `"{formatted_messages}\n\n{context}"`
- If only context: `"{context}"`
- If only messages: `"{formatted_messages}"` (unusual - typically validation feedback leads to a follow-up action)
- If neither: No output (no follow-up)

**Example:** 
- `messages`: `[Warning("Tests failed with 3 errors")]`
- `context`: `"Please run the tests again with verbose output and fix the failures"`
- Formatted messages: `"⚠️ Tests failed with 3 errors"`
- Result: `"⚠️ Tests failed with 3 errors\n\nPlease run the tests again with verbose output and fix the failures"`

**Cursor JSON output (with follow-up):**
```json
{
  "followup_message": "⚠️ Tests failed with 3 errors\n\nPlease run the tests again with verbose output and fix the failures"
}
```

**No follow-up:**
```json
{}
```

**Exit code:**
- Always exit 0 (cannot block after agent completes)

**Notes:**
- Maximum of 5 auto follow-ups enforced by Cursor (tracked via `loop_count` input field)
- Unlike Claude Code which separates validation messages (`additionalContext`) from autoreplies, Cursor combines everything into one `followup_message`
- The agent sees the full context (formatted validation messages + autoreply text) in a single message
- **Phase 8 fix**: Messages and context are now properly combined instead of being mutually exclusive

---

## Claude Code Translator Requirements

**IMPORTANT: All Claude Code hooks must return exit code 0 for JSON parsing to work.**
- Use structured decision control (`decision: "block"`, `permissionDecision: "deny"`, etc.)
- Exit code 2 bypasses JSON parsing and should not be used
- This is different from Cursor (uses exit code 2) and ACP (uses exit code 2)

---

### Event: SessionStart → SessionStart

**Claude Code documentation:** https://code.claude.com/docs/en/hooks#sessionstart-decision-control

**HookResponse fields used:**
- `messages`: Warnings/info about session initialization
- `context`: Initial context to inject (e.g., "Repository: /path/to/repo. Branch: main.")
- `exit_code`: Cannot block (SessionStart doesn't support blocking)

**Claude Code JSON output:**
```json
{
  "systemMessage": "🎉 aiki initialized"
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "⚠️ Working directory has uncommitted changes\n\nSession initialized. Repository: /path/to/repo. Branch: main."
  }
}
```

**Building additionalContext:**
- Combine formatted `messages` + `context` (if both exist)
- Use only `context` if no messages
- Use only formatted `messages` if no context

**No context:**
```json
{}
```

**Exit code:**
- Always exit 0
- Exit 2 shows stderr to user but doesn't block session

**Limitations:**
- SessionStart **cannot block** - it only supports adding context via `additionalContext`
- No `continue`, `decision`, or `reason` fields available
- Designed for environment setup and context loading, not access control
- **Phase 8 fix**: Messages and context are properly combined in `additionalContext`

---

### Event: UserPromptSubmit → PrePrompt

**HookResponse fields used:**
- `messages`: Validation warnings/errors (shown to user and agent)
- `context`: Prepended context (project conventions, active files, etc.)
- `exit_code`: Controls blocking (must be 0 for Claude Code)

**Blocking output:**
```json
{
  "decision": "block",
  "reason": "❌ Prompt too long (10,234 chars). Please shorten your prompt.", // from messages
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "My additional context here" // from context
  }

}
```

**Success with context and/or messages:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "My additional context here" // from messages + context
  }
}
```

**Building additionalContext:**
- Combine formatted `messages` + `context` + original prompt (all three if present)
- Pattern: `"{formatted_messages}\n\n{context}\n\n{original_prompt}"`
- **Phase 8 fix**: Messages and context are properly combined instead of being mutually exclusive

**Exit code:**
- Always exit 0 (use `decision: "block"` to block, not exit code 2)

**Important:** Claude Code uses `decision: "block"` with exit code 0 for blocking, unlike traditional hooks that use exit code 2

---

### Event: PreToolUse → PreFileChange
https://code.claude.com/docs/en/hooks#pretooluse-decision-control

**HookResponse fields used:**
- `messages`: Warnings about stashing or policy violations (shown to user and/or agent depending on decision)
- `context`: Should be None
- `exit_code`: **Always 0** for Claude Code (use `permissionDecision` for control)

**Decision options (exit code 0):**`

**Deny (block tool execution):**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Policy blocked editing this file" // build from messages + context
  }
}
```
*Note: `permissionDecisionReason` is shown to Claude for deny decisions*


**Exit code:**
- **Always exit 0** for Claude Code hooks
- Use `permissionDecision: "deny"` to block, not exit code 2
- Exit code 2 bypasses JSON parsing (not recommended for Claude Code) -> should not do this

---

### Event: PostToolUse → PostFileChange
https://code.claude.com/docs/en/hooks#posttooluse-decision-control

**HookResponse fields used:**
- `messages`: Provenance recording status (shown to user via additionalContext)
- `context`: Extra context to pass back to claude on block
- `exit_code`: 0 for structured `decision` control (block/None  )

**Blocking output:**
Automatically reprompts Claude with reason

```json
{
  "decision": "block",
  "reason": "Provenance recording failed", // from messages, should never be blank
  "hookSpecificOutput": {
      "hookEventName": "PostToolUse",
      "additionalContext": "Additional information for Claude" // from context
    }
  
}
```

**Success output:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Provenance recorded for 3 files" // from messages + context
  }
}
```

**Exit code:**
- Always exit 0

---

### Event: Stop → PostResponse
https://code.claude.com/docs/en/hooks#stop%2Fsubagentstop-decision-control

**HookResponse fields used:**
- `messages`: Test/lint validation results (shown to user via stderr, and to agent in autoreply)
- `context`: Autoreply text (combined with messages)
- `exit_code`: **Always 0** for Claude Code (use `decision: "block"` for control)

**Decision options (exit code 0):**

**Block (force Claude to continue):**
```json
{
  "decision": "block",
  "reason": "⚠️ Tests failed with 3 errors\n\nPlease run the tests again with verbose output and fix the failures"
}
```
*Note: `reason` must be provided when blocking - it tells Claude how to proceed*

**Allow (normal stop):**
```json
{}
```
*Or omit the `decision` field entirely*

**Building the autoreply/reason:**
- Combine formatted `messages` + `context` (if both exist)
- Use only `context` if no messages
- Use only formatted `messages` if no context
- **Phase 8 fix**: Messages and context are properly combined instead of being mutually exclusive
- The combined text goes into the `reason` field when blocking

**Exit code:**
- **Always exit 0** for Claude Code hooks
- Use `decision: "block"` with `reason` to force continuation
- Exit code 2 bypasses JSON parsing (not recommended for Claude Code)
- Other codes: Non-blocking error, execution continues

**Important:** 
- Stop hook only runs when Claude finishes naturally (not on user interrupt)
- Input includes `stop_hook_active` field to prevent infinite autoreply loops
- When using `decision: "block"`, the `reason` field acts as the autoreply/follow-up instruction

---

## ACP Proxy Requirements

### Event: session/new response → SessionStart

**HookResponse fields used:**
- `messages`: Initialization warnings/errors (shown to user via stderr)
- `context`: Initial context (currently not used for SessionStart in ACP)
- `exit_code`: Should never block (can't block session creation)

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Ignore `exit_code` (can't block session creation in ACP)
3. Continue normally

---

### Event: session/prompt → PrePrompt

**HookResponse fields used:**
- `messages`: Validation warnings/errors (shown to user via stderr AND agent)
- `context`: Prepended context (project conventions, active files, etc.)
- `exit_code`: Blocking if validation fails

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. If `is_blocking()` → return error, don't forward prompt to agent
3. Build final prompt by combining:
   - Formatted messages (if any)
   - Context from `response.context` (if any)
   - Original prompt text
4. Pattern: `"{formatted_messages}\n\n{context}\n\n{original_prompt}"`
5. **Phase 8 fix**: Messages and context are properly combined
6. Forward modified prompt to agent stdin

---

### Event: session/request_permission → PreFileChange

**HookResponse fields used:**
- `messages`: Warnings about stashing (shown to user via stderr)
- `context`: Should be None
- `exit_code`: Should never block (can't block permission requests)

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Ignore `exit_code` (PreFileChange never blocks)
3. Grant permission (send response)

---

### Event: session/update (tool complete) → PostFileChange

**HookResponse fields used:**
- `messages`: Provenance recording status (shown to user via stderr)
- `context`: Should be None
- `exit_code`: Should never block

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. No action needed (provenance recording is fire-and-forget)

---

### Event: session/update (stopReason) → PostResponse

**HookResponse fields used:**
- `messages`: Test/lint validation results (shown to user via stderr AND agent)
- `context`: Autoreply text
- `exit_code`: Should never block

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Build autoreply by combining:
   - Formatted messages (if any)
   - Context from `response.context` (if any)
3. Pattern: `"{formatted_messages}\n\n{context}"`
4. **Phase 8 fix**: Messages and context are properly combined
5. If combined autoreply exists:
   - Check autoreply limit (MAX_AUTOREPLIES)
   - Queue autoreply via autoreply channel
6. If no autoreply, just show messages to user

---

## Phase 8 Architecture Changes

### Key Changes

1. **Renamed field**: `HookResponse.prompt` → `HookResponse.context`
   - More accurate name: represents agent context, not just prompts
   - Used for: modified prompts, autoreply text, commit messages

2. **Combination logic**: Messages and context are now properly combined
   - **Before**: Mutually exclusive (either messages OR context)
   - **After**: Combined when both exist
   - Pattern: `"{formatted_messages}\n\n{context}\n\n{original}"`

3. **Consistent formatting**: Messages are formatted with emoji prefixes
   - Info: `ℹ️ {text}`
   - Warning: `⚠️ {text}`
   - Error: `❌ {text}`

### Migration Guide for Translators

**Old code (BROKEN):**
```rust
let final_prompt = if !agent_context.is_empty() {
    agent_context           // Only messages
} else {
    response.prompt         // Only context (renamed to response.context)
};
```

**New code (CORRECT):**
```rust
let agent_context = build_agent_context(&response);  // Formatted messages
let prepended_context = response.context.as_deref().unwrap_or("");

let final_prompt = match (!agent_context.is_empty(), !prepended_context.is_empty()) {
    (true, true) => format!("{}\n\n{}\n\n{}", agent_context, prepended_context, original),
    (true, false) => format!("{}\n\n{}", agent_context, original),
    (false, true) => format!("{}\n\n{}", prepended_context, original),
    (false, false) => original.to_string(),
};
```

### Validation

**Events that should never have `context`:**
- PreFileChange (only uses messages for warnings)
- PostFileChange (only uses messages for status)

**Events that should never have `messages`:**
- (None - all events can have validation messages)

**Recommendation:** Emit a debug warning if `AIKI_DEBUG=1` when `context` is set on events that don't use it, otherwise ignore.

---

## Summary Table

| Vendor | Event | messages → | context → | exit_code → | Combination? |
|--------|-------|-----------|----------|-------------|--------------|
| **Cursor** | beforeSubmitPrompt (SessionStart) | user_message | N/A | exit code | N/A |
| **Cursor** | beforeSubmitPrompt (PrePrompt) | user_message | N/A (unsupported) | exit code | N/A |
| **Cursor** | beforeMCPExecution | user_message | N/A | 0 (never block) | N/A |
| **Cursor** | afterFileEdit | (not shown to user) | N/A | 0 | N/A |
| **Cursor** | stop | formatted in followup | followup_message | 0 | ✅ Yes |
| **Claude** | SessionStart | formatted in additionalContext | additionalContext | 0 | ✅ Yes |
| **Claude** | UserPromptSubmit | formatted in modifiedPrompt | modifiedPrompt | 0 | ✅ Yes |
| **Claude** | PreToolUse | stderr | N/A | 0 | N/A |
| **Claude** | PostToolUse | additionalContext | N/A | 0 | N/A |
| **Claude** | Stop | formatted in autoreply | metadata["autoreply"] | 0 | ✅ Yes |
| **ACP** | SessionStart | stderr | (not used) | ignored | N/A |
| **ACP** | PrePrompt | stderr + agent | modified prompt | blocks if exit=2 | ✅ Yes |
| **ACP** | PreFileChange | stderr | N/A | ignored | N/A |
| **ACP** | PostFileChange | stderr | N/A | ignored | N/A |
| **ACP** | PostResponse | stderr + agent | autoreply | ignored | ✅ Yes |

**Legend:**
- ✅ **Combination**: Messages and context are combined using Phase 8 logic
- **N/A**: Event doesn't use context, or doesn't support combination

# Translator Requirements by Vendor and Event

This document details exactly what each translator needs to do for each event type.

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
- `prompt`: Should be None (SessionStart doesn't modify prompts)
- `exit_code`: Blocking if initialization fails

**Cursor JSON output:**
```json
{
  "user_message": "Session started with warnings",
  "agent_message": "Session started with warnings",
  "continue": true
}
```

**Blocking output:**
```json
{
  "user_message": "❌ Failed to initialize session: Repository not found",
  "agent_message": "Failed to initialize session: Repository not found",
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
- `prompt`: **Cannot be used** - Cursor doesn't support prompt modification
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
- The `prompt` field in HookResponse must be ignored for Cursor
- Use `messages` to show warnings/errors to the user, but prompt text cannot be changed

---

### Event: beforeShellExecution/beforeMCPExecution → PreFileChange

**Cursor documentation:** https://cursor.com/docs/agent/hooks#beforeshellexecution-beforemcpexecution

**HookResponse fields used:**
- `messages`: Warnings about stashing user edits or blocking reasons
- `prompt`: Should be None (not used for PreFileChange)
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
  "user_message": "Policy blocked editing file"
  "agent_message": "Policy blocked editing file"
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
- `prompt`: Should be None (not used for PostFileChange)
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
- `prompt`: Follow-up instruction (e.g., "Please run the tests again with verbose output")
- `exit_code`: Should never block (agent loop already finished)

**Building followup_message:**
Combine `messages` and `prompt` into a single string for the agent:
- If both exist: `"{messages}\n\n{prompt}"`
- If only prompt: `"{prompt}"`
- If only messages: `"{messages}"` (unusual - typically validation feedback leads to a follow-up action)
- If neither: No output (no follow-up)

**Example:** 
- `messages`: `[Warning("Tests failed with 3 errors")]`
- `prompt`: `"Please run the tests again with verbose output and fix the failures"`
- Result: `"Tests failed with 3 errors\n\nPlease run the tests again with verbose output and fix the failures"`

**Cursor JSON output (with follow-up):**
```json
{
  "followup_message": "Tests failed with 3 errors\n\nPlease run the tests again with verbose output and fix the failures"
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
- Unlike Claude Code which separates validation context (`additionalContext`) from user prompts, Cursor combines everything into one `followup_message`
- The agent sees the full context (validation results + follow-up instruction) in a single message

---

## Claude Code Translator Requirements

### Event: SessionStart → SessionStart

**Claude Code documentation:** https://code.claude.com/docs/en/hooks#sessionstart-decision-control

**HookResponse fields used:**
- `messages`: Context to inject at session start
- `prompt`: Should be None (not used for SessionStart)
- `exit_code`: Cannot block (SessionStart doesn't support blocking)

**Claude Code JSON output:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Session initialized. Repository: /path/to/repo. Branch: main."
  }
}
```

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
- Any warnings/errors in `messages` should be formatted as context for the agent

---

### Event: UserPromptSubmit → PrePrompt

**HookResponse fields used:**
- `messages`: Validation warnings/errors
- `prompt`: Modified prompt text
- `exit_code`: Blocking if validation fails

**Blocking output:**
```json
{
  "continue": false,
  "stopReason": "❌ Prompt too long",
  "systemMessage": "Prompt too long"
}
```

**Success with modified prompt:**
```json
{
  "modifiedPrompt": "Modified prompt text here",
  "metadata": [["modified_prompt", "Modified prompt text here"]]
}
```

**Exit code:**
- Always exit 0

---

### Event: PreToolUse → PreFileChange

**HookResponse fields used:**
- `messages`: Warnings about stashing
- `prompt`: Should be None
- `exit_code`: Should never block

**Output format:**
```json
{}
```
(Usually no output needed)

**Exit code:**
- Always exit 0

---

### Event: PostToolUse → PostFileChange

**HookResponse fields used:**
- `messages`: Provenance recording status
- `prompt`: Should be None
- `exit_code`: Non-blocking even on failure

**Blocking output (should not happen):**
```json
{
  "decision": "block",
  "reason": "Provenance recording failed"
}
```

**Success output:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Provenance recorded for 3 files"
  }
}
```

**Exit code:**
- Always exit 0

---

### Event: Stop → PostResponse

**HookResponse fields used:**
- `messages`: Test/lint validation results
- `prompt`: Autoreply text
- `exit_code`: Should never block

**Success with autoreply:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Tests failed, rerunning with verbose output"
  },
  "metadata": [["autoreply", "Please run the tests again with verbose output"]]
}
```

**Success without autoreply:**
```json
{}
```

**Exit code:**
- Always exit 0

---

## ACP Proxy Requirements

### Event: session/new response → SessionStart

**HookResponse fields used:**
- `messages`: Initialization warnings/errors
- `prompt`: Should be None
- `exit_code`: Should never block (can't block session creation)

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Ignore `exit_code` (can't block session creation in ACP)
3. Continue normally

---

### Event: session/prompt → PrePrompt

**HookResponse fields used:**
- `messages`: Validation warnings/errors
- `prompt`: Modified prompt text
- `exit_code`: Blocking if validation fails

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. If `is_blocking()` → return error, don't forward prompt to agent
3. Use `prompt` field as modified prompt (or original if None)
4. **Question:** Should we prepend messages to the prompt so agent sees validation context?
5. Forward modified prompt to agent stdin

---

### Event: session/request_permission → PreFileChange

**HookResponse fields used:**
- `messages`: Warnings about stashing
- `prompt`: Should be None
- `exit_code`: Should never block (can't block permission requests)

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Ignore `exit_code` (PreFileChange never blocks)
3. Grant permission (send response)

---

### Event: session/update (tool complete) → PostFileChange

**HookResponse fields used:**
- `messages`: Provenance recording status
- `prompt`: Should be None
- `exit_code`: Should never block

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. No action needed (provenance recording is fire-and-forget)

---

### Event: session/update (stopReason) → PostResponse

**HookResponse fields used:**
- `messages`: Test/lint validation results
- `prompt`: Autoreply text
- `exit_code`: Should never block

**Processing:**
1. Emit all messages to stderr with emoji prefixes
2. Check `prompt` field for autoreply
3. If autoreply exists:
   - **Question:** Should we prepend messages to autoreply?
   - Queue autoreply via autoreply channel
4. If no autoreply, just show messages to user

---

## Open Questions

### 1. Should ACP proxy prepend messages to prompts/autoreplies?

**PrePrompt scenario:**
- Flow returns: `messages: [Warning("Tests are failing")], prompt: "Original prompt"`
- Should agent see: `"Note: Tests are failing\n\nOriginal prompt"`?
- Or just: `"Original prompt"` (with user seeing warning on stderr)?

**PostResponse scenario:**
- Flow returns: `messages: [Warning("Tests failed")], prompt: "Please run tests with verbose output"`
- Should autoreply be: `"Note: Tests failed\n\nPlease run tests with verbose output"`?
- Or just: `"Please run tests with verbose output"`?

**Recommendation:** Don't prepend messages to prompts/autoreplies. Messages are for user feedback (stderr), `prompt` field is for agent communication. Keep them separate.

### 2. Should Cursor/Claude Code translators prepend messages to metadata prompts?

**Current behavior:**
- Cursor: `user_message` and `agent_message` both get all messages
- Metadata gets `prompt` field as-is
- No prepending

**Question:** Should we change this to match ACP (separate messages from prompt)?

**Recommendation:** Keep current behavior. Cursor/Claude Code have separate fields for user/agent messages and metadata, so no need to combine them.

### 3. Should we validate that `prompt` is None for events that don't use it?

**Events that should never have `prompt`:**
- SessionStart
- PreFileChange
- PostFileChange
- PrepareCommitMessage

**Should translators warn or error if `prompt` is set on these events?**

**Recommendation:** Emit a debug warning if `AIKI_DEBUG=1`, otherwise ignore.

---

## Summary Table

| Vendor | Event | messages → | prompt → | exit_code → |
|--------|-------|-----------|----------|-------------|
| **Cursor** | beforeSubmitPrompt | user_message + agent_message | N/A | exit code |
| **Cursor** | beforeMCPExecution | user_message + agent_message | N/A | 0 (never block) |
| **Cursor** | afterFileEdit | user_message + agent_message | N/A | 0 (never block) |
| **Claude** | SessionStart | systemMessage (warnings only) | N/A | 0 (use continue:false) |
| **Claude** | UserPromptSubmit | systemMessage (warnings only) | modifiedPrompt + metadata | 0 (use continue:false) |
| **Claude** | PreToolUse | (usually empty) | N/A | 0 (never block) |
| **Claude** | PostToolUse | hookSpecificOutput.additionalContext | N/A | 0 (never block) |
| **Claude** | Stop | hookSpecificOutput.additionalContext | metadata["autoreply"] | 0 (never block) |
| **ACP** | SessionStart | stderr | N/A | ignored |
| **ACP** | PrePrompt | stderr | modified prompt | blocks if exit=2 |
| **ACP** | PreFileChange | stderr | N/A | ignored |
| **ACP** | PostFileChange | stderr | N/A | ignored |
| **ACP** | PostResponse | stderr | autoreply | ignored |

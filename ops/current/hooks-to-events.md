# Aiki Events

A unified event model for AI-assisted software development.

## Design Principles

1. **Namespaced** — Events grouped by domain (`session.*`, `shell.*`, `change.*`)
2. **Past tense / state** — Events describe what happened or what state we're in
3. **Intent-revealing** — `permission_asked` signals you can gate; `done` signals you can react
4. **Agent-agnostic** — Same events whether using Claude Code, Cursor, or future agents
5. **Versioned** — Event payloads include version info for safe evolution
6. **Type-safe responses** — Different response types for gateable vs. non-gateable events

## Naming Convention

```
{domain}.{state}
```

| State | Meaning | Can Block? |
|-------|---------|------------|
| `permission_asked` | Action is about to happen, approval requested | ✅ Yes |
| `done` | Action completed | ❌ No (react only) |
| `started` / `ended` / `resumed` | Lifecycle boundary | ❌ No |
| `submitted` / `received` | Message passed | ❌ No (informational) |

## Event Reference

### Session Lifecycle

| Event | Description | Use Cases |
|-------|-------------|-----------|
| `session.started` | New agent session began (startup or clear) | Initialize logging, reset state, track session metrics |
| `session.resumed` | Continuing previous session | Load prior context, apply previous approvals, audit trail continuity |
| `session.ended` | Agent session terminated | Cleanup resources, finalize logs, calculate session stats |

**Note:** `session.started` and `session.resumed` are distinct to help hooks differentiate between brand-new sessions and continuations.

### User / Agent Interaction

| Event | Description | Use Cases |
|-------|-------------|-----------|
| `prompt.submitted` | User submitted a prompt to the agent | Block inappropriate prompts, inject context (project conventions, active files), log user requests |
| `response.received` | Agent finished responding (final response) | Send autoreply to agent (autonomous loops), record agent outputs, trigger follow-up actions |

**Pairing:** These events are always paired (one `prompt.submitted` → one `response.received`). Streaming/multi-turn interactions are transparent to hooks — only the final response triggers `response.received`.

**Key capabilities:**
- **`prompt.submitted`** with `context` → Inject additional context into the prompt before agent sees it (e.g., "Active files: src/main.rs, tests/test.rs")
- **`response.received`** with `context` → Send autoreply to agent for autonomous loops (e.g., "Please run tests before committing")

### File Changes

| Event | Description | Gateable | Payload |
|-------|-------------|----------|---------|
| `change.permission_asked` | Agent is about to modify a file | ✅ Claude Code only | `file_path`, `old_content`, `new_content`, `change_type` (edit/create/delete), `agent` |
| `change.done` | Agent finished modifying a file | ❌ | `file_path`, `change_type`, `success` |

**Example payload:**
```json
{
  "version": "1.0",
  "event": "change.permission_asked",
  "file_path": "/path/to/file.rs",
  "old_content": "...",
  "new_content": "...",
  "change_type": "edit",
  "agent": "claude-code"
}
```

**Use cases:** Conditional approval based on file patterns, content analysis, automatic formatting checks.

### Shell Commands

| Event | Description | Gateable | Payload |
|-------|-------------|----------|---------|
| `shell.permission_asked` | Agent is about to execute a shell command | ✅ Both agents | `command`, `cwd`, `agent` |
| `shell.done` | Shell command completed | ❌ | `command`, `exit_code`, `stdout`, `stderr` |

**`shell.permission_asked` is the critical path for autonomous review** — intercept `git commit`, run review, provide feedback for self-correction.

**Example payload:**
```json
{
  "version": "1.0",
  "event": "shell.permission_asked",
  "command": "git commit -m 'fix: resolve type errors'",
  "cwd": "/path/to/repo",
  "agent": "claude-code"
}
```

**Hook response format** (for `permission_asked` events):
```json
{
  "decision": "denied",
  "feedback": [
    "Linter found 3 errors"
  ],
  "suggestions": [
    "Run `cargo fmt`",
    "Fix unused variable on line 42"
  ]
}
```

This enables **autonomous self-correction** — the agent receives structured feedback with actionable suggestions, not just "denied".

### MCP Tools

| Event | Description | Gateable | Payload |
|-------|-------------|----------|---------|
| `mcp.permission_asked` | Agent is about to call an MCP tool | ✅ Both agents | `tool_name`, `parameters`, `agent` |
| `mcp.done` | MCP tool call completed | ❌ | `tool_name`, `success`, `result` |

**Use cases:** Gate expensive operations (API calls, database queries), enforce rate limits, audit tool usage.

### Git Integration

| Event | Description | Payload |
|-------|-------------|---------|
| `git.prepare_commit_message` | Git prepare-commit-msg hook fired | `commit_msg_file`, `commit_source`, `commit_sha` |

**Use cases:** Inject co-authors from JJ, enforce commit message format, add issue references.

## Agent Support Matrix

| Event | Claude Code | Cursor | Notes |
|-------|-------------|--------|-------|
| `session.started` | ✅ | ✅ | Context injection supported in Claude Code |
| `session.resumed` | ✅ | ✅ | Context injection supported in Claude Code |
| `session.ended` | ✅ | ✅ | |
| `prompt.submitted` | ✅ Block + Context | ⚠️ Context only | Cursor cannot block, only inject context |
| `response.received` | ✅ Autoreply | ✅ Autoreply | Both support autoreply via `context` field |
| `change.permission_asked` | ✅ | ❌ | Claude Code only (ACP protocol support) |
| `change.done` | ✅ | ❌ | Claude Code only |
| `shell.permission_asked` | ✅ | ✅ | Split from `PreFileChange` - the autonomous review wedge |
| `shell.done` | ✅ | ✅ | Split from `PostFileChange` |
| `mcp.permission_asked` | ✅ | ✅ | Split from `PreFileChange` - for non-shell tools |
| `mcp.done` | ✅ | ✅ | Split from `PostFileChange` |
| `git.prepare_commit_message` | ✅ | ✅ | Standard Git hook, works with all agents |

## Migration from Previous Names

**Breaking change:** Old hook-style names are no longer supported. Update your configurations to use the new semantic event names.

| Old (Hook-Style) | New (Semantic) | Notes |
|------------------|----------------|-------|
| `SessionStart` | `session.started` | |
| `SessionEnd` | `session.ended` | |
| *(none)* | `session.resumed` | **New**: Separate from `session.started` |
| `PrePrompt` | `prompt.submitted` | **Renamed**: Now fires when user submits prompt, supports blocking & context injection |
| `PreFileChange` | `change.permission_asked` | **Renamed**: Now split into `change.*`, `shell.*`, `mcp.*` based on tool type |
| `PostFileChange` | `change.done` | **Renamed**: Now split into `change.*`, `shell.*`, `mcp.*` based on tool type |
| `PostResponse` | `response.received` | **Renamed**: Supports autoreply via `context` field |
| `PrepareCommitMessage` | `git.prepare_commit_message` | |
| *(none)* | `shell.permission_asked` | **New**: Split from `PreFileChange`, the autonomous review wedge |
| *(none)* | `shell.done` | **New**: Split from `PostFileChange` |
| *(none)* | `mcp.permission_asked` | **New**: Split from `PreFileChange` |
| *(none)* | `mcp.done` | **New**: Split from `PostFileChange` |

## Why This Change?

**Before:** Hook-style names leaked implementation details
```yaml
PreFileChange:    # "Pre" what? Can I block it? What hook is this?
PostResponse:     # Post whose response?
```

**After:** Semantic names reveal intent
```yaml
change.permission_asked:   # Clear: permission requested, I can approve/deny
response.received:         # Clear: agent responded, I can react
```

The new naming follows patterns developers already know from GitHub Actions, Stripe webhooks, and CloudEvents — semantic events, not implementation hooks.

## Complete Event Set

```yaml
# Session lifecycle
session.started             # New session begins
session.resumed             # Continue previous session  
session.ended               # Session terminates

# User / agent interaction  
prompt.submitted            # User sent a message
response.received           # Agent finished responding

# File changes
change.permission_asked     # ⭐ About to modify file (Cursor Unsuppored)
change.done                 # File modification complete

# Shell commands
shell.permission_asked      # ⭐ The autonomous review wedge
shell.done                  # Command execution complete

# MCP tools
mcp.permission_asked        # ⭐ About to call MCP tool
mcp.done                    # Tool call complete

# Git integration
git.prepare_commit_message  # Git hook fired
```

**12 events total.** Extensible as new capabilities emerge.

### Key Events for Autonomous Workflows

**Gateable (pre-approval):**
- **`shell.permission_asked`** — Intercept `git commit`, run autonomous review, provide feedback for self-correction
- **`change.permission_asked`** — Gate file modifications, enforce code standards
- **`mcp.permission_asked`** — Control expensive operations, enforce rate limits

**Context injection & autoreplies:**
- **`prompt.submitted`** with `context` — Inject project context, conventions, or active files into prompts
- **`response.received`** with `context` — Send autoreply to agent for autonomous loops (e.g., "run tests", "fix linter errors")

These five events enable full autonomous validation and self-correction loops.

---

## Response Types

Aiki uses **two distinct response types** to enforce type safety and clarify intent.

### ApprovalResponse (Gateable Events)

Used for `permission_asked` events — hooks can **approve or deny** the operation before it happens.

```rust
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,  // Approved or Denied
    pub feedback: Vec<String>,       // Why was this decision made?
    pub suggestions: Vec<String>,    // How can the agent fix it?
}

pub enum ApprovalDecision {
    Approved,
    Denied,
}
```

**When to use:** `change.permission_asked`, `shell.permission_asked`, `mcp.permission_asked`, `prompt.submitted` (partial support)

**Example:**
```json
{
  "decision": "denied",
  "feedback": [
    "Tests would fail with this change",
    "Type error on line 42"
  ],
  "suggestions": [
    "Fix the type annotation in function foo()",
    "Run `cargo test` to verify"
  ]
}
```

**Key insight:** The `suggestions` field enables **autonomous self-correction**. The agent receives actionable steps to fix the issue, not just a rejection.

### ValidationResponse (Non-Gateable Events)

Used for all other events — hooks can **react, log, or provide context** but cannot block the operation.

```rust
pub struct ValidationResponse {
    pub result: ValidationResult,  // Success or Failure
    pub messages: Vec<String>,     // Info, warnings, or errors
    pub context: Option<String>,   // Modified prompt or autoreply
}

pub enum ValidationResult {
    Success,
    Failure,
}
```

**When to use:** All `done`, `started`, `ended`, `resumed`, `received` events, and `prompt.submitted` when only context injection is needed (no blocking)

**Example (validation only):**
```json
{
  "result": "failure",
  "messages": [
    "⚠️ Change violates style guide",
    "Missing documentation for public function"
  ],
  "context": null
}
```

**Example (with context for prompt modification):**
```json
{
  "result": "success",
  "messages": [],
  "context": "Original prompt + added safety guidelines"
}
```

**Example (with context for autoreply):**
```json
{
  "result": "success",
  "messages": [],
  "context": "Thanks! Please run tests before committing."
}
```

### Response Type by Event

| Event | Response Type | Can Block? | Has Context? | Notes |
|-------|---------------|------------|--------------|-------|
| `session.started` | ValidationResponse | ❌ Never | ✅ (initial context) | Cannot block session creation in any vendor |
| `session.resumed` | ValidationResponse | ❌ Never | ✅ (resume context) | Cannot block session resumption |
| `session.ended` | ValidationResponse | ❌ Never | ❌ | Session already ended |
| `prompt.submitted` | **ApprovalResponse** | ⚠️ Partial | ✅ (modify prompt) | Claude Code/ACP: Yes, Cursor: No |
| `response.received` | ValidationResponse | ❌ Never | ✅ (autoreply) | Agent already finished, can only trigger autoreply |
| `change.permission_asked` | **ApprovalResponse** | ⚠️ Partial | ❌ | Claude Code only (ACP protocol support) |
| `change.done` | ValidationResponse | ❌ Never | ❌ | Change already completed |
| `shell.permission_asked` | **ApprovalResponse** | ✅ Yes | ❌ | All vendors support blocking |
| `shell.done` | ValidationResponse | ❌ Never | ❌ | Command already executed |
| `mcp.permission_asked` | **ApprovalResponse** | ✅ Yes | ❌ | All vendors support blocking |
| `mcp.done` | ValidationResponse | ❌ Never | ❌ | Tool already executed |
| `git.prepare_commit_message` | ValidationResponse | ❌ Never | ✅ (modify message) | Git hook, cannot block commit |

### Why Two Types?

**Before (overloaded `Decision` enum):**
```rust
// In a change.done handler - what does "Block" mean here?
Decision::Block  // ❌ Confusing - the change already happened!
```

**After (type-safe responses):**
```rust
// Gateable event - clear semantics
ApprovalResponse {
    decision: ApprovalDecision::Denied,  // ✅ Can actually prevent the operation
    feedback: vec!["Tests will fail".into()],
    suggestions: vec!["Fix assertion on line 42".into()],
}

// Non-gateable event - clear semantics
ValidationResponse {
    result: ValidationResult::Failure,  // ✅ Indicates validation failed (but can't prevent it)
    messages: vec!["⚠️ Change violates style guide".into()],
    context: None,
}
```

**Benefits:**
1. **Type safety** — Can't accidentally try to "block" a non-gateable event
2. **Clearer intent** — `ApprovalDecision::Denied` vs `ValidationResult::Failure` have distinct meanings
3. **Better tooling** — IDEs can autocomplete the right fields for each event type
4. **Structured feedback** — `suggestions` field enables autonomous self-correction
5. **Unified context** — `context` field handles both prompt modification and autoreplies

---

## Implementation Plan

Phased approach to migrate from hook-style names to semantic event names, then add new functionality.

### Strategy

Separate migration from feature development to reduce risk and complexity:

1. **Phase 1: Migration** - Rename existing hooks to semantic events (no new functionality)
2. **Phase 2: New Events** - Add new events (shell.*, mcp.*, session.resumed)
3. **Phase 3: Response Types** - Split into ApprovalResponse/ValidationResponse
4. **Phase 4: Context & Autoreply** - Add context injection and autoreply capabilities

---

## Phase 1: Migration (No New Functionality)

**Goal:** Rename existing hooks to semantic event names without changing behavior.

**Timeline:** 1 week

### Events to Migrate

| Old Name | New Name | Behavior Change? |
|----------|----------|------------------|
| `SessionStart` | `session.started` | None - 1:1 rename |
| `SessionEnd` | `session.ended` | None - 1:1 rename |
| `PrePrompt` | `prompt.submitted` | None - 1:1 rename |
| `PostResponse` | `response.received` | None - 1:1 rename |
| `PreFileChange` | `change.permission_asked` | None - 1:1 rename |
| `PostFileChange` | `change.done` | None - 1:1 rename |
| `PrepareCommitMessage` | `git.prepare_commit_message` | None - 1:1 rename |

**Note:** `PreFileChange`/`PostFileChange` currently handle all tools (file edits, shell commands, MCP tools). We'll keep that behavior in Phase 1, then split in Phase 2.

### Implementation Steps

#### Step 1.1: Update Event Type Enum
**File:** `cli/src/events.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    // New semantic names only - no backwards compatibility
    #[serde(rename = "session.started")]
    SessionStarted { /* ... */ },
    
    #[serde(rename = "session.ended")]
    SessionEnded { /* ... */ },
    
    #[serde(rename = "prompt.submitted")]
    PromptSubmitted { /* ... */ },
    
    #[serde(rename = "response.received")]
    ResponseReceived { /* ... */ },
    
    #[serde(rename = "change.permission_asked")]
    ChangePermissionAsked { /* ... */ },
    
    #[serde(rename = "change.done")]
    ChangeDone { /* ... */ },
    
    #[serde(rename = "git.prepare_commit_message")]
    GitPrepareCommitMessage { /* ... */ },
}
```

**Deliverable:** Event enum uses new semantic names only.

#### Step 1.2: Update Vendor Integrations
**Files:** `cli/src/vendors/{claude_code,cursor,acp}.rs`

```rust
// Update event emission to use new names
let event = AikiEvent::SessionStarted { /* ... */ };
```

**Deliverable:** All vendors emit new event names.

#### Step 1.3: Update Flow Engine
**File:** `cli/src/flows/engine.rs`

```rust
// Update event handler dispatch
match event {
    AikiEvent::SessionStarted { .. } => {
        // Handle session.started
    }
    AikiEvent::PromptSubmitted { .. } => {
        // Handle prompt.submitted
    }
    // ... repeat for all events
}
```

**Deliverable:** Flow engine handles new event names.

#### Step 1.4: Update Documentation
**Files:** 
- `README.md`
- Example flows in `aiki/default/`

**Deliverable:** All docs reference new names with migration guide.

### Testing Phase 1

- [ ] Unit tests for event serialization with new names
- [ ] Integration tests for all vendors
- [ ] Manual testing with updated flows
- [ ] Verify all code paths use new event names

### Success Criteria Phase 1

- ✅ All events renamed to semantic names
- ✅ All vendors emit new event names
- ✅ Flow engine handles new event names
- ✅ Documentation updated with migration guide
- ✅ No old event names remain in codebase

---

## Phase 2: New Events (Shell, MCP, Session.Resumed)

**Goal:** Add new events that didn't exist before.

**Timeline:** 2 weeks

**Dependencies:** Phase 1 complete

### Events to Add

| Event | Source | Behavior |
|-------|--------|----------|
| `session.resumed` | Split from `session.started` | Fires when resuming existing session |
| `shell.permission_asked` | Split from `change.permission_asked` | Fires before shell command execution |
| `shell.done` | Split from `change.done` | Fires after shell command execution |
| `mcp.permission_asked` | Split from `change.permission_asked` | Fires before MCP tool call |
| `mcp.done` | Split from `change.done` | Fires after MCP tool call |

### Implementation Steps

#### Step 2.1: Add New Event Variants
**File:** `cli/src/events.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AikiEvent {
    // Existing events (from Phase 1)
    #[serde(rename = "session.started")]
    SessionStarted { /* ... */ },
    
    // New events
    #[serde(rename = "session.resumed")]
    SessionResumed {
        session_id: String,
        previous_session_id: String,
    },
    
    #[serde(rename = "shell.permission_asked")]
    ShellPermissionAsked {
        command: String,
        cwd: PathBuf,
        agent: String,
    },
    
    #[serde(rename = "shell.done")]
    ShellDone {
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    
    #[serde(rename = "mcp.permission_asked")]
    McpPermissionAsked {
        tool_name: String,
        parameters: serde_json::Value,
        agent: String,
    },
    
    #[serde(rename = "mcp.done")]
    McpDone {
        tool_name: String,
        success: bool,
        result: Option<String>,
    },
}
```

**Deliverable:** New event types defined.

#### Step 2.2: Update Vendor Integrations
**Strategy:** Detect tool type and emit appropriate event.

**Files:** `cli/src/vendors/{claude_code,cursor,acp}.rs`

```rust
// In PreFileChange hook (now change.permission_asked)
match determine_tool_type(tool_name) {
    ToolType::FileChange => {
        emit(AikiEvent::ChangePermissionAsked { /* ... */ });
    }
    ToolType::ShellCommand => {
        emit(AikiEvent::ShellPermissionAsked { 
            command: extract_command(params),
            cwd: get_cwd(),
            agent: "claude-code",
        });
    }
    ToolType::McpTool => {
        emit(AikiEvent::McpPermissionAsked {
            tool_name: tool_name.to_string(),
            parameters: params.clone(),
            agent: "claude-code",
        });
    }
}
```

**Deliverable:** Vendors emit granular events based on tool type.

#### Step 2.3: Detect Session Resumption
**Strategy:** Check for existing session metadata in JJ.

**Files:** `cli/src/vendors/{claude_code,cursor,acp}.rs`

```rust
// In session initialization
if let Some(previous_session) = load_previous_session()? {
    emit(AikiEvent::SessionResumed {
        session_id: new_session_id,
        previous_session_id: previous_session.id,
    });
} else {
    emit(AikiEvent::SessionStarted {
        session_id: new_session_id,
    });
}
```

**Deliverable:** Session lifecycle properly tracked (started vs. resumed).

#### Step 2.4: Update Flow Engine
**File:** `cli/src/flows/engine.rs`

```rust
match event {
    AikiEvent::SessionResumed { .. } => {
        handle_session_resumed(event)?;
    }
    AikiEvent::ShellPermissionAsked { .. } => {
        handle_shell_permission_asked(event)?;
    }
    // ... add handlers for all new events
}
```

**Deliverable:** Flow engine handles new events.

#### Step 2.5: Create Example Flows
**Files:** `aiki/default/shell-review.yaml`, `aiki/default/session-resume.yaml`

```yaml
# shell-review.yaml - Autonomous review on git commit
shell.permission_asked:
  - if: $command.starts_with("git commit")
    then:
      - run: "cargo clippy --all-targets"
      - if: $exit_code != 0
        then:
          deny:
            feedback:
              - "Clippy found issues"
            suggestions:
              - "Fix clippy errors before committing"
```

**Deliverable:** Working examples demonstrating new events.

### Testing Phase 2

- [ ] Unit tests for new event types
- [ ] Integration tests for tool type detection
- [ ] Integration tests for session resumption
- [ ] Manual testing with shell commands
- [ ] Manual testing with MCP tools
- [ ] Manual testing with session resume

### Success Criteria Phase 2

- ✅ All new events fire correctly
- ✅ Tool type detection works (file vs. shell vs. MCP)
- ✅ Session resumption detection works
- ✅ Flow engine handles all new events
- ✅ Example flows demonstrate new capabilities
- ✅ Documentation complete for new events

---

## Phase 3: Response Types (ApprovalResponse vs. ValidationResponse)

**Goal:** Split response types for type safety and clarity.

**Timeline:** 1 week

**Dependencies:** Phase 2 complete

### Implementation Steps

#### Step 3.1: Define Response Types
**File:** `cli/src/flows/types.rs`

Response types already defined in the "Response Types" section above.

**Deliverable:** Type-safe response types defined.

#### Step 3.2: Update Flow Actions
**File:** `cli/src/flows/actions.rs`

```rust
// Old (generic Decision)
pub enum Decision {
    Allow,
    Block,
}

// New (type-specific responses)
pub enum FlowAction {
    Approve {
        feedback: Vec<String>,
        suggestions: Vec<String>,
    },
    Deny {
        feedback: Vec<String>,
        suggestions: Vec<String>,
    },
    Validate {
        result: ValidationResult,
        messages: Vec<String>,
    },
    // ... other actions
}
```

**Deliverable:** Actions map to correct response types.

#### Step 3.3: Update Flow Engine
**File:** `cli/src/flows/engine.rs`

```rust
fn execute_flow(event: &AikiEvent) -> Result<FlowResponse> {
    match event {
        // Gateable events → ApprovalResponse
        AikiEvent::ChangePermissionAsked { .. } => {
            let actions = load_and_execute_flow(event)?;
            Ok(FlowResponse::Approval(build_approval_response(actions)))
        }
        
        // Non-gateable events → ValidationResponse
        AikiEvent::ChangeDone { .. } => {
            let actions = load_and_execute_flow(event)?;
            Ok(FlowResponse::Validation(build_validation_response(actions)))
        }
        
        // ... handle all events
    }
}

pub enum FlowResponse {
    Approval(ApprovalResponse),
    Validation(ValidationResponse),
}
```

**Deliverable:** Flow engine returns type-appropriate responses.

#### Step 3.4: Update Vendor Integrations
**Files:** `cli/src/vendors/{claude_code,cursor,acp}.rs`

```rust
match flow_response {
    FlowResponse::Approval(approval) => {
        match approval.decision {
            ApprovalDecision::Approved => allow_operation(),
            ApprovalDecision::Denied => {
                block_operation();
                show_feedback(&approval.feedback);
                show_suggestions(&approval.suggestions);
            }
        }
    }
    FlowResponse::Validation(validation) => {
        if validation.result == ValidationResult::Failure {
            log_warnings(&validation.messages);
        }
        if let Some(context) = validation.context {
            inject_context(context);
        }
    }
}
```

**Deliverable:** Vendors handle both response types correctly.

#### Step 3.5: Update Flow YAML Syntax
**New syntax:**

```yaml
# Gateable event - use approve/deny actions
shell.permission_asked:
  - if: $command.starts_with("rm -rf")
    then:
      deny:
        feedback:
          - "Dangerous command detected"
        suggestions:
          - "Use a safer alternative"
      
  - approve:  # Explicit approval

# Non-gateable event - use validate action
shell.done:
  - if: $exit_code != 0
    then:
      validate:
        result: failure
        messages:
          - "Command failed with exit code $exit_code"
```

**Deliverable:** YAML syntax matches response types.

### Testing Phase 3

- [ ] Unit tests for response type serialization
- [ ] Unit tests for flow action → response mapping
- [ ] Integration tests for approval flow (approve/deny)
- [ ] Integration tests for validation flow
- [ ] Manual testing with real approval scenarios
- [ ] Manual testing with real validation scenarios

### Success Criteria Phase 3

- ✅ ApprovalResponse/ValidationResponse types defined
- ✅ Flow engine returns correct response type per event
- ✅ Vendors handle both response types
- ✅ YAML syntax supports approve/deny/validate actions
- ✅ Type safety enforced (can't deny a done event)
- ✅ Documentation updated with new syntax

---

## Phase 4: Context & Autoreply

**Goal:** Add context injection (prompt.submitted) and autoreply (response.received) capabilities.

**Timeline:** 2 weeks

**Dependencies:** Phase 3 complete

### Implementation Steps

#### Step 4.1: Add Context Field to ValidationResponse
**File:** `cli/src/flows/types.rs`

Context field already included in ValidationResponse definition above.

**Note:** Context field already added in Phase 3. This phase implements the behavior.

#### Step 4.2: Implement Context Injection (prompt.submitted)
**File:** `cli/src/flows/handlers/prompt_submitted.rs`

```rust
pub fn handle_prompt_submitted(
    event: PromptSubmitted,
    flow: &Flow,
) -> Result<ValidationResponse> {
    let mut context_builder = Vec::new();
    
    for action in &flow.actions {
        match action {
            FlowAction::InjectContext { content } => {
                context_builder.push(content.clone());
            }
            // ... other actions
        }
    }
    
    let context = if context_builder.is_empty() {
        None
    } else {
        Some(context_builder.join("\n\n"))
    };
    
    Ok(ValidationResponse {
        result: ValidationResult::Success,
        messages: vec![],
        context,
    })
}
```

**Deliverable:** Context injection works for `prompt.submitted`.

#### Step 4.3: Implement Autoreply (response.received)
**File:** `cli/src/flows/handlers/response_received.rs`

```rust
pub fn handle_response_received(
    event: ResponseReceived,
    flow: &Flow,
) -> Result<ValidationResponse> {
    let mut autoreply_builder = Vec::new();
    
    for action in &flow.actions {
        match action {
            FlowAction::Autoreply { message } => {
                autoreply_builder.push(message.clone());
            }
            // ... other actions
        }
    }
    
    let context = if autoreply_builder.is_empty() {
        None
    } else {
        Some(autoreply_builder.join("\n\n"))
    };
    
    Ok(ValidationResponse {
        result: ValidationResult::Success,
        messages: vec![],
        context,
    })
}
```

**Deliverable:** Autoreply works for `response.received`.

#### Step 4.4: Update Vendor Integrations
**Files:** `cli/src/vendors/{claude_code,cursor,acp}.rs`

```rust
// In prompt.submitted handler
let response = execute_flow(&event)?;
if let Some(context) = response.context {
    // Prepend or append context to original prompt
    let modified_prompt = format!("{}\n\n{}", context, original_prompt);
    send_to_agent(modified_prompt);
} else {
    send_to_agent(original_prompt);
}

// In response.received handler
let response = execute_flow(&event)?;
if let Some(autoreply) = response.context {
    // Send autoreply back to agent
    send_to_agent(autoreply);
}
```

**Deliverable:** Vendors inject context and send autoreplies.

#### Step 4.5: Add Flow YAML Syntax
**New actions:**

```yaml
prompt.submitted:
  - inject_context: |
      # Project Architecture
      This is a Rust CLI tool using clap for argument parsing.
      All code must follow the error handling patterns in CLAUDE.md.
  
  - inject_context:
      file: .aiki/arch/backend.md

response.received:
  - let: errors = self.count_typescript_errors
  - if: $errors > 0
    then:
      autoreply: "Please fix the TypeScript errors before continuing."
```

**Deliverable:** YAML syntax supports context injection and autoreplies.

#### Step 4.6: Implement MessageBuilder Pattern
**File:** `cli/src/flows/actions/messages.rs`

```rust
pub struct MessageBuilder {
    prepend: Vec<String>,
    append: Vec<String>,
}

impl MessageBuilder {
    pub fn new() -> Self { /* ... */ }
    
    pub fn prepend(&mut self, content: String) { /* ... */ }
    pub fn append(&mut self, content: String) { /* ... */ }
    
    pub fn build(&self, original: &str) -> String {
        let mut result = Vec::new();
        result.extend(self.prepend.iter().cloned());
        result.push(original.to_string());
        result.extend(self.append.iter().cloned());
        result.join("\n\n")
    }
}
```

**Deliverable:** MessageBuilder enables prepend/append syntax.

### Testing Phase 4

- [ ] Unit tests for context injection
- [ ] Unit tests for autoreply
- [ ] Unit tests for MessageBuilder
- [ ] Integration tests for prompt modification
- [ ] Integration tests for autoreply loops
- [ ] Manual testing with Claude Code
- [ ] Manual testing with Cursor

### Success Criteria Phase 4

- ✅ Context injection works for `prompt.submitted`
- ✅ Autoreply works for `response.received`
- ✅ MessageBuilder supports prepend/append
- ✅ Vendors inject context correctly
- ✅ Vendors send autoreplies correctly
- ✅ YAML syntax documented
- ✅ Example flows demonstrate context/autoreply

---

## Overall Timeline

| Phase | Duration | Dependencies | Deliverable |
|-------|----------|--------------|-------------|
| **Phase 1: Migration** | 1 week | None | All events renamed (breaking change) |
| **Phase 2: New Events** | 2 weeks | Phase 1 | Shell, MCP, session.resumed events working |
| **Phase 3: Response Types** | 1 week | Phase 2 | ApprovalResponse/ValidationResponse split |
| **Phase 4: Context & Autoreply** | 2 weeks | Phase 3 | Context injection and autoreply working |

**Total:** 6 weeks

---

## Risk Mitigation

### Risk 1: Breaking Changes in Phase 1
**Mitigation:**
- Publish migration guide clearly documenting all name changes
- Update all example flows in `aiki/default/` as reference
- Provide clear error messages when old event names are detected
- Document the migration in release notes

### Risk 2: Tool Type Detection Complexity (Phase 2)
**Mitigation:**
- Start with simple heuristics (command string contains "bash", "sh", etc.)
- Add vendor-specific detection (ACP protocol has explicit tool types)
- Document edge cases clearly
- Allow manual override in flow config if needed

### Risk 3: Context Injection Breaks Agent Behavior (Phase 4)
**Mitigation:**
- Make context injection opt-in (requires explicit flow)
- Test with multiple prompt sizes to ensure no token limit issues
- Add max context size limit in config
- Provide clear examples of good vs. bad context injection

### Risk 4: Autoreply Loops (Phase 4)
**Mitigation:**
- Add max autoreply depth config (default: 3)
- Track autoreply count in session metadata
- Error clearly when limit exceeded
- Document best practices for autoreply conditions

---

## User Migration Guide

### From Old Hook Names to New Event Names

**Step 1:** Update your flow files

```yaml
# Before (old hook names)
SessionStart:
  - run: echo "Session started"

PrePrompt:
  - run: echo "About to send prompt"

# After (new event names)
session.started:
  - run: echo "Session started"

prompt.submitted:
  - run: echo "About to send prompt"
```

**Step 2:** Test your flows

```bash
# Validate flow syntax
aiki flows validate .aiki/flows/

# Test with dry-run mode (coming soon)
aiki flows test .aiki/flows/my-flow.yaml
```

**Step 3:** Update to new syntax (Phases 3-4)

```yaml
# Phase 3: Use approve/deny instead of allow/block
shell.permission_asked:
  - if: $command.contains("rm -rf")
    then:
      deny:
        feedback: ["Dangerous command"]
        suggestions: ["Use safer alternative"]

# Phase 4: Use inject_context and autoreply
prompt.submitted:
  - inject_context:
      file: .aiki/arch/patterns.md

response.received:
  - if: $errors > 0
    then:
      autoreply: "Fix errors before continuing"
```

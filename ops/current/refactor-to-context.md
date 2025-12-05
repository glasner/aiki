# Refactor: Replace `prompt` Field with `context` Field

## Problem Statement

Through research into vendor hook capabilities, we discovered that **hook-based integrations (Cursor, Claude Code) cannot fully rewrite prompts**. They can only:
- **Block prompts** (PrePrompt validation)
- **Add context** via `additionalContext` field (SessionStart, PrePrompt)
- **Send follow-up messages** (PostResponse autoreplies)

However, our current design assumes full prompt rewriting is possible via the `prompt` field in `HookResponse`.

To create a **unified abstraction** that works across all vendors (ACP, Cursor, Claude Code), we need to:
1. Replace `prompt` field with `context` field
2. Accept that we prepend context to original prompts (not replace them)
3. Track the original prompt separately for flow variable access

---

## Current Design (from refactor.md)

```rust
pub struct HookResponse {
    pub messages: Vec<Message>,
    pub prompt: Option<String>,  // ❌ Assumes full prompt rewriting
    pub exit_code: Option<i32>,
}
```

**Usage:**
- **PrePrompt**: `prompt` = "modified prompt text" (full replacement)
- **PostResponse**: `prompt` = "autoreply text" (full message)

**Problem:** Cursor/Claude Code cannot do full prompt replacement, only context injection.

---

## New Design

```rust
pub struct HookResponse {
    pub messages: Vec<Message>,
    pub context: Option<Context>,  // ✅ Context prepended to prompts/autoreplies
    pub exit_code: Option<i32>,
}

pub struct Context {
    pub prepend: Option<String>,  // Prepended to context block
    pub append: Option<String>,   // Appended to context block
}
```

**Ordering:**
```
[Context.prepend]

[Context.append]

[Original prompt/autoreply]
```

**Note:** `prepend`/`append` control ordering **within the context block**, not relative to the original prompt. The context block is always prepended.

---

## Flow YAML Changes

### PrePrompt: Old vs New

**Old (prompt action):**
```yaml
PrePrompt:
  - prompt: "Additional context here"
```

**New (context action):**

**Simple form (append only):**
```yaml
PrePrompt:
  - context: "Additional context here"
```

**Explicit form:**
```yaml
PrePrompt:
  - context:
      prepend: "Project uses TypeScript with strict mode"
      append: "Follow the existing code style guide"
```

**Result:**
```
Project uses TypeScript with strict mode

Follow the existing code style guide

[Original user prompt]
```

---

### PostResponse: autoreply stays the same

**Syntax (unchanged):**
```yaml
PostResponse:
  - autoreply: "Please run the tests again"
```

**Or explicit form:**
```yaml
PostResponse:
  - autoreply:
      prepend: "Tests failed with 3 errors"
      append: "Please run the tests again with verbose output"
```

**Note:** `autoreply` action stores its content in `HookResponse.context` field, same as `context` action. The distinction is semantic (context vs follow-up message) but they use the same underlying mechanism.

---

## Implementation Changes

### 1. Update HookResponse Struct

**File:** `cli/src/handlers.rs`

```rust
pub struct HookResponse {
    pub messages: Vec<Message>,
    pub context: Option<Context>,
    pub exit_code: Option<i32>,
}

pub struct Context {
    pub prepend: Option<String>,
    pub append: Option<String>,
}

impl Context {
    /// Build the context block from prepend/append
    pub fn build(&self) -> String {
        match (&self.prepend, &self.append) {
            (Some(pre), Some(app)) => format!("{}\n\n{}", pre, app),
            (Some(pre), None) => pre.clone(),
            (None, Some(app)) => app.clone(),
            (None, None) => String::new(),
        }
    }
}

impl HookResponse {
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
}
```

---

### 2. Update Flow Types

**File:** `cli/src/flows/types.rs`

**Remove PromptAction:**
```rust
// OLD - REMOVE
pub struct PromptAction {
    pub prompt: PromptContent,
    pub on_failure: FailureMode,
}
```

**Keep AutoreplyAction (stores in HookResponse.context):**
```rust
// KEEP - but stores result in context field
pub struct AutoreplyAction {
    pub autoreply: AutoreplyContent,
    pub on_failure: FailureMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutoreplyContent {
    Simple(String),
    Explicit {
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        append: Option<String>,
    },
}
```

**Add ContextAction:**
```rust
pub struct ContextAction {
    pub context: ContextContent,
    pub on_failure: FailureMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextContent {
    /// Simple form: defaults to append
    /// YAML: `context: "text"`
    Simple(String),

    /// Explicit form with prepend/append
    /// YAML: `context: { prepend: "...", append: "..." }`
    Explicit {
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        append: Option<String>,
    },
}
```

**Update Action enum:**
```rust
pub enum Action {
    If(IfAction),
    Switch(SwitchAction),
    Shell(ShellAction),
    Jj(JjAction),
    Log(LogAction),
    Let(LetAction),
    Self_(SelfAction),
    Context(ContextAction),     // ✅ NEW: Replaces Prompt
    Autoreply(AutoreplyAction), // ✅ KEEP: Stores in context field
    CommitMessage(CommitMessageAction),
}
```

---

### 3. Update Flow State (Message Building)

**File:** `cli/src/flows/state.rs`

**Current:** `state.message` holds the full modified prompt/autoreply

**New:** `state.context` holds prepend/append parts separately

```rust
pub struct AikiState {
    // ... existing fields ...
    
    /// Context to prepend to prompts/autoreplies
    pub context: Context,
}

impl AikiState {
    pub fn new(event: AikiEvent) -> Self {
        Self {
            // ... existing fields ...
            context: Context {
                prepend: None,
                append: None,
            },
        }
    }
    
    /// Build HookResponse with accumulated context
    pub fn build_response(&self) -> HookResponse {
        let context_opt = if self.context.prepend.is_some() || self.context.append.is_some() {
            Some(self.context.clone())
        } else {
            None
        };
        
        HookResponse {
            messages: self.messages.clone(),
            context: context_opt,
            exit_code: self.exit_code,
        }
    }
}
```

---

### 4. Update Flow Engine (Context Action Execution)

**File:** `cli/src/flows/engine.rs`

**Add context action executor:**
```rust
fn execute_context_action(
    action: &ContextAction,
    state: &mut AikiState,
) -> Result<ActionResult> {
    let (prepend, append) = match &action.context {
        ContextContent::Simple(text) => {
            // Simple form defaults to append
            (None, Some(expand_variables(text, state)?))
        }
        ContextContent::Explicit { prepend, append } => {
            let prepend_expanded = prepend
                .as_ref()
                .map(|s| expand_variables(s, state))
                .transpose()?;
            let append_expanded = append
                .as_ref()
                .map(|s| expand_variables(s, state))
                .transpose()?;
            (prepend_expanded, append_expanded)
        }
    };
    
    // Accumulate into state.context
    if let Some(pre) = prepend {
        state.context.prepend = Some(match &state.context.prepend {
            Some(existing) => format!("{}\n\n{}", existing, pre),
            None => pre,
        });
    }
    
    if let Some(app) = append {
        state.context.append = Some(match &state.context.append {
            Some(existing) => format!("{}\n\n{}", existing, app),
            None => app,
        });
    }
    
    Ok(ActionResult::Continue)
}

fn execute_autoreply_action(
    action: &AutoreplyAction,
    state: &mut AikiState,
) -> Result<ActionResult> {
    // Same logic as context action - stores in state.context
    let (prepend, append) = match &action.autoreply {
        AutoreplyContent::Simple(text) => {
            (None, Some(expand_variables(text, state)?))
        }
        AutoreplyContent::Explicit { prepend, append } => {
            let prepend_expanded = prepend
                .as_ref()
                .map(|s| expand_variables(s, state))
                .transpose()?;
            let append_expanded = append
                .as_ref()
                .map(|s| expand_variables(s, state))
                .transpose()?;
            (prepend_expanded, append_expanded)
        }
    };
    
    // Accumulate into state.context (same field as context action)
    if let Some(pre) = prepend {
        state.context.prepend = Some(match &state.context.prepend {
            Some(existing) => format!("{}\n\n{}", existing, pre),
            None => pre,
        });
    }
    
    if let Some(app) = append {
        state.context.append = Some(match &state.context.append {
            Some(existing) => format!("{}\n\n{}", existing, app),
            None => app,
        });
    }
    
    Ok(ActionResult::Continue)
}
```

**Update action dispatcher:**
```rust
match action {
    Action::Context(ctx) => execute_context_action(ctx, state)?,
    Action::Autoreply(ar) => execute_autoreply_action(ar, state)?,
    // ... other actions ...
}
```

---

### 5. Update Handlers

**File:** `cli/src/handlers.rs`

**PrePrompt handler:**
```rust
pub fn handle_pre_prompt(event: AikiPrePromptEvent) -> Result<HookResponse> {
    let core_flow = crate::flows::load_core_flow()?;
    let mut state = AikiState::new(event);
    state.flow_name = Some("aiki/core".to_string());
    
    let (flow_result, _timing) = FlowEngine::execute_actions(&core_flow.pre_prompt, &mut state)?;
    
    match flow_result {
        FlowResult::Success => Ok(state.build_response()),
        FlowResult::FailedContinue(_) => {
            // Graceful degradation: no context on error
            Ok(HookResponse::success())
        }
        FlowResult::FailedStop(_) => Ok(HookResponse::success()),
        FlowResult::FailedBlock(msg) => {
            Ok(HookResponse::blocking()
                .with_error(format!("PrePrompt validation failed: {}", msg)))
        }
    }
}
```

**PostResponse handler:**
```rust
pub fn handle_post_response(event: AikiPostResponseEvent) -> Result<HookResponse> {
    let core_flow = crate::flows::load_core_flow()?;
    let mut state = AikiState::new(event);
    state.flow_name = Some("aiki/core".to_string());
    
    let (flow_result, _timing) = FlowEngine::execute_actions(&core_flow.post_response, &mut state)?;
    
    match flow_result {
        FlowResult::Success => Ok(state.build_response()),
        FlowResult::FailedContinue(_) => Ok(HookResponse::success()),
        FlowResult::FailedStop(_) => Ok(HookResponse::success()),
        FlowResult::FailedBlock(_) => {
            // PostResponse cannot block, treat as no autoreply
            Ok(HookResponse::success())
        }
    }
}
```

---

### 6. Update ACP Proxy

**File:** `cli/src/commands/acp.rs`

**PrePrompt handler:**
```rust
fn handle_session_prompt(...) -> Result<()> {
    // Fire PrePrompt event
    let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        original_prompt: original_text.clone(),
    });

    let response = event_bus::dispatch(event)?;

    // Emit messages to stderr
    for msg in &response.messages {
        match msg {
            Message::Info(s) => eprintln!("[aiki] ℹ️ {}", s),
            Message::Warning(s) => eprintln!("[aiki] ⚠️ {}", s),
            Message::Error(s) => eprintln!("[aiki] ❌ {}", s),
        }
    }

    // Check if blocked
    if response.is_blocking() {
        return Err(AikiError::Other(anyhow!("PrePrompt validation blocked prompt")));
    }

    // Build final prompt: agent_context + original
    let agent_context = build_agent_context(&response);
    let final_prompt = if !agent_context.is_empty() {
        format!("{}\n\n{}", agent_context, original_text)
    } else {
        original_text
    };

    // Forward final_prompt to agent stdin
    // ... existing forwarding logic ...
}
```

**PostResponse handler:**
```rust
fn handle_post_response(...) -> Result<()> {
    // Fire PostResponse event
    let event = AikiEvent::PostResponse(AikiPostResponseEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
        response: response_text.to_string(),
        modified_files: Vec::new(),
    });

    let response = event_bus::dispatch(event)?;

    // Emit messages to stderr (user-visible only)
    for msg in &response.messages {
        match msg {
            Message::Info(s) => eprintln!("[aiki] ℹ️ {}", s),
            Message::Warning(s) => eprintln!("[aiki] ⚠️ {}", s),
            Message::Error(s) => eprintln!("[aiki] ❌ {}", s),
        }
    }

    // Check for autoreply (agent-visible via next prompt)
    let agent_context = build_agent_context(&response);
    if !agent_context.is_empty() {
        // Queue autoreply with combined messages + context
        let autoreply_msg = Autoreply::new(session_id, agent_context, new_count);
        autoreply_tx.send(AutoreplyMessage::SendAutoreply(autoreply_msg))?;
    }

    Ok(())
}
```

---

### 7. Update Cursor Translator

**File:** `cli/src/vendors/cursor.rs`

**SessionStart:**
```rust
// SessionStart: additionalContext not supported by Cursor
// Just return success/blocking
```

**PrePrompt (beforeSubmitPrompt):**
```rust
// Cursor doesn't support modifying prompts, but we can still validate
if response.is_blocking() {
    json.insert("continue", json!(false));
    if let Some(first_error) = response.messages.iter().find_map(|m| match m {
        Message::Error(s) => Some(s),
        _ => None,
    }) {
        json.insert("user_message", json!(format!("❌ {}", first_error)));
    }
} else {
    json.insert("continue", json!(true));
}

// Ignore response.context (Cursor can't inject context via beforeSubmitPrompt)
```

**PostResponse (stop):**
```rust
let followup_text = build_agent_context(&response);

if !followup_text.is_empty() {
    json.insert("followup_message", json!(followup_text));
}
```

---

### 8. Update Claude Code Translator

**File:** `cli/src/vendors/claude_code.rs`

**SessionStart:**
```rust
let agent_context = build_agent_context(&response);

if !agent_context.is_empty() {
    json.insert("hookSpecificOutput", json!({
        "hookEventName": "SessionStart",
        "additionalContext": agent_context
    }));
}
```

**PrePrompt (UserPromptSubmit):**
```rust
if response.is_blocking() {
    json.insert("continue", json!(false));
    // ... stopReason, systemMessage ...
} else {
    json.insert("decision", json!("proceed"));
    
    let agent_context = build_agent_context(&response);
    if !agent_context.is_empty() {
        json.insert("hookSpecificOutput", json!({
            "hookEventName": "UserPromptSubmit",
            "additionalContext": agent_context
        }));
    }
}
```

**PostResponse (Stop):**
```rust
let agent_context = build_agent_context(&response);

if !agent_context.is_empty() {
    json.insert("hookSpecificOutput", json!({
        "hookEventName": "PostToolUse",  // Stop uses PostToolUse format
        "additionalContext": agent_context
    }));
}

// Autoreply as metadata (if context field was used for autoreply)
if let Some(context) = &response.context {
    let autoreply_text = context.build();
    if !autoreply_text.is_empty() {
        json.insert("metadata", json!([["autoreply", autoreply_text]]));
    }
}
```

---

## Original Prompt Tracking

**Problem:** Flows need access to the original user prompt for variable substitution.

**Solution:** Rename `original_prompt` field to `prompt` in the event struct.

**File:** `cli/src/events.rs`

```rust
pub struct AikiPrePromptEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub prompt: String,  // ✅ Renamed from original_prompt
}
```

**Usage in flows:**
```yaml
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - log: "Prompt too long: ${event.prompt.len()} chars"
    on_failure: block
```

---

## Migration Path

### Phase 1: Add Context Support (Non-Breaking)

1. Add `Context` struct to `handlers.rs`
2. Add `context` field to `HookResponse` (alongside `prompt` field - deprecated)
3. Add `ContextAction` to flow types
4. Update flow engine to handle `context` action
5. Update translators to read from `context` field (fallback to `prompt` for compat)

### Phase 2: Update Core Flow

1. Replace `prompt:` actions with `context:` in `.aiki/flows/aiki/core.yml`
2. Test with all three vendors (ACP, Cursor, Claude Code)

### Phase 3: Update Handlers

1. Change handlers to return `context` instead of metadata `prompt` field
2. Update ACP proxy to build final prompts from context + original

### Phase 4: Remove Deprecated `prompt` Field

1. Remove `prompt` field from `HookResponse`
2. Remove `PromptAction` from flow types
3. Keep `AutoreplyAction` (it stores in `context` field)
4. Clean up any remaining references

---

## Benefits

1. **Unified abstraction** - Works consistently across ACP, Cursor, Claude Code
2. **Accurate mental model** - We prepend context, not replace prompts
3. **Vendor parity** - All vendors can add context (SessionStart, PrePrompt, PostResponse)
4. **Clearer semantics** - "context" vs "prompt" makes the distinction obvious
5. **Future-proof** - If vendors add full prompt modification later, we can extend Context struct

---

## Breaking Changes

### For Flow Authors

**Old:**
```yaml
PrePrompt:
  - prompt: "Additional context"
```

**New:**
```yaml
PrePrompt:
  - context: "Additional context"
```

**Old:**
```yaml
PostResponse:
  - autoreply: "Please run tests"
```

**New:**
```yaml
PostResponse:
  - context: "Please run tests"
```

### For Custom Flows

Any custom flows using `prompt:` or `autoreply:` actions will need to be updated to use `context:`.

---

## Testing Strategy

1. **Unit tests** - Test `Context.build()` with various prepend/append combinations
2. **Flow engine tests** - Test context action execution and accumulation
3. **Handler tests** - Test that handlers return correct `context` field
4. **Translator tests** - Test each vendor's context handling:
   - Cursor: Stop followup_message concatenation
   - Claude: SessionStart/PrePrompt additionalContext
   - ACP: Context + original prompt concatenation
5. **Integration tests** - End-to-end test with all three vendors

---

## Resolved Design Decisions

1. **SessionStart context in Cursor**: ✅ Skip - Cursor doesn't support `additionalContext` for any hooks

2. **Empty context handling**: ✅ Omit - If `prepend` and `append` are both None/empty, omit `hookSpecificOutput` entirely

3. **Multiple context actions**: ✅ No limits - If a flow has multiple `context:` actions, they accumulate without enforced limits (natural limit is vendor message size constraints)

4. **Error messages in context**: ✅ Yes - When building final context for `additionalContext` (Claude Code, ACP final prompt), combine `messages` and `context` fields so agents can see validation messages

---

## Implementation Notes for Message + Context Combination

When building `additionalContext` or final prompts for agents, **always combine messages and context**:

```rust
// Pattern for building agent-visible context
fn build_agent_context(response: &HookResponse) -> String {
    let mut parts = vec![];
    
    // Add validation messages
    for msg in &response.messages {
        match msg {
            Message::Info(s) => parts.push(format!("ℹ️ {}", s)),
            Message::Warning(s) => parts.push(format!("⚠️ {}", s)),
            Message::Error(s) => parts.push(format!("❌ {}", s)),
        }
    }
    
    // Add context
    if let Some(context) = &response.context {
        let context_text = context.build();
        if !context_text.is_empty() {
            parts.push(context_text);
        }
    }
    
    parts.join("\n\n")
}
```

**Where this applies:**
- **Claude Code**: `hookSpecificOutput.additionalContext` (SessionStart, PrePrompt, PostResponse)
- **ACP**: Final prompt concatenation (context + original)
- **Cursor**: `followup_message` in stop hook (PostResponse)

**Where messages are user-only:**
- **ACP stderr**: Always emit messages to stderr for user visibility
- **Cursor blocking**: Only first error shown in `user_message` when blocking

---

## Summary

This refactor aligns our abstraction with vendor reality:
- ✅ Context injection (all vendors support)
- ❌ Full prompt replacement (only ACP supports, via our own concatenation)

By renaming `prompt` → `context` and accepting prepend-only semantics, we create a unified model that works across all integrations while being honest about capabilities.

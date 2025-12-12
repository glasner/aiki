# Design: Flow-Level Message Emission (info, warning, error)

## Problem Statement

**Current architecture:** Messages are hardcoded in handler functions
```rust
// handlers.rs
FlowResult::FailedContinue(msg) => Ok(HookResponse::success_with_message(
    "⚠️ Session started with warnings",  // ❌ Hardcoded in handler
).with_info(format!("Some initialization actions failed: {}", msg))),
```

**Issue:** Flows cannot control what messages users see. The handler decides the message format, not the flow.

**Goal:** Allow flows to emit messages that appear in `HookResponse.messages`

## Proposed Solution: YAML Helpers (info, warning, error)

Three simple action types with clean syntax:

```yaml
PostFileChange:
  - info: "✅ Provenance recorded for ${event.file_count} files"
  - warning: "⚠️ Tests failed but changes were saved"
  - error: "❌ Validation failed: ${error_msg}"
```

**No verbose `message:` action needed!**

## Use Cases

### 1. Success notification
```yaml
PostFileChange:
  - let: metadata = self.build_metadata
  - jj: "describe -m '[aiki]$metadata'"
  - info: "✅ Provenance recorded for ${event.file_count} files"
```

### 2. Conditional warning with autoreply
```yaml
PostFileChange:
  - shell: "cargo test --quiet"
    alias: test_result
    on_failure: continue
  
  - if: "$test_result.exit_code != 0"
    then:
      - warning: "⚠️ Tests failed but changes were saved"
      - autoreply: "Please fix these test failures:\n\n$test_result.stderr"
```

**Message audience:**
- **User sees:** `⚠️ Tests failed but changes were saved` in IDE (via stderr in ACP, UI notification in vendor hooks)
- **Agent sees:** Autoreply with test output

### 3. Blocking error
```yaml
PrePrompt:
  - if: "$event.prompt.len() > 10000"
    then:
      - error: "❌ Prompt too long (${event.prompt.len()} chars, max 10000)"
    on_failure: block
```

### 4. Multiple messages
```yaml
SessionStart:
  - shell: "jj workspace list"
    alias: workspace_check
    on_failure: continue
  
  - if: "$workspace_check.exit_code != 0"
    then:
      - warning: "⚠️ JJ repository not initialized"
      - info: "Run 'aiki init' to set up provenance tracking"
```

## Implementation

### 1. Add to types.rs

```rust
/// Info message action (user-visible info notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoAction {
    pub info: String,
    
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Warning message action (user-visible warning notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningAction {
    pub warning: String,
    
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

/// Error message action (user-visible error notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAction {
    pub error: String,
    
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}

// Add to Action enum (untagged, order matters for parser)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    // Complex actions first (have required fields that distinguish them)
    If(IfAction),
    Switch(SwitchAction),
    Let(LetAction),
    Self_(SelfAction),
    
    // Actions with single key field (order matters to avoid ambiguity)
    Shell(ShellAction),
    Jj(JjAction),
    Log(LogAction),
    Context(ContextAction),
    Autoreply(AutoreplyAction),
    CommitMessage(CommitMessageAction),
    
    // Message actions last (simple string fields)
    Info(InfoAction),
    Warning(WarningAction),
    Error(ErrorAction),
}
```

### 2. Add to state.rs

```rust
pub struct AikiState {
    // ... existing fields ...
    
    /// Accumulated messages from info/warning/error actions
    messages: Vec<crate::handlers::Message>,
}

impl AikiState {
    pub fn new(event: crate::events::AikiEvent) -> Self {
        Self {
            event,
            let_vars: HashMap::new(),
            variable_metadata: HashMap::new(),
            flow_name: None,
            context_assembler: None,
            messages: Vec::new(),  // ← Initialize
        }
    }
    
    /// Add a message to be included in HookResponse
    pub fn add_message(&mut self, message: crate::handlers::Message) {
        self.messages.push(message);
    }
    
    /// Take all accumulated messages (consumes them)
    pub fn take_messages(&mut self) -> Vec<crate::handlers::Message> {
        std::mem::take(&mut self.messages)
    }
    
    /// Get messages without consuming (for inspection)
    pub fn messages(&self) -> &[crate::handlers::Message] {
        &self.messages
    }
}
```

### 3. Add to engine.rs

```rust
impl FlowEngine {
    fn execute_action(action: &Action, context: &mut AikiState) -> Result<ActionResult> {
        match action {
            Action::If(if_action) => Self::execute_if(if_action, context),
            Action::Switch(switch_action) => Self::execute_switch(switch_action, context),
            Action::Shell(shell_action) => Self::execute_shell(shell_action, context),
            Action::Jj(jj_action) => Self::execute_jj(jj_action, context),
            Action::Log(log_action) => Self::execute_log(log_action, context),
            Action::Let(let_action) => Self::execute_let(let_action, context),
            Action::Self_(self_action) => Self::execute_self(self_action, context),
            Action::Context(context_action) => Self::execute_context(context_action, context),
            Action::Autoreply(autoreply_action) => Self::execute_autoreply(autoreply_action, context),
            Action::CommitMessage(commit_msg_action) => Self::execute_commit_message(commit_msg_action, context),
            Action::Info(info_action) => Self::execute_info(info_action, context),
            Action::Warning(warning_action) => Self::execute_warning(warning_action, context),
            Action::Error(error_action) => Self::execute_error(error_action, context),
        }
    }
    
    /// Execute an info message action
    fn execute_info(action: &InfoAction, context: &mut AikiState) -> Result<ActionResult> {
        use crate::handlers::Message;
        
        let mut resolver = Self::create_resolver(context);
        let text = resolver.resolve(&action.info);
        
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Info: {}", text);
        }
        
        context.add_message(Message::Info(text.clone()));
        
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: text,
            stderr: String::new(),
        })
    }
    
    /// Execute a warning message action
    fn execute_warning(action: &WarningAction, context: &mut AikiState) -> Result<ActionResult> {
        use crate::handlers::Message;
        
        let mut resolver = Self::create_resolver(context);
        let text = resolver.resolve(&action.warning);
        
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Warning: {}", text);
        }
        
        context.add_message(Message::Warning(text.clone()));
        
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: text,
            stderr: String::new(),
        })
    }
    
    /// Execute an error message action
    fn execute_error(action: &ErrorAction, context: &mut AikiState) -> Result<ActionResult> {
        use crate::handlers::Message;
        
        let mut resolver = Self::create_resolver(context);
        let text = resolver.resolve(&action.error);
        
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Error: {}", text);
        }
        
        context.add_message(Message::Error(text.clone()));
        
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: text,
            stderr: String::new(),
        })
    }
    
    fn store_action_result(action: &Action, result: &ActionResult, context: &mut AikiState) {
        match action {
            // ... existing cases ...
            Action::Info(_) | Action::Warning(_) | Action::Error(_) => {
                // Message actions don't store results (they add to messages collection)
            }
        }
    }
}
```

### 4. Update handlers.rs

**Simple pattern for all handlers** - no fallback logic needed:

```rust
pub fn handle_post_file_change(event: AikiPostFileChangeEvent) -> Result<HookResponse> {
    // Load core flow
    let core_flow = crate::flows::load_core_flow()?;
    
    // Build execution state from event
    let mut state = AikiState::new(event.clone());
    state.flow_name = Some("aiki/core".to_string());
    
    // Execute PostFileChange actions from the core flow
    let (flow_result, _timing) = 
        FlowEngine::execute_actions(&core_flow.post_file_change, &mut state)?;
    
    // Extract flow-emitted messages (empty if flow didn't add any)
    let messages = state.take_messages();
    
    // Map FlowResult to Decision
    let decision = match flow_result {
        FlowResult::Success => Decision::Allow,
        FlowResult::FailedContinue(_) => Decision::Allow,
        FlowResult::FailedStop(_) => Decision::Allow,
        FlowResult::FailedBlock(msg) => Decision::Block(msg),
    };
    
    Ok(HookResponse {
        context: state.build_context().ok(),
        decision,
        messages,  // Empty if flow didn't emit any - that's fine!
    })
}
```

**Generic pattern for all 6 event handlers:**
```rust
pub fn handle_x(event: XEvent) -> Result<HookResponse> {
    // 1. Execute flow
    let (flow_result, _) = FlowEngine::execute_actions(&actions, &mut state)?;
    
    // 2. Extract flow messages (may be empty)
    let messages = state.take_messages();
    
    // 3. Map FlowResult to Decision
    let decision = match flow_result {
        FlowResult::Success => Decision::Allow,
        FlowResult::FailedContinue(_) => Decision::Allow,
        FlowResult::FailedStop(_) => Decision::Allow,
        FlowResult::FailedBlock(msg) => Decision::Block(msg),
    };
    
    // 4. Return response (messages may be empty)
    Ok(HookResponse {
        context: state.build_context().ok(),
        decision,
        messages,
    })
}
```

## Message Routing by Integration

Messages in `HookResponse.messages` are routed differently depending on the integration:

### ACP Proxy (`cli/src/commands/acp.rs`)
```rust
// PrePrompt: Messages prepended to prompt (visible to agent)
let formatted_messages = response.format_messages();
if !formatted_messages.is_empty() {
    final_prompt = format!("{}\n\n{}", formatted_messages, original_text);
}

// Also emitted to stderr (visible to user)
for msg in &response.messages {
    match msg {
        Message::Info(s) => eprintln!("[aiki] ℹ️  {}", s),
        Message::Warning(s) => eprintln!("[aiki] ⚠️  {}", s),
        Message::Error(s) => eprintln!("[aiki] ❌ {}", s),
    }
}
```

**Result:** Both user and agent see messages (transparent protocol)

### Claude Code Hooks (`cli/src/vendors/claude_code.rs`)
```rust
// Messages shown in UI notification
json!({
    "systemMessage": response.format_messages(),
    "hookSpecificOutput": {
        "additionalContext": context  // Separate agent-only context
    }
})
```

**Result:** Messages shown to user in UI, context sent to agent separately

### Cursor Hooks (`cli/src/vendors/cursor.rs`)
```rust
// Messages shown to user
json!({
    "user_message": response.format_messages()
})

// Separate agent_message field available for agent-only content
json!({
    "agent_message": context
})
```

**Result:** Messages shown to user, separate channel for agent

## Benefits

### 1. Concise Syntax
```yaml
# ✅ Clean
- info: "✅ Done"

# ❌ Verbose (not used)
- message:
    type: info
    text: "✅ Done"
```

### 2. Type-Safe
Parser ensures only valid message types (info, warning, error)

### 3. Self-Documenting
Intent is clear from action name:
- `info:` = Success/completion notification
- `warning:` = Non-critical issue
- `error:` = Failure/validation error

### 4. Variable Interpolation
```yaml
- info: "Processed ${event.file_count} files in ${duration}ms"
```

### 5. Composable
```yaml
- warning: "Some tests skipped"
- autoreply: "Fix the skipped tests"  # Both appear in response
```

### 6. Integration-Agnostic
Flows emit messages without knowing the integration. Each integration routes messages appropriately:
- **ACP:** Messages visible to both user (stderr) and agent (prepended to prompt)
- **Vendor hooks:** Messages shown to user (UI), context sent to agent separately

### 7. No Backwards Compatibility Needed
Flows control their own messages. If a flow doesn't emit messages, the handler returns empty messages - simple and clean.

## Edge Cases

### 1. Empty Message Text
```yaml
- info: ""
```
**Behavior:** Creates empty Message::Info(""), allowed but not useful

### 2. Message in Conditional
```yaml
- if: "$test_result.exit_code != 0"
  then:
    - warning: "Tests failed"
  else:
    - info: "Tests passed"
```
**Behavior:** Only the executed branch's message is added

### 3. Message with on_failure
```yaml
- info: "Starting validation..."
  on_failure: continue
```
**Behavior:** Info/warning/error actions always succeed (exit_code=0), so `on_failure` only matters if message creation throws exception

### 4. Multiple Messages of Same Type
```yaml
- info: "Step 1 complete"
- info: "Step 2 complete"
- info: "Step 3 complete"
```
**Behavior:** All three messages added to `HookResponse.messages` array

### 5. Flow Without Messages
```yaml
PostFileChange:
  - jj: "describe ..."
```
**Behavior:** Handler returns empty messages - no user feedback shown (flows decide what to show)

## Parser Considerations

The `#[serde(untagged)]` enum tries to parse in order. To avoid ambiguity:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    // Complex actions first (distinguishable by required fields)
    If(IfAction),          // Has "if" + "then"
    Switch(SwitchAction),  // Has "switch" + "cases"
    Let(LetAction),        // Has "let"
    Self_(SelfAction),     // Has "self"
    
    // Single-key actions (order matters)
    Shell(ShellAction),    
    Jj(JjAction),          
    Log(LogAction),        
    Context(ContextAction),
    Autoreply(AutoreplyAction),
    CommitMessage(CommitMessageAction),
    
    // Message actions last
    Info(InfoAction),      
    Warning(WarningAction),
    Error(ErrorAction),    
}
```

**Test case:**
```yaml
- info: "test message"     # → InfoAction
- shell: "echo info"       # → ShellAction (matched first)
```

## Implementation Checklist

- [ ] Add `InfoAction`, `WarningAction`, `ErrorAction` to types.rs
- [ ] Add to `Action` enum (untagged, correct order)
- [ ] Add `messages: Vec<Message>` field to AikiState
- [ ] Add `add_message()`, `take_messages()`, `messages()` to AikiState
- [ ] Add `execute_info()`, `execute_warning()`, `execute_error()` to engine.rs
- [ ] Update `execute_action()` match to handle message actions
- [ ] Update `store_action_result()` to handle message actions (no-op)
- [ ] Update all 6 handlers to use simple pattern (extract messages, map FlowResult, return)
- [ ] Remove hardcoded message generation from handlers
- [ ] Add tests for message accumulation
- [ ] Add tests for parser ordering
- [ ] Update core flow to use message helpers
- [ ] Update documentation

## Testing

### Unit Tests (engine.rs)
```rust
#[test]
fn test_execute_info_message() {
    let action = InfoAction {
        info: "Test info".to_string(),
        on_failure: FailureMode::Continue,
    };
    let mut state = AikiState::new(/* ... */);
    
    let result = FlowEngine::execute_info(&action, &mut state).unwrap();
    
    assert!(result.success);
    assert_eq!(state.messages().len(), 1);
    assert!(matches!(state.messages()[0], Message::Info(_)));
}

#[test]
fn test_multiple_messages_accumulate() {
    let actions = vec![
        Action::Info(InfoAction { info: "Info 1".into(), on_failure: FailureMode::Continue }),
        Action::Warning(WarningAction { warning: "Warning 1".into(), on_failure: FailureMode::Continue }),
        Action::Error(ErrorAction { error: "Error 1".into(), on_failure: FailureMode::Continue }),
    ];
    let mut state = AikiState::new(/* ... */);
    
    FlowEngine::execute_actions(&actions, &mut state).unwrap();
    
    assert_eq!(state.messages().len(), 3);
}

#[test]
fn test_message_variable_interpolation() {
    let action = InfoAction {
        info: "Processed ${count} files".to_string(),
        on_failure: FailureMode::Continue,
    };
    let mut state = AikiState::new(/* ... */);
    state.store_action_result("count".into(), ActionResult::success_with_stdout("5"));
    
    FlowEngine::execute_info(&action, &mut state).unwrap();
    
    let messages = state.take_messages();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Info(text) => assert_eq!(text, "Processed 5 files"),
        _ => panic!("Expected Info message"),
    }
}
```

### Integration Tests (handlers.rs)
```rust
#[test]
fn test_handler_uses_flow_messages() {
    let event = AikiPostFileChangeEvent { /* ... */ };
    // Flow with: - info: "Custom message"
    
    let response = handle_post_file_change(event).unwrap();
    
    assert_eq!(response.messages.len(), 1);
    assert!(matches!(response.messages[0], Message::Info(ref s) if s.contains("Custom")));
}

#[test]
fn test_handler_with_no_flow_messages() {
    let event = AikiPostFileChangeEvent { /* ... */ };
    // Flow without any info/warning/error actions
    
    let response = handle_post_file_change(event).unwrap();
    
    // Should have empty messages
    assert_eq!(response.messages.len(), 0);
}
```

### Parser Tests (types.rs)
```rust
#[test]
fn test_parse_info_action() {
    let yaml = "- info: \"Test message\"";
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(actions[0], Action::Info(_)));
}

#[test]
fn test_parse_disambiguates_shell_vs_info() {
    let yaml = r#"
    - shell: "info"
    - info: "test"
    "#;
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(actions[0], Action::Shell(_)));
    assert!(matches!(actions[1], Action::Info(_)));
}
```

## Migration Example

### Before
```yaml
# .aiki/flows/aiki/core.yml
PostFileChange:
  - let: metadata = self.build_metadata
  - jj: "describe -m '[aiki]$metadata'"
```

```rust
// handlers.rs - Hardcoded
FlowResult::Success => HookResponse::success_with_message(
    format!("✅ Provenance recorded for {} files", event.file_paths.len())
),
```

### After
```yaml
# .aiki/flows/aiki/core.yml
PostFileChange:
  - let: metadata = self.build_metadata
  - jj: "describe -m '[aiki]$metadata'"
  - info: "✅ Provenance recorded for ${event.file_count} files"
```

```rust
// handlers.rs - Generic
let messages = state.take_messages();
Ok(HookResponse {
    decision: Decision::Allow,
    context: None,
    messages,  // ← From flow!
})
```

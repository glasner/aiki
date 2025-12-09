# Plan: Add PreChange hook to stash user edits + Make if conditions truthy for non-empty strings

> **Note:** After implementation, `PreChange` and `PostFileChange` were renamed to `PreChange` and `PostFileChange` for better clarity. See [rename-to-prefilechange-postfilechange.md](rename-to-prefilechange-postfilechange.md) for details.

## Goal

1. Give the AI a clean working copy by stashing any user changes made between previous PostFileChange and current PreChange
2. Make `if:` conditions treat non-empty strings as truthy (more intuitive)

## Implementation

### 1. Update condition evaluation in executor.rs

Change the "no operator" case in `evaluate_condition()` from:

```rust
// Current: only "true" string is truthy
Ok(val == "true")
```

To:

```rust
// New: non-empty strings are truthy, empty string and "false" are falsy
Ok(!val.is_empty() && val != "false")
```

**Location:** `cli/src/flows/executor.rs` line ~703

### 2. Update/add tests for new truthy behavior

Add tests to verify:
- Empty string evaluates to false
- Non-empty string evaluates to true
- "false" literal evaluates to false
- "true" still evaluates to true

**Location:** `cli/src/flows/executor.rs` in tests module

### 3. Add PreChange event handler to flow.yaml

```yaml
PreChange:
  # Check if working copy has any changes
  - jj: diff -r @ --name-only
    alias: changed_files
  
  # If there are changes (non-empty string), stash them before AI starts
  - if: $changed_files
    then:
      - jj: describe --message "User changes (pre-AI edit)" --reset-author
      - jj: new  # Give AI a clean working copy
```

**Location:** `cli/src/flows/core/flow.yaml`

## How it works

### PreChange Flow

1. `jj diff -r @ --name-only` returns:
   - Empty string if working copy is clean
   - List of filenames (newline-separated) if there are changes

2. Store output in `$changed_files` variable

3. `if: $changed_files` evaluates to:
   - `false` when empty string (no changes)
   - `true` when non-empty (has changes)

4. When true:
   - Describe current change with "User changes (pre-AI edit)" message
   - Use `--reset-author` to set author to git user (not AI)
   - Create new empty change with `jj new` for AI to work in

### Timeline

```
After previous PostFileChange:
  @ (empty change from jj new)

User makes edits:
  @ (user changes in working copy)

PreChange fires:
  - Detects changes via jj diff
  - Describes as "User changes (pre-AI edit)" with user as author
  - Creates new empty change
  
Result:
  @ (empty change) ← AI will work here
  @- (user changes) ← User's work saved here

AI makes edits:
  @ (AI changes only, no user changes mixed in)

PostFileChange:
  - classify_edits now simpler (no pre-AI user changes to worry about)
  - Only detects changes made during/after AI's work
```

## Benefits

### Cleaner syntax
- `if: $variable` instead of `if: $variable != ""`
- Follows common programming language conventions
- More readable flows

### Intuitive behavior
- Empty strings are falsy (like JavaScript, Python, etc.)
- Non-empty strings are truthy
- Explicit "false" string is still falsy

### Backward compatible
- Existing `if: $var == "true"` still works
- No breaking changes to existing flows
- Only affects bare variable checks `if: $var`

### Simpler flows
- PreChange stashing is just 4 lines of YAML
- No need for string comparison operators for emptiness checks

### Better separation
- User's pre-AI changes are saved separately
- Clear attribution (user changes have user as author)
- AI gets clean working copy to start from
- Simplifies PostFileChange detection logic

## Files modified

1. `cli/src/flows/executor.rs`
   - Update `evaluate_condition()` method (line ~703)
   - Add tests for truthy/falsy behavior
   - Add PreChange event handling in `create_resolver()` method

2. `cli/src/flows/core/flow.yaml`
   - Add PreChange event handler

3. `cli/src/flows/types.rs`
   - Add `pre_change: Vec<Action>` field to `Flow` struct
   - Add serde rename annotation `#[serde(rename = "PreChange", default)]`

4. `cli/src/events.rs`
   - Add `AikiPreChangeEvent` struct
   - Add `PreChange(AikiPreChangeEvent)` variant to `AikiEvent` enum
   - Add event helper methods (`cwd()`, `agent_type()`)
   - Add `From<AikiPreChangeEvent> for AikiEvent` implementation

5. `cli/src/event_bus.rs` (if exists) or `cli/src/commands/acp.rs`
   - Add PreChange event emission/handling
   - Trigger PreChange before AI starts editing (in ACP proxy or hooks)

## Testing

### Unit tests for truthy evaluation
```rust
#[test]
fn test_if_condition_truthy_values() {
    let mut context = AikiState::new(create_test_event());
    
    // Empty string is falsy
    context.store_action_result("empty".to_string(), ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "".to_string(),
        stderr: String::new(),
    });
    assert!(!FlowExecutor::evaluate_condition("$empty", &context).unwrap());
    
    // Non-empty string is truthy
    context.store_action_result("nonempty".to_string(), ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "some content".to_string(),
        stderr: String::new(),
    });
    assert!(FlowExecutor::evaluate_condition("$nonempty", &context).unwrap());
    
    // "false" literal is falsy
    context.store_action_result("false_str".to_string(), ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "false".to_string(),
        stderr: String::new(),
    });
    assert!(!FlowExecutor::evaluate_condition("$false_str", &context).unwrap());
    
    // "true" literal is truthy
    context.store_action_result("true_str".to_string(), ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: "true".to_string(),
        stderr: String::new(),
    });
    assert!(FlowExecutor::evaluate_condition("$true_str", &context).unwrap());
}
```

### Integration test
Create a test that:
1. Starts with clean working copy
2. Creates user changes in working copy
3. Triggers PreChange
4. Verifies user change was created and described
5. Verifies new empty change exists for AI

## Potential Issues

### None expected
- Change is minimal and well-scoped
- Backward compatible with existing flows
- Intuitive behavior matches common programming languages
- PreChange hook uses only existing jj commands

## Future Enhancements

1. Could add support for "0" as falsy (like some languages)
2. Could add `||` and `&&` operators for boolean logic
3. Could add negation operator `!$variable`

These are not needed for current use case but could be added later if needed.

## Detailed Implementation Steps

### Step 4: Add PreChange Event Type

**File:** `cli/src/events.rs`

Add the PreChange event struct after `AikiStartEvent`:

```rust
/// Pre-change event (before file modification starts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPreChangeEvent {
    pub agent_type: AgentType,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}
```

Add the variant to the `AikiEvent` enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AikiEvent {
    /// Session initialization (maps to SessionStart, beforeSubmitPrompt)
    SessionStart(AikiStartEvent),
    /// Before file modification starts (pre-AI edit)
    PreChange(AikiPreChangeEvent),
    /// After file modification (maps to PostToolUse, afterFileEdit)
    PostFileChange(AikiPostFileChangeEvent),
    /// Prepare commit message (Git's prepare-commit-msg hook)
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
}
```

Update the helper methods to include PreChange:

```rust
impl AikiEvent {
    #[must_use]
    pub fn cwd(&self) -> &Path {
        match self {
            Self::SessionStart(e) => &e.cwd,
            Self::PreChange(e) => &e.cwd,
            Self::PostFileChange(e) => &e.cwd,
            Self::PrepareCommitMessage(e) => &e.cwd,
        }
    }

    #[must_use]
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::SessionStart(e) => e.agent_type,
            Self::PreChange(e) => e.agent_type,
            Self::PostFileChange(e) => e.agent_type,
            Self::PrepareCommitMessage(e) => e.agent_type,
        }
    }
}
```

Add the `From` implementation:

```rust
impl From<AikiPreChangeEvent> for AikiEvent {
    fn from(event: AikiPreChangeEvent) -> Self {
        AikiEvent::PreChange(event)
    }
}
```

### Step 5: Add PreChange to Flow Definition

**File:** `cli/src/flows/types.rs`

Add the field to the `Flow` struct (maintain alphabetical order by event name):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    /// Flow name
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Flow version
    #[serde(default = "default_version")]
    pub version: String,

    /// PostFileChange event handler
    #[serde(rename = "PostFileChange", default)]
    pub post_change: Vec<Action>,

    /// PreChange event handler
    #[serde(rename = "PreChange", default)]
    pub pre_change: Vec<Action>,

    /// PrepareCommitMessage event handler (Git's prepare-commit-msg hook)
    #[serde(rename = "PrepareCommitMessage", default)]
    pub prepare_commit_message: Vec<Action>,

    /// SessionStart event handler
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<Action>,

    /// Stop event handler
    #[serde(rename = "Stop", default)]
    pub stop: Vec<Action>,
}
```

### Step 6: Add PreChange Handling in Executor

**File:** `cli/src/flows/executor.rs`

Add PreChange case in the `create_resolver()` method (around line 62):

```rust
fn create_resolver(context: &AikiState) -> VariableResolver {
    let mut resolver = VariableResolver::new();

    // Add event-specific variables based on event type
    match &context.event {
        crate::events::AikiEvent::PostFileChange(e) => {
            resolver.add_var("event.tool_name".to_string(), e.tool_name.clone());
            resolver.add_var("event.file_paths".to_string(), e.file_paths.join(" "));
            resolver.add_var(
                "event.file_count".to_string(),
                e.file_paths.len().to_string(),
            );
            resolver.add_var("event.session_id".to_string(), e.session_id.clone());
        }
        crate::events::AikiEvent::PreChange(e) => {
            if let Some(ref session_id) = e.session_id {
                resolver.add_var("event.session_id".to_string(), session_id.clone());
            }
        }
        crate::events::AikiEvent::SessionStart(e) => {
            if let Some(ref session_id) = e.session_id {
                resolver.add_var("event.session_id".to_string(), session_id.clone());
            }
        }
        crate::events::AikiEvent::PrepareCommitMessage(e) => {
            // Add commit message file path if available
            if let Some(ref path) = e.commit_msg_file {
                resolver.add_var(
                    "event.commit_msg_file".to_string(),
                    path.display().to_string(),
                );
            }
        }
    }
    
    // ... rest of method unchanged
}
```

### Step 7: Wire Up PreChange Event Emission

**Files:** `cli/src/event_bus.rs` and/or flow loading code

Find where flows are executed (likely in `event_bus.rs` or similar) and ensure PreChange handlers are invoked:

```rust
// Example pseudocode - actual implementation depends on event_bus architecture
pub fn emit_pre_change_event(event: AikiPreChangeEvent) -> Result<()> {
    let flow = load_flow("aiki/core")?;
    
    if !flow.pre_change.is_empty() {
        let mut state = AikiState::new(event.clone().into());
        let (result, _timing) = FlowExecutor::execute_actions(&flow.pre_change, &mut state)?;
        
        match result {
            FlowResult::Success => {},
            FlowResult::FailedStop(msg) => {
                eprintln!("[aiki] PreChange stopped: {}", msg);
            },
            FlowResult::FailedBlock(msg) => {
                return Err(AikiError::ActionFailed(msg));
            },
            FlowResult::FailedContinue(msg) => {
                eprintln!("[aiki] PreChange had errors but continued: {}", msg);
            },
        }
    }
    
    Ok(())
}
```

### Step 8: Trigger PreChange in ACP Proxy

**File:** `cli/src/commands/acp.rs`

#### ACP Protocol Flow

The ACP proxy observes `session/update` notifications from the agent. These notifications contain:
1. **ToolCall** - Initial tool call with status (Pending/InProgress/Completed/Failed)
2. **ToolCallUpdate** - Updates to tool call status and fields

**Key insight from ACP protocol:** `ToolCallStatus::Pending` means "the tool call hasn't started running yet because the input is either streaming or **we're awaiting approval**" (from agent-client-protocol-schema).

This is the official pre-execution blocking point in ACP! The `session/request_permission` flow happens during the `Pending` state.

For PreChange, we should:
- Detect when a tool call has `status = Pending` 
- Check if it's a file-modifying tool (Edit, Delete, Move)
- Fire PreChange **before** the tool transitions to InProgress/Completed

#### Implementation: Add PreChange Detection

**Location:** In `process_tool_call()` function (around line 506)

Add PreChange firing when tool call starts:

```rust
fn process_tool_call(
    session_id: &str,
    tool_call: &ToolCall,
    agent_type: &AgentType,
    client_name: &Option<String>,
    client_version: &Option<String>,
    agent_version: &Option<String>,
    cwd: &Option<PathBuf>,
    tool_call_contexts: &mut HashMap<ToolCallId, ToolCallContext>,
) -> Result<()> {
    let context = ToolCallContext {
        kind: tool_call.kind,
        paths: paths_from_locations(&tool_call.locations),
        content: tool_call.content.clone(),
    };

    let status = tool_call.status;

    // NEW: Fire PreChange when file-modifying tool starts
    if status == ToolCallStatus::Pending && is_file_modifying_tool(tool_call.kind) {
        fire_pre_change_event(
            session_id,
            agent_type,
            client_name,
            client_version,
            agent_version,
            cwd,
        )?;
    }

    // Store context for potential updates
    tool_call_contexts.insert(tool_call.id.clone(), context.clone());
    if matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) {
        tool_call_contexts.remove(&tool_call.id);
    }

    // Existing: Fire PostFileChange when completed
    if status == ToolCallStatus::Completed {
        record_post_change_events(
            session_id,
            agent_type,
            client_name,
            client_version,
            agent_version,
            cwd,
            context,
        )?;
    }

    Ok(())
}

/// Check if a tool kind modifies files
fn is_file_modifying_tool(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Edit | ToolKind::Delete | ToolKind::Move)
}

/// Fire PreChange event before tool execution
fn fire_pre_change_event(
    session_id: &str,
    agent_type: &AgentType,
    client_name: &Option<String>,
    client_version: &Option<String>,
    agent_version: &Option<String>,
    cwd: &Option<PathBuf>,
) -> Result<()> {
    // Get working directory (required)
    let working_dir = cwd
        .as_ref()
        .ok_or_else(|| AikiError::Other(anyhow::anyhow!("Working directory not available")))?
        .clone();

    // Create PreChange event
    let event = AikiEvent::PreChange(crate::events::AikiPreChangeEvent {
        agent_type: *agent_type,
        session_id: Some(session_id.to_string()),
        cwd: working_dir,
        timestamp: chrono::Utc::now(),
    });

    // Dispatch to event bus (non-blocking - errors are logged but don't fail the proxy)
    if let Err(e) = event_bus::dispatch(event) {
        eprintln!("[aiki] PreChange event dispatch failed: {}", e);
    }

    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("[acp] Fired PreChange event for session {}", session_id);
    }

    Ok(())
}
```

#### Alternative: Fire PreChange on First Update

If `ToolCall` with `Pending` status isn't reliable, we can also fire PreChange in `process_tool_call_update()` when we first see a file-modifying tool:

```rust
fn process_tool_call_update(
    session_id: &str,
    tool_call: &ToolCallUpdate,
    agent_type: &AgentType,
    client_name: &Option<String>,
    client_version: &Option<String>,
    agent_version: &Option<String>,
    cwd: &Option<PathBuf>,
    tool_call_contexts: &mut HashMap<ToolCallId, ToolCallContext>,
) -> Result<()> {
    let entry = tool_call_contexts
        .entry(tool_call.id.clone())
        .or_insert_with(|| ToolCallContext {
            kind: tool_call.fields.kind.unwrap_or(ToolKind::Other),
            paths: Vec::new(),
            content: Vec::new(),
        });

    // NEW: Fire PreChange when we first see a file-modifying tool
    let is_new_entry = entry.paths.is_empty();
    
    if let Some(kind) = tool_call.fields.kind {
        entry.kind = kind;
    }

    if let Some(locations) = &tool_call.fields.locations {
        entry.paths = paths_from_locations(locations);
    }

    if let Some(content) = &tool_call.fields.content {
        entry.content = content.clone();
    }

    // Fire PreChange if this is a new file-modifying tool
    if is_new_entry && !entry.paths.is_empty() && is_file_modifying_tool(entry.kind) {
        fire_pre_change_event(
            session_id,
            agent_type,
            client_name,
            client_version,
            agent_version,
            cwd,
        )?;
    }

    // Rest of existing logic...
    let status = tool_call.fields.status;
    let should_record =
        matches!(status, Some(ToolCallStatus::Completed)) && !entry.paths.is_empty();
    let context = if should_record {
        Some(entry.clone())
    } else {
        None
    };

    if matches!(
        status,
        Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
    ) {
        tool_call_contexts.remove(&tool_call.id);
    }

    if let Some(context) = context {
        record_post_change_events(
            session_id,
            agent_type,
            client_name,
            client_version,
            agent_version,
            cwd,
            context,
        )?;
    }

    Ok(())
}
```

#### Notes

- PreChange fires once per file-modifying tool call
- Errors in PreChange don't block the tool execution (non-blocking)
- Debug logging helps trace when PreChange fires
- Use `ToolCallStatus::Pending` if reliable, otherwise fire on first update with paths

### Step 9: Trigger PreChange in Hooks

**File:** `cli/src/vendors/claude_code.rs` and `cli/src/vendors/cursor.rs`

#### Hook Events That Fire PreChange

**Claude Code:**
- **PreToolUse** - Fires before any tool executes
  - Fire PreChange for file-modifying tools (Edit, Write, etc.)
  - Payload includes: tool_name, tool_input, session_id, cwd

**Cursor:**
- **beforeMCPExecution** - Fires before MCP tools execute
  - Assumption: File edits go through MCP tools
  - Fire PreChange for file operations
  - Exact payload structure TBD (will determine parameters later)

#### Implementation

**Claude Code** (`cli/src/vendors/claude_code.rs`):

```rust
pub fn handle(event_name: &str) -> Result<()> {
    let payload: ClaudeCodePayload = super::read_stdin_json()?;

    let event = match event_name {
        "SessionStart" => {
            // ... existing SessionStart implementation ...
        }
        "PreToolUse" => {
            // NEW: Fire PreChange for file-modifying tools
            let tool_name = payload.tool_name.as_str();
            
            // Only fire PreChange for file-modifying tools
            if is_file_modifying_tool(tool_name) {
                AikiEvent::PreChange(AikiPreChangeEvent {
                    agent_type: AgentType::Claude,
                    session_id: Some(payload.session_id),
                    cwd: PathBuf::from(&payload.cwd),
                    timestamp: chrono::Utc::now(),
                })
            } else {
                // Non-file tools (Bash, Read, etc.) - no PreChange needed
                if std::env::var("AIKI_DEBUG").is_ok() {
                    eprintln!("[aiki] PreToolUse: Ignoring non-file tool: {}", tool_name);
                }
                return Ok(());
            }
        }
        "PostToolUse" => {
            // ... existing PostToolUse implementation ...
        }
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Claude Code event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Dispatch to event bus and get generic response
    let response = event_bus::dispatch(event)?;

    // Translate to Claude Code JSON format
    let (json_output, exit_code) = translate_response(response, event_name);

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with appropriate code
    std::process::exit(exit_code);
}

/// Check if a tool modifies files
fn is_file_modifying_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}
```

**Cursor** (`cli/src/vendors/cursor.rs`):

```rust
pub fn handle(event_name: &str) -> Result<()> {
    let payload: CursorPayload = super::read_stdin_json()?;

    let event = match event_name {
        "beforeSubmitPrompt" => {
            // ... existing beforeSubmitPrompt implementation ...
        }
        "beforeMCPExecution" => {
            // NEW: Fire PreChange before MCP tool execution
            // Assumption: File edits go through MCP tools
            // TODO: Determine exact conditions for file operations once we see real payloads
            
            AikiEvent::PreChange(AikiPreChangeEvent {
                agent_type: AgentType::Cursor,
                session_id: Some(payload.session_id),
                cwd: PathBuf::from(&payload.working_directory),
                timestamp: chrono::Utc::now(),
            })
        }
        "afterFileEdit" => {
            // ... existing afterFileEdit implementation ...
        }
        _ => {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[aiki] Ignoring unknown Cursor event: {}", event_name);
            }
            return Ok(());
        }
    };

    // Dispatch to event bus and get generic response
    let response = event_bus::dispatch(event)?;

    // Translate to Cursor JSON format
    let (json_output, exit_code) = translate_response(response);

    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }

    // Exit with appropriate code
    std::process::exit(exit_code);
}
```

## Implementation Checklist

- [ ] Update `evaluate_condition()` for truthy strings
- [ ] Add tests for truthy/falsy behavior
- [ ] Add `AikiPreChangeEvent` struct to `events.rs`
- [ ] Add `PreChange` variant to `AikiEvent` enum
- [ ] Update `cwd()` and `agent_type()` helper methods
- [ ] Add `From<AikiPreChangeEvent>` implementation
- [ ] Add `pre_change` field to `Flow` struct in `types.rs`
- [ ] Add PreChange case in `create_resolver()` in `executor.rs`
- [ ] Add PreChange handler to `flow.yaml`
- [ ] Wire up PreChange event emission in event bus
- [ ] Trigger PreChange in ACP proxy before tool execution
- [ ] Trigger PreChange in hooks (if needed)
- [ ] Add integration tests for PreChange flow

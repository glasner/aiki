# Phase 5.X: Event System Hardening with Automatic Response Translation

## Problem

Current event system has several gaps:
- Inconsistent naming for agent events (before/after vs pre/post)
- Handlers return generic `Result<()>` with no rich feedback capability
- Both Claude Code and Cursor support JSON responses but in different formats
- Flows/handlers shouldn't need to know about editor-specific response formats
- Need automatic translation layer between generic responses and editor-specific JSON

## Solution

Standardize agent event naming, add generic response system in new `hook_response` module, implement automatic translation layer.

### Key Design Principle

**Handlers and flows return generic, editor-agnostic responses. The event dispatcher automatically translates them to editor-specific JSON formats.**

## What We Build

### 1. Event Naming Standardization

**Important: Git hook names stay unchanged**

**Rename agent events only:**
- `Start` → `SessionStart` in `cli/src/events.rs`
- `PostChange` → Keep as-is (already uses Post prefix)
- `PrepareCommitMessage` → **Keep unchanged** (Git hook name)

**Update flow fields:**
- `start` → `session_start` in `cli/src/flows/types.rs`
- `prepare_commit_message` → **Keep unchanged**

### 2. Generic Response System (Editor-Agnostic)

**Add to existing module: `cli/src/handlers.rs`**

```rust
// cli/src/handlers.rs - Add to top of file

use serde_json::{json, Map, Value};

/// Generic hook response (editor-agnostic)
#[derive(Debug, Clone)]
pub struct HookResponse {
    /// Success or failure
    pub success: bool,
    
    /// Message shown to user in editor UI (optional)
    pub user_message: Option<String>,
    
    /// Message sent to AI agent (optional)
    pub agent_message: Option<String>,
    
    /// Metadata key-value pairs (optional)
    pub metadata: Vec<(String, String)>,
    
    /// Exit code override (optional, defaults based on success)
    pub exit_code: Option<i32>,
}

impl HookResponse {
    #[must_use]
    pub fn success() -> Self {
        Self {
            success: true,
            user_message: None,
            agent_message: None,
            metadata: Vec::new(),
            exit_code: None,
        }
    }
    
    #[must_use]
    pub fn success_with_message(user_msg: impl Into<String>) -> Self {
        Self {
            success: true,
            user_message: Some(user_msg.into()),
            agent_message: None,
            metadata: Vec::new(),
            exit_code: None,
        }
    }
    
    #[must_use]
    pub fn success_with_metadata(metadata: Vec<(String, String)>) -> Self {
        Self {
            success: true,
            user_message: None,
            agent_message: None,
            metadata,
            exit_code: None,
        }
    }
    
    #[must_use]
    pub fn failure(user_msg: impl Into<String>, agent_msg: Option<String>) -> Self {
        Self {
            success: false,
            user_message: Some(user_msg.into()),
            agent_message: agent_msg,
            metadata: Vec::new(),
            exit_code: Some(1),
        }
    }
    
    #[must_use]
    pub fn with_metadata(mut self, metadata: Vec<(String, String)>) -> Self {
        self.metadata = metadata;
        self
    }
    
    #[must_use]
    pub fn with_agent_message(mut self, msg: impl Into<String>) -> Self {
        self.agent_message = Some(msg.into());
        self
    }
}
```

### 3. Automatic Translation Layer

**Add to `cli/src/commands/event.rs`:**

```rust
// cli/src/commands/event.rs - Add translation functions

use crate::handlers::HookResponse;

#[derive(Debug, Clone, Copy)]
enum EditorType {
    ClaudeCode,
    Cursor,
    Unknown,
}

fn detect_editor() -> EditorType {
    // Detect from environment variables
    if env::var("CLAUDE_SESSION_ID").is_ok() {
        EditorType::ClaudeCode
    } else if env::var("CURSOR_SESSION_ID").is_ok() {
        EditorType::Cursor
    } else {
        EditorType::Unknown
    }
}

fn translate_response(response: HookResponse, editor: EditorType) -> (Option<String>, i32) {
    let exit_code = response.exit_code.unwrap_or(if response.success { 0 } else { 1 });
    
    match editor {
        EditorType::ClaudeCode => translate_claude(response, exit_code),
        EditorType::Cursor => translate_cursor(response, exit_code),
        EditorType::Unknown => translate_generic(response, exit_code),
    }
}

fn translate_claude(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    let mut json = Map::new();
    
    if let Some(msg) = response.user_message {
        json.insert("userMessage".to_string(), json!(msg));
    }
    
    if let Some(msg) = response.agent_message {
        json.insert("agentMessage".to_string(), json!(msg));
    }
    
    if !response.metadata.is_empty() {
        let metadata: Vec<Vec<String>> = response.metadata
            .into_iter()
            .map(|(k, v)| vec![k, v])
            .collect();
        json.insert("metadata".to_string(), json!(metadata));
    }
    
    if json.is_empty() {
        (None, exit_code)
    } else {
        (Some(serde_json::to_string(&json).unwrap()), exit_code)
    }
}

fn translate_cursor(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    let mut json = Map::new();
    
    if let Some(msg) = response.user_message {
        json.insert("user_message".to_string(), json!(msg));
    }
    
    if let Some(msg) = response.agent_message {
        json.insert("agent_message".to_string(), json!(msg));
    }
    
    if !response.metadata.is_empty() {
        let metadata: Map<String, Value> = response.metadata
            .into_iter()
            .map(|(k, v)| (k, json!(v)))
            .collect();
        json.insert("metadata".to_string(), json!(metadata));
    }
    
    if json.is_empty() {
        (None, exit_code)
    } else {
        (Some(serde_json::to_string(&json).unwrap()), exit_code)
    }
}

fn translate_generic(response: HookResponse, exit_code: i32) -> (Option<String>, i32) {
    // For unknown editors, log to stderr
    if let Some(msg) = response.user_message {
        eprintln!("[aiki] {}", msg);
    }
    (None, exit_code)
}

pub fn run_prepare_commit_message() -> Result<()> {
    let event = AikiPrepareCommitMessageEvent { /* ... */ };
    
    // Get generic response from handler
    let response = event_bus::dispatch(AikiEvent::PrepareCommitMessage(event))?;
    
    // Detect editor and translate
    let editor = detect_editor();
    let (json_output, exit_code) = translate_response(response, editor);
    
    // Output JSON if present
    if let Some(json) = json_output {
        println!("{}", json);
    }
    
    // Exit with code
    std::process::exit(exit_code);
}
```

### 4. Update Handlers to Use Generic Responses

```rust
// cli/src/handlers.rs

// HookResponse is defined at top of this file

pub fn handle_prepare_commit_message(event: AikiPrepareCommitMessageEvent) -> Result<HookResponse> {
    let core_flow = load_core_flow()?;
    let mut state = AikiState::new(event);
    state.flow_name = Some("aiki/core".to_string());
    
    match FlowExecutor::execute_actions(&core_flow.prepare_commit_message, &mut state) {
        Ok(_) => {
            Ok(HookResponse::success_with_message("✅ Co-authors added")
                .with_metadata(vec![
                    ("aiki_version".to_string(), env!("CARGO_PKG_VERSION").to_string()),
                    ("flow".to_string(), "aiki/core".to_string()),
                ]))
        }
        Err(e) => {
            Ok(HookResponse::failure(
                format!("⚠️ Failed to add co-authors: {}", e),
                Some(format!("Co-author attribution failed. Commit proceeds without AI attribution."))
            ))
        }
    }
}

pub fn handle_post_change(event: AikiPostChangeEvent) -> Result<HookResponse> {
    let core_flow = load_core_flow()?;
    let mut state = AikiState::new(event.clone());
    state.flow_name = Some("aiki/core".to_string());
    
    match FlowExecutor::execute_actions(&core_flow.post_change, &mut state) {
        Ok(_) => {
            Ok(HookResponse::success_with_message(
                format!("✅ Provenance recorded for {}", event.file_path)
            ))
        }
        Err(e) => {
            Ok(HookResponse::failure(
                format!("⚠️ Provenance recording failed: {}", e),
                Some("Provenance tracking failed. Changes saved but not attributed.".to_string())
            ))
        }
    }
}

pub fn handle_start(event: AikiStartEvent) -> Result<HookResponse> {
    let core_flow = load_core_flow()?;
    let mut state = AikiState::new(event);
    state.flow_name = Some("aiki/core".to_string());
    
    FlowExecutor::execute_actions(&core_flow.start, &mut state)?;
    
    Ok(HookResponse::success()
        .with_metadata(vec![
            ("session_initialized".to_string(), "true".to_string()),
            ("aiki_version".to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ]))
}
```

### 5. Update Event Bus

```rust
// cli/src/event_bus.rs

use crate::handlers::HookResponse;

pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    match event {
        AikiEvent::Start(e) => handlers::handle_start(e),
        AikiEvent::PostChange(e) => handlers::handle_post_change(e),
        AikiEvent::PrepareCommitMessage(e) => handlers::handle_prepare_commit_message(e),
    }
}
```

## Architecture

```
Handler → HookResponse (generic) → Translation Layer → Editor-Specific JSON + Exit Code
```

**Flow:**
1. Handler returns `HookResponse` with generic fields (success, user_message, agent_message, metadata)
2. Event dispatcher detects editor type (Claude Code, Cursor, Unknown)
3. Translation function converts to editor-specific JSON format
4. JSON printed to stdout (if present), process exits with code

## Response Translation Examples

| Generic Response | Claude Code JSON | Cursor JSON |
|-----------------|------------------|-------------|
| `success()` | *(no JSON, exit 0)* | *(no JSON, exit 0)* |
| `success_with_message("✅ Done")` | `{"userMessage": "✅ Done"}` | `{"user_message": "✅ Done"}` |
| `failure("Error", Some("Context"))` | `{"userMessage": "Error", "agentMessage": "Context"}` | `{"user_message": "Error", "agent_message": "Context"}` |
| `success().with_metadata([("k","v")])` | `{"metadata": [["k","v"]]}` | `{"metadata": {"k":"v"}}` |

## Files Modified

1. **`cli/src/handlers.rs`** - Add `HookResponse` struct, return `HookResponse` from handlers
2. **`cli/src/events.rs`** - Rename `Start` → `SessionStart`
3. **`cli/src/commands/event.rs`** - Add translation layer (detect_editor, translate_response, etc.)
4. **`cli/src/event_bus.rs`** - Change return type to `Result<HookResponse>`
5. **`cli/src/flows/types.rs`** - Rename `start` field → `session_start`
6. **`cli/src/flows/core/flow.yaml`** - Update `Start:` → `SessionStart:`
7. **`ops/ROADMAP.md`** - Add Phase 5.X milestone

## Implementation Steps

1. **Add HookResponse to handlers.rs** - Add struct definition and builder methods to top of `cli/src/handlers.rs`
2. **Rename events** - `Start` → `SessionStart` in `cli/src/events.rs`
3. **Add translation layer** - Functions in `cli/src/commands/event.rs`
4. **Update handlers** - Return `HookResponse` in `cli/src/handlers.rs`
5. **Update event_bus** - Return `HookResponse` in `cli/src/event_bus.rs`
6. **Update flow types** - Rename `start` → `session_start` in `cli/src/flows/types.rs`
7. **Update core flow** - Rename `Start:` → `SessionStart:` in `cli/src/flows/core/flow.yaml`
8. **Add milestone** - Update `ops/ROADMAP.md` with Phase 5.X
9. **Test** - Verify translations work with both Claude Code and Cursor
10. **Document** - Add examples and translation guide

## Success Criteria

- ✅ Agent events use Pre/Post naming (`SessionStart`, `PostChange`)
- ✅ Git hooks keep official names (`PrepareCommitMessage` unchanged)
- ✅ `HookResponse` defined in `handlers.rs` (keeps response types with handler logic)
- ✅ Handlers return generic `HookResponse` (no editor knowledge)
- ✅ Automatic translation to Claude Code JSON format
- ✅ Automatic translation to Cursor JSON format
- ✅ User messages shown in editor UI
- ✅ Agent messages provide context to AI
- ✅ Metadata properly formatted per editor
- ✅ Backward compatible (exit code fallback when no messages)
- ✅ All tests passing
- ✅ Documentation complete with translation examples

## Implementation Status

**✅ COMPLETED** - All components implemented and integrated.

### What Was Built

1. **Translation layer in `cli/src/vendors/mod.rs`** (cli/src/vendors/mod.rs:18-218)
   - `translate_response()` - Central translation function
   - `translate_claude()` - Claude Code JSON format
   - `translate_cursor()` - Cursor JSON format
   - `translate_generic()` - Stderr fallback for unknown editors
   - `EditorType` enum for editor detection

2. **Vendor handlers updated** to translate and output responses:
   - `cli/src/vendors/claude_code.rs:handle()` (cli/src/vendors/claude_code.rs:89-107)
   - `cli/src/vendors/cursor.rs:handle()` (cli/src/vendors/cursor.rs:72-90)

3. **Event command updated** to use shared translation:
   - `cli/src/commands/event.rs:run_prepare_commit_message()` (cli/src/commands/event.rs:27-53)
   - Removed duplicate translation code (200+ lines eliminated)

4. **Generic HookResponse system** in `cli/src/handlers.rs`:
   - Builder methods: `success()`, `failure()`, `blocking_failure()`
   - Chaining methods: `with_metadata()`, `with_agent_message()`
   - Used by all handlers for consistent responses

### Key Architecture

```
Flow:
  Editor hook → Vendor adapter → Event bus → Handler → HookResponse (generic)
                     ↓
              Translation layer (vendors::translate_response)
                     ↓
              Editor-specific JSON + stdout + exit code
```

**Separation of concerns:**
- Handlers return generic `HookResponse` (no editor knowledge)
- Vendor adapters call translation layer
- Translation layer knows editor-specific formats
- All editors share the same translation functions

### Issue Fixed

**Problem:** Translation layer existed but was never called by vendor hooks. The handlers returned `HookResponse` objects that were immediately dropped with `Ok(())`, so editors never received JSON output.

**Solution:** Updated vendor handlers to:
1. Capture `HookResponse` from `event_bus::dispatch()`
2. Call `vendors::translate_response()` to convert to editor JSON
3. Output JSON to stdout
4. Exit with appropriate code

**Files modified:**
- `cli/src/vendors/mod.rs` - Added translation layer (200 lines)
- `cli/src/vendors/claude_code.rs` - Output JSON + exit (lines 92-107)
- `cli/src/vendors/cursor.rs` - Output JSON + exit (lines 75-90)
- `cli/src/commands/event.rs` - Use shared translation (removed 200+ duplicate lines)

## Benefits

- **Separation of concerns**: Handlers focus on logic, not output format
- **Easy to add editors**: Just add new translation function
- **Testing**: Test handlers with generic responses, test translations separately
- **Maintainability**: Change editor format without touching handlers
- **Future-proof**: Add new response fields without breaking handlers

## Scope

**Phase 5.X includes:**
- ✅ Generic response system
- ✅ Automatic translation for Claude Code and Cursor
- ✅ User messages shown in editor UI
- ✅ Agent messages provide context to AI
- ✅ Metadata tracking
- ✅ Exit code support (backward compatible)

**Phase 5.X excludes:**
- ❌ Prompt responses (not needed for current hooks)
- ❌ Permission hooks (Cursor beforeReadFile) - Phase 7
- ❌ Control hooks (Cursor beforeSubmitPrompt) - Phase 7

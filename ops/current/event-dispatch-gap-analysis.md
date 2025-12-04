# Event Dispatching Gap Analysis: PrePrompt & PostResponse

**Status**: 🔴 Critical - Events defined but never dispatched  
**Impact**: Milestone 1.1 and 1.2 features are non-functional  
**Date**: 2025-12-03

## Executive Summary

PrePrompt and PostResponse events are fully implemented in the handler layer (`cli/src/handlers.rs`) and event bus (`cli/src/event_bus.rs`), but **no vendor adapter ever constructs or dispatches these events**. As a result:

- ✅ PrePrompt handler exists and works (returns `modified_prompt` via metadata)
- ✅ PostResponse handler exists and works (returns `autoreply` via metadata)
- ❌ **PrePrompt events are never created** by any vendor
- ❌ **PostResponse events are never created** by any vendor
- ❌ No integration with ACP protocol lifecycle events
- ❌ No integration with hook-based vendor lifecycle events

This means **all PrePrompt and PostResponse flows are dead code** - they cannot execute.

---

## Current Architecture

### Event Flow (Intended)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Vendor Integration                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  ACP Proxy   │  │ Claude Hooks │  │ Cursor Hooks │          │
│  │              │  │              │  │              │          │
│  │  - Observes  │  │  - stdin/    │  │  - stdin/    │          │
│  │    JSON-RPC  │  │    stdout    │  │    stdout    │          │
│  │  - Extracts  │  │    JSON      │  │    JSON      │          │
│  │    events    │  │              │  │              │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
│         │                 │                  │                   │
│         └─────────────────┼──────────────────┘                   │
│                           │                                      │
│                           ▼                                      │
│                  ┌────────────────┐                             │
│                  │  AikiEvent     │                             │
│                  │  Construction  │                             │
│                  └────────┬───────┘                             │
└──────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Event Bus                                  │
│                  event_bus::dispatch()                           │
│                                                                   │
│   Routes events to appropriate handlers based on type            │
└───────────────────────────┬─────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Handler Layer                              │
│                    cli/src/handlers.rs                           │
│                                                                   │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐   │
│  │ handle_start() │  │handle_pre_     │  │handle_post_    │   │
│  │                │  │  prompt()      │  │  response()    │   │
│  │ ✅ Working     │  │ ✅ Implemented │  │ ✅ Implemented │   │
│  └────────────────┘  │ ❌ Never       │  │ ❌ Never       │   │
│                      │    called      │  │    called      │   │
│  ┌────────────────┐  └────────────────┘  └────────────────┘   │
│  │handle_pre_file_│  ┌────────────────┐  ┌────────────────┐   │
│  │  change()      │  │handle_post_file│  │handle_prepare_ │   │
│  │                │  │  _change()     │  │  commit_msg()  │   │
│  │ ✅ Working     │  │ ✅ Working     │  │ ✅ Working     │   │
│  └────────────────┘  └────────────────┘  └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Flow Engine                                │
│                  cli/src/flows/engine.rs                         │
│                                                                   │
│   Executes flow YAML, returns HookResponse with metadata         │
└─────────────────────────────────────────────────────────────────┘
```

### Events Currently Dispatched

| Event | ACP Proxy | Claude Hooks | Cursor Hooks | Status |
|-------|-----------|--------------|--------------|--------|
| **SessionStart** | ❌ No | ✅ Yes | ⚠️ Yes (incorrect use of `beforeSubmitPrompt`) | Partially working (hook-only) |
| **PrePrompt** | ❌ **Never** | ❌ **Never** (`UserPromptSubmit` ready) | ❌ **Never** (`beforeSubmitPrompt` blocking-only) | 🔴 **DEAD CODE** |
| **PreFileChange** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Working |
| **PostFileChange** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Working |
| **PostResponse** | ❌ **Never** | ❌ **Never** (`Stop` ready) | ❌ **Never** (`Stop` ready) | 🔴 **DEAD CODE** |
| **PrepareCommitMessage** | N/A | N/A | N/A | ✅ Working (Git hook) |

**Key Findings:**
- ✅ **Claude Code**: Full support - `UserPromptSubmit` (modify prompts) + `Stop` (autoreply)
- ✅ **Cursor PostResponse**: `Stop` hook supports autoreplies via `followup_message` (max 5 loops)
- ⚠️ **Cursor PrePrompt**: Partial support - `beforeSubmitPrompt` can block but cannot modify prompts
  - Can use for validation workflows (e.g., "Run tests first")
  - Cannot use for context injection (e.g., prepending architecture docs)
- ⚠️ Cursor's current SessionStart usage is incorrect (hook fires per-prompt, not per-session)
- ❌ ACP proxy needs PrePrompt/PostResponse integration (protocol methods TBD)
- 🎯 **Implementation Priority**: Claude Code hooks → Cursor hooks (both PrePrompt + PostResponse) → ACP proxy

---

## Gap Analysis by Vendor

### 1. ACP Proxy (`cli/src/commands/acp.rs`)

**Currently Dispatches:**
- ✅ PreFileChange: `session/request_permission` for file-modifying tools
- ✅ PostFileChange: `session/update` with completed tool calls

**Missing Dispatches:**

#### PrePrompt Event
- **ACP Lifecycle Point**: `session/new` or first message in a turn
- **Required Action**: Parse user prompt from IDE→Agent messages
- **Handler Response**: Returns `modified_prompt` in metadata
- **Required Wiring**: Intercept IDE→Agent message, fire PrePrompt event, replace prompt with `modified_prompt` metadata

```rust
// MISSING: In IDE→Agent thread, detect prompt submission
"session/new" | "session/submit" => {
    // Extract user prompt from params
    let user_prompt = /* extract from params */;
    
    // Fire PrePrompt event
    let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
        agent_type: validated_agent_type,
        session_id: Some(session_id),
        cwd: cwd.clone().unwrap_or_default(),
        timestamp: chrono::Utc::now(),
        original_prompt: user_prompt.clone(),
    });
    
    let response = event_bus::dispatch(event)?;
    
    // Extract modified_prompt from metadata
    if let Some(modified) = extract_metadata(&response, "modified_prompt") {
        // Replace prompt in params before forwarding to agent
        // ... modify JSON and forward ...
    }
}
```

#### PostResponse Event
- **ACP Lifecycle Point**: JSON-RPC response with `stopReason` field
- **Required Action**: Detect when agent finishes response (stopReason in response)
- **Handler Response**: Returns `autoreply` in metadata
- **Required Wiring**: Intercept Agent→IDE JSON-RPC response, fire PostResponse, send new `session/prompt` for autoreply

See [ACP Protocol Integration Points](#postresponse-integration) section below for full details and implementation code.

### 2. Claude Code Hooks (`cli/src/vendors/claude_code.rs`)

**Currently Handles:**
- ✅ `SessionStart`: Maps to SessionStart event
- ✅ `PreToolUse`: Maps to PreFileChange (file-modifying tools only)
- ✅ `PostToolUse`: Maps to PostFileChange

**Available Hooks for PrePrompt & PostResponse:**

#### ✅ PrePrompt: `UserPromptSubmit` Hook

**Hook Documentation**: https://code.claude.com/docs/en/hooks#userpromptsubmit

**Example Payload:**
```json
{
  "session_id": "abc123",
  "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
  "cwd": "/Users/...",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "Write a function to calculate the factorial of a number"
}
```

**Prompt Manipulation**: https://code.claude.com/docs/en/hooks#userpromptsubmit-decision-control

The hook can modify the prompt by returning:
```json
{
  "decision": "continue",
  "modifiedPrompt": "Enhanced prompt with additional context..."
}
```

**Implementation:**

```rust
"UserPromptSubmit" => {
    // Extract prompt from payload
    let user_prompt = payload.prompt;
    
    // Fire PrePrompt event
    let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
        agent_type: AgentType::Claude,
        session_id: Some(payload.session_id),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        original_prompt: user_prompt.clone(),
    });
    
    let response = event_bus::dispatch(event)?;
    
    // Extract modified_prompt from metadata
    let modified_prompt = response.metadata.iter()
        .find(|(k, _)| k == "modified_prompt")
        .map(|(_, v)| v.clone())
        .unwrap_or(user_prompt); // Fallback to original on error
    
    // Return Claude Code JSON with modifiedPrompt
    let json = json!({
        "decision": "continue",
        "modifiedPrompt": modified_prompt
    });
    
    println!("{}", serde_json::to_string(&json)?);
    std::process::exit(0);
}
```

**Status**: ✅ **Ready to implement**

#### ✅ PostResponse: `Stop` Hook

**Hook Documentation**: https://code.claude.com/docs/en/hooks#stop

**Example Payload:**
```json
{
  "session_id": "abc123",
  "transcript_path": "~/.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
  "permission_mode": "default",
  "hook_event_name": "Stop",
  "stop_hook_active": true
}
```

**Autoreply Control**: https://code.claude.com/docs/en/hooks#stop/subagentstop-decision-control

The hook can force Claude to continue (autoreply) by returning:
```json
{
  "decision": "continue",
  "additionalContext": "Fix the TypeScript errors before continuing."
}
```

**Implementation:**

```rust
"Stop" => {
    // Note: The Stop hook doesn't include response text in the payload.
    // We fire the event with empty response and rely on flow functions
    // like self.count_typescript_errors() that check the filesystem.
    
    let event = AikiEvent::PostResponse(AikiPostResponseEvent {
        agent_type: AgentType::Claude,
        session_id: Some(payload.session_id),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        response: String::new(), // Empty - flows use self.* functions
        modified_files: vec![],  // Could track from PostToolUse events
    });
    
    let response = event_bus::dispatch(event)?;
    
    // Extract autoreply from metadata
    let autoreply = response.metadata.iter()
        .find(|(k, _)| k == "autoreply")
        .map(|(_, v)| v.clone());
    
    if let Some(autoreply_text) = autoreply {
        // Force continuation with autoreply
        let json = json!({
            "decision": "continue",
            "additionalContext": autoreply_text
        });
        
        println!("{}", serde_json::to_string(&json)?);
        std::process::exit(0);
    } else {
        // No autoreply - allow normal stop
        let json = json!({"decision": "stop"});
        println!("{}", serde_json::to_string(&json)?);
        std::process::exit(0);
    }
}
```

**Status**: ✅ **Ready to implement**

**Note**: The `Stop` hook fires after Claude completes its response. It doesn't provide the response text, but this is ideal for validation workflows that check files/run tests rather than parse response text.

### 3. Cursor Hooks (`cli/src/vendors/cursor.rs`)

**Currently Handles:**
- ✅ `beforeSubmitPrompt`: Maps to SessionStart (incorrect usage)
- ✅ `beforeMCPExecution`: Maps to PreFileChange (file-modifying tools only)
- ✅ `afterFileEdit`: Maps to PostFileChange

**Critical Limitation:**

#### ⚠️ PrePrompt: `beforeSubmitPrompt` Supports Blocking Only (No Modification)

**Hook Documentation**: https://cursor.com/docs/agent/hooks#beforesubmitprompt

**Payload Structure:**
```json
{
  "prompt": "<user prompt text>",
  "attachments": [
    {
      "type": "file" | "rule",
      "filePath": "<absolute path>"
    }
  ],
  "conversation_id": "...",
  "generation_id": "...",
  "model": "...",
  "hook_event_name": "beforeSubmitPrompt",
  "cursor_version": "...",
  "workspace_roots": [...],
  "user_email": "..."
}
```

**Return Value:**
```json
{
  "continue": true | false,
  "user_message": "<message shown to user when blocked>"
}
```

**Capability**: This hook can **block** submissions and show user messages, but **cannot modify** the prompt text.

**PrePrompt Use Cases for Cursor:**
- ✅ **Validation**: Block prompts that don't meet requirements (e.g., "Please describe the task in more detail")
- ✅ **Enforcement**: Require certain conditions before agent runs (e.g., "Run tests first")
- ✅ **Warnings**: Show messages to user based on prompt analysis
- ❌ **Context Injection**: Cannot prepend/append content to prompt (use Claude Code or ACP for this)

**Implementation:**

```rust
"beforeSubmitPrompt" => {
    // Extract prompt from payload
    let user_prompt = payload.prompt;
    
    // Fire PrePrompt event
    let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
        agent_type: AgentType::Cursor,
        session_id: Some(payload.conversation_id.clone()),
        cwd: PathBuf::from(&payload.workspace_roots[0]), // Use first workspace root
        timestamp: chrono::Utc::now(),
        original_prompt: user_prompt,
    });
    
    let response = event_bus::dispatch(event)?;
    
    // Note: Cursor cannot use modified_prompt - only blocking is supported
    // Check if flow wants to block (via exit_code 2 or specific metadata)
    
    if response.exit_code == Some(2) {
        // Block submission
        let user_msg = response.user_message.unwrap_or_else(|| 
            "Prompt blocked by Aiki flow".to_string()
        );
        
        let json = json!({
            "continue": false,
            "user_message": user_msg
        });
        
        println!("{}", serde_json::to_string(&json)?);
        std::process::exit(0);
    } else {
        // Allow submission (ignore modified_prompt if present)
        let json = json!({"continue": true});
        println!("{}", serde_json::to_string(&json)?);
        std::process::exit(0);
    }
}
```

**Status**: ✅ **Can implement for validation workflows** (blocking only)

**Limitation**: Cursor users cannot use PrePrompt for context injection. Flows that modify prompts will be ignored. Only blocking (`on_failure: block`) will work.

#### ✅ PostResponse: `Stop` Hook Available

**Hook Documentation**: https://cursor.com/docs/agent/hooks#stop

**When It Fires**: When the agent loop ends (completion, abortion, or error)

**Payload Structure:**
```json
{
  "status": "completed" | "aborted" | "error",
  "loop_count": 0,
  "conversation_id": "...",
  "generation_id": "...",
  "model": "...",
  "hook_event_name": "Stop",
  "cursor_version": "...",
  "workspace_roots": [...],
  "user_email": "..."
}
```

**Autoreply via Follow-up Message:**
```json
{
  "followup_message": "<message text>"
}
```

When `followup_message` is provided, "Cursor will automatically submit it as the next user message," enabling autoreply functionality.

**Constraint**: Maximum of 5 automatic follow-ups per conversation to prevent infinite loops.

**Implementation:**

```rust
"Stop" => {
    // Extract status from payload
    let status = payload.status; // "completed" | "aborted" | "error"
    let loop_count = payload.loop_count;
    
    // Only fire PostResponse for successful completions
    if status != "completed" {
        return Ok(HookResponse::success());
    }
    
    // Fire PostResponse event (empty response text, flows use self.* functions)
    let event = AikiEvent::PostResponse(AikiPostResponseEvent {
        agent_type: AgentType::Cursor,
        session_id: payload.conversation_id.clone(),
        cwd: PathBuf::from(&payload.workspace_roots[0]), // Use first workspace root
        timestamp: chrono::Utc::now(),
        response: String::new(), // Empty - flows check files/run tests
        modified_files: vec![],  // Could track from afterFileEdit events
    });
    
    let response = event_bus::dispatch(event)?;
    
    // Extract autoreply from metadata
    let autoreply = response.metadata.iter()
        .find(|(k, _)| k == "autoreply")
        .map(|(_, v)| v.clone());
    
    if let Some(autoreply_text) = autoreply {
        // Check loop count to prevent infinite autoreplies
        if loop_count >= 5 {
            eprintln!("Warning: Maximum autoreply loop count (5) reached, skipping autoreply");
            return Ok(HookResponse::success());
        }
        
        // Return follow-up message for autoreply
        let json = json!({
            "followup_message": autoreply_text
        });
        
        println!("{}", serde_json::to_string(&json)?);
        std::process::exit(0);
    } else {
        // No autoreply - return empty/success
        Ok(HookResponse::success())
    }
}
```

**Status**: ✅ **Ready to implement**

**Note**: Like Claude Code's `Stop` hook, Cursor's version doesn't provide the response text. Flows rely on `self.*` functions that check the filesystem, which is ideal for validation workflows (e.g., `self.count_typescript_errors()`).

---

## ACP Protocol Integration Points

### PrePrompt Integration

**ACP Method**: `session/prompt` (IDE→Agent)

**Documentation**: https://agentclientprotocol.com/protocol/prompt-turn#1-user-message

**Example Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "session/prompt",
  "params": {
    "sessionId": "sess_abc123def456",
    "prompt": [
      {
        "type": "text",
        "text": "Can you analyze this code for potential issues?"
      },
      {
        "type": "resource",
        "resource": {
          "uri": "file:///home/user/project/main.py",
          "mimeType": "text/x-python",
          "text": "def process_data(items):\n    for item in items:\n        print(item)"
        }
      }
    ]
  }
}
```

**Integration Strategy:**
1. Intercept `session/prompt` in IDE→Agent thread
2. Extract text content from `prompt` array (all `type: "text"` items)
3. Fire PrePrompt event with combined text
4. Get `modified_prompt` from handler metadata
5. **Replace text in first `prompt` array item** or prepend/append new text items
6. Forward modified JSON to agent

**Implementation Approach:**

```rust
// In IDE→Agent thread
"session/prompt" => {
    if let Some(params) = &msg.params {
        let session_id = params.get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        
        // Extract all text content from prompt array
        let prompt_array = params.get("prompt")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![]);
        
        let mut original_text = String::new();
        for item in prompt_array {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !original_text.is_empty() {
                        original_text.push_str("\n\n");
                    }
                    original_text.push_str(text);
                }
            }
        }
        
        // Fire PrePrompt event
        let event = AikiEvent::PrePrompt(AikiPrePromptEvent {
            agent_type: validated_agent_type,
            session_id: Some(session_id.to_string()),
            cwd: cwd.clone().unwrap_or_default(),
            timestamp: chrono::Utc::now(),
            original_prompt: original_text.clone(),
        });
        
        let response = event_bus::dispatch(event)?;
        
        // Extract modified_prompt from metadata
        let modified_prompt = response.metadata.iter()
            .find(|(k, _)| k == "modified_prompt")
            .map(|(_, v)| v.clone())
            .unwrap_or(original_text);
        
        // Modify the JSON params to replace prompt text
        let mut modified_msg = msg.clone();
        if let Some(params) = modified_msg.params.as_mut() {
            if let Some(params_obj) = params.as_object_mut() {
                // Option 1: Replace first text item
                if let Some(prompt_arr) = params_obj.get_mut("prompt")
                    .and_then(|v| v.as_array_mut()) {
                    
                    // Find first text item and replace it
                    for item in prompt_arr.iter_mut() {
                        if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                            if let Some(item_obj) = item.as_object_mut() {
                                item_obj.insert("text".to_string(), json!(modified_prompt));
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        // Forward modified message to agent
        writeln!(agent_stdin, "{}", serde_json::to_string(&modified_msg)?)?;
        agent_stdin.flush()?;
        continue; // Don't forward original message
    }
}
```

**Challenges:**
- Prompt array may contain multiple text items and resource items
- Need to decide: replace first text item, or prepend new text item?
- Need to preserve resource items (attached files)
- Need to handle graceful degradation if modification fails

### PostResponse Integration

**ACP Method**: JSON-RPC response to `session/prompt` with `stopReason` (Agent→IDE)

**Documentation**: https://agentclientprotocol.com/protocol/prompt-turn#4-check-for-completion

**Completion Detection**: The agent responds to the original `session/prompt` request with a `stopReason` field indicating why the turn ended.

**Example Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "stopReason": "end_turn"
  }
}
```

**StopReason Values:**
- `"end_turn"`: Language model finishes responding without requesting more tools (normal completion)
- `"max_tokens"`: Maximum token limit reached
- `"max_turn_requests"`: Maximum number of model requests exceeded
- `"refusal"`: Agent refuses to continue
- `"cancelled"`: Client cancelled the turn

**Integration Strategy:**
1. Monitor Agent→IDE messages in ACP proxy for JSON-RPC responses
2. Detect response with `stopReason` field (matches original `session/prompt` request ID)
3. Only fire PostResponse for `stopReason == "end_turn"` (successful completions)
4. Fire PostResponse event with empty response text (flows use `self.*` functions)
5. Get `autoreply` from handler metadata
6. If non-empty, **construct new `session/prompt` request** and send to agent
7. Forward original response to IDE

**Implementation Approach:**

```rust
// In Agent→IDE thread, detect JSON-RPC responses
if msg.id.is_some() && msg.result.is_some() {
    if let Some(result) = &msg.result {
        if let Some(stop_reason) = result.get("stopReason").and_then(|v| v.as_str()) {
            // This is a turn completion response
            
            // Only fire PostResponse for successful completions
            if stop_reason != "end_turn" {
                // Forward error/cancellation responses as-is
                println!("{}", line);
                io::stdout().flush()?;
                continue;
            }
            
            // Fire PostResponse event
            let event = AikiEvent::PostResponse(AikiPostResponseEvent {
                agent_type: validated_agent_type,
                session_id: /* extract from tracked context */,
                cwd: cwd.clone().unwrap_or_default(),
                timestamp: chrono::Utc::now(),
                response: String::new(), // Empty - flows check files/run tests
                modified_files: vec![],  // Track from tool_call events
            });
            
            let response = event_bus::dispatch(event)?;
            
            // Extract autoreply from metadata
            let autoreply = response.metadata.iter()
                .find(|(k, _)| k == "autoreply")
                .map(|(_, v)| v.clone());
            
            // Forward original response to IDE first
            println!("{}", line);
            io::stdout().flush()?;
            
            // Then send autoreply as new prompt if present
            if let Some(autoreply_text) = autoreply {
                let session_id = /* extract from tracked context */;
                
                // Construct new session/prompt request
                let autoreply_request = json!({
                    "jsonrpc": "2.0",
                    "id": generate_unique_id(), // Need ID tracking
                    "method": "session/prompt",
                    "params": {
                        "sessionId": session_id,
                        "prompt": [
                            {
                                "type": "text",
                                "text": autoreply_text
                            }
                        ]
                    }
                });
                
                // Send to agent
                writeln!(agent_stdin, "{}", serde_json::to_string(&autoreply_request)?)?;
                agent_stdin.flush()?;
            }
            
            continue;
        }
    }
}
```

**Challenges:**
- Need to track request IDs to match responses to original `session/prompt` requests
- Need to track session_id from the original request
- Need to generate unique IDs for autoreply requests
- Need to track modified files from tool_call events
- Autoreply creates a new turn, which will generate another `stopReason` response
- Need to prevent infinite autoreply loops (track loop count)

---

## Implementation Plan

### Phase 1: ACP Proxy Integration (Priority 1)

**Rationale**: ACP is the official protocol and will work with all ACP-compatible agents (Claude Code, future agents)

1. **Add PrePrompt to ACP proxy** (`cli/src/commands/acp.rs`)
   - [ ] Intercept `session/prompt` in IDE→Agent thread
   - [ ] Extract text from prompt array (all `type: "text"` items)
   - [ ] Fire PrePrompt event with combined text
   - [ ] Extract `modified_prompt` from metadata
   - [ ] Replace text in first prompt array item (preserve resources)
   - [ ] Forward modified JSON to agent
   - [ ] Add error handling (graceful degradation on failure)
   - [ ] Test with real Claude Code via ACP

2. **Add PostResponse to ACP proxy** (`cli/src/commands/acp.rs`)
   - [ ] Detect JSON-RPC responses with `stopReason` field in Agent→IDE thread
   - [ ] Match response ID to original `session/prompt` request
   - [ ] Only fire PostResponse for `stopReason == "end_turn"` (successful completions)
   - [ ] Track session_id from original request
   - [ ] Fire PostResponse event (empty response text)
   - [ ] Extract `autoreply` from metadata
   - [ ] Construct new `session/prompt` request for autoreply (if non-empty)
   - [ ] Generate unique request ID for autoreply
   - [ ] Add loop count tracking to prevent infinite autoreplies
   - [ ] Forward original response to IDE, then send autoreply to agent
   - [ ] Test with real Claude Code via ACP

### Phase 2: Cursor Hook Integration (Priority 2)

**Rationale**: Cursor has `Stop` hook for PostResponse, but cannot implement PrePrompt (prompt modification not supported)

**Status**: 
- ❌ **Cannot Implement PrePrompt** - `beforeSubmitPrompt` only blocks, cannot modify
- ✅ **Can Implement PostResponse** - `Stop` hook supports `followup_message` autoreplies

**Actions:**

1. **Implement Cursor `beforeSubmitPrompt` for SessionStart + PrePrompt** (`cli/src/vendors/cursor.rs`)
   - [ ] Track `conversation_id` in handler state (static or thread-local)
   - [ ] Check if `conversation_id` changed from previous call
   - [ ] If changed, fire SessionStart event first (new session detected)
   - [ ] Extract `prompt` field from payload
   - [ ] Fire PrePrompt event (every call)
   - [ ] Check `exit_code == 2` for blocking
   - [ ] Return JSON with `continue: false` + `user_message` if blocking
   - [ ] Return JSON with `continue: true` if allowing (ignore `modified_prompt`)
   - [ ] Test with real Cursor

2. **Implement Cursor `Stop` hook for PostResponse** (`cli/src/vendors/cursor.rs`)
   - [ ] Add `Stop` case to `handle()` function
   - [ ] Extract `status` and `loop_count` from payload
   - [ ] Fire PostResponse event (only on `status == "completed"`)
   - [ ] Extract `autoreply` from metadata
   - [ ] Return JSON with `followup_message` if autoreply exists
   - [ ] Enforce max 5 follow-ups (check `loop_count`)
   - [ ] Test with real Cursor

3. **Document Cursor limitations**
   - [ ] Add warning to milestone docs about PrePrompt limitation (blocking only, no modification)
   - [ ] Document which PrePrompt use cases work (validation) vs don't work (context injection)
   - [ ] Update examples to show Cursor-compatible validation workflows

**Result**: Cursor users get:
- ✅ PrePrompt for **validation** (blocking prompts that don't meet requirements)
- ❌ PrePrompt for **context injection** (cannot modify prompt text)
- ✅ PostResponse for **validation + autoreply** (full support)

### Phase 3: Claude Code Hook Integration (Priority 2)

**Rationale**: Claude Code has documented hooks for both PrePrompt and PostResponse - these should be implemented alongside Cursor

1. **Implement `UserPromptSubmit` hook** (`cli/src/vendors/claude_code.rs`)
   - [ ] Add `UserPromptSubmit` case to `handle()` function
   - [ ] Extract `prompt` field from payload
   - [ ] Fire PrePrompt event
   - [ ] Extract `modified_prompt` from metadata
   - [ ] Return JSON with `modifiedPrompt` field
   - [ ] Test with real Claude Code

2. **Implement `Stop` hook** (`cli/src/vendors/claude_code.rs`)
   - [ ] Add `Stop` case to `handle()` function
   - [ ] Fire PostResponse event (with empty response text)
   - [ ] Extract `autoreply` from metadata
   - [ ] Return JSON with `decision: "continue"` and `additionalContext` if autoreply exists
   - [ ] Return JSON with `decision: "stop"` if no autoreply
   - [ ] Test with real Claude Code

**Status**: ✅ Both hooks are documented and ready to implement

### Phase 4: SessionStart Audit (Priority 4)

**Issue**: SessionStart is fired from hooks but not from ACP proxy, and Cursor fires it on every prompt

1. **Add SessionStart to ACP proxy**
   - [ ] Fire on `session/new` or `initialize` method
   - [ ] Ensure consistent with hook-based SessionStart
   - [ ] Test with real Claude Code via ACP

2. **Fix Cursor SessionStart to track session changes**
   - [ ] Track `conversation_id` from `beforeSubmitPrompt` payload in handler state
   - [ ] Only fire SessionStart when `conversation_id` changes (new session)
   - [ ] Fire PrePrompt on every `beforeSubmitPrompt` (blocking only)
   - [ ] Document that Cursor SessionStart requires conversation_id tracking
   - [ ] Test with real Cursor to verify conversation_id behavior

**Rationale**: Cursor's `beforeSubmitPrompt` fires on every prompt and includes `conversation_id` in the payload. By tracking conversation ID changes, we can detect new sessions and fire SessionStart appropriately, while also firing PrePrompt for validation on every prompt.

---

## Testing Strategy

### Unit Tests

- [ ] Test PrePrompt event construction with all required fields
- [ ] Test PostResponse event construction with all required fields
- [ ] Test metadata extraction from HookResponse
- [ ] Test JSON modification (replacing prompt in params)
- [ ] Test JSON construction (creating session/submit for autoreply)

### Integration Tests

- [ ] Test PrePrompt flow execution returns modified_prompt
- [ ] Test PostResponse flow execution returns autoreply
- [ ] Test graceful degradation when flow fails (original prompt used, no autoreply)

### End-to-End Tests

- [ ] Test PrePrompt with ACP proxy + real Claude Code
- [ ] Test PostResponse with ACP proxy + real Claude Code
- [ ] Test PrePrompt with Cursor hooks
- [ ] Test PostResponse with Cursor hooks (if supported)
- [ ] Test that autoreplies appear as user messages to agent
- [ ] Test that modified prompts are invisible to user

---

## Success Criteria

### PrePrompt

✅ User submits prompt "Add login endpoint"  
✅ PrePrompt event fires with original_prompt  
✅ Flow adds context: `.aiki/arch/backend.md`  
✅ Agent receives modified prompt (original + context)  
✅ User doesn't see the modification  
✅ Flow errors fall back to original prompt (graceful degradation)

### PostResponse

✅ Agent completes response with TypeScript errors  
✅ PostResponse event fires with response text  
✅ Flow detects errors via `self.count_typescript_errors`  
✅ Flow adds autoreply: "Fix TypeScript errors first"  
✅ Autoreply sent to agent as new user message  
✅ Agent receives autoreply and can respond  
✅ User sees both agent's response and the autoreply exchange  
✅ Flow errors result in no autoreply (graceful degradation)

---

## Open Questions

1. ~~**ACP Prompt Submission Method**~~: ✅ **RESOLVED** - `session/prompt` method (documented)

2. ~~**ACP Response Completion Detection**~~: ✅ **RESOLVED** - `stopReason` field in JSON-RPC response (no buffering needed)

3. **Autoreply UX**: Should autoreply be visible to user in IDE?
   - Currently planned: Agent receives it as new prompt, user sees agent's response
   - Alternative: Show autoreply in UI as system message (requires IDE support)
   - Recommendation: Keep current approach (works with all IDEs)

4. ~~**Claude Code Hook Availability**~~: ✅ **RESOLVED** - Both `UserPromptSubmit` and `Stop` documented

5. ~~**Cursor PrePrompt Capability**~~: ✅ **RESOLVED** - `beforeSubmitPrompt` can block but cannot modify prompts

6. **ACP Loop Prevention**: How to prevent infinite autoreplies?
   - Cursor: Built-in (max 5 `followup_message` per conversation)
   - Claude Code: Unknown if `Stop` hook has loop protection
   - ACP: Need to track loop count in proxy state
   - Recommendation: Implement proxy-side loop counter (max 5 like Cursor)

7. **ACP Request ID Generation**: How to generate unique IDs for autoreply requests?
   - Need atomic counter or UUID generation
   - Must not conflict with IDE-generated IDs
   - Recommendation: Use prefix (e.g., `aiki-autoreply-{counter}`) to distinguish

---

## Related Files

### Core Implementation
- `cli/src/events.rs` - Event struct definitions (PrePrompt, PostResponse already exist)
- `cli/src/event_bus.rs` - Event routing to handlers
- `cli/src/handlers.rs` - Handler implementations (PrePrompt, PostResponse already implemented)

### Vendor Integration
- `cli/src/commands/acp.rs` - ACP proxy (needs PrePrompt + PostResponse dispatch)
- `cli/src/vendors/claude_code.rs` - Claude Code hooks (needs PrePrompt + PostResponse)
- `cli/src/vendors/cursor.rs` - Cursor hooks (needs PrePrompt fix, PostResponse addition)

### Protocol Definitions
- `cli/src/acp/protocol.rs` - ACP JSON-RPC types
- `agent-client-protocol` crate - Official ACP types (SessionUpdate, etc.)

### Milestones
- `ops/current/milestone-1.1-preprompt.md` - PrePrompt specification
- `ops/current/milestone-1.2-post-response.md` - PostResponse specification
- `ops/current/milestone-1.md` - Milestone 1 overview

---

## Next Actions

**Immediate (this session):**
1. ✅ Complete this analysis document
2. Implement PrePrompt dispatch in ACP proxy
3. Implement PostResponse dispatch in ACP proxy
4. Fix Cursor `beforeSubmitPrompt` to fire PrePrompt
5. Add unit tests for event construction

**Follow-up (next session):**
1. Test with real Claude Code via ACP
2. Test with real Cursor via hooks
3. Document any ACP protocol findings
4. Update milestone docs with implementation notes
5. Add E2E test cases to CI

**Research (async):**
1. Review official ACP spec for prompt submission method
2. Check Claude Code docs for available hooks
3. Check Cursor docs for response completion hooks
4. Consider reaching out to Anthropic/Cursor for clarification

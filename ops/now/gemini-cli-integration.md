# Gemini CLI Hooks Integration with Aiki

## Summary: Gemini CLI Hooks System

**Release:** v0.26.0+ (enabled by default in Preview/Nightly channels)

Gemini CLI hooks allow developers to intercept and customize behavior at specific points in the agentic loop without modifying the CLI source code. They follow a similar pattern to Claude Code hooks.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Gemini CLI Agent Loop                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SessionStart ──► BeforeAgent ──► BeforeModel ──► BeforeToolSelection
│       │               │               │                 │        │
│       ▼               ▼               ▼                 ▼        │
│    [HOOK]          [HOOK]          [HOOK]            [HOOK]      │
│       │               │               │                 │        │
│       ▼               ▼               ▼                 ▼        │
│  ◄────────────────────────────────────────────────────────────── │
│                                                                  │
│  BeforeTool ──► [ACTION] ──► AfterTool ──► AfterModel ──► AfterAgent
│       │                          │             │             │   │
│       ▼                          ▼             ▼             ▼   │
│    [HOOK]                     [HOOK]        [HOOK]        [HOOK] │
│   (can block)                                                    │
│                                                                  │
│  PreCompress ──► SessionEnd ──► Notification                     │
│       │              │               │                           │
│       ▼              ▼               ▼                           │
│    [HOOK]         [HOOK]          [HOOK]                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Hook Events

| Event | Description | Can Block? | Payload |
|-------|-------------|------------|---------|
| **SessionStart** | New session begins | No | `session_id`, source (`startup`/`resume`/`clear`) |
| **SessionEnd** | Session terminates | No | `session_id`, reason (`exit`/`clear`/`logout`/`prompt_input_exit`/`other`) |
| **PreCompress** | Before context compression | No | source (`manual`/`auto`) |
| **BeforeModel** | Before LLM request | No | `llm_request` object |
| **AfterModel** | After LLM response | No | `llm_request`, `llm_response` |
| **BeforeToolSelection** | Before tool planning | No | tool list |
| **BeforeTool** | Before tool execution | **Yes** | `tool_name`, `tool_input`, `mcp_context` |
| **AfterTool** | After tool execution | No | `tool_name`, `tool_input`, `tool_response` |
| **BeforeAgent** | Before agent prompt processing | No | `prompt` |
| **AfterAgent** | After agent response | No | `prompt`, `prompt_response` |
| **Notification** | System notifications | No | notification details |

### Communication Protocol

**Input:** JSON on stdin
```json
{
  "hook_event_name": "BeforeTool",
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "tool_name": "write_file",
  "tool_input": { "path": "foo.txt", "content": "..." }
}
```

**Output:** JSON on stdout (blocking decisions)
```json
{
  "decision": "deny",
  "reason": "Secrets detected in content"
}
```

**Exit Codes:**
- `0`: Success - stdout parsed as JSON
- `2`: System Block - stderr used as rejection reason
- Other: Non-blocking warning - logged but continues

### Configuration

```json
// .gemini/settings.json or ~/.gemini/settings.json
{
  "hooks": {
    "enabled": true,
    "BeforeTool": [
      {
        "matcher": "write_file|replace",
        "hooks": [
          {
            "name": "secret-scanner",
            "type": "command",
            "command": "$GEMINI_PROJECT_DIR/.gemini/hooks/block-secrets.sh",
            "description": "Prevent committing secrets",
            "timeout": 60000
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "hooks": [
          {
            "name": "init-context",
            "type": "command",
            "command": "aiki hooks gemini SessionStart"
          }
        ]
      }
    ]
  }
}
```

**Key Configuration Fields:**
- `name` (string): Unique identifier for /hooks enable/disable
- `type` (string): Currently only "command" supported (plugin hooks planned)
- `command` (string): Shell command to execute
- `description` (string, optional): Human-readable description
- `timeout` (number, optional): Milliseconds, default 60000
- `matcher` (string, optional): Regex/wildcard to filter when hook runs

### Environment Variables

- `$GEMINI_PROJECT_DIR`: Project root directory
- `$GEMINI_SESSION_ID`: Current session identifier
- Standard shell environment inherited

### Comparison: Gemini CLI vs Claude Code Hooks

| Aspect | Gemini CLI | Claude Code |
|--------|------------|-------------|
| **Hook Events** | 11 events (more granular) | 6 events |
| **Model Events** | BeforeModel, AfterModel | None |
| **Tool Selection** | BeforeToolSelection | None |
| **Compression** | PreCompress | None |
| **Tool Events** | BeforeTool/AfterTool | PreToolUse/PostToolUse |
| **Session Events** | SessionStart/End | SessionStart/End |
| **Agent Events** | BeforeAgent/AfterAgent | UserPromptSubmit/Stop |
| **Notification** | Yes | No |
| **Blocking** | BeforeTool only | PreToolUse only |
| **Config Location** | `.gemini/settings.json` | `.claude/settings.json` |
| **Extension Hooks** | Yes (with consent flow) | No |
| **Plugin Hooks** | Planned (npm packages) | No |
| **Matcher Syntax** | Regex/wildcards | Not documented |

---

## Integration Plan: Aiki + Gemini CLI

### Phase 1: Event Mapping

Map Gemini CLI events to Aiki's unified event model:

| Gemini CLI Event | Aiki Event | Notes |
|------------------|------------|-------|
| `SessionStart` (startup) | `session.started` | Direct mapping |
| `SessionStart` (resume) | `session.resumed` | Gemini uses source field |
| `SessionStart` (clear) | `session.started` | Treat as new session |
| `SessionEnd` | `session.ended` | Map reason field |
| `BeforeAgent` | `turn.started` | Maps to user prompt |
| `AfterAgent` | `turn.completed` | Maps to agent response |
| `BeforeTool` (file tools) | `change.permission_asked` | Filter by tool_name |
| `BeforeTool` (shell) | `shell.permission_asked` | Filter by tool_name |
| `BeforeTool` (mcp) | `mcp.permission_asked` | Filter by mcp_context |
| `AfterTool` (file tools) | `change.completed` | Filter by tool_name |
| `AfterTool` (shell) | `shell.completed` | Filter by tool_name |
| `AfterTool` (mcp) | `mcp.completed` | Filter by mcp_context |
| `PreCompress` | `context.compressing` | **New event** - add to Aiki |
| `BeforeModel` | `model.request` | **New event** - add to Aiki |
| `AfterModel` | `model.response` | **New event** - add to Aiki |
| `BeforeToolSelection` | `tools.selecting` | **New event** - add to Aiki |
| `Notification` | `notification.received` | **New event** - add to Aiki |

### Phase 2: New Editor Module

Create `cli/src/editors/gemini/` with the following structure:

```
cli/src/editors/gemini/
├── mod.rs          # Module exports and handle() entry point
├── events.rs       # GeminiEvent enum, payload structs, event building
├── tools.rs        # GeminiTool enum, tool type classification
├── session.rs      # Session creation/management
└── output.rs       # Hook response formatting
```

### Phase 3: Implementation Tasks

#### 3.1 Add Gemini Agent Type
**File:** `cli/src/provenance.rs`

```rust
pub enum AgentType {
    ClaudeCode,
    Cursor,
    Codex,
    Gemini,  // Add this
    // ...
}
```

#### 3.2 Create Gemini Event Structures
**File:** `cli/src/editors/gemini/events.rs`

```rust
use serde::Deserialize;

/// Gemini CLI hook event - discriminated by hook_event_name (same pattern as Claude)
#[derive(Deserialize, Debug)]
#[serde(tag = "hook_event_name")]
enum GeminiEvent {
    #[serde(rename = "SessionStart")]
    SessionStart { #[serde(flatten)] payload: SessionStartPayload },

    #[serde(rename = "SessionEnd")]
    SessionEnd { #[serde(flatten)] payload: SessionEndPayload },

    #[serde(rename = "BeforeAgent")]
    BeforeAgent { #[serde(flatten)] payload: BeforeAgentPayload },

    #[serde(rename = "AfterAgent")]
    AfterAgent { #[serde(flatten)] payload: AfterAgentPayload },

    #[serde(rename = "BeforeTool")]
    BeforeTool { #[serde(flatten)] payload: BeforeToolPayload },

    #[serde(rename = "AfterTool")]
    AfterTool { #[serde(flatten)] payload: AfterToolPayload },

    #[serde(rename = "PreCompress")]
    PreCompress { #[serde(flatten)] payload: PreCompressPayload },

    #[serde(rename = "BeforeModel")]
    BeforeModel { #[serde(flatten)] payload: BeforeModelPayload },

    #[serde(rename = "AfterModel")]
    AfterModel { #[serde(flatten)] payload: AfterModelPayload },

    #[serde(rename = "BeforeToolSelection")]
    BeforeToolSelection { #[serde(flatten)] payload: BeforeToolSelectionPayload },

    #[serde(rename = "Notification")]
    Notification { #[serde(flatten)] payload: NotificationPayload },
}

/// SessionStart payload - matches Gemini CLI format
#[derive(Deserialize, Debug)]
struct SessionStartPayload {
    session_id: String,
    cwd: String,
    #[serde(default = "default_source")]
    source: String,  // "startup", "resume", "clear"
}

/// BeforeTool payload - the critical blocking event
#[derive(Deserialize, Debug)]
pub struct BeforeToolPayload {
    pub session_id: String,
    pub cwd: String,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub mcp_context: Option<McpContext>,
}

#[derive(Deserialize, Debug)]
pub struct McpContext {
    pub server_name: String,
    pub tool_name: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub cwd: Option<String>,
}

// ... similar structs for other events
```

#### 3.3 Create Gemini Tool Classification
**File:** `cli/src/editors/gemini/tools.rs`

```rust
use crate::events::FileOperation;
use crate::tools::ToolType;
use serde::Deserialize;

/// Gemini CLI built-in tools
#[derive(Debug)]
pub enum GeminiTool {
    // File operations
    ReadFile(ReadFileInput),
    WriteFile(WriteFileInput),
    EditFile(EditFileInput),
    ListFiles(ListFilesInput),

    // Shell
    RunTerminalCommand(RunTerminalCommandInput),

    // Search
    SearchFiles(SearchFilesInput),

    // Web (if applicable)
    WebSearch(WebSearchInput),
    WebFetch(WebFetchInput),

    // Unknown/MCP
    Unknown(String),
}

impl GeminiTool {
    pub fn parse(tool_name: &str, input: Option<&serde_json::Value>) -> Self {
        match tool_name {
            "read_file" => /* ... */,
            "write_file" => /* ... */,
            "edit_file" | "replace" => /* ... */,
            "list_files" => /* ... */,
            "run_terminal_command" | "shell" => /* ... */,
            "search_files" | "grep" => /* ... */,
            _ => GeminiTool::Unknown(tool_name.to_string()),
        }
    }

    pub fn tool_type(&self) -> ToolType {
        match self {
            GeminiTool::ReadFile(_) | GeminiTool::WriteFile(_) |
            GeminiTool::EditFile(_) | GeminiTool::ListFiles(_) => ToolType::File,
            GeminiTool::RunTerminalCommand(_) => ToolType::Shell,
            GeminiTool::SearchFiles(_) => ToolType::File,
            GeminiTool::WebSearch(_) | GeminiTool::WebFetch(_) => ToolType::Web,
            GeminiTool::Unknown(_) => ToolType::Mcp,  // Assume MCP for unknown
        }
    }

    pub fn file_operation(&self) -> Option<FileOperation> {
        match self {
            GeminiTool::ReadFile(_) | GeminiTool::ListFiles(_) |
            GeminiTool::SearchFiles(_) => Some(FileOperation::Read),
            GeminiTool::WriteFile(_) | GeminiTool::EditFile(_) => Some(FileOperation::Write),
            _ => None,
        }
    }
}
```

#### 3.4 Update Hooks Command
**File:** `cli/src/commands/hooks.rs`

```rust
fn parse_agent_type(agent: &str) -> Result<provenance::AgentType> {
    match agent {
        "claude-code" => Ok(provenance::AgentType::ClaudeCode),
        "cursor" => Ok(provenance::AgentType::Cursor),
        "codex" => Ok(provenance::AgentType::Codex),
        "gemini" => Ok(provenance::AgentType::Gemini),  // Add this
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}

fn handle_event(agent: provenance::AgentType, event: &str, payload: Option<&str>) -> Result<()> {
    match agent {
        AgentType::ClaudeCode => Ok(editors::claude_code::handle(event)?),
        AgentType::Cursor => Ok(editors::cursor::handle(event)?),
        AgentType::Codex => Ok(editors::codex::handle(event, payload)?),
        AgentType::Gemini => Ok(editors::gemini::handle(event)?),  // Add this
        _ => Err(AikiError::UnsupportedAgentType(format!("{:?}", agent))),
    }
}
```

#### 3.5 Add Gemini to Init Command
**File:** `cli/src/commands/init.rs`

Add Gemini configuration alongside Claude Code and Cursor:

```rust
fn configure_gemini_hooks(project_dir: &Path) -> Result<()> {
    let gemini_settings_path = project_dir.join(".gemini/settings.json");

    let hooks_config = json!({
        "hooks": {
            "enabled": true,
            "SessionStart": [{
                "hooks": [{
                    "name": "aiki-session-start",
                    "type": "command",
                    "command": "aiki hooks gemini SessionStart"
                }]
            }],
            "BeforeTool": [{
                "hooks": [{
                    "name": "aiki-before-tool",
                    "type": "command",
                    "command": "aiki hooks gemini BeforeTool"
                }]
            }],
            "AfterTool": [{
                "hooks": [{
                    "name": "aiki-after-tool",
                    "type": "command",
                    "command": "aiki hooks gemini AfterTool"
                }]
            }],
            "BeforeAgent": [{
                "hooks": [{
                    "name": "aiki-before-agent",
                    "type": "command",
                    "command": "aiki hooks gemini BeforeAgent"
                }]
            }],
            "AfterAgent": [{
                "hooks": [{
                    "name": "aiki-after-agent",
                    "type": "command",
                    "command": "aiki hooks gemini AfterAgent"
                }]
            }],
            "SessionEnd": [{
                "hooks": [{
                    "name": "aiki-session-end",
                    "type": "command",
                    "command": "aiki hooks gemini SessionEnd"
                }]
            }]
        }
    });

    merge_json_config(&gemini_settings_path, hooks_config)?;
    Ok(())
}
```

#### 3.6 Add Doctor Check for Gemini
**File:** `cli/src/commands/doctor.rs`

```rust
fn check_gemini_hooks() -> Result<CheckResult> {
    let gemini_settings = dirs::home_dir()
        .map(|h| h.join(".gemini/settings.json"))
        .filter(|p| p.exists());

    if let Some(path) = gemini_settings {
        let config: serde_json::Value = read_json(&path)?;
        if config.get("hooks").and_then(|h| h.get("enabled")) == Some(&json!(true)) {
            return Ok(CheckResult::Pass("Gemini CLI hooks configured"));
        }
    }

    Ok(CheckResult::Warn("Gemini CLI hooks not configured"))
}
```

### Phase 4: New Aiki Events (Optional)

Consider adding these events to support Gemini's richer hook model:

```rust
// cli/src/events/mod.rs

// Context compression event (triggered before memory compression)
pub struct AikiContextCompressingPayload {
    pub session: Session,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub source: String,  // "manual" | "auto"
}

// Model request event (before LLM call)
pub struct AikiModelRequestPayload {
    pub session: Session,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub request: serde_json::Value,  // Opaque LLM request
}

// Model response event (after LLM call)
pub struct AikiModelResponsePayload {
    pub session: Session,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub request: serde_json::Value,
    pub response: serde_json::Value,  // Opaque LLM response
}
```

### Phase 5: Testing

1. **Unit Tests:**
   - Gemini event parsing
   - Tool classification
   - Event mapping to Aiki events

2. **Integration Tests:**
   - Full hook flow with mock Gemini CLI
   - Blocking scenarios (BeforeTool deny)
   - Session lifecycle

3. **Manual Testing:**
   - Install Gemini CLI (nightly channel)
   - Configure aiki hooks
   - Verify provenance tracking
   - Test blocking behavior

---

## Migration Utility

Gemini CLI provides a Claude Code Migration Utility to convert existing hooks. We can leverage this or provide our own bidirectional conversion.

### Aiki Migration Command

```bash
# Convert Claude Code hooks to Gemini CLI format
aiki migrate hooks --from claude-code --to gemini

# Convert Gemini CLI hooks to Claude Code format
aiki migrate hooks --from gemini --to claude-code
```

---

## Timeline Estimate

| Phase | Duration | Dependencies |
|-------|----------|--------------|
| Phase 1: Event Mapping Design | 1 day | None |
| Phase 2: Module Structure | 1 day | Phase 1 |
| Phase 3: Implementation | 3-5 days | Phase 2 |
| Phase 4: New Events (optional) | 2 days | Phase 3 |
| Phase 5: Testing | 2 days | Phase 3/4 |

**Total:** 7-11 days

---

## Sources

- [Gemini CLI Hooks Documentation](https://geminicli.com/docs/hooks/)
- [Hooks Reference](https://geminicli.com/docs/hooks/reference/)
- [Writing Hooks Guide](https://geminicli.com/docs/hooks/writing-hooks/)
- [Google Developers Blog: Hooks Announcement](https://developers.googleblog.com/tailor-gemini-cli-to-your-workflow-with-hooks/)
- [GitHub: google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli)
- [Feature: Comprehensive System of Hooks (Issue #11703)](https://github.com/google-gemini/gemini-cli/issues/11703)
- [Hook System Documentation PR](https://github.com/google-gemini/gemini-cli/pull/14307)
- [Hook Session Lifecycle PR](https://github.com/google-gemini/gemini-cli/pull/14151)

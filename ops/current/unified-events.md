# Unified Event Model Design

## Overview

Aiki abstracts vendor-specific hooks (Claude Code's `PreToolUse`, Cursor's `beforeShellExecution`, etc.) into a clean, semantic event model. Flows subscribe to **what happened**, not implementation details.

## Naming Convention

```
{resource}.{state}
```

| State | Meaning | Can Gate? |
|-------|---------|-----------|
| `permission_asked` | Action about to happen, approval requested | Yes |
| `completed` | Action completed | No (react only) |
| `started` / `ended` / `resumed` | Lifecycle boundary | No |
| `submitted` / `received` | Message passed | No |

## Event Catalog

### Session Lifecycle (3 events)
| Event | Description |
|-------|-------------|
| `session.started` | New session began |
| `session.resumed` | Continuing previous session |
| `session.ended` | Session terminated |

### User / Agent Interaction (2 events)
| Event | Description |
|-------|-------------|
| `prompt.submitted` | User sent a message |
| `response.received` | Agent finished responding |

### Filesystem Access (2 events)
| Event | Description | Operations |
|-------|-------------|------------|
| `file.permission_asked` | Agent wants to access a file | `read`, `write`, `delete` |
| `file.completed` | File operation completed | |

### Network Access (2 events)
| Event | Description | Operations |
|-------|-------------|------------|
| `web.permission_asked` | Agent wants to make a network request | `fetch`, `search` |
| `web.completed` | Network request completed | |

### Command Execution (4 events)
| Event | Description |
|-------|-------------|
| `shell.permission_asked` | Agent wants to run a shell command |
| `shell.completed` | Command completed |
| `mcp.permission_asked` | Agent wants to call an MCP tool |
| `mcp.completed` | Tool call completed |

### Commit Integration (1 event)
| Event | Description |
|-------|-------------|
| `commit.message_started` | Git hook for commit message modification |

**Total: 14 events**

## Resource Boundaries

Each category represents a distinct security/audit boundary:

| Category | What It Gates | Risk |
|----------|---------------|------|
| `file.*` | Local filesystem access | Data access, modification |
| `web.*` | Network requests | Data exfiltration, external calls |
| `shell.*` | Arbitrary code execution | Full system access |
| `mcp.*` | External tool integrations | Third-party actions |

## Payload Structures

### Common Fields (all events)

```rust
pub session: AikiSession,
pub cwd: PathBuf,
pub timestamp: DateTime<Utc>,
```

### file.permission_asked / file.completed

```rust
pub struct AikiFilePayload {
    // Common fields...
    pub operation: FileOperation,  // read, write, delete
    pub path: String,              // File path or glob pattern
    pub pattern: Option<String>,   // Search pattern (grep only)
    // completed only:
    pub success: Option<bool>,
}

pub enum FileOperation {
    Read,   // Read, LS, Glob, Grep
    Write,  // Edit, Write, NotebookEdit
    Delete, // rm, rmdir (parsed from Bash commands)
}
```

### web.permission_asked / web.completed

```rust
pub struct AikiWebPayload {
    // Common fields...
    pub operation: WebOperation,  // fetch, search
    pub url: Option<String>,      // For fetch
    pub query: Option<String>,    // For search
    // completed only:
    pub success: Option<bool>,
}

pub enum WebOperation {
    Fetch,
    Search,
}
```

### shell.permission_asked / shell.completed

```rust
pub struct AikiShellPayload {
    // Common fields...
    pub command: String,
    // completed only:
    pub success: bool,
    // Optional - available when vendor provides them
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}
```

Flows can react to output when available:
```yaml
shell.completed:
  - when: $event.stderr.contains("warning")
    do:
      - log: "Command had warnings"
```

### mcp.permission_asked / mcp.completed

```rust
pub struct AikiMcpPayload {
    // Common fields...
    pub tool_name: String,
    pub server: Option<String>,
    pub parameters: serde_json::Value,
    // completed only:
    pub success: bool,
    pub result: Option<String>,
}
```

## Vendor Mapping

### Claude Code Tool Classification

| Claude Tool | Aiki Event | Operation |
|-------------|------------|-----------|
| `Read` | `file.permission_asked` | `read` |
| `LS` | `file.permission_asked` | `read` |
| `Glob` | `file.permission_asked` | `read` |
| `Grep` | `file.permission_asked` | `read` |
| `Edit` | `file.permission_asked` | `write` |
| `Write` | `file.permission_asked` | `write` |
| `NotebookEdit` | `file.permission_asked` | `write` |
| `Bash` (general) | `shell.permission_asked` | - |
| `Bash` (`rm`/`rmdir`) | `file.permission_asked` | `delete` |
| `WebFetch` | `web.permission_asked` | - |
| `WebSearch` | `web.permission_asked` | - |
| `Task` | (skip - internal) | - |
| `TodoRead` | (skip - internal) | - |
| `mcp__*` | `mcp.permission_asked` | - |
| Unknown | warn + skip | - |

**Classification logic:**

```rust
fn classify_tool(tool_name: &str) -> ToolClassification {
    match tool_name {
        // File operations (read-only)
        "Read" | "LS" | "Glob" | "Grep" => File(FileOperation::Read),
        
        // File operations (write)
        "Edit" | "Write" | "NotebookEdit" => File(FileOperation::Write),

        // Shell
        "Bash" => Shell,

        // Web
        "WebFetch" | "WebSearch" => Web,

        // Internal tools - skip silently
        "Task" | "TodoRead" => Skip,

        // MCP tools (prefixed by server name)
        name if name.contains("__") => Mcp,

        // Unknown tool - warn and skip
        _ => {
            warn!("Unknown tool: {}. Skipping event.", tool_name);
            Skip
        }
    }
}
```

**Rationale:** If Anthropic adds a new native tool, we want to notice it (via warning) rather than silently routing it to MCP. This makes it easier to catch new tools that need explicit handling.

### Shell Command Parsing for File Operations

When `Bash` tool is used, we parse the command to detect file operations and emit `file.*` events instead of `shell.*` events:

```rust
fn classify_shell_command(command: &str) -> ToolClassification {
    // Parse command to detect file operations
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    
    match parts.first() {
        Some(&"rm") | Some(&"rmdir") => {
            // Extract file paths from command
            let paths = parse_rm_paths(&parts[1..]);
            File(FileOperation::Delete)
        }
        _ => Shell,
    }
}
```

**Examples:**
- `rm file.txt` → `file.permission_asked` with `operation: delete` (no shell event)
- `rm -rf directory/` → `file.permission_asked` with `operation: delete` (no shell event)
- `git status` → `shell.permission_asked`
- `ls -la` → `shell.permission_asked`

**Benefits:**
- Flows can gate deletions separately from other shell commands
- Audit log clearly shows file deletions as file operations
- Security policies can treat `rm` differently from general shell access

### Cursor Event Mapping

| Cursor Hook | Aiki Event |
|-------------|------------|
| `beforeSubmitPrompt` | `prompt.submitted` |
| `stop` | `response.received` |
| `beforeShellExecution` | `shell.permission_asked` |
| `afterShellExecution` | `shell.completed` |
| `beforeMCPExecution` | `mcp.permission_asked` or `file.permission_asked` |
| `afterMCPExecution` | `mcp.completed` or `file.completed` |
| `afterFileEdit` | `file.completed` |

**Note:** Cursor may not have hooks for read/web operations. Need to verify.

## Migration: change.* → file.*

### Breaking Changes

- `change.permission_asked` → `file.permission_asked` (with `operation: write`)
- `change.completed` → `file.completed` (with `operation: write`)

### Migration Strategy

1. Add `file.*` events alongside `change.*`
2. Deprecation warning when `change.*` is used in flows
3. Remove `change.*` in next major version

### Flow Migration

Before:
```yaml
change.permission_asked:
  - block: "No file changes allowed"
```

After:
```yaml
file.permission_asked:
  - when: $event.operation == "write"
    block: "No file changes allowed"
```

### Example: Gating File Deletions

```yaml
file.permission_asked:
  - when: $event.operation == "delete"
    block: "File deletion requires manual approval. Please ask first."
    
  - when: $event.operation == "delete" && $event.path.contains("/node_modules/")
    allow: "OK to delete node_modules"
    
  - when: $event.operation == "write" && $event.path.endsWith(".rs")
    log: "Modifying Rust file: $event.path"
```

## Implementation Plan

### Phase 1: Add file.* events (write operations)

1. Rename `AikiChangePermissionAskedPayload` → `AikiFilePermissionAskedPayload`
2. Add `operation: FileOperation` field
3. Update Claude Code vendor to set `operation: Write`
4. Update Cursor vendor to set `operation: Write`
5. Keep `change.*` as alias (deprecated)

### Phase 2: Add file.* events (read operations)

1. Remove `ReadOnly` classification from Claude Code vendor
2. Map `Read`, `Glob`, `Grep` tools to `file.permission_asked`
3. Set appropriate `operation` field

### Phase 3: Add web.* events

1. Create `AikiWebPermissionAskedPayload` and `AikiWebDonePayload`
2. Map `WebFetch` → `web.permission_asked` with `operation: Fetch`
3. Map `WebSearch` → `web.permission_asked` with `operation: Search`

### Phase 4: Normalize shell/mcp payloads

1. Add `success: bool` to `AikiShellDonePayload` (computed from exit_code)
2. Make `exit_code`, `stdout`, `stderr` optional (available when vendor provides)
3. Update flow variable resolution to handle optional fields

### Phase 5: Remove change.* alias

1. Remove deprecated `change.*` events
2. Update documentation

## Resolved Questions

1. **Cursor read/web hooks** - Cursor only gets `file.*` events for writes (via `afterFileEdit`). Accept the limitation - Cursor has less granular observability than Claude Code.

2. **Delete operation** - Parse `rm`/`rmdir` from Bash commands and emit `file.permission_asked` with `operation: delete` instead of `shell.permission_asked`. This allows flows to gate deletions separately.

3. **Internal tools** - `Task` and `TodoRead` skip silently (internal orchestration). `LS`, `Glob`, `Grep` all map to `file.permission_asked` with `operation: read`.

4. **Grep patterns** - Include search pattern in payload (`pattern: Option<String>`) for full observability.

## References

- Current events: `cli/src/events/*.rs`
- Claude Code hooks: https://code.claude.com/docs/en/hooks
- Cursor hooks: https://cursor.com/docs/agent/hooks

# Codex Change Events

## Problem

Codex changes don't get provenance metadata (`[aiki]` blocks in JJ change descriptions) because Codex doesn't fire `change.completed` events.

Currently:
- **Claude Code, Cursor, ACP** → Fire explicit `change.completed` events → Full provenance tracking
- **Codex** → Only fires `turn.started`/`turn.completed` → No per-file provenance

## Root Cause

In `cli/src/commands/otel_receive.rs:623-626`:
```rust
CodexOtelEvent::ToolResult { conversation_id, .. } => {
    // Modified files come from JJ file tracking, not OTel
    debug_log(|| format!("OTel: tool_result: conv={} (ignored, files from JJ)", conversation_id));
}
```

The `ToolResult` event contains everything needed but is explicitly ignored.

## Available Data

### codex.tool_result (POST tool call)
From `cli/src/editors/codex/otel.rs:211-215`:
```rust
ToolResult {
    conversation_id: String,
    tool_name: Option<String>,      // e.g., "apply_patch", "read_file", "shell"
    arguments: Option<String>,      // JSON with file_path, content, etc.
}
```

### Actual Codex Tool Names (from [spec.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/spec.rs))

**File Operations:**
- `read_file` - Read local files with line numbers
- `list_dir` - List directory entries
- `grep_files` - Search files by pattern
- `view_image` - View local image files
- `apply_patch` - The ONLY file-modifying tool (handles create/edit/delete/move via patch format)

**Execution:**
- `shell` - Execute shell commands (array format)
- `shell_command` - Execute shell commands (string format)
- `exec_command` - Run commands in PTY
- `write_stdin` - Write to unified exec session stdin

**Other:**
- `web_search` - Web searches
- `update_plan` - Task planning
- `request_user_input` - User interaction
- `spawn_agent`, `send_input`, `wait`, `close_agent` - Sub-agent management
- `list_mcp_resources`, `read_mcp_resource` - MCP integration

**Key insight**: `apply_patch` handles ALL file modifications via its patch format:
- `*** Add File:` - create new file
- `*** Delete File:` - delete file
- `*** Update File:` - edit file (with optional `*** Move to:` for rename)

### codex.tool_decision (PRE tool call)
Currently parsed as `Unknown` at line 382-386. May contain:
- Tool name being called
- Arguments being passed
- Could be used for `change.permission_asked` events (deferred)

### extract_modified_files() helper
Already implemented at `cli/src/editors/codex/otel.rs:570-626`:
- Parses tool arguments JSON
- Extracts `file_path`, `path`, `filename` fields
- Handles array of files
- Resolves relative paths against cwd

**Note:** This function parses JSON arguments but does NOT parse `apply_patch` patch content format. The patch content needs separate parsing (see "Parsing apply_patch Arguments" section).

## Implementation Plan

### Phased Rollout

Given the race condition risks (see Risks section), implement in phases:

| Phase | Scope | Risk Level | JJ Mutations |
|-------|-------|------------|--------------|
| 0 | Current state (turn-level only) | None | Turn boundaries |
| 1 | Add ToolKind + read events | None | None |
| 2 | Add change events (metadata-only) | Low | `jj metaedit` only |
| 3 | Full per-file attribution | Medium | `jj new` per change |

---

### Phase 0: Current State

- `turn.started` / `turn.completed` events work
- All changes in a turn attributed to that turn
- No per-file provenance, but no race conditions

---

### Phase 1: Infrastructure + Read Events

**Goal:** Add classification infrastructure and emit read events (safe, no JJ mutations).

#### Step 1.1: Add ToolKind enum (`otel.rs`)

```rust
/// Classification of Codex tool types for event routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// Read operations: read_file, list_dir, grep_files, view_image, MCP reads
    Read,
    /// Write operations: apply_patch (the ONLY file-modifying tool)
    Write,
    /// Shell operations: shell, shell_command, exec_command, write_stdin
    Shell,
    /// Everything else: web_search, update_plan, agent tools, etc.
    Other,
}
```

#### Step 1.2: Add classification function (`otel.rs`)

```rust
/// Classify a Codex tool by operation type.
pub fn classify_tool(tool_name: &str) -> ToolKind {
    let lower = tool_name.to_lowercase();

    // Read operations
    if lower == "read_file" || lower == "list_dir" || lower == "grep_files"
        || lower == "view_image" || lower.starts_with("list_mcp")
        || lower == "read_mcp_resource" {
        return ToolKind::Read;
    }

    // Write operations (apply_patch is the ONLY file-modifying tool)
    if lower == "apply_patch" {
        return ToolKind::Write;
    }

    // Shell operations (may modify files indirectly)
    if lower == "shell" || lower == "shell_command"
        || lower == "exec_command" || lower == "write_stdin" {
        return ToolKind::Shell;
    }

    // Everything else: web_search, update_plan, agent tools, etc.
    ToolKind::Other
}
```

#### Step 1.3: Route tool_result by kind (`otel_receive.rs`)

```rust
CodexOtelEvent::ToolResult { conversation_id, tool_name, arguments } => {
    let kind = tool_name.as_deref()
        .map(otel::classify_tool)
        .unwrap_or(ToolKind::Other);

    match kind {
        ToolKind::Read => {
            maybe_emit_read_completed(&conversation_id, context, &tool_name, &arguments);
        }
        ToolKind::Write => {
            // Phase 1: Log only, don't emit change.completed yet
            debug_log(|| format!(
                "OTel: apply_patch detected: conv={} (change events deferred to Phase 2)",
                conversation_id
            ));
        }
        ToolKind::Shell => {
            // Phase 1: Log only, shell events deferred
            debug_log(|| format!(
                "OTel: shell tool detected: conv={} (shell events deferred)",
                conversation_id
            ));
        }
        ToolKind::Other => {
            debug_log(|| format!("OTel: tool_result ignored (non-file tool): {:?}", tool_name));
        }
    }
}
```

#### Step 1.4: Implement maybe_emit_read_completed()

```rust
fn maybe_emit_read_completed(
    conversation_id: &str,
    context: &CodexOtelContext,
    tool_name: &Option<String>,
    arguments: &Option<String>,
) {
    // Extract file path from arguments JSON
    let file_path = arguments.as_ref()
        .and_then(|args| extract_read_path(args))
        .unwrap_or_default();

    if file_path.is_empty() {
        debug_log(|| format!("OTel: read event skipped (no path): {:?}", tool_name));
        return;
    }

    let session = build_session_from_context(conversation_id, context);
    let cwd = context.cwd.clone().unwrap_or_default();

    let event = AikiEvent::ReadCompleted(AikiReadCompletedPayload {
        session,
        cwd,
        file_path: PathBuf::from(&file_path),
        timestamp: Utc::now(),
    });

    if let Err(e) = event_bus::dispatch(event) {
        debug_log(|| format!("Failed to dispatch read.completed: {}", e));
    }
}

/// Extract file path from read tool arguments JSON.
fn extract_read_path(arguments: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(arguments).ok()?;

    // Try common field names
    for key in &["path", "file_path", "file", "filename"] {
        if let Some(serde_json::Value::String(p)) = json.get(key) {
            return Some(p.clone());
        }
    }
    None
}
```

#### Phase 1 Success Criteria

- [ ] `ToolKind` enum and `classify_tool()` added to otel.rs
- [ ] `tool_result` handler routes by ToolKind
- [ ] Read events (`read_file`, etc.) emit `read.completed`
- [ ] Write/Shell tools logged but no events emitted yet

---

### Phase 2: Change Events (Metadata-Only Mode)

**Goal:** Emit `change.completed` for `apply_patch`, but WITHOUT running `jj new` to avoid race conditions.

#### Step 2.1: Add apply_patch parser (`otel.rs`)

```rust
/// Parse apply_patch content to extract operation type and affected files.
///
/// The patch content determines the operation:
/// - `*** Add File:` → Create
/// - `*** Delete File:` → Delete
/// - `*** Update File:` → Edit (may include `*** Move to:` for rename)
pub fn parse_apply_patch(patch_content: &str) -> Vec<(ChangeOperation, String)> {
    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_op: Option<ChangeOperation> = None;

    for line in patch_content.lines() {
        if line.starts_with("*** Add File:") {
            // Flush previous
            if let (Some(op), Some(file)) = (current_op.take(), current_file.take()) {
                results.push((op, file));
            }
            current_file = Some(line.trim_start_matches("*** Add File:").trim().to_string());
            current_op = Some(ChangeOperation::Write(WriteOperation::Create));
        } else if line.starts_with("*** Delete File:") {
            if let (Some(op), Some(file)) = (current_op.take(), current_file.take()) {
                results.push((op, file));
            }
            current_file = Some(line.trim_start_matches("*** Delete File:").trim().to_string());
            current_op = Some(ChangeOperation::Delete(DeleteOperation::default()));
        } else if line.starts_with("*** Update File:") {
            if let (Some(op), Some(file)) = (current_op.take(), current_file.take()) {
                results.push((op, file));
            }
            current_file = Some(line.trim_start_matches("*** Update File:").trim().to_string());
            current_op = Some(ChangeOperation::Write(WriteOperation::Edit));
        } else if line.starts_with("*** Move to:") {
            // Rename within Update File section - upgrade operation to Move
            current_op = Some(ChangeOperation::Move(MoveOperation::default()));
        }
    }

    // Flush final
    if let (Some(op), Some(file)) = (current_op, current_file) {
        results.push((op, file));
    }

    results
}
```

#### Step 2.2: Implement maybe_emit_change_completed()

```rust
fn maybe_emit_change_completed(
    conversation_id: &str,
    context: &CodexOtelContext,
    tool_name: &Option<String>,
    arguments: &Option<String>,
) {
    let patch_content = match arguments {
        Some(args) => {
            // apply_patch arguments contain the patch as a JSON string field
            let json: serde_json::Value = match serde_json::from_str(args) {
                Ok(j) => j,
                Err(_) => {
                    debug_log(|| "OTel: apply_patch args not valid JSON");
                    return;
                }
            };
            // Try "patch" or "content" fields
            json.get("patch")
                .or_else(|| json.get("content"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default()
        }
        None => return,
    };

    if patch_content.is_empty() {
        debug_log(|| "OTel: apply_patch skipped (no patch content)");
        return;
    }

    let operations = otel::parse_apply_patch(&patch_content);
    if operations.is_empty() {
        debug_log(|| "OTel: apply_patch skipped (no operations parsed)");
        return;
    }

    let session = build_session_from_context(conversation_id, context);
    let cwd = context.cwd.clone().unwrap_or_default();

    // Emit one change.completed per file operation
    for (operation, file_path) in operations {
        let event = AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
            session: session.clone(),
            cwd: cwd.clone(),
            file_path: cwd.join(&file_path),
            operation,
            tool: tool_name.clone().unwrap_or_else(|| "apply_patch".to_string()),
            timestamp: Utc::now(),
        });

        if let Err(e) = event_bus::dispatch(event) {
            debug_log(|| format!("Failed to dispatch change.completed for {}: {}", file_path, e));
        }
    }
}
```

#### Step 2.3: Update hooks.yaml for metadata-only mode

**Critical:** For Codex, the `change.completed` hook must NOT run `jj new` to avoid race conditions. Add an agent-type guard:

```yaml
# In cli/src/flows/core/hooks.yaml

change.completed:
  # Embed provenance in current change
  - run: |
      jj metaedit -r @ -m "$(jj log -r @ --no-graph -T description)

      [aiki]
      agent=${{ event.session.agent_type }}
      session=${{ event.session.session_id }}
      tool=${{ event.tool }}
      file=${{ event.file_path }}
      [/aiki]"

  # Only create new change for synchronous agents (NOT Codex)
  # Codex handles change boundaries at turn.completed instead
  - if: ${{ event.session.agent_type != "codex" }}
    run: jj new
```

This ensures:
- All agents get provenance metadata via `jj metaedit`
- Only synchronous agents (Claude Code, Cursor) run `jj new` per change
- Codex relies on turn boundaries for JJ change separation

#### Phase 2 Success Criteria

- [ ] `parse_apply_patch()` extracts operations and file paths
- [ ] `change.completed` events emitted for `apply_patch` operations
- [ ] hooks.yaml guards `jj new` with agent type check
- [ ] Codex changes show `[aiki]` metadata in `jj show`
- [ ] No race conditions (verified by rapid consecutive edits)

---

### Phase 3: Full Per-File Attribution (Deferred)

**Goal:** Full per-file change separation for Codex (each `apply_patch` creates its own JJ change).

**Why deferred:** Requires event queuing and debouncing to avoid race conditions. The async nature of OTel means events can arrive out of order or while Codex is mid-operation.

**Required infrastructure:**
- Event queue in `otel_receive.rs`
- Debounce/batch at turn boundaries
- File lock for JJ operations
- Turn end triggers queue flush

**Consider implementing when:**
- Per-file provenance becomes a real need
- Users request fine-grained `aiki blame` for Codex
- Race condition mitigations are proven in Phase 2

---

## Files to Modify

| File | Phase | Changes |
|------|-------|---------|
| `cli/src/editors/codex/otel.rs` | 1 | Add `ToolKind`, `classify_tool()` |
| `cli/src/editors/codex/otel.rs` | 2 | Add `parse_apply_patch()` |
| `cli/src/commands/otel_receive.rs` | 1 | Route `tool_result` by ToolKind |
| `cli/src/commands/otel_receive.rs` | 1 | Add `maybe_emit_read_completed()` |
| `cli/src/commands/otel_receive.rs` | 2 | Add `maybe_emit_change_completed()` |
| `cli/src/flows/core/hooks.yaml` | 2 | Add agent-type guard on `jj new` |

**Imports needed:**
- `AikiReadCompletedPayload`
- `AikiChangeCompletedPayload`
- `ChangeOperation`, `WriteOperation`, `DeleteOperation`, `MoveOperation`

---

## Risks

### 1. CRITICAL: OTel Event Race Conditions

**The Problem:**
- **Claude Code/Cursor** (synchronous): Edit → file written → `change.completed` → hooks run → next tool. Serialized.
- **Codex** (asynchronous): `apply_patch` → OTel event sent (batched/async) → Codex continues → `aiki otel receive` processes later

**Race Scenarios:**

1. **Interleaved JJ Operations**:
   - Codex writes file A → Event 1 sent
   - Codex writes file B → Event 2 sent
   - Event 1 arrives → hook runs `jj metaedit` + `jj new`
   - Event 2 arrives before `jj new` completes → wrong change modified

2. **Split During Active Editing**:
   - Hook runs `jj new` to separate changes
   - Codex is mid-operation on another file
   - That edit ends up in the NEW change, breaking attribution

**Mitigation (Phase 2):** Metadata-only mode
- Emit `change.completed` events
- Run `jj metaedit` (safe, idempotent)
- Skip `jj new` for Codex (let turn boundaries separate changes)
- Guard in hooks.yaml: `if: ${{ event.session.agent_type != "codex" }}`

### 2. Patch Parsing Complexity

- `apply_patch` format may have edge cases
- **Mitigation:** Start with header parsing (`*** Add/Delete/Update File:`), enhance as needed
- Patch content is in `arguments` JSON field (key: `"patch"` or `"content"`)

### 3. Missing CWD

- Some events may lack cwd
- **Mitigation:** Fall back to session file lookup (already implemented)

### 4. Shell Commands (Deferred)

- Shell can modify files indirectly without explicit paths
- **Decision:** Defer shell.completed events. Accept that shell provenance is best-effort or requires different approach (e.g., file system watchers).

---

## Testing

### Phase 1 Testing

```bash
# Enable debug logging
export AIKI_DEBUG=1

# Start Codex in a test repo
codex

# Make Codex read a file
> read the contents of README.md

# Check debug output shows:
# - ToolKind::Read classification
# - read.completed event dispatched
```

### Phase 2 Testing

```bash
# Make Codex edit a file
> add a comment to main.rs

# Verify JJ metadata:
jj show @
# Should show [aiki] block with:
#   agent=codex
#   tool=apply_patch
#   file=src/main.rs

# Stress test: rapid consecutive edits
> add comments to file1.rs, file2.rs, and file3.rs

# Verify no race conditions:
jj log --no-graph -r 'description("agent=codex")'
# All changes should have correct attribution
```

---

## Success Criteria

### Phase 1
- [ ] `ToolKind` enum classifies Codex tools correctly
- [ ] `read.completed` events fire for read operations
- [ ] Write/Shell tools logged but not emitting events yet

### Phase 2
- [ ] `change.completed` events fire for `apply_patch`
- [ ] `jj show <change>` shows `[aiki]` metadata for Codex changes
- [ ] `aiki authors` correctly attributes Codex changes
- [ ] `task=` appears when Codex is working on a task
- [ ] No race conditions with rapid consecutive edits
- [ ] `jj new` only runs for non-Codex agents

### Phase 3 (Deferred)
- [ ] Per-file JJ change separation for Codex
- [ ] Event queue prevents race conditions
- [ ] Turn boundaries flush queued events

---

## References

- [Codex tool specs](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/spec.rs)
- [apply_patch instructions](https://github.com/openai/codex/blob/main/codex-rs/apply-patch/apply_patch_tool_instructions.md)

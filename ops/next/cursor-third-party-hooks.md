---
status: draft
---

# Cursor Third-Party Hooks: Unified Hook Configuration

**Date**: 2026-02-20
**Status**: Draft
**Purpose**: Migrate Cursor hooks to use Cursor's new third-party hooks system, eliminating the separate `~/.cursor/hooks.json` configuration in favor of a single `.claude/settings.json` source of truth.

**Related Documents**:
- [Cursor Hooks Docs](https://cursor.com/docs/agent/hooks) - Official Cursor hooks reference
- [Claude Code Hooks Docs](https://code.claude.com/docs/en/hooks) - Official Claude Code hooks reference
- `cli/src/editors/cursor/` - Current Cursor hook implementation
- `cli/src/editors/claude_code/` - Current Claude Code hook implementation
- `cli/src/config.rs` - Hook installation (both `install_claude_code_hooks_global` and `install_cursor_hooks_global`)

---

## Executive Summary

Cursor 2.5 now supports "third-party hooks" — it can load and execute hooks defined in `.claude/settings.json`, using Claude Code's event naming conventions (PascalCase: `PreToolUse`, `PostToolUse`, `SessionStart`, etc.). This means we can stop maintaining a separate `~/.cursor/hooks.json` and instead define hooks once in `.claude/settings.json`, getting both Claude Code and Cursor support from a single configuration.

The catch: while Cursor fires the same event names, the **payload format** it sends on stdin is still Cursor's own (with `conversationId`, `workspaceRoots`, `eventName`, etc. — not Claude Code's `session_id`, `cwd`, `hook_event_name`). So the parsing layer must auto-detect which agent is calling, rather than relying on an explicit `--agent` flag.

---

## Current State

### Two Separate Configurations

**Claude Code** (`~/.claude/settings.json`):
```json
{
  "hooks": {
    "SessionStart": [{ "matcher": "startup", "hooks": [{ "type": "command", "command": "aiki hooks stdin --agent claude-code --event SessionStart" }] }],
    "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": "aiki hooks stdin --agent claude-code --event UserPromptSubmit" }] }],
    "PreToolUse": [{ "matcher": "Edit|Write|...", "hooks": [{ "type": "command", "command": "aiki hooks stdin --agent claude-code --event PreToolUse" }] }],
    "PostToolUse": [{ "matcher": "Edit|Write|...", "hooks": [{ "type": "command", "command": "aiki hooks stdin --agent claude-code --event PostToolUse" }] }],
    "Stop": [...],
    "SessionEnd": [...]
  }
}
```

**Cursor** (`~/.cursor/hooks.json`):
```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [{ "command": "aiki hooks stdin --agent cursor --event beforeSubmitPrompt" }],
    "afterFileEdit": [{ "command": "aiki hooks stdin --agent cursor --event afterFileEdit" }],
    "beforeShellExecution": [{ "command": "aiki hooks stdin --agent cursor --event beforeShellExecution" }],
    "stop": [{ "command": "aiki hooks stdin --agent cursor --event stop" }],
    ...
  }
}
```

### Two Separate Code Paths

| Layer | Claude Code | Cursor |
|-------|-------------|--------|
| **Config install** | `install_claude_code_hooks_global()` writes to `~/.claude/settings.json` | `install_cursor_hooks_global()` writes to `~/.cursor/hooks.json` |
| **Event parser** | `claude_code/events.rs` — discriminates by `hook_event_name` field | `cursor/events.rs` — discriminates by `eventName` field |
| **Payload structs** | `session_id`, `cwd`, `tool_name`, `tool_input` (JSON Value) | `conversationId`, `workspaceRoots`, `cursorVersion`, `toolInput` (String) |
| **Output formatter** | `claude_code/output.rs` — `hookSpecificOutput`, `permissionDecision`, etc. | `cursor/output.rs` — `continue: true/false`, `user_message`, etc. |
| **Session creation** | Uses `session_id` directly | Maps `conversationId` to session |

### What Cursor's Third-Party Hooks Changes

Cursor now:
1. **Loads hooks from `.claude/settings.json`** — it reads Claude Code's hook config natively
2. **Fires hooks using PascalCase event names** (when loaded from Claude's config) — `PreToolUse`, `PostToolUse`, `SessionStart`, etc.
3. **Sets `CLAUDE_PROJECT_DIR`** environment variable as an alias for the workspace root
4. **Supports the same event set**: `preToolUse`/`PreToolUse`, `postToolUse`/`PostToolUse`, `sessionStart`/`SessionStart`, `sessionEnd`/`SessionEnd`, `preCompact`/`PreCompact`
5. **Sends its own payload format** — the JSON on stdin still uses Cursor's field names (`conversationId`, `workspaceRoots`, etc.)

---

## How It Works

### Single Configuration, Auto-Detected Agent

Instead of writing hooks to two files, we write only to `.claude/settings.json`. Cursor reads from there via its third-party hooks support. The command drops the `--agent` flag and auto-detects which agent is calling by inspecting the JSON payload on stdin.

**Detection heuristic**: Claude Code payloads contain `hook_event_name` and `session_id`. Cursor payloads contain `eventName` and `conversationId`. These fields are mutually exclusive — checking for either is sufficient.

```
┌─────────────┐     ┌──────────────────────┐     ┌──────────────────┐
│ Claude Code  │────▶│ .claude/settings.json │────▶│ aiki hooks stdin │
│  (native)    │     │   (single config)     │     │  --event PreTool │
└─────────────┘     └──────────────────────┘     │                  │
                              ▲                   │  Auto-detects:   │
┌─────────────┐               │                   │  - Claude Code   │
│   Cursor     │───────────────┘                   │  - Cursor        │
│  (3rd-party) │  reads Claude config              │                  │
└─────────────┘                                   │  Dispatches to   │
                                                  │  correct parser  │
                                                  └──────────────────┘
```

### Event Mapping (Unified)

With Cursor loading from Claude's config, both agents fire the same event names:

| Claude Config Event | Claude Code Fires | Cursor Fires | Aiki Event |
|--------------------|--------------------|--------------|------------|
| `SessionStart` | `SessionStart` | `SessionStart` | `session.started` |
| `UserPromptSubmit` | `UserPromptSubmit` | `UserPromptSubmit` | `turn.started` |
| `PreToolUse` | `PreToolUse` | `PreToolUse` | `change/shell/mcp.permission_asked` |
| `PostToolUse` | `PostToolUse` | `PostToolUse` | `change/shell/mcp.completed` |
| `Stop` | `Stop` | `Stop` | `turn.completed` |
| `SessionEnd` | `SessionEnd` | `SessionEnd` | `session.ended` |

**Cursor-only events that have no Claude equivalent:**
- `afterFileEdit` — Cursor fires this independently; with `PostToolUse` available from Claude's config, this becomes redundant
- `beforeShellExecution` / `afterShellExecution` — subsumed by `PreToolUse` / `PostToolUse`
- `beforeMCPExecution` / `afterMCPExecution` — subsumed by `PreToolUse` / `PostToolUse`

### Payload Differences (Still Exist)

Even though the event names unify, the payload JSON differs:

**Claude Code sends:**
```json
{
  "hook_event_name": "PreToolUse",
  "session_id": "abc123",
  "cwd": "/home/user/project",
  "tool_name": "Edit",
  "tool_input": { "file_path": "src/main.rs", "old_string": "...", "new_string": "..." }
}
```

**Cursor sends (when firing from Claude config):**
```json
{
  "eventName": "PreToolUse",
  "conversationId": "conv-xyz",
  "generationId": "gen-789",
  "model": "claude-sonnet-4-6-20250514",
  "cursorVersion": "2.5.0",
  "workspaceRoots": ["/home/user/project"],
  "toolName": "edit",
  "toolInput": "{ \"file_path\": \"src/main.rs\" }"
}
```

### Response Format Differences (Still Exist)

**Claude Code expects:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "additionalContext": "..."
  }
}
```

**Cursor expects:**
```json
{
  "continue": true,
  "agent_message": "..."
}
```

The output formatting layer (`output.rs`) must still be agent-specific. Auto-detection determines which formatter to use.

---

## Implementation Plan

### Phase 1: Auto-Detection Layer

Add agent auto-detection to `cli/src/editors/mod.rs`:

```rust
/// Detect which agent is calling based on the raw JSON payload
pub fn detect_agent(raw_json: &serde_json::Value) -> AgentType {
    if raw_json.get("hook_event_name").is_some() || raw_json.get("session_id").is_some() {
        AgentType::ClaudeCode
    } else if raw_json.get("eventName").is_some() || raw_json.get("conversationId").is_some() {
        AgentType::Cursor
    } else {
        // Fallback: check for codex-specific fields
        AgentType::Unknown
    }
}
```

Modify `cli/src/commands/hooks.rs` to:
1. Read stdin into a raw `serde_json::Value`
2. Auto-detect agent if `--agent` is not provided
3. Pass raw JSON to the appropriate editor's event builder (avoid double-parsing)

Keep `--agent` as an optional override for backward compatibility and for agents (like Codex) that can't be auto-detected from payloads.

### Phase 2: Unify Config Installation

Modify `install_cursor_hooks_global()` to become a no-op or redirect:
- Stop writing to `~/.cursor/hooks.json`
- Instead, ensure Claude Code hooks are installed (call `install_claude_code_hooks_global()`)
- Print a message: "Cursor hooks are now provided via ~/.claude/settings.json (third-party hooks)"

Update `aiki init` and `aiki doctor` to:
- Check that `.claude/settings.json` has hooks (covers both Claude and Cursor)
- Warn if stale `~/.cursor/hooks.json` exists with aiki hooks (offer to clean up)
- Verify Cursor can read from `.claude/settings.json` (check `CLAUDE_PROJECT_DIR` is set in Cursor contexts)

### Phase 3: Cursor Event Parser Migration

Update `cli/src/editors/cursor/events.rs` to handle PascalCase event names from the Claude config:
- The `CursorEvent` enum currently expects camelCase (`beforeSubmitPrompt`, `afterFileEdit`, etc.)
- Add PascalCase variants (`PreToolUse`, `PostToolUse`, `SessionStart`, etc.) or make parsing case-insensitive
- Map the new events to the same Aiki events as the old ones

The Cursor payload structs remain unchanged — only the discriminant field changes from old Cursor event names to Claude-compatible event names.

### Phase 4: Clean Up Legacy Code

- Remove `install_cursor_hooks_global()` (or keep as deprecated wrapper)
- Update docs and help text to reflect single-config approach
- Remove `~/.cursor/hooks.json` migration logic once users have had time to transition

---

## Open Questions

1. **Cursor payload format when loading from Claude config**: Does Cursor send its own payload format (conversationId, workspaceRoots) or does it adapt to Claude's format (session_id, cwd) when loading from `.claude/settings.json`? This needs verification. The spec assumes Cursor always sends its own format.

2. **Cursor `matcher` support**: Does Cursor honor the `matcher` field from Claude's hook config (e.g., `"matcher": "Edit|Write|Bash"`)? If not, Cursor would fire hooks for ALL tools regardless of matcher, which could cause performance issues.

3. **Cursor `timeout` support**: Does Cursor respect the `timeout` field from Claude's config? If not, hooks could hang indefinitely.

4. **Backward compatibility**: Should we keep writing to `~/.cursor/hooks.json` during a transition period? Older Cursor versions (pre-2.5) won't read from `.claude/settings.json`.

5. **Hook response format negotiation**: When Cursor fires a hook loaded from Claude's config, does it expect Claude-format responses (hookSpecificOutput) or Cursor-format responses (continue: true/false)? If Cursor expects Claude format, the output layer could potentially be unified too.

---

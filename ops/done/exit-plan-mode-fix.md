# Fix: ExitPlanMode Workspace Absorption

## Problem

Files written during plan mode are not absorbed back to the main repo when the agent requests to exit plan mode. The user can't see what was written during planning when asked to approve the exit.

## Solution

Directly call `workspace_absorb_all()` in the Claude Code event handler when `ExitPlanMode` PreToolUse fires, before the user sees the approval prompt.

## Changes Made

### 1. Add ExitPlanMode/EnterPlanMode to known tools

**File**: `cli/src/editors/claude_code/tools.rs` (line 191)

```rust
// Before:
"Task" | "TodoRead" | "TodoWrite" => Some(ClaudeTool::Internal(tool_name.to_string())),

// After:
"Task" | "TodoRead" | "TodoWrite" | "EnterPlanMode" | "ExitPlanMode"
    => Some(ClaudeTool::Internal(tool_name.to_string())),
```

This prevents the "unknown tool" warning for these Claude Code internal tools.

### 2. Absorb workspace on ExitPlanMode request

**File**: `cli/src/editors/claude_code/events.rs` (lines 189-196)

```rust
// Before:
ToolType::Internal => AikiEvent::Unsupported,

// After:
ToolType::Internal => {
    // Special handling for ExitPlanMode: absorb workspace before showing approval prompt
    if payload.tool_name == "ExitPlanMode" {
        let session = create_session(&payload.session_id, &payload.cwd);
        let _ = crate::flows::core::workspace_absorb_all(&session);
    }
    AikiEvent::Unsupported
}
```

When PreToolUse fires for ExitPlanMode, we directly call `workspace_absorb_all()` to absorb any files written during planning before showing the user the approval prompt.

## Why This Works

- **workspace_absorb_all is idempotent** - safe to call multiple times
- **User sees plan files before approving** - files are absorbed before the approval dialog
- **Claude Code specific** - no generic events needed since only Claude Code has plan mode
- **Simple and direct** - no event hijacking, no new event types, just fixes the immediate problem
- **The real turn.completed still runs** - normal end-of-turn absorption finds nothing left to do

## Files Changed

| File | Lines Changed | Description |
|------|---------------|-------------|
| `cli/src/editors/claude_code/tools.rs` | 2 | Add plan mode tools to known internal tools |
| `cli/src/editors/claude_code/events.rs` | 7 | Call workspace_absorb_all on ExitPlanMode |

**Total**: 2 files, 9 lines

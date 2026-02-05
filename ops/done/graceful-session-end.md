# Graceful Session End for Task-Driven Sessions

**Date**: 2026-02-03
**Status**: Ready
**Purpose**: Auto-end agent sessions when their driving task closes

**Related Documents**:
- [Workflow Commands Overview](workflow-commands.md) - Commands that spawn task-driven sessions
- [Task Execution: aiki task run](../done/run-task.md) - Background task execution

---

## Executive Summary

When `aiki spec` spawns an interactive agent session, the session is **task-driven** - it exists to work on a specific spec task. When that task closes, the session should end automatically.

This document specifies:
1. How sessions track their driving task
2. How task close triggers session end
3. The `session.end` hook action

**Scope**: This applies to `aiki spec` only. Background task runs (`aiki task run --async`) continue to use external termination.

---

## Problem

Currently:
- `aiki task run --async` spawns background sessions that can be killed externally
- `aiki spec` spawns interactive sessions, but when the spec task closes, the session continues
- Users must manually exit Claude after completing a spec session

**Goal**: `aiki spec` sessions should end gracefully when the spec task closes.

---

## Design

### Session Metadata

Sessions track their driving task via the `task` field:

```
[aiki]
session_id=claude-session-abc123
agent=claude-code
mode=interactive
task=wvuzvuvuutzsvxwkvntrvnmqywruxqwm
parent_pid=12345
[/aiki]
```

**Fields:**
- `task` - The task ID driving this session (if any)
- `mode` - `interactive` or `background`
- `parent_pid` - PID of the agent process (for termination)

### Environment Variable

`AIKI_TASK` - Set by workflow commands when spawning sessions.

```bash
# aiki spec sets this before spawning Claude
AIKI_TASK=<spec-task-id> claude ...
```

The session system reads this env var and records it in session metadata.

### Behavior Matrix

| Mode | `task` field | On Task Close |
|------|--------------|---------------|
| Interactive | Set | **Auto-end session** (graceful) |
| Interactive | Not set | No auto-end (normal user session) |
| Background | Set | External kill via `terminate_background_task()` |
| Background | Not set | N/A (background always has task) |

**Key insight**: Normal user sessions (started by typing `claude` directly) have no `task` field, so they are unaffected.

---

## Implementation

### 1. Rename Session Field

**File:** `cli/src/session/mod.rs`

Rename `runner_task` to `task`:
- Field: `runner_task: Option<String>` → `task: Option<String>`
- Method: `runner_task()` → `task()`
- Method: `with_runner_task()` → `with_task()`
- Method: `with_runner_task_from_env()` → `with_task_from_env()`

### 2. Rename Environment Variable

**Files:** `cli/src/session/mod.rs`, `cli/src/agents/runtime/*.rs`

Rename `AIKI_RUNNER_TASK` to `AIKI_TASK`:
```rust
// Before
std::env::var("AIKI_RUNNER_TASK")

// After
std::env::var("AIKI_TASK")
```

### 3. Add `session.end` Hook Action

**File:** `cli/src/flows/types.rs`

```rust
/// Session end action - terminates the current session gracefully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndAction {
    /// Reason for ending (logged)
    #[serde(rename = "session.end")]
    pub session_end: String,

    #[serde(default)]
    pub on_failure: OnFailure,
}
```

**File:** `cli/src/flows/engine.rs`

```rust
fn execute_session_end(action: &SessionEndAction, state: &mut AikiState) -> Result<ActionResult> {
    // Get current session's parent PID
    let parent_pid = state.session.parent_pid()
        .ok_or_else(|| AikiError::Other(anyhow!("No parent PID for session")))?;

    // Fork a process that waits then sends SIGTERM
    // This allows the hook to complete before termination
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        unsafe { libc::kill(parent_pid as libc::pid_t, libc::SIGTERM) };
    });

    Ok(ActionResult::success())
}
```

### 4. Add `task.closed` Hook Event

**File:** `cli/src/flows/types.rs`

```rust
/// task.closed event handler
#[serde(rename = "task.closed", default)]
pub task_closed: Vec<HookStatement>,
```

**File:** `cli/src/flows/sugar.rs`

Add `"task.closed"` to event list.

### 5. Add Core Hook Logic

**File:** `cli/src/flows/core/hooks.yaml`

```yaml
# Auto-end interactive task-driven sessions when their task closes
task.closed:
  - if: $task.id == $session.task.id && $session.mode == "interactive"
    then:
      - log: "Task $task.id closed, ending session"
      - session.end: "Task completed"
```

### 6. Expose `$session.task.id` Variable

**File:** `cli/src/flows/engine.rs`

In `create_resolver()`, add:
```rust
if let Some(task_id) = state.session.task() {
    resolver.add_var("session.task.id".to_string(), task_id.to_string());
}
resolver.add_var("session.mode".to_string(), state.session.mode().to_string());
```

### 7. Update Spec Command

**File:** `cli/src/commands/spec.rs`

Set `AIKI_TASK` when spawning Claude:
```rust
Command::new("claude")
    .env("AIKI_TASK", &spec_task_id)
    // ...
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `cli/src/session/mod.rs` | Rename `runner_task` → `task`, `AIKI_RUNNER_TASK` → `AIKI_TASK` |
| `cli/src/agents/runtime/claude_code.rs` | Update env var name |
| `cli/src/agents/runtime/codex.rs` | Update env var name |
| `cli/src/tasks/runner.rs` | Update env var and field references |
| `cli/src/flows/types.rs` | Add `SessionEndAction`, `task.closed` event |
| `cli/src/flows/engine.rs` | Add `execute_session_end()`, expose `$session.task.id` |
| `cli/src/flows/sugar.rs` | Add `"task.closed"` event |
| `cli/src/flows/core/hooks.yaml` | Add auto-end hook logic |
| `cli/src/commands/spec.rs` | Set `AIKI_TASK` when spawning Claude |

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| No parent PID in session | Log warning, skip termination |
| SIGTERM fails (process gone) | Ignore (ESRCH), session already ended |
| Hook fails before session.end | Session continues (fail-safe) |
| Multiple tasks close at once | Each triggers hook, only matching session ends |

---

## Testing

### Unit Tests

1. `test_session_task_field` - Verify `task` field is read/written correctly
2. `test_aiki_task_env_var` - Verify env var is captured in session
3. `test_session_end_action` - Verify deferred termination logic

### Integration Tests

1. `test_spec_session_auto_end` - Run `aiki spec`, close spec task, verify Claude exits
2. `test_normal_session_unaffected` - Start Claude directly, close tasks, verify no auto-exit
3. `test_background_task_unchanged` - Verify `aiki task run --async` still uses external kill
4. `test_spec_subtask_close_no_end` - Close a subtask within spec, verify session continues (only parent spec task triggers end)

---

## Open Questions

1. ~~**Field naming**: `runner_task` vs `driving_task` vs `task`?~~
   - Decision: `task` (simplest)

2. ~~**Env var naming**: `AIKI_RUNNER_TASK` vs `AIKI_TASK`?~~
   - Decision: `AIKI_TASK` (matches field name)

3. ~~**Termination delay**: How long to wait before SIGTERM?~~
   - Decision: 100ms (allows hook to complete)

4. ~~**Graceful vs immediate**: Should we try SIGTERM then SIGKILL?~~
   - Decision: Just SIGTERM for v1. Claude handles it gracefully.

---

## Future Enhancements

1. **Configurable delay** - Allow hooks to specify termination delay
2. **Exit message** - Display message to user before session ends
3. **Cleanup actions** - Run additional cleanup before termination
4. **Session handoff** - Transfer session to another task instead of ending

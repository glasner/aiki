# Background Task Execution

**Status**: Implemented
**Related**: [Review and Fix](review-and-fix.md)

---

## Summary

Add ability to run tasks in the background and wait for their completion.

## Changes

### `aiki task run --async`

New flag to spawn agent and return immediately instead of blocking.

```bash
aiki task run <task_id> --async
```

**Behavior:**
- Spawns agent process for the task
- Returns immediately with task ID
- Agent runs in background until task completes

**Output:**
```xml
<aiki_task cmd="run" status="ok">
  <started task_id="xqrmnpst" async="true">
    Task started asynchronously.
  </started>
</aiki_task>
```

### `aiki wait <task_id>`

New command to block until a task reaches terminal state.

```bash
aiki wait [<task_id>]
```

**Arguments:**
- `<task_id>` - Task ID to wait for (reads from stdin if not provided)

**Behavior:**
- Blocks until task reaches terminal state (closed, stopped, or failed)
- Uses exponential backoff polling: 100ms → 200ms → 400ms → ... → 2s max
- Outputs task ID to stdout (passthrough for piping)
- Exit code 0 if task completed successfully
- Exit code 1 if task failed or was stopped

**Piping support:**
```bash
# Background task, wait, then do something
aiki task run xqrmnpst --async | aiki wait
```

### `aiki task stop` (Extended)

Extend existing command to terminate background agent processes.

**Current behavior:** Stops a running task by updating its status.

**New behavior:** Also terminates the background agent process if one is running.

## Implementation

### Agent Runtime

Add `spawn_background()` method to `AgentRuntime` trait:

```rust
// agents/runtime/mod.rs
trait AgentRuntime {
    fn spawn_blocking(&self, options: RunOptions) -> Result<SessionResult>;
    fn spawn_background(&self, options: RunOptions) -> Result<BackgroundHandle>;
}

struct BackgroundHandle {
    pid: u32,
    task_id: String,
}
```

### Background Task Tracking

Track background tasks in `tasks/runner.rs`:

- Store PID when spawning background agent
- Poll task status for `wait` command
- Kill process on `stop` command

### Files

- `cli/src/commands/task.rs` - Add `--async` flag, extend `stop`
- `cli/src/commands/wait.rs` - New wait command
- `cli/src/tasks/runner.rs` - Background execution logic
- `cli/src/agents/runtime/mod.rs` - Add `spawn_background()` method

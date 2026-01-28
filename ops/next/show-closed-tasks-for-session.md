# Plan: Add tasks completed to `aiki session show` output

## Approach

Compute tasks-completed at **display time** by loading task events from the `aiki/tasks` branch and correlating them with turn time windows. No schema changes needed.

**Why display-time**: Works retroactively for existing sessions, only 1 file to change, no migration needed.

## File to modify

`cli/src/commands/session.rs`

## Changes

### 1. Add imports

```rust
use crate::tasks;
use crate::tasks::types::{TaskEvent, TaskOutcome};
use std::path::Path;
```

### 2. Add helper struct

```rust
struct TaskClosedInTurn {
    task_id: String,
    task_name: String,
    outcome: TaskOutcome,
    closed_at: DateTime<chrono::Utc>,
}
```

### 3. Add `get_tasks_closed_in_session()` function

Loads task events, builds a `task_id -> session_id` map from `TaskEvent::Started` events, materializes tasks for names, then collects `TaskEvent::Closed` events for tasks owned by the target session. Returns sorted by timestamp. Gracefully returns empty vec if tasks branch doesn't exist.

### 4. Modify `run_show()` display logic

Before the event display loop:
- Call `get_tasks_closed_in_session()` with the global aiki dir and session ID
- Build prompt timestamp list from matching events to define turn time windows
- Assign each closed task to a turn based on its timestamp falling within that turn's window
- Store as `HashMap<u32, Vec<&TaskClosedInTurn>>`

In the `Response` match arm, after existing files display:
- Look up tasks for this turn number
- Print each task as: `  Tasks: <name> (<short-id>) [done/won't do]`

### 5. Output format

```
  🤖 response  (14:32, 2m)
  Fixed the authentication bug by...
  Files: src/auth.rs, src/middleware.rs
  Tasks: Fix auth bug (a1b2c3d4) [done]
```

## Edge cases

- **No tasks branch**: `read_events` returns error -> return empty vec
- **No tasks for session**: No "Tasks:" line printed (clean output)
- **Task closed outside any turn window**: Silently omitted
- **Task ID display**: Show first 8 chars for readability

## Verification

```bash
cargo build -p aiki
aiki session show <session-id>  # verify tasks appear under responses
```

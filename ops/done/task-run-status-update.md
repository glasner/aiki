# Task Run Status Updates

**Date**: 2026-02-05
**Status**: Ready for Review
**Owner**: TBD

---

## Summary

Provide real-time status updates when users run tasks via `aiki task run`, `aiki review`, or `aiki fix`. Updates show subtask progress and comments as they are added by the working agent.

---

## Problem

When running tasks synchronously:
- Users have no visibility into what the agent is doing
- Subtask creation/completion is invisible until task finishes
- Comments added during work (progress updates) are not shown
- User is left staring at a blank terminal while waiting

---

## Goals

1. **Real-time visibility** - Show subtasks and comments as they're created
2. **Non-intrusive** - Updates shouldn't interrupt the agent's work
3. **Consistent interface** - Same update mechanism for `task run`, `review`, `fix`
4. **Terminal-friendly** - Works well in CLI environments

---

## Non-Goals

- GUI/web dashboard (CLI only for now)
- Push notifications to external systems
- Historical playback of status changes

---

## Design

### 1. Real-Time Status Updates in Existing Commands

**No new commands introduced.** Status updates are shown by enhancing existing commands:

- `aiki task run <id>` - Shows progress during sync execution (default behavior)
- `aiki review <id>` - Shows progress during sync execution (default behavior)
- `aiki fix <id>` - Shows progress during sync execution (default behavior)

When running in sync mode (waiting for completion), these commands already block. We enhance them to poll and display task events while waiting, giving users visibility into agent progress.

### 2. Output Format

**Live visualization that updates in place**, showing task as a tree:

**Example 1: Task with subtasks**
```
▶ Review changes (abc123...)                  [5m 30s]
├─ ✓ Digest changes
├─ ▶ Review code                              [2m 15s]
│  └─ 💬 Found 3 issues in auth.rs
├─ ○ Write summary
└─ ○ Create followup tasks

[Ctrl+C to detach]
```

**Example 2: Simple task without subtasks**
```
▶ Fix authentication bug (def456...)          [1m 45s]
└─ 💬 Updated auth handler to validate tokens
└─ 💬 Added error handling for invalid sessions

[Ctrl+C to detach]
```

The display:
- **Updates in place** (overwrites previous state, no scrolling log)
- **Tree structure** with parent task at the top
- Subtasks shown as children with tree lines (├─, └─)
- Shows status for each task (✓ done, ▶ in progress, ○ pending)
- Shows elapsed time for parent and in-progress subtasks
- Latest comment shown indented under relevant subtask
- Minimal, clean interface that fits in terminal

Symbols:
- `✓` = completed
- `▶` = in progress
- `○` = pending/open
- `✗` = failed/stopped
- `💬` = comment/progress note

### 3. Implementation Approach

**Polling with live terminal updates:**
- Poll task events at interval (e.g., 500ms)
- Materialize full task state (parent + all subtasks)
- Only re-render when state has changed (not on every poll tick)
- Use cursor save/restore to update in place without clearing screen
- Track elapsed time for in-progress subtasks

**Terminal handling:**
- Check if stderr is a TTY before showing visualization
- Use libraries like `crossterm` or `termion` for terminal control
- Graceful fallback: if not a TTY, show nothing (silent mode)

**Ctrl+C behavior (detach):**
- On Ctrl+C, the visualization exits but the agent continues running in the background
- The task remains in `in_progress` state
- Command exits with code 0 (successful detach)
- Final output shows: "Detached. Task {id} still running. Use `aiki task show {id}` to check status."
- User can re-attach later or wait for task completion notification

### 4. Integration Points

#### `aiki task run <id>`
**Sync mode (default):**
- Show real-time status updates while waiting for agent to complete
- Poll every 500ms, display new events (subtasks, comments)
- Exit when task reaches terminal state

**Async mode (`--async`):**
- No status updates (returns immediately)
- Current behavior unchanged

#### `aiki review <id>` and `aiki fix <id>`
These commands create a task (review or fix) and then call `task_run()` internally, so they get the same behavior:

**Sync mode (default):**
- Show real-time status updates while waiting for completion
- Uses the same visualization as `task run`

**Start mode (`--start`):**
- No status updates (hands off to caller immediately)
- Current behavior unchanged

**Async mode (`--async`):**
- No status updates (returns immediately)
- Current behavior unchanged

---

## Data Model

No schema changes required. Status updates are derived from existing events:
- `TaskEvent::Created` → subtask creation
- `TaskEvent::Started` → task/subtask started
- `TaskEvent::Closed` → task/subtask completed
- `TaskEvent::CommentAdded` → progress comments

---

## CLI Interface

**No new commands.** Only existing command enhancements:

```bash
# Task run - status updates in sync mode (default)
aiki task run <task-id>             # Shows real-time status while running
aiki task run <task-id> --async     # No status (returns immediately)
aiki task run <task-id> --quiet     # Suppress status updates (optional)

# Review - status updates in sync mode (default)
aiki review <task-id>               # Shows real-time status while running
aiki review <task-id> --async       # No status (returns immediately)
aiki review <task-id> --start       # No status (hands off to caller)

# Fix - status updates in sync mode (default)
aiki fix <task-id>                  # Shows real-time status while running
aiki fix <task-id> --async          # No status (returns immediately)
aiki fix <task-id> --start          # No status (hands off to caller)
```

---

## Decisions

1. **No new commands**: All functionality integrated into existing commands (`task run`, `review`, `fix`)

2. **Default behavior**: Status updates shown by default in sync mode. Users can opt-out with `--quiet` if desired.

3. **Polling interval**: 500ms default, configurable via env var (`AIKI_STATUS_INTERVAL_MS`) or future config file

4. **Terminal handling**: Ctrl+C to detach (agent continues). Visualization only shown when stderr is a TTY.

5. **Output format**: Live-updating visualization written to stderr (leaves stdout clean for piping task IDs)

6. **Display mode**: Live visualization that updates in place (not a scrolling log). Uses ANSI escape codes for terminal control.

---

## Implementation Details

### Core Status Monitor Module

Create a reusable status monitoring module (`cli/src/tasks/status_monitor.rs`):

```rust
pub struct StatusMonitor {
    task_id: String,
    last_event_index: usize,
    poll_interval: Duration,
}

impl StatusMonitor {
    pub fn new(task_id: &str) -> Self { ... }
    
    /// Poll for new events and display them
    /// Returns true if task reached terminal state
    pub fn poll_and_display(&mut self, cwd: &Path) -> Result<bool> { ... }
    
    /// Run until task completion
    pub fn monitor_until_complete(&mut self, cwd: &Path) -> Result<()> { ... }
}
```

### Integration Pattern

The status monitor is integrated into the `task_run()` function in `cli/src/tasks/runner.rs`.

Since `aiki review` and `aiki fix` both call `task_run()` internally (after creating their respective tasks), they automatically get the visualization with no additional changes needed.

```rust
// In task_run() function:

// 1. Spawn the agent session (existing code)
spawn_agent_session(&task_id)?;

// 2. Monitor status with live visualization (NEW)
if stderr().is_terminal() {
    let mut monitor = StatusMonitor::new(&task_id);
    monitor.monitor_until_complete(cwd)?;
}

// 3. Output final result (existing code)
output_task_completed(&task_id)?;
```

Only `task_run()` needs to be modified. The `review` and `fix` commands inherit this behavior automatically.

### Visualization Rendering

The status monitor renders a live view to stderr:

```rust
pub struct StatusDisplay {
    task_id: String,
    start_time: Instant,
}

impl StatusDisplay {
    /// Render current task state to terminal (flicker-free)
    pub fn render(&self, tasks: &HashMap<String, Task>, prev_line_count: usize) -> usize {
        // Save cursor position
        // Move cursor up by prev_line_count lines
        // Draw task tree with status symbols
        // Clear any remaining lines from previous render (if tree shrunk)
        // Return new line count for next render
    }
}
```

The display updates in place every 500ms. Only draws when stderr is a TTY.

---

## Implementation Plan

### Phase 1: Core Status Monitoring Infrastructure
- [x] Implement event polling and task state materialization
- [x] Create live visualization renderer using terminal control (crossterm/termion)
- [x] Build tree formatter (parent + subtasks + comments)
- [x] Handle terminal sizing and graceful degradation for non-TTY

### Phase 2: Integrate into task_run()
- [x] Add status monitor to `task_run()` function in `cli/src/tasks/runner.rs`
- [x] Only show visualization when stderr is a TTY
- [x] Test with `aiki task run`, `aiki review`, and `aiki fix`

### Phase 3: Bug Fixes from Review
- [x] Fix Ctrl+C (detach) incorrectly marking tasks as stopped - Added `Detached` variant to `AgentSessionResult`
- [x] Fix monitor looping forever if agent exits without terminal status - Added PID tracking with `is_process_alive()` check

**That's it for v1.** Additional features (colors, progress bars, --quiet flag, etc.) can be added based on user feedback.

---

## Testing Strategy

1. **Unit tests**: Task state rendering, visualization formatting, polling logic
2. **Integration tests**: Live visualization with mocked task progression in sync mode
3. **Manual testing**: 
   - Real agent sessions with `task run`, `review`, `fix`
   - Verify visualization shown in sync mode, suppressed in async/start modes
   - Verify `--quiet` flag suppresses visualization
   - Test Ctrl+C handling (graceful exit, restores terminal)
   - Test behavior when stderr is redirected (no visualization)
   - Test with varying terminal sizes

---

## Future Considerations

- Interactive TUI mode with keyboard controls (space to expand subtask details, etc.)
- Aggregated view for multiple concurrent tasks
- Progress estimation based on historical task durations
- Tree view with collapsible sections for large task hierarchies
- WebSocket-based updates for web/IDE integrations

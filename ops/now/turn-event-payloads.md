# Turn Event Payloads: Task Context

**Date**: 2026-02-12
**Status**: Draft
**Priority**: P2

**Related Documents**:
- [Default Hooks](default-hooks.md) - Init-time hook scaffolding
- [Review Loop](review-loop.md) - Review-fix cycle plugin

---

## Problem

Today, the `turn.completed` event payload only includes basic turn metadata:

```rust
pub struct AikiTurnCompletedPayload {
    pub turn: Turn,  // Contains: number, id, source
}

pub struct Turn {
    pub number: u32,
    pub id: String,
    pub source: TurnSource,
}
```

This is insufficient for workflow automation plugins that need to trigger based on **what work was done during the turn**. Specifically:

1. **Review-loop plugin** (`aiki/review-loop`) needs to know which tasks were closed during the turn so it can trigger reviews only for original work (not review/fix tasks)
2. **Build-check plugin** (future) needs to know if any tasks that changed code were completed
3. **Notification plugins** (future) need task context for status updates

Without task context in the payload, plugins must query task state separately, which is inefficient and error-prone.

---

## Summary

Add a `tasks` field to `AikiTurnCompletedPayload` that contains a summary of task activity during the turn:

```rust
pub struct AikiTurnCompletedPayload {
    pub turn: Turn,
    pub tasks: TaskActivity,  // NEW: general-purpose task list type
}

// General-purpose types in cli/src/tasks/types.rs
pub struct TaskActivity {
    pub closed: Vec<TaskSummary>,
    pub started: Vec<TaskSummary>,
    pub stopped: Vec<TaskSummary>,
}

pub struct TaskSummary {
    pub id: String,
    pub description: String,
    pub task_type: Option<String>,  // e.g., Some("review"), Some("fix"), None
}
```

**Design note**: `TaskActivity` and `TaskSummary` are general-purpose types defined in `cli/src/tasks/types.rs`, making them reusable across:
- Event payloads (`turn.completed`, future `session.ended`, etc.)
- CLI output (JSON mode for `aiki task list`)
- Hook responses (returning task context)
- API endpoints (if we add an HTTP API later)

This allows plugins to make intelligent decisions based on what happened during the turn:

```yaml
# Example: trigger review only for original work tasks
turn.completed:
  - if: $event.tasks.closed | any(.task_type == null)
    then:
      - autoreply: "aiki review --fix --start"
```

---

## Design

### Payload Structure

```rust
// cli/src/tasks/types.rs (general-purpose task list types)

/// A lightweight task summary for event payloads and APIs.
/// Used in hook payloads, CLI output, and anywhere we need task info without full Task objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Task ID (32-char change_id)
    pub id: String,
    
    /// Task description
    pub description: String,
    
    /// Task type (None for original work, Some for generated tasks like review/fix)
    pub task_type: Option<String>,
}

/// A categorized list of tasks by state transitions.
/// Used for grouping tasks by activity (closed/started/stopped) in event payloads and queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    /// Tasks that were closed
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed: Vec<TaskSummary>,
    
    /// Tasks that were started
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TaskSummary>,
    
    /// Tasks that were stopped
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<TaskSummary>,
}

impl TaskActivity {
    /// Create an empty task activity
    pub fn empty() -> Self {
        Self {
            closed: Vec::new(),
            started: Vec::new(),
            stopped: Vec::new(),
        }
    }
    
    /// Check if there was any activity
    pub fn is_empty(&self) -> bool {
        self.closed.is_empty() 
            && self.started.is_empty()
            && self.stopped.is_empty()
    }
}
```

```rust
// cli/src/events/turn_completed.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiTurnCompletedPayload {
    pub turn: Turn,
    /// Task activity during this turn
    pub tasks: TaskActivity,
}
```

### Populating the Payload

In `handle_turn_completed`, query task state to populate `tasks.closed` and `tasks.started`:

```rust
// cli/src/events/turn_completed.rs

pub fn handle_turn_completed(state: &AikiState) -> Result<()> {
    let turn = state.current_turn()?;
    
    // Query task activity during this turn
    let task_manager = TaskManager::new(state)?;
    let tasks = task_manager.get_turn_task_activity(&turn)?;
    
    let payload = AikiTurnCompletedPayload {
        turn: turn.clone(),
        tasks,
    };
    
    execute_hook(state, "turn.completed", &payload)?;
    Ok(())
}
```

### Querying Task Activity

Add a method to `TaskManager` to get task activity for a turn:

```rust
// cli/src/tasks/manager.rs

impl TaskManager {
    pub fn get_turn_task_activity(&self, turn: &Turn) -> Result<TurnTasks> {
        let events = read_events(self.cwd)?;
        
        let mut list = TaskActivity::empty();
        
        // Filter events by turn_id (UUID v5 identifier)
        for event in events {
            match event {
                TaskEvent::Closed { task_ids, turn_id: Some(tid), .. } 
                    if tid == turn.id => {
                    list.closed.extend(self.task_ids_to_summaries(&task_ids)?);
                }
                TaskEvent::Started { task_ids, turn_id: Some(tid), .. }
                    if tid == turn.id => {
                    list.started.extend(self.task_ids_to_summaries(&task_ids)?);
                }
                TaskEvent::Stopped { task_ids, turn_id: Some(tid), .. }
                    if tid == turn.id => {
                    list.stopped.extend(self.task_ids_to_summaries(&task_ids)?);
                }
                _ => {}
            }
        }
        
        Ok(list)
    }
}
```

**Key design point**: We filter events by matching `turn_id` (the UUID v5 identifier), not by JJ change_ids. This cleanly separates:
- **Turn tracking**: Session-level turn numbering with stable UUIDs
- **Event storage**: JJ commits on `aiki/tasks` branch

Events store the turn_id they were created during, making it trivial to query "what happened in turn X".

### Exposing in Engine Variable Resolution

Update the hook engine to expose `$event.tasks.closed` and `$event.tasks.started`:

```rust
// cli/src/flows/engine.rs

// The engine already deserializes payloads into serde_json::Value
// The TurnTasks struct will automatically be available via:
//   $event.tasks.closed
//   $event.tasks.closed[0].id
//   $event.tasks.closed[0].description
//   $event.tasks.closed[0].task_type
```

No special handling needed if the engine already uses `serde_json::Value` for variable resolution.

---

## Implementation Plan

### Step 1: Add turn_id to Task Events & Materialized Task

Add `turn_id` field to `TaskEvent::Started`, `TaskEvent::Closed`, and `TaskEvent::Stopped`:

```rust
// cli/src/tasks/types.rs

TaskEvent::Started {
    task_ids: Vec<String>,
    agent_type: String,
    session_id: Option<String>,
    turn_id: Option<String>,  // NEW: UUID v5 turn identifier from TurnState
    timestamp: DateTime<Utc>,
    stopped: Vec<String>,
}

TaskEvent::Closed {
    task_ids: Vec<String>,
    outcome: TaskOutcome,
    summary: Option<String>,
    turn_id: Option<String>,  // NEW: UUID v5 turn identifier from TurnState
    timestamp: DateTime<Utc>,
}

TaskEvent::Stopped {
    task_ids: Vec<String>,
    reason: Option<String>,
    turn_id: Option<String>,  // NEW: UUID v5 turn identifier from TurnState
    timestamp: DateTime<Utc>,
}
```

**Key insight**: `turn_id` is the UUID v5 identifier from `Turn::id` (generated as `uuid_v5(session_uuid, turn_number)`), NOT a JJ change_id. This is a stable, deterministic identifier that persists across the session.

**Materialized in Task struct**: During event replay, these turn_ids are captured in the materialized `Task`:

```rust
// cli/src/tasks/types.rs

pub struct Task {
    // ... existing fields ...
    
    /// Turn ID when this task was started (most recent start if started multiple times)
    pub turn_started: Option<String>,
    
    /// Turn ID when this task was closed
    pub turn_closed: Option<String>,
    
    /// Turn ID when this task was stopped (if currently stopped)
    pub turn_stopped: Option<String>,
}
```

Update task commands to populate these fields:
- `aiki task start` reads `TurnState::current_turn_id` and stores it in the event
- `aiki task close` reads `TurnState::current_turn_id` and stores it in the event  
- `aiki task stop` reads `TurnState::current_turn_id` and stores it in the event

During materialization, these fields are set from the corresponding events.

**Note**: Events on the `aiki/tasks` branch have their own JJ change_ids (commit IDs), which are different from turn IDs. Turn IDs correlate events to logical turns within a session, not to specific commits.

### Step 2: Add General-Purpose Task List Types

Define `TaskActivity` and `TaskSummary` in `cli/src/tasks/types.rs`:

```rust
// cli/src/tasks/types.rs

/// A lightweight task summary for event payloads and APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub description: String,
    pub task_type: Option<String>,
}

/// A categorized list of tasks by state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed: Vec<TaskSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TaskSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<TaskSummary>,
}
```

Export these from `cli/src/tasks/mod.rs`:

```rust
pub use types::{TaskActivity, TaskSummary, /* ...existing exports... */};
```

### Step 3: Add General Task Filtering Functions

Add functions for filtering tasks by various criteria. Start with turn-based filtering:

```rust
// cli/src/tasks/manager.rs

/// Get task activity during a specific turn.
/// 
/// Returns a TaskActivity with tasks that transitioned during the turn.
/// For turn.completed payloads and turn-based queries.
/// 
/// Uses the materialized task graph - no need to re-read events since
/// turn_started/turn_closed/turn_stopped are captured during materialization.
pub fn get_task_activity_by_turn(graph: &TaskGraph, turn_id: &str) -> TaskActivity {
    let mut activity = TaskActivity::empty();
    
    for task in graph.tasks.values() {
        // Check if task was closed during this turn
        if task.turn_closed.as_deref() == Some(turn_id) {
            activity.closed.push(TaskSummary {
                id: task.id.clone(),
                description: task.name.clone(),
                task_type: task.task_type.clone(),
            });
        }
        
        // Check if task was started during this turn
        if task.turn_started.as_deref() == Some(turn_id) {
            activity.started.push(TaskSummary {
                id: task.id.clone(),
                description: task.name.clone(),
                task_type: task.task_type.clone(),
            });
        }
        
        // Check if task was stopped during this turn
        if task.turn_stopped.as_deref() == Some(turn_id) {
            activity.stopped.push(TaskSummary {
                id: task.id.clone(),
                description: task.name.clone(),
                task_type: task.task_type.clone(),
            });
        }
    }
    
    activity
}
```

**Note**: These are free functions (not methods) following the existing pattern in `manager.rs` where functions like `get_ready_queue()` take a `&TaskGraph` parameter.

**Performance**: This scans all tasks in the graph, but at <1000 tasks this is negligible (microseconds). The turn_id fields are indexed in memory as part of the `Task` struct.

### Step 4: Update handle_turn_completed

Populate `tasks` field in `AikiTurnCompletedPayload`:

```rust
// cli/src/events/turn_completed.rs

pub fn handle_turn_completed(mut payload: AikiTurnCompletedPayload) -> Result<HookResult> {
    // ... existing turn number lookup ...
    
    // Load task graph (with materialized turn_started/turn_closed/turn_stopped fields)
    let graph = TaskGraph::load(&payload.cwd)?;
    
    // Get task activity for this turn
    payload.tasks = get_task_activity_by_turn(&graph, &payload.turn.id);
    
    execute_hook(&payload.cwd, "turn.completed", &payload)?;
    Ok(HookResult::Continue)
}
```

**Clean and efficient**: We only load the graph once, and the turn_id fields are already materialized in the `Task` structs.

### Step 5: Test Variable Resolution

Verify that `$event.tasks.*` fields are accessible in hook conditions and expressions:

```yaml
# Test hook
turn.completed:
  - if: $event.tasks.closed | length > 0
    then:
      - shell: echo "Closed tasks: $($event.tasks.closed | map(.description) | join(', '))"
  - if: $event.tasks.started | length > 0
    then:
      - shell: echo "Started tasks: $($event.tasks.started | map(.description) | join(', '))"
```

### Step 6: Future Extensions

The `TaskActivity` type is designed to be extensible for future use cases:

**CLI JSON output:**
```bash
# Future: aiki task list --json --by-status
{
  "closed": [...],
  "started": [...],
  "stopped": [...]
}
```

**Session-level summaries:**
```rust
// Future: get all task activity for a session
// Would need to add session_started/session_closed to Task struct
pub fn get_task_activity_by_session(
    graph: &TaskGraph,
    session_id: &str,
) -> TaskActivity {
    // Filter tasks by session fields
}
```

**Time-range queries:**
```rust
// Future: get task activity in a time range
// Uses existing started_at/closed_at timestamp fields
pub fn get_task_activity_by_time_range(
    graph: &TaskGraph,
    start: DateTime<Utc>, 
    end: DateTime<Utc>,
) -> TaskActivity {
    // Filter tasks by timestamp ranges
}
```

---

## Usage in Plugins

### Review Loop Plugin

```yaml
# aiki/review-loop
turn.completed:
  - if: $event.tasks.closed | any(.task_type == null)
    then:
      - autoreply: |
          Original work tasks closed: $event.tasks.closed | filter(.task_type == null) | map(.description)
          
          Review your work and fix any issues:
          aiki review --fix --start
```

### Build Check Plugin (Future)

```yaml
# aiki/build-check
turn.completed:
  - if: $event.tasks.closed | length > 0
    then:
      - autoreply: |
          Running build to verify changes...
          aiki build
```

### Notification Plugin (Future)

```yaml
# aiki/slack-notify
turn.completed:
  - if: $event.tasks.closed | length > 0
    then:
      - shell: |
          curl -X POST https://slack.com/api/chat.postMessage \
            -d "text=Tasks completed: $($event.tasks.closed | map(.description) | join(', '))"
```

---

## Migration

This is a **backwards-compatible addition**:
- Existing hooks that don't use `$event.tasks` continue to work
- New hooks can access `$event.tasks.closed` and `$event.tasks.started`
- No changes required to existing code outside of `turn_completed.rs` and `tasks/`

---

## Design Rationale

### Why store turn_id in events (not just rely on timestamps)?

**Alternative considered**: Match events to turns by timestamp range.

**Problem**: Timing is fuzzy. If multiple turns happen quickly (e.g., autoreply loops), timestamp-based filtering becomes unreliable.

**Solution**: Store the deterministic turn_id (UUID v5) directly in events. This gives us:
- Exact correlation: "This event happened during turn X"
- No ambiguity from timing
- Works retroactively (can always query "what happened in turn X")

### Why turn_id and not JJ change_id?

**Key insight**: Events are stored on the `aiki/tasks` branch, which has its own commit history separate from the working copy. An event's JJ change_id (the commit that contains it) is different from the turn's change_id (the working copy commit).

**Turn ID** (UUID v5) acts as a session-level identifier that bridges these two histories:
- Events store their turn_id
- Turn completion hooks know the current turn_id
- We can correlate them without dealing with cross-branch JJ queries

## Open Questions

1. **Should `tasks.started` include tasks that auto-started (e.g., subtask `.0`)?** Probably yes - the hook can filter by `task_type` if needed.

2. **Should we include tasks that changed state but weren't closed/started (e.g., stopped, commented)?** Not yet - keep it simple. Can add `tasks.stopped`, `tasks.commented` later if needed.

3. **Should old events without turn_id be backfilled?** No - events created before this feature will have `turn_id: None`, which is fine. They won't appear in turn-based queries, but that's expected (historical data predates the feature).

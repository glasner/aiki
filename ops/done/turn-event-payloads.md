# Turn Event Payloads: Task Context

**Date**: 2026-02-12
**Status**: Draft
**Priority**: P2

**Related Documents**:
- [Default Hooks](default-hooks.md) - Init-time hook scaffolding
- [Review Loop](review-loop.md) - Review-fix cycle plugin

---

## Problem

Today, the `turn.completed` event payload includes session info, turn metadata, and file changes, but no task context:

```rust
pub struct AikiTurnCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub turn: Turn,              // number, id, source
    pub response: String,
    pub modified_files: Vec<PathBuf>,
    // No task information!
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
    // ... existing fields (session, cwd, timestamp, turn, response, modified_files) ...
    pub tasks: TaskActivity,  // NEW
}

// General-purpose types in cli/src/tasks/types.rs
pub struct TaskActivity {
    pub closed: Vec<TaskReference>,
    pub started: Vec<TaskReference>,
    pub stopped: Vec<TaskReference>,
}

pub struct TaskReference {
    pub id: String,
    pub name: String,
    pub task_type: Option<String>,  // e.g., Some("review"), Some("fix"), None
}
```

**Design note**: `TaskActivity` and `TaskReference` are general-purpose types defined in `cli/src/tasks/types.rs`, making them reusable across:
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

/// A lightweight reference to a task for event payloads and APIs.
/// Used in hook payloads, CLI output, and anywhere we need task info without full Task objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReference {
    /// Task ID (32-char change_id)
    pub id: String,

    /// Task name
    pub name: String,

    /// Task type (None for original work, Some for generated tasks like review/fix)
    pub task_type: Option<String>,
}

/// A categorized list of tasks by state transitions.
/// Used for grouping tasks by activity (closed/started/stopped) in event payloads and queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    /// Tasks that were closed
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed: Vec<TaskReference>,
    
    /// Tasks that were started
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TaskReference>,
    
    /// Tasks that were stopped
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<TaskReference>,
}

impl Default for TaskActivity {
    fn default() -> Self {
        Self {
            closed: Vec::new(),
            started: Vec::new(),
            stopped: Vec::new(),
        }
    }
}

impl TaskActivity {
    /// Check if there was any activity
    pub fn is_empty(&self) -> bool {
        self.closed.is_empty()
            && self.started.is_empty()
            && self.stopped.is_empty()
    }
}
```

```rust
// cli/src/events/turn_completed.rs (addition to existing struct)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiTurnCompletedPayload {
    // ... existing fields ...
    /// Task activity during this turn
    #[serde(default)]
    pub tasks: TaskActivity,  // NEW
}
```

### Populating the Payload

In `handle_turn_completed`, query task state to populate `tasks.closed` and `tasks.started`.

The existing function signature is `handle_turn_completed(mut payload: AikiTurnCompletedPayload) -> Result<HookResult>`. We add the task query after the turn info is resolved and before the payload is passed to `AikiState::new`:

```rust
// cli/src/events/turn_completed.rs (additions to existing function)

pub fn handle_turn_completed(mut payload: AikiTurnCompletedPayload) -> Result<HookResult> {
    // ... existing turn number lookup via history::get_current_turn_info ...
    // ... payload.turn = Turn::new(turn_number, ...) ...

    // NEW: Query task activity for this turn (defensive — falls back to empty on failure)
    // Matches the existing pattern for turn lookup (lines 42-60) where failures are logged
    // and defaults are used rather than propagating errors.
    payload.tasks = match crate::tasks::storage::read_events(&payload.cwd) {
        Ok(events) => {
            let graph = crate::tasks::graph::materialize_graph(&events);
            crate::tasks::manager::get_task_activity_by_turn(&graph, &payload.turn.id)
        }
        Err(e) => {
            debug_log(|| format!("Task activity lookup failed, using empty: {}", e));
            TaskActivity::default()
        }
    };

    // ... existing: let mut state = AikiState::new(payload) ...
    // ... existing: execute_hook(EventType::TurnCompleted, &mut state, &core_hook.turn_completed) ...
}
```

### Querying Task Activity

See **Step 3** in the Implementation Plan below for the `get_task_activity_by_turn` free function.

**Key design point**: We filter by matching `turn_id` (the UUID v5 identifier) on materialized `Task` structs, not by scanning raw events. The turn_id fields are captured during graph materialization, so the query is just a simple filter over the in-memory task map.

### Exposing in Engine Variable Resolution

Update the hook engine to expose `$event.tasks.closed` and `$event.tasks.started`:

```rust
// cli/src/flows/engine.rs

// The engine already deserializes payloads into serde_json::Value
// The TaskActivity struct will automatically be available via:
//   $event.tasks.closed
//   $event.tasks.closed[0].id
//   $event.tasks.closed[0].name
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

Update task commands to populate these fields. Task commands get the turn_id via the existing session detection infrastructure:

```rust
// In run_start(), run_close(), run_stop():

// 1. Session detection (already exists — PID-based matching via find_active_session)
let session_match = find_active_session(cwd);
let our_session_id = session_match.as_ref().map(|m| m.session_id.clone());

// 2. NEW: Query current turn from conversation history
let turn_id = our_session_id.as_ref().and_then(|sid| {
    let (turn_number, _) = history::get_current_turn_info(&global::global_aiki_dir(), sid).ok()?;
    Some(generate_turn_id(sid, turn_number))
});

// 3. Store turn_id in the event
let close_event = TaskEvent::Closed {
    task_ids: ids_to_close,
    outcome,
    summary: summary_text,
    turn_id,  // NEW
    timestamp: close_timestamp,
};
```

**Note**: `turn_id` is `Option<String>` — it will be `None` when running outside a session (e.g., human running `aiki task close` from a terminal). This is fine; those events simply won't appear in turn-based queries.

During materialization, these fields are set from the corresponding events.

**Persistence contract**: TaskEvent is serialized/deserialized through custom metadata logic in `storage.rs` (`event_to_metadata_block` and `parse_metadata_block`), NOT auto-derived serde. Adding `turn_id` requires symmetric changes to both functions:

1. **`event_to_metadata_block`** — In the `Started`, `Closed`, and `Stopped` match arms, add:
   ```rust
   if let Some(tid) = turn_id {
       add_metadata("turn_id", tid, &mut lines);
   }
   ```

2. **`parse_metadata_block`** — In the `"started"`, `"closed"`, and `"stopped"` match arms, add:
   ```rust
   let turn_id = fields.get("turn_id").and_then(|v| v.first()).map(|s| s.to_string());
   ```
   and include `turn_id` in the constructed `TaskEvent` variant.

3. **Backward compatibility** — Old events without `turn_id=` lines will parse as `turn_id: None` because we use `.and_then()` (not `?`). No migration needed.

4. **Tests** — Add round-trip tests for each event type confirming `turn_id` survives serialize → deserialize, and that old events without `turn_id` still parse correctly.

**Note**: Events on the `aiki/tasks` branch have their own JJ change_ids (commit IDs), which are different from turn IDs. Turn IDs correlate events to logical turns within a session, not to specific commits.

### Step 2: Add General-Purpose Task List Types

Define `TaskActivity` and `TaskReference` in `cli/src/tasks/types.rs`:

```rust
// cli/src/tasks/types.rs

/// A lightweight reference to a task for event payloads and APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReference {
    pub id: String,
    pub name: String,
    pub task_type: Option<String>,
}

/// A categorized list of tasks by state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed: Vec<TaskReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TaskReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<TaskReference>,
}
```

Export these from `cli/src/tasks/mod.rs`:

```rust
pub use types::{TaskActivity, TaskReference, /* ...existing exports... */};
```

**Note**: The existing `TaskEventPayload` in `cli/src/events/task_started.rs` and `task_closed.rs` overlaps with `TaskReference`. Consider renaming `TaskEventPayload` to `TaskReference` and unifying them — both serve as lightweight task representations in event payloads. This can be done as part of this work or as a follow-up.

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
    let mut activity = TaskActivity::default();

    for task in graph.tasks.values() {
        // Check if task was closed during this turn
        if task.turn_closed.as_deref() == Some(turn_id) {
            activity.closed.push(TaskReference {
                id: task.id.clone(),
                name: task.name.clone(),
                task_type: task.task_type.clone(),
            });
        }

        // Check if task was started during this turn
        if task.turn_started.as_deref() == Some(turn_id) {
            activity.started.push(TaskReference {
                id: task.id.clone(),
                name: task.name.clone(),
                task_type: task.task_type.clone(),
            });
        }

        // Check if task was stopped during this turn
        if task.turn_stopped.as_deref() == Some(turn_id) {
            activity.stopped.push(TaskReference {
                id: task.id.clone(),
                name: task.name.clone(),
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

Populate `tasks` field in `AikiTurnCompletedPayload` (see "Populating the Payload" section above for the exact insertion point in the existing function).

**Clean and efficient**: We only materialize the graph once, and the turn_id fields are already captured in the `Task` structs during materialization.

### Step 5: Test Variable Resolution

Verify that `$event.tasks.*` fields are accessible in hook conditions and expressions:

```yaml
# Test hook
turn.completed:
  - if: $event.tasks.closed | length > 0
    then:
      - shell: echo "Closed tasks: $($event.tasks.closed | map(.name) | join(', '))"
  - if: $event.tasks.started | length > 0
    then:
      - shell: echo "Started tasks: $($event.tasks.started | map(.name) | join(', '))"
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
          Original work tasks closed: $event.tasks.closed | filter(.task_type == null) | map(.name)

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
            -d "text=Tasks completed: $($event.tasks.closed | map(.name) | join(', '))"
```

---

## Migration

This is a **backwards-compatible addition**:
- Existing hooks that don't use `$event.tasks` continue to work
- New hooks can access `$event.tasks.closed`, `$event.tasks.started`, and `$event.tasks.stopped`
- Task events created before this feature will have `turn_id: None` and simply won't appear in turn-based queries
- Changes touch: `tasks/types.rs` (new types + turn_id fields), `tasks/manager.rs` (query function), `tasks/storage.rs` (serialization), `commands/task.rs` (populate turn_id), `events/turn_completed.rs` (populate payload), `tasks/graph.rs` (materialize turn_id)

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

2. **Should we include tasks that changed state but weren't closed/started (e.g., stopped, commented)?** `tasks.stopped` is included in the initial design. `tasks.commented` can be added later if needed.

3. **Should old events without turn_id be backfilled?** No - events created before this feature will have `turn_id: None`, which is fine. They won't appear in turn-based queries, but that's expected (historical data predates the feature).

## Resolved Questions

### task_type semantics for review-loop filtering

**Concern raised**: The review-loop plugin gates on `task_type == null` to mean "original work", but many creation paths leave `task_type` as `None` (subtask `.0`, blockers, manual `aiki task add`). Could follow-up/generated tasks be misclassified?

**Resolution**: Use `Task.task_type` directly (the stored `Option<String>`). This is the right signal because:
- Templates are the authoritative source of task type — `aiki/review` sets `Some("review")`, `aiki/fix` sets `Some("fix")`, etc.
- Ad-hoc user tasks (`aiki task add/start`) intentionally have `None` — they *are* original work.
- The edge cases (subtask `.0`, blockers) are rarely *closed* during a normal turn — `.0` is a planning subtask and blockers are assigned to humans.
- If a template-created task is missing `task_type`, that's a bug in the template, not in the filtering logic.

**Alternative considered**: Use `infer_task_type()` (the heuristic in `commands/task.rs` that guesses "review"/"bug"/"feature" from name patterns). Rejected because:
- It's a leaky heuristic — "Fix typo in README" becomes `"bug"`, "Review subtasks" becomes `"review"` even for non-review tasks.
- It conflates two separate concerns: event dispatch labels (where approximate is fine) vs. workflow gating (where precision matters).
- Templates already provide an explicit, precise signal — no need for inference.

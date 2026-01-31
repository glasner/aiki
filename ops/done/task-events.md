# Task Lifecycle Events for Flows

**Status**: Implemented
**Related**: [Review and Fix](review-and-fix.md)

---

## Summary

Add base `task.started` and `task.closed` events plus syntactic sugar for common patterns like `review.completed`.

## Motivation

Flows currently can't react to task lifecycle changes. To enable patterns like:

- Log when a task starts
- Trigger review when a task completes
- Update dependent tasks when blockers resolve
- Notify on task start/completion

We need task events wired into the flow system.

## Changes

### `task.started` Event

Fire when a task is started (via `aiki task start` or `aiki task run`).

**Event trigger in flow YAML:**
```yaml
task.started:
  - log: "Task started: ${event.task.name}"
```

**Event payload:**
```json
{
  "task": {
    "id": "xqrmnpst",
    "name": "Implement user auth",
    "type": "feature",
    "status": "in_progress",
    "assignee": "claude-code"
  }
}
```

**Payload fields:**
- `task.id` - Task ID
- `task.name` - Task name/title
- `task.type` - Task type (e.g., "review", "feature", "bug")
- `task.status` - Always "in_progress" for this event
- `task.assignee` - Assigned agent (if any)

### `task.closed` Event

Fire when any task reaches closed state.

**Event trigger in flow YAML:**
```yaml
task.closed:
  - if: $event.task.name | startswith("Feature:")
    then:
      - log: "Feature task completed: ${event.task.name}"
```

**Event payload:**
```json
{
  "task": {
    "id": "xqrmnpst",
    "name": "Implement user auth",
    "type": "feature",
    "status": "closed",
    "outcome": "done",
    "source": "task:abc123",
    "files": ["src/auth.ts", "src/middleware.ts"]
  }
}
```

**Payload fields:**
- `task.id` - Task ID
- `task.name` - Task name/title
- `task.type` - Task type (e.g., "review", "feature", "bug")
- `task.status` - Always "closed" for this event
- `task.outcome` - "done", "wont_do", etc.
- `task.source` - Source field if present (for lineage)
- `task.files` - Files changed while working on this task (from provenance)

### Emit Points

**`task.started`** - Fire inside `task_start()` after updating task status:

```rust
// tasks/mod.rs
pub fn task_start(task_id: &str) -> Result<()> {
    // ... update task status to in_progress ...

    emit_event(AikiEvent::TaskStarted {
        task_id: task_id.to_string(),
    })?;

    Ok(())
}
```

**`task.closed`** - Fire inside `task_close()` after updating task status:

```rust
// tasks/mod.rs
pub fn task_close(task_id: &str, outcome: Outcome) -> Result<()> {
    // ... update task status ...

    emit_event(AikiEvent::TaskClosed {
        task_id: task_id.to_string(),
        outcome,
    })?;

    Ok(())
}
```

## Syntactic Sugar System

Flows support shorthand triggers that expand ("desugar") to base events with filters. This enables concise, readable flow definitions for common patterns.

### Concept

Sugar triggers are **convenience syntax**, not separate event types. The flow engine recognizes patterns and expands them at parse time.

### Sugar Patterns

#### `{type}.started`

Expands to `task.started` with a type filter.

```yaml
# Sugar
review.started:
  - log: "Review started: ${event.task.name}"

# Desugars to
task.started:
  - if: $event.task.type == "review"
    then:
      - log: "Review started: ${event.task.name}"
```

#### `{type}.completed`

Expands to `task.closed` with type and outcome filters.

```yaml
# Sugar
review.completed:
  - log: "Review completed: ${event.task.name}"
  - run: notify-team

# Desugars to
task.closed:
  - if: $event.task.type == "review" && $event.task.outcome == "done"
    then:
      - log: "Review completed: ${event.task.name}"
      - run: notify-team
```

### Desugaring Rules

| Sugar Pattern | Desugars To |
|---------------|-------------|
| `{type}.started` | `task.started` where `type == "{type}"` |
| `{type}.completed` | `task.closed` where `type == "{type}"` AND `outcome == "done"` |

### No `{type}.closed` Sugar

There is intentionally **no** `{type}.closed` sugar. The `.closed` event includes all outcomes (done, wont_do, etc.), which is rarely what you want. For the rare cases where you need to react to any closure:

```yaml
# Use task.closed with explicit filter
task.closed:
  - if: $event.task.type == "review"
    then:
      - log: "Review closed with outcome: ${event.task.outcome}"
```

### Common Sugar Examples

```yaml
# Code review completed successfully
review.completed:
  - run: update-pr-status --status=approved

# Feature implementation started
feature.started:
  - log: "Feature work started: ${event.task.name}"

# Bug fix completed
bug.completed:
  - run: close-issue --id=${event.task.source}
```

## Implementation

### Event Enum

Add variants to `AikiEvent`:

```rust
// events/mod.rs
pub enum AikiEvent {
    // ... existing variants ...
    TaskStarted {
        task_id: String,
    },
    TaskClosed {
        task_id: String,
        outcome: Outcome,
    },
}
```

### Flow Engine Routing

Map events to flow triggers:

```rust
// flows/engine.rs
fn event_to_trigger(event: &AikiEvent) -> &str {
    match event {
        AikiEvent::TaskStarted { .. } => "task.started",
        AikiEvent::TaskClosed { .. } => "task.closed",
        // ... other events ...
    }
}
```

### Sugar Pattern Recognition

The flow engine expands sugar at parse time:

```rust
// flows/parser.rs
fn expand_trigger(trigger: &str) -> ExpandedTrigger {
    // Check for {type}.started pattern
    if let Some(task_type) = trigger.strip_suffix(".started") {
        if task_type != "task" {
            return ExpandedTrigger {
                base_event: "task.started",
                filter: Some(format!("$event.task.type == \"{}\"", task_type)),
            };
        }
    }

    // Check for {type}.completed pattern
    if let Some(task_type) = trigger.strip_suffix(".completed") {
        return ExpandedTrigger {
            base_event: "task.closed",
            filter: Some(format!(
                "$event.task.type == \"{}\" && $event.task.outcome == \"done\"",
                task_type
            )),
        };
    }

    // Not sugar, return as-is
    ExpandedTrigger { base_event: trigger, filter: None }
}
```

### Files

- `cli/src/events/mod.rs` - Add `TaskStarted` and `TaskClosed` variants to `AikiEvent`
- `cli/src/tasks/mod.rs` - Emit events in `task_start()` and `task_close()`
- `cli/src/flows/engine.rs` - Route `task.started` and `task.closed` triggers to handlers
- `cli/src/flows/parser.rs` - Expand sugar patterns during flow parsing

# Task Types (MVP)

**Status**: Minimal implementation for code review  
**Related**: Code Review System

---

## Motivation

We need a way to identify review tasks vs regular tasks for filtering (`aiki review list`).

---

## Design (MVP)

### Schema

```rust
pub enum TaskType {
    Review,  // Code review task (creates followup tasks)
}
```

**Storage in task events**:
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: created
type: review  # Optional field, null for standard tasks
name: "Review: @ (working copy)"
assignee: codex
---
```

**Standard tasks**: `type` field is `null` or absent

### Usage

**Internal only** - not exposed in CLI help or user-facing documentation:

```rust
// Creating a review task
let task_event = TaskEvent::Created {
    task_id: generate_task_id(),
    task_type: Some(TaskType::Review),  // Mark as review
    name: format!("Review: {}", revset),
    // ...
};

// Filtering review tasks
pub fn list_reviews() -> Result<Vec<Task>> {
    get_tasks_by_type(Some(TaskType::Review))
}

// Standard tasks
let task_event = TaskEvent::Created {
    task_type: None,  // Standard task (no type)
    name: "Implement authentication",
    // ...
};
```

---

## Benefits

1. **Enables review filtering**: `aiki review list` can query `type: review`
2. **Minimal complexity**: Only one type value vs null
3. **Internal implementation detail**: Users don't need to know about types
4. **Future-proof**: Can add more types later if needed

---

## Non-Goals (Future)

These are **not** part of the MVP:

- ❌ Type-based filtering flags (`--bug`, `--feature`)
- ❌ Auto-priority based on type
- ❌ User-facing type selection (`--type` flag)
- ❌ Multiple task types (Bug, Feature, Chore, Spike)
- ❌ Type displayed in output or help text

Task name already conveys semantic meaning. Start simple, add complexity only if users request it.

---

## Implementation

### Phase 1: Add type field to schema

```rust
pub struct TaskEvent {
    pub task_id: String,
    pub task_type: Option<TaskType>,  // None = standard task
    pub name: String,
    // ...
}

pub enum TaskType {
    Review,
}
```

### Phase 2: Use in review creation

```rust
pub fn review(revset: &str, from: Option<String>) -> Result<()> {
    // ...
    let task_id = task_add_with_children(
        format!("Review: {}", revset),
        reviewer,
        Some(TaskType::Review),  // Mark as review type
        review_steps,
        scope,
    )?;
    // ...
}
```

### Phase 3: Filter by type

```rust
pub fn list_reviews() -> Result<Vec<Task>> {
    // Query tasks where type = review
    get_tasks_by_type(Some(TaskType::Review))
}
```

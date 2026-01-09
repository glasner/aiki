# Flow Actions

**Status**: Future Idea  
**Related**: Task System, Flow Composition

---

## Motivation

Allow flows to create and close tasks programmatically.

---

## Design

```yaml
task.create:
  name: "Task name"
  priority: p0 | p1 | p2 | p3
  body: "Optional description"
  scope:
    files:
      - path: src/file.ts
        lines: [42, 50]

task.close:
  id: task-123
  outcome: done
  code_change: optional-change-id
```

---

## Examples

### Auto-Create Tasks from Errors

```yaml
response.received:
  - let: errors = self.parse_response_errors($response)
  - for: error in $errors
    then:
      task.create:
        name: "Fix: $error.message"
        priority: p0
        body: |
          Error: $error.message
          File: $error.file:$error.line
        scope:
          files:
            - path: $error.file
              lines: $error.line
  
  # Show created tasks inline:
  - if: $errors | length > 0
    then:
      autoreply:
        append: |
          📋 Created $errors.length task(s)
          Run `aiki task list` to see ready tasks
```

### Auto-Close Tasks on Fix

```yaml
change.completed:
  - if: $event.write
    then:
      - let: done_tasks = self.task_check_done($modified_files)
      - for: task in $done_tasks
        then:
          task.close:
            id: $task.id
            outcome: done
```

---

## Why Not Phase 1-5

- Requires flows system to be stable first
- Complex integration with event sourcing
- Manual task creation covers most use cases initially
- Can add when flow composition is mature

---

## Implementation Notes

- `task.create` generates events same as CLI
- `task.close` validates task exists and is open
- Scope tracking enables "which tasks did this change fix?"
- Requires `self.parse_response_errors()` and `self.task_check_done()` functions

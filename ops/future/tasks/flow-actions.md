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

task.run:
  task_id: task-123
  # Starts agent session to work on the task
  # Blocks until task completes or agent returns control

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

### Auto-Run Review Followup Tasks

```yaml
# Automatically start working on review issues
review.completed:
  - if: $event.review.issues_found > 0
    then:
      - log: "Review found ${event.review.issues_found} issues, starting fixes..."
      - task.run:
          task_id: $event.review.followup_task_id
      # Agent works on the followup task (and its children)
      # When done, control returns here
      - log: "Finished working on review issues"
```

### Review Loop with Auto-Fix

```yaml
# Complete review loop: review → auto-fix → re-review
review.completed:
  - if: $event.review.loop_enabled && $event.review.issues_found > 0
    then:
      - log: "Starting automatic fix of ${event.review.issues_found} issues..."
      - task.run:
          task_id: $event.review.followup_task_id
      
      # After fixes complete, trigger re-review
      - log: "Fixes complete, re-reviewing..."
      - review:
          scope: $event.review.scope
          files: $event.review.files
          prompt: $event.review.prompt
          loop: true
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

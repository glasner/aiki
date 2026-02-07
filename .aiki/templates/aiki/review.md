---
version: 1.0.0
type: review
---

# Review: {{scope.name}}

Review the code changes from task `{{scope.id}}`.

When all subtasks are complete, close this task with a comment of "Review complete (n issues found)" 

# Subtasks

## Understand the changes

Run these commands to understand the intent of the task and what was modified:

```bash
aiki task show {{scope.id}} --with-source  # Understand intent
aiki task diff {{scope.id}}                 # View all code changes
```

Leave a comment on {{parent.id}} to explain your understanding of the changes.

## Review 

Review the work and if any issues are found, leave a comment on {{parent.id}} for each finding.

---
version: 1.0.0
type: review
---

# Review: {{scope.name}}

Review the code changes from task `{{scope.id}}`.

## 1. Understand the changes

Run these commands to understand the intent of the task and what was modified:

```bash
aiki task show {{scope.id}} --with-source  # Understand intent
aiki task diff {{scope.id}}                 # View all code changes
```

Leave a comment on the task to explain your understanding of the changes.

## 2. Review for changes for issues and leave a comment for each finding

## 3. Close the review

When done, close this task with a comment of "Review complete (n issues found)"

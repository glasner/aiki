---
version: 2.0.0
type: review
---

# Review: {{data.scope.name}}

When done with all subtasks close this task with a summary of review:

```bash
aiki task close {{id}} --summary "Review complete (n issues found)"                # View all code changes
```

# Subtasks

{% subtask aiki/review/{{data.scope.kind}} %}

## Review

Review the work and if any issues are found, leave a comment on {{parent.id}} for each finding.

{% subtask aiki/fix/loop if data.options.fix %}

---
version: 1.1.0
data:
  loop.index1: 1
spawns:
  - when: "not subtasks.review.data.approved"
    max_iterations: 10
    subtask:
      template: aiki/fix/quality
      data:
        fix_task: subtasks.fix.id
        loop.index1: "data.loop.index1 + 1"
---

# Fix Quality Check {{data.loop.index1}}

Reviewing fix task {{data.fix_task}} for quality.

# Subtasks

## Review Fix
---
slug: review
---

Review the fix task for quality issues:

```bash
aiki review {{data.fix_task}} --start
```

When done, close this subtask.

## Fix Issues
---
slug: fix
---

{% if parent.subtasks.review.data.approved %}
No issues found — fix is clean. Close this subtask as won't-do:
```bash
aiki task close {{id}} --wont-do --summary "Fix approved, no issues"
```
{% else %}
Fix the issues found in the quality review:
```bash
aiki fix {{parent.subtasks.review.id}} --start
```

When done, close this subtask.
{% endif %}

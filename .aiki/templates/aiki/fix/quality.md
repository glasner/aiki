---
version: 1.0.0
spawns:
  - when: "not subtasks.review.approved and data.loop_index < data.max_inner"
    subtask:
      template: aiki/fix/quality
      data:
        fix_task: subtasks.fix.id
        max_inner: data.max_inner
        loop_index: "data.loop_index + 1"
---

# Fix Quality Check {{data.loop_index}}/{{data.max_inner}}

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

{% if parent.subtasks.review.approved %}
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

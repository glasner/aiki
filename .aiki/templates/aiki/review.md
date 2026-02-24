---
version: 2.0.0
type: review
---

# Review: {{data.scope.name}}

When done with all subtasks close this task with a summary of your review:

```bash
aiki task close {{id}} --summary "Review complete (n issues found)"
```

# Subtasks

## Explore Scope

Run the following command to explore the scope. The `--start` flag assigns the explore task to you so the context is available for the review phase:

```bash
aiki explore {{data.scope.id}} --start
```

{% subtask aiki/review/criteria/plan if data.scope.kind == "plan" %}
{% subtask aiki/review/criteria/code if data.scope.kind != "plan" %}

## Review

With your understanding of the criteria review the work. Track **each issue** found using the following command:

```bash
aiki review issue add {{parent.id}} "Description of the issue"
```

Each issue becomes a trackable fix item. Regular comments (`aiki task comment`) are for progress notes and won't trigger fixes.

When all issues have been recorded, close this subtask.

<!--{% subtask aiki/fix/loop if data.options.fix %}-->

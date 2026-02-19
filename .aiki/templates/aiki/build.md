---
version: 2.0.0
type: orchestrator
---

# Build: {{data.spec}}

**Overall Goal**: Execute plan to implement the spec.

When all plan subtasks are complete, close this task:

```bash
aiki task close {{id}} --summary "Build completed: all subtasks done."
```

# Subtasks

{% subtask aiki/plan if not data.plan %}

## Execute Subtasks
---
slug: execute
---

Execute each subtask of the plan task sequentially. The plan task ID is available via `data.plan` ({{data.plan}}):

```bash
aiki task run {{data.plan}}.<subtask number>
...
```

Run them in order. If a subtask fails do not continue, stop and report the failure using the following command:

```bash
aiki task stop {{parent.id}} --reason "Failed subtask <subtask_number>: <reason>"
```

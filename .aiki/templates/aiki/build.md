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

Execute each subtask of the plan task sequentially. The plan subtasks are nested under the plan task ({{parent.id}}.1):

```bash
aiki task run {{parent.id}}.1.<subtask number>
...
```

Run them in order. If a subtask fails do not continue, stop and report the failure using the following command:

```bash
aiki task stop {{parent.id}} --reason "Failed subtask <subtask_number>: <reason>"
```

---
version: 2.0.0
type: orchestrator
---

# Build: {{data.plan}} ({{data.epic}})

**Overall Goal**: Execute plan to implement the plan.

The plan is a task ({{data.epic}}) that includes all the necessary steps to build the plan at {{data.plan}}.

Start the plan:

```bash
aiki task start {{data.epic}}
```
Execute each subtask of the plan sequentially until they are all completed:

```bash
aiki task run {{data.epic}} --next-subtask
```

If a subtask fails **do not continue**, stop all work and report the failure:

```bash
aiki task stop {{data.epic}} {{id}} --reason "Failed subtask <subtask_id>: <reason>"
```

When **all epic subtasks** are complete, close the epic and this task:

```bash
aiki task close {{data.epic}} {{id}} --summary "Build completed: plan:{{data.epic}}."
```

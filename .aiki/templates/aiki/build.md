---
version: 2.0.0
type: orchestrator
---

# Build: {{data.spec}} ({{data.plan}})

**Overall Goal**: Execute plan to implement the spec.

The plan is a task ({{data.plan}}) that includes all the necessary steps to build the spec at {{data.spec}}.

Start the plan: 

```bash
aiki task start {{data.plan}}
```
Execute each subtask of the plan sequentially until they are all completed: 

```bash
aiki task run {{data.plan}} --next-subtask
```

If a subtask fails **do not continue**, stop all work and report the failure:

```bash
aiki task stop {{data.plan}} {{id}} --reason "Failed subtask <subtask_id>: <reason>"
```

When **all plan subtasks** are complete, close the plan and this task:

```bash
aiki task close {{data.plan}} {{id}} --summary "Build completed: plan:{{data.plan}}."
```

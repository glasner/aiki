---
version: 1.0.0
type: build
---

# Build: {{data.spec}}

**Overall Goal**: Execute plan to implement the spec.

When all plan subtasks are complete, close this task:

```bash
aiki task close {{id}} --comment "Build completed: all N subtasks done. Plan task: <plan_task_id>"
```

## Subtasks

{% if not data.plan %}
### Create Plan
No existing plan. Create one using the plan command:

```bash
aiki plan {{data.spec}}
```

This command will:
- Read and analyze the spec file
- Create a plan task with subtasks
- Output the plan task ID

Capture the plan task ID and store it in the task metadata:

```bash
# Capture plan ID (from XML output: <aiki_plan plan_id="..."/>)
PLAN_ID=<extracted-plan-id>

# Store it in build task data for future reference
aiki task update {{parent.id}} --data plan=$PLAN_ID
```
{% endif %}

### Execute Subtasks

Execute each subtask of the plan task sequentially:

```bash
aiki task run <plan_task_id>.<subtask number>
...
```

Run them in order. If a subtask fails do not continue, stop and report the failure using the following command:

```bash
aiki task stop {{parent.id}} --comment "Failed subtask <subtask_number>: <reason>"
```

...

---
version: 1.0.0
type: plan
---

# Plan: {{data.spec}}

**Goal**: Read the spec file and create an implementation plan with subtasks.

## Create Implementation Plan

Read the spec file at `{{data.spec}}` and understand:
- What is the goal/vision?
- What are the requirements?
- What are the constraints?
- Are there open questions that need resolving?

Identify the implementation steps needed. Each step should be a discrete, testable change.

## Create Plan Task

Create the plan task (a container for subtasks):

```bash
aiki task add "Plan: <spec title from the spec file>" \
  --source task:{{parent.id}} \
  --source file:{{data.spec}} \
  --data spec={{data.spec}}
```

The command outputs the task ID on stdout (single line, e.g., `xyxkluynlnonltwtprxupswknoquuvnz`). Capture this as the plan task ID.

**Spec title extraction:** Use the first H1 heading (`# Title`) from the spec file. If no H1 found, use the filename without extension.

## Add Subtasks

For each implementation step identified, create a subtask:

```bash
aiki task add "<step description>" --parent <plan_task_id>
```

**Guidelines for subtasks:**
- Each subtask should be discrete and actionable
- Include enough context for the executing agent
- Order subtasks logically (dependencies first)
- Keep subtask names concise but descriptive

## Close Planning Task

When all subtasks are added, close this planning task and report the plan:

```bash
aiki task close --summary "Plan created with N subtasks. Plan ID: <plan_task_id>"
```

Output the plan task ID and subtask summary for the user.

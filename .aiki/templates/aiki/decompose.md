---
version: 1.0.0
type: decompose
slug: decompose
---

# Decompose: {{data.plan}}

**Goal**: Read the plan file and create an epic with implementation subtasks.

## Create Implementation Plan

Read the plan file at `{{data.plan}}` and understand:
- What is the goal/vision?
- What are the requirements?
- What are the constraints?
- Are there open questions that need resolving?

## Epic

The epic has already been created: `{{data.epic}}`

Set instructions on the epic:

```bash
aiki task set {{data.epic}} --instructions <<'MD'
Implementation plan for <plan title>.
See plan: {{data.plan}}
MD
```

## Add Subtasks

For each implementation step identified, create a subtask and set instructions:

```bash
TASK_ID=$(aiki task add "<step description>" --subtask-of {{data.epic}} --output id)
aiki task set $TASK_ID --instructions <<'MD'
<detailed instructions for this step — enough context for an
executing agent to complete the step without re-reading the plan>
MD
```

When a subtask depends on another subtask's output (e.g., phase 2 depends on phase 1), link them:

```bash
aiki task link $LATER_TASK_ID --depends-on $EARLIER_TASK_ID
```

**Guidelines for subtasks:**
- Each subtask should be discrete and actionable
- Include enough context in the instructions for an executing agent
- Order subtasks logically (dependencies first)
- Add `depends-on` links when a subtask truly requires another to complete first
- Keep subtask names concise but descriptive
- Each subtask should have verifiable acceptance criteria


## Close Decompose Task

When all subtasks are added, close this decompose task and report the epic:

```bash
aiki task close {{id}} --summary "Epic created with N subtasks. Epic ID: {{data.epic}}"
```

Output the epic ID and subtask summary for the user.

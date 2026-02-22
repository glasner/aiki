---
version: 1.0.0
type: plan
slug: plan
---

# Plan: {{data.spec}}

**Goal**: Read the spec file and create an implementation plan with subtasks.

## Create Implementation Plan

Read the spec file at `{{data.spec}}` and understand:
- What is the goal/vision?
- What are the requirements?
- What are the constraints?
- Are there open questions that need resolving?

## Plan Task

The plan task has already been created: `{{data.plan}}`

Set instructions on the plan task:

```bash
aiki task set {{data.plan}} --instructions <<'MD'
Implementation plan for <spec title>.
See spec: {{data.spec}}
MD
```

## Add Subtasks

For each implementation step identified, create a subtask and set instructions:

```bash
TASK_ID=$(aiki task add "<step description>" --subtask-of {{data.plan}} --output id)
aiki task set $TASK_ID --instructions <<'MD'
<detailed instructions for this step — enough context for an
executing agent to complete the step without re-reading the spec>
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


## Close Planning Task

When all subtasks are added, close this planning task and report the plan:

```bash
aiki task close {{id}} --summary "Plan created with N subtasks. Plan ID: {{data.plan}}"
```

Output the plan task ID and subtask summary for the user.

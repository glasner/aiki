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

### Linking subtasks

Links between subtasks control how the orchestrator schedules work into parallel **lanes**. Use the right link type:

**`--depends-on`** — The later task needs the earlier task's *output* but runs in a fresh agent session. Creates lane boundaries: fan-out points split into parallel lanes, fan-in points create lanes that wait on predecessors.

```bash
# implement-frontend can't start until plan is done, but doesn't need plan's session context
aiki task link $FRONTEND_ID --depends-on $PLAN_ID
aiki task link $BACKEND_ID --depends-on $PLAN_ID    # fan-out: frontend + backend run in parallel
aiki task link $TESTS_ID --depends-on $FRONTEND_ID   # fan-in: tests waits on both
aiki task link $TESTS_ID --depends-on $BACKEND_ID
```

**`--needs-context`** — The later task must run in the *same agent session* as the earlier task (shared in-memory context). Forms linear chains within a lane. Max one forward, one reverse.

```bash
# explore then plan in one session — plan needs explore's codebase understanding
EXPLORE_ID=$(aiki task add "Explore codebase" --subtask-of {{data.epic}} --output id)
PLAN_ID=$(aiki task add "Create implementation plan" --subtask-of {{data.epic}} --needs-context $EXPLORE_ID --output id)
```

**When to use which:**

| Situation | Link type | Why |
|-----------|-----------|-----|
| Task B needs A's code changes on disk | `--depends-on` | Fresh session is fine, just needs the committed output |
| Task B needs A's in-memory understanding | `--needs-context` | Same agent session preserves context |
| Tasks are independent (no shared state) | No link | Each becomes its own parallel lane |
| Task B reviews/validates A's work | `--depends-on` | Review runs independently after implementation |

**Guidelines for subtasks:**
- Each subtask should be discrete and actionable
- Include enough context in the instructions for an executing agent
- Order subtasks logically (dependencies first)
- Use `--depends-on` for tasks that need another's output but not its session context
- Use `--needs-context` for tasks that must share an agent session (e.g., explore → plan)
- Leave tasks unlinked when they are truly independent — they'll run as parallel lanes
- Keep subtask names concise but descriptive
- Each subtask should have verifiable acceptance criteria


## Close Decompose Task

When all subtasks are added, close this decompose task and report the epic:

```bash
aiki task close {{id}} --summary "Epic created with N subtasks. Epic ID: {{data.epic}}"
```

Output the epic ID and subtask summary for the user.

# Build

`aiki build` takes a plan file, decomposes it into an epic with implementation subtasks, and executes them until the plan is fully implemented.

## Usage

```bash
# Build from a plan file
aiki build ops/now/user-auth.md

# Build from an existing epic
aiki build <epic-id>

# Build in the background
aiki build ops/now/user-auth.md --async

# Build then review
aiki build ops/now/user-auth.md --review

# Build, review, and auto-fix
aiki build ops/now/user-auth.md --fix
```

## How It Works

Build has two phases: **decompose** and **execute**.

### Phase 1: Decompose

An agent reads the plan file and creates an epic — a parent task with implementation subtasks. For each step identified in the plan, the agent creates a subtask with detailed instructions (enough context for an executing agent to complete it without re-reading the plan).

The agent also sets up dependencies between subtasks using links:

| Link type | When to use | Effect |
|-----------|-------------|--------|
| `--depends-on` | Task B needs A's output but not its session context | B waits for A, then runs in a fresh session |
| `--needs-context` | Task B must share A's in-memory understanding | B runs in the same agent session as A |
| *(no link)* | Tasks are independent | Tasks run as parallel lanes |

### Phase 2: Execute

The orchestrator runs subtasks using `aiki task run --next-session`, which automatically picks the next ready task, delegates it to an agent, waits for completion, and moves to the next one.

### Lanes

Dependencies create an execution graph that Aiki schedules into parallel **lanes**:

```
         ┌─ Frontend ─┐
Plan ──▶ │             ├──▶ Tests
         └─ Backend  ──┘
```

In this example, Frontend and Backend fan out from Plan (both `--depends-on` Plan) and run in parallel. Tests fans in (depends on both) and waits for both to finish.

## Options

| Flag | Effect |
|------|--------|
| `--async` | Run in the background, return immediately |
| `--restart` | Ignore existing epic, create a new one |
| `--review` | Run a review after all subtasks complete |
| `--fix` | Run review + fix loop after build (implies `--review`) |
| `--template <name>` | Use a custom build template (default: `aiki/implement`) |
| `--agent <type>` | Choose orchestrator agent (default: `claude-code`) |

## Subcommands

```bash
# Show build status for a plan
aiki build show ops/now/user-auth.md
```

## Idempotency

Running `aiki build` on the same plan file is idempotent. If an epic already exists for that plan, Aiki reuses it rather than creating a new one. Use `--restart` to force a fresh decomposition.

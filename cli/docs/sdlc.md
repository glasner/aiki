# SDLC: Plan, Build, Review, Fix

Aiki provides a structured software development lifecycle for AI-assisted coding. Four commands form a closed feedback loop: you plan what to build, an agent builds it, another agent reviews the output, and issues get fixed automatically.

## The Workflow

```
  plan            build           review            fix
┌────────┐    ┌──────────┐    ┌───────────┐    ┌──────────┐
│ Write a │───▶│Decompose │───▶│ Evaluate  │───▶│ Plan,    │
│  spec   │    │& execute │    │ against   │    │decompose,│──┐
│         │    │  tasks   │    │ criteria  │    │& loop    │  │
└────────┘    └──────────┘    └───────────┘    └──────────┘  │
                                    ▲                         │
                                    └─────────────────────────┘
                                       re-review until clean
```

## Phases

### [Plan](sdlc/plan.md)

Interactive session where you and an AI agent collaborate on a specification. The output is a plan file (typically in `ops/now/`) that describes what to build, the requirements, constraints, and implementation approach.

```bash
aiki plan ops/now/user-auth.md
```

### [Build](sdlc/build.md)

Takes a plan file and runs it through the pipeline: `plan/epic` → `decompose` → `loop`. The plan stage creates an epic, decompose breaks it into implementation subtasks, and loop orchestrates their execution via parallel lanes.

```bash
aiki build ops/now/user-auth.md
```

### [Review](sdlc/review.md)

Evaluates code or plans against structured criteria. Reviews track issues as comments that can be piped directly into `fix`. Aiki auto-assigns a different agent than the one that wrote the code.

```bash
aiki review <task-id>
```

### [Fix](sdlc/fix.md)

Reads issues from a review and runs them through a pipeline: `plan/fix` → `decompose` → `loop`. A Rust-driven review loop re-reviews fixes until the code is clean (up to 10 iterations). Use `--once` for a single pass without the review loop.

```bash
aiki fix <review-task-id>
```

### Loop

`aiki loop` orchestrates a parent task's subtasks via parallel lanes. It creates a loop task from the `aiki/loop` template, wires up an `orchestrates` link, and runs subtasks to completion. Both `build` and `fix` use `loop` internally, but it can also be invoked standalone.

```bash
aiki loop <parent-task-id>
```

### Resolve

`aiki resolve` resolves JJ merge conflicts. It takes a change ID with conflicts, creates a task from the `aiki/resolve` template, and runs an agent to resolve the conflicts.

```bash
aiki resolve <change-id>
```

## Composable Stages

Build and fix are pipelines composed from three reusable stages:

### `plan`

Produces a plan file. Two families exist:

- **`plan/epic`** — interactive plan authoring for new features (used by `aiki plan` and `aiki build`)
- **`plan/fix`** — generates a fix plan from review issues (used internally by `aiki fix`)

### `decompose`

Reads a plan file and creates subtasks under a parent task (an epic). Each subtask contains enough context for an agent to complete it independently. Dependencies between subtasks are expressed via links (`--depends-on`, `--needs-context`), which determine the execution graph.

### `loop`

Orchestrates subtask execution via parallel lanes. Schedules subtasks based on their dependency graph, running independent tasks concurrently and respecting ordering constraints. Both `aiki build` and `aiki fix` call `loop` after decomposition; `aiki loop` exposes it as a standalone command.

**Build pipeline:** `plan/epic` → `decompose` → `loop`
**Fix pipeline:** `plan/fix` → `decompose` → `loop` → review → *(repeat until clean)*

## Pipelines

Commands are composable. Use pipes or flags to chain phases together:

```bash
# Review then fix
aiki review <task-id> | aiki fix

# Build then review
aiki build ops/now/plan.md --review

# Build, review, and auto-fix
aiki build ops/now/plan.md --fix

# Review with auto-fix
aiki review <task-id> --fix
```

## Commands

| Command | Default mode | Flags | Notes |
|---------|-------------|-------|-------|
| `aiki fix` | blocking (review loop) | `--once`, `--async` | `--once` disables the post-fix review loop |
| `aiki resolve` | blocking | `--async`, `--start` | Merge-conflict resolution |
| `aiki review` | blocking | `--async` | Auto-assigns a different agent |
| `aiki loop` | blocking | `--async` | Standalone subtask orchestration |

## Next Steps

- [Plan](sdlc/plan.md) — interactive plan authoring
- [Build](sdlc/build.md) — decomposition and execution
- [Review](sdlc/review.md) — structured evaluation
- [Fix](sdlc/fix.md) — automated remediation

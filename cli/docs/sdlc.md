# SDLC: Plan, Build, Review, Fix

Aiki provides a structured software development lifecycle for AI-assisted coding. Four commands form a closed feedback loop: you plan what to build, an agent builds it, another agent reviews the output, and issues get fixed automatically.

## The Workflow

```
  plan            build           review            fix
┌────────┐    ┌──────────┐    ┌───────────┐    ┌──────────┐
│ Write a │───▶│Decompose │───▶│ Evaluate  │───▶│ Create   │
│  spec   │    │& execute │    │ against   │    │ followup │──┐
│         │    │  tasks   │    │ criteria  │    │  tasks   │  │
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

Takes a plan file, decomposes it into an epic with implementation subtasks, and executes them. Tasks can run sequentially or in parallel lanes depending on their dependencies.

```bash
aiki build ops/now/user-auth.md
```

### [Review](sdlc/review.md)

Evaluates code or plans against structured criteria. Reviews track issues as comments that can be piped directly into `fix`. Aiki auto-assigns a different agent than the one that wrote the code.

```bash
aiki review <task-id>
```

### [Fix](sdlc/fix.md)

Reads issues from a review and creates followup tasks for each one. Includes a quality loop that re-reviews fixes until the code is clean (up to 10 iterations).

```bash
aiki fix <review-task-id>
```

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

## Next Steps

- [Plan](sdlc/plan.md) — interactive plan authoring
- [Build](sdlc/build.md) — decomposition and execution
- [Review](sdlc/review.md) — structured evaluation
- [Fix](sdlc/fix.md) — automated remediation

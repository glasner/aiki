# Aiki: AI coding with engineer control

Aiki is a workflow layer for teams that let AI edit code without losing control.
It keeps every AI-assisted change tied to a task, tracks how it was made, and gives you a built-in review loop so work stays reviewable instead of becoming context soup.

For teams, this means AI can move faster **without** becoming a black box.

## What Aiki solves

AI coding in a repository usually breaks into the same two problems:

1. **Context falls apart** across sessions and agents.
2. **Quality gets uneven** when speed is the only goal.

Aiki addresses both by making edits first-class, trackable work items:

- **Provenance by default** — every change is linked to tasks, comments, and agent sessions.
- **Task orchestration** — planning, execution, review, and fixes run as connected stages.
- **Multi-tool consistency** — the same workflow works across Claude Code, Codex, and other tools.
- **Safe concurrency** — parallel agents can work in isolated workspaces and merge cleanly.

## Start in 2 minutes

If this is your first run:

- Follow **[Getting Started](docs/getting-started.md)** for install + repository bootstrap.
- Run `aiki init` and `aiki doctor` in one repo.
- Try one tiny change and watch it flow as a task:
  - start in your chat tool
  - `aiki task start`
  - AI makes the edit
  - `aiki task show` / `aiki task diff` show what happened.

## Two workflow modes

### 1) Chat mode (human-in-the-loop)
Use AI inside your normal editor workflow for interactive work. Aiki records each step so you can pause, inspect, and intervene anytime.

### 2) Headless mode (Plan → Build → Review → Fix)
Use `aiki plan`, `aiki build`, and (optionally) `aiki review`/`aiki fix` for larger, repeatable changes with less manual coordination.

This is the path for “spec first, execute later” work where automation should run as a loop until clean.

## What to read next

- **[Getting Started](docs/getting-started.md)** — install and first run
- **[SDLC: Plan, Build, Review, Fix](docs/sdlc.md)** — full workflow loop
- **[Customizing Defaults](docs/customizing-defaults.md)** — project-specific event hooks and policy
- **[Creating Plugins](docs/creating-plugins.md)** — share reusable flows/templates
- **[Task Types and Links](docs/tasks/kinds.md)** — task graph relationships
- **[Session Isolation Workflow](docs/session-isolation.md)** — how concurrent sessions stay safe

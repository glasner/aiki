# Add `aiki run -i` (interactive mode)

## Goal

Add `-i` / `--interactive` flag to `aiki run` that opens the agent in
the terminal (sync) instead of spawning a background session. This is
the primitive that `aiki plan` builds on.

## Prerequisites

- **[run-command.md](../now/run-command.md)** — The
  `aiki run` command must exist first. This plan adds the interactive
  mode on top.

## Design

```
aiki run <task-id> -i                       # interactive session
aiki run <parent-id> --next-session -i      # resolve next, interactive
```

`-i` / `--interactive` opens the agent in the current terminal. The
command blocks until the agent session ends (the user closes it or the
agent completes). Returns the session UUID on exit.

Default mode (without `-i`) remains background: spawn agent and return
session UUID after the `Started` event.

### How `aiki plan` uses this

`aiki plan` resolves a plan file → creates/starts tasks → calls
`aiki run <task-id> -i` to open an interactive agent session with the
task context. The `-i` flag is what makes `plan` interactive.

## Changes

- Add `interactive: bool` field to the `Run` CLI variant
- In `commands/run.rs`, branch on `interactive`:
  - `true`: call `run_task_with_output()` (sync, terminal-attached)
  - `false`: call `run_task_async_with_output()` (background)
- Both paths discover session UUID from task events

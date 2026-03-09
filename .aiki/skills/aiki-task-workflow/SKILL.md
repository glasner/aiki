---
name: aiki-task-workflow
description: Workflow contract and delegation protocol for running work with `aiki task` lifecycle, review, and validation.
---

# Aiki Task Workflow Skill

## Core rule
Before **any state-changing action** (edit/create/delete/run commands that change repo state), start with:

```bash
aiki task start "<description>" --source prompt
```

Reading files and diagnostics is not work and does not require a task.

Never use native spawner tooling (`Task`, `spawn_agent`, background agents, etc.). Use `aiki task ...` only.

## What counts as work
- Any file modification (write/edit/delete)
- Any build/deploy/installer/configuration change
- Any multi-step implementation or investigation
- Any review with concrete findings

Do **not** start a task for:
- read-only questions
- read-only file reads (`sed`, `cat`, `ls`, `git status`)

## Task start patterns
- One-step: `aiki task start "Task summary"`
- Two-step: `aiki task add "Task summary"` then `aiki task start <id>`
- Template-driven: `aiki task start` + `aiki task run --template ...`

Use `--source prompt` when the user request itself is the task driver.

## Delegation protocol
- Create/run from task ID: `aiki task run <id>`
- Parallel only: create multiple task IDs, run with `--async`, then `aiki task wait <id1> <id2> ...`
- `--async` is **not** fire-and-forget; inspect all `aiki task show <id>` results before replying.

## Progress + closure
- For long work: `aiki task comment add <id> "..."`
- Close only when done: `aiki task close <id> --summary "what/why/results"`
- Do not keep stale open tasks. Leave open only if actively in progress.
- Include task IDs in user-visible summaries and tie results to IDs.

## Review path
If asked for review of changes:

```bash
aiki review <task-id> --start
```

- Add issues with severity + location:
  - `aiki review issue add <review-id> "..." --high --file src/foo.rs:42`
  - supported severities: `--high` / `--low`
- If follow-up is needed: `aiki fix <review-id>`

## Validation requirements
Every closed task summary should include:
- what changed
- how validated (command or proof)
- IDs (task/review IDs, output paths)
- pass/fail + any warnings

## Conflict resolution
Resolve JJ markers before close:
- `<<<<<<<`
- `=======`
- `>>>>>>>`
- `%%%%%%%`
- `+++++++`

## Priorities
`p0` > `p1` > `p2` > `p3`.

## Quick command sheet

```bash
aiki task add/start "Task summary"
aiki task comment add <id> "progress update"
aiki task run <id>
aiki task run <id> --async
aiki task wait <id1> <id2>
aiki task show <id>
aiki task close <id> --summary "done"
aiki review <id> --start
aiki review issue add <id> "..." --high --file path:line
```

## Bad patterns to avoid
- Starting tasks after edits
- Using native subagent spawners
- Closing a task immediately after start
- Responding before async results are collected
- Generic prose without IDs or evidence

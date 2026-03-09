---
name: aiki-task-workflow
description: Detailed workflow for running state-changing work with aiki task lifecycle, review, and delegation.
---

# Aiki Task Workflow Skill

## TL;DR Commands

```bash
aiki task start "Task summary"                      # quick start + tracking
aiki task comment add <id> "progress update"
aiki task run <id>                                # execute task run
aiki task wait <id1> <id2>                        # wait for async runs
aiki task show <id>                               # inspect results and evidence
aiki task close <id> --summary "done"             # close with concise outcome
aiki review <task-id> --start                      # start review task for completed work
aiki review issue add <id> "..." --high --file path:line
```

## Always Do Before Changing State
1. `aiki task start "..."` (or `aiki task add` then `aiki task start`).
2. Perform work.
3. Add progress comments for multi-step work.
4. Close task with evidence-rich summary.

## Delegation Flow
- Synchronous: `aiki task run <id>`.
- Parallel: `aiki task run <idA> --async` + `aiki task run <idB> --async`, then `aiki task wait <idA> <idB>`.
- Do not respond before results are inspected.

## Common Anti-Patterns (Avoid)
- Creating tasks after edits.
- Delegating with native subagent spawners.
- Exiting without closing task.
- Generic progress with no task IDs.
- Closing before verifying outcomes.

## Review Path
- Start review from task ID: `aiki review <task-id> --start`
- Add concrete issues: `aiki review issue add <review-task-id> "..." --high --file <path>:<line>`
- If follow-up is needed: `aiki fix <review-task-id>`.

## Conflict Resolution
When conflict text appears, resolve markers manually:
- `<<<<<<< Conflict N of M`
- `%%%%%%% Changes from base`
- `+++++++ Contents of side #2`
- `>>>>>>> ...`

## Evidence Required in Summaries
Include at minimum:
- what changed
- validation command
- result IDs
- pass/fail status and warnings

## Task Priorities
`p0` (urgent) > `p1` (high) > `p2` (normal) > `p3` (low)

## Quick Review Use Cases
- `aiki task start "Review code in foo.rs"`
- `aiki review <task-id> --start`
- `aiki review issue add <id> "scope bug" --high --file src/foo.rs:45`
- `aiki fix <review-id>` for follow-up tasks

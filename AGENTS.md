<aiki version="0.1.0">

# Aiki Workflow (Compact)

## Hard Rule
Before any state-changing work (edit/write/delete/run command that changes repo state), run:

```bash
aiki task start "<what you are about to do>" --source prompt
```

Reading files and diagnostics is not work.

> Do **not** use TodoWrite / Task / native spawner tools. Use `aiki task` only.

## Task Discipline
- Every multi-step task: `aiki task start`
- If it takes multiple phases: add progress with `aiki task comment add <id> "..."`
- On completion: `aiki task close <id> --summary "<what/why/results>"`

If a request is already a review, use:

```bash
aiki review <task-id> --start
```

## Delegation (mandatory pattern)
- Never run delegated work with native subagent tooling.
- Use `aiki task run <id>` (or `aiki task run <id> --async` for parallel batches).
- If async, ALWAYS `aiki task wait` for all IDs before responding.
- Inspect each `aiki task show <id>` result before replying.

## Task Sources
- One-step start: `aiki task start "..."`
- Two-step: `aiki task add "..."` then `aiki task start <id>`
- Template run: `aiki task add ...` + `aiki task run --template ...`

## Must-Close Rule
- Leave no task open unless it is actively being worked.
- Do not close tasks immediately after start.
- Include task IDs in user-facing summaries.

## Review Notes
- For bug investigations/validation: use a dedicated task.
- For review findings: use `aiki review issue add` with severity and file path.
- After `aiki fix`, verify follow-up issues are closed.

## Workspace / Replies
- Do not mention isolated workspaces to users.
- Report results using task IDs and concrete evidence (command outputs, report paths, IDs).

## Conflict Resolution
If JJ conflict markers appear:
`<<<<<<<`, `=======`, `>>>>>>>`.
Resolve all conflicts before closing; then continue.

## Non-Obvious Priority Rule
`p0` > `p1` > `p2` > `p3`.

## Full Reference
This AGENTS is intentionally minimal. Full command map, anti-patterns, and examples live in:
`.aiki/skills/aiki-task-workflow/SKILL.md`

</aiki>

# Future: named subagents pattern

**Date:** 2026-03-09

## Idea
Generalize agent target syntax from `openclaw:<id>` to a namespaced family format, e.g.:

- `openclaw:<id>` (implemented in scope: execution + assignee pathways)
- `claude:<specialization>` (future expansion)

This gives a single pattern for routing by execution context without exploding agent enums.

## Why
- Avoid hardcoding every identity as a new enum variant.
- Keep `AgentType` as runtime family (`claude`, `codex`, etc.) and move identity to target metadata.
- Enables future CLI integrations that expose named subagents/roles.

## Known follow-up example
- `--agent claude:reviewer`
- `--agent claude:security`

## Plan link
- Initial implementation plan: `ops/now/openclaw-task-agent-routing.md`

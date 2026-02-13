# Understand the plan described in spec

Read the spec file at {{data.scope.id}} to understand plan.

If this spec was created from a task, check the task for additional context:

```bash
aiki task list --source file:{{data.scope.id}}
```

## Review Criteria

Evaluate the spec against these categories:

**Completeness**
- All sections filled, no TODOs or placeholder content
- Open questions documented and flagged
- Dependencies and prerequisites identified

**Clarity**
- Unambiguous requirements with clear acceptance criteria
- No vague language ("should probably", "might need to")
- Technical terms defined or consistently used

**Implementability**
- Can be decomposed into discrete, actionable tasks
- Sufficient technical detail for implementation
- No circular dependencies or impossible constraints

**UX**
- User experience considered where applicable
- Intuitive command syntax and behavior
- Error messages and edge cases addressed

When done close this task with a summary of your understanding:

```bash
aiki task close {{id}} --summary <your summary here>
```

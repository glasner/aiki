# Debug Command

draft: true

## Idea

Add an `aiki debug` command that investigates bug reports and feeds findings into the existing fix pipeline.

### Pipeline

Debug slots in before `plan/fix` — it investigates the problem and produces findings that `plan/fix` can consume (like a review produces issues).

```
BUILD:  plan/epic → decompose → loop
FIX:    plan/fix  → decompose → loop
BUG:    debug → plan/fix → decompose → loop
```

### Four composable stages

| Stage | Input | Output |
|---|---|---|
| `debug` | bug report | findings/issues (like a review) |
| `plan/fix` | issues (from review or debug) | fix plan file |
| `decompose` | plan file | subtasks under parent |
| `loop` | parent with subtasks | executed work |

Debug produces output in the same format as a review's issues, so `plan/fix` doesn't need to know whether it's reading review issues or debug findings. Same interface, different source.

### Template: `aiki/debug`

Takes a bug report (issue, error log, user description) and produces structured findings:
- Reproduce the issue
- Identify root cause
- Identify affected files/components
- Output findings as issues that `plan/fix` can read

### Open Questions

- Input format: GitHub issue? Plain text? Error log?
- Should debug output structured issues (like `aiki review issue add`) so plan/fix can consume them uniformly?
- Integration with `aiki fix` — should `aiki fix <bug-report>` detect non-review input and auto-insert the debug phase?
- Or is `aiki debug` a separate command that pipes into `aiki fix`?

## Related

- [implement-loop-refactor.md](../now/implement-loop-refactor.md) — introduces `aiki/plan/fix`, `aiki/plan/epic`, and `aiki/loop` that this builds on

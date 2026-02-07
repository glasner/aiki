# Template Variables Reference

All variables available inside aiki templates, organized by namespace.

## Source Files

- **Variable context & substitution**: `cli/src/tasks/templates/variables.rs`
- **Template resolution**: `cli/src/tasks/templates/resolver.rs`
- **Variable population in commands**: `cli/src/commands/task.rs` (~line 4082-4107)

---

## 1. Builtins — `{{variable}}`

Set automatically for every task.

| Variable | Description |
|----------|-------------|
| `{{id}}` | The task's own ID |
| `{{assignee}}` | Assigned agent/human |
| `{{priority}}` | Priority level (defaults to `p2`) |
| `{{type}}` | Task type from template defaults |
| `{{created}}` | ISO 8601 timestamp |

## 2. Data — `{{data.key}}`

Custom values from `--data key=value` or template frontmatter defaults.

| Example | Set via |
|---------|---------|
| `{{data.scope}}` | `--data scope="@"` |
| `{{data.spec}}` | `--data spec="ops/now/feature.md"` |
| `{{data.anything}}` | `--data anything=value` |

Values are **auto-coerced**: `"true"` -> bool, `"42"` -> int, `"3.14"` -> float, else string.

## 3. Source — `{{source}}`, `{{source.field}}`

From `--source` flag, with parsed sub-fields depending on type.

| Source type | Sub-fields available |
|-------------|---------------------|
| `task:abc123` | `{{source.id}}`, `{{source.type}}` |
| `file:path/to/file` | `{{source.id}}`, `{{source.path}}`, `{{source.type}}` |
| `comment:c1a2b3` | `{{source.id}}`, `{{source.type}}` |
| `issue:GH-123` | `{{source.id}}`, `{{source.type}}` |
| `prompt:nzwtoqqr` | `{{source.id}}`, `{{source.type}}` |

## 4. Parent — `{{parent.key}}`

Only available in subtask templates. References the parent task.

| Variable | Description |
|----------|-------------|
| `{{parent.id}}` | Parent task ID |
| `{{parent.name}}` | Parent task name |
| `{{parent.assignee}}` | Parent's assignee |
| `{{parent.priority}}` | Parent's priority |
| `{{parent.data.key}}` | Parent's data variables |
| `{{parent.source}}` | Parent's primary source |
| `{{parent.source.field}}` | Parsed parent source fields |

## 5. Item — `{{item.key}}`

Only inside `{% for %}` loops (dynamic subtasks iterating over data sources).

| Variable | Description |
|----------|-------------|
| `{{item}}` | The item value (string, number, or object) |
| `{{item.field}}` | Object field access (if item is an object) |

## 6. Loop Metadata — `{{loop.property}}`

Inside `{% for %}` blocks.

| Variable | Description |
|----------|-------------|
| `{{loop.index}}` | 1-based iteration number |
| `{{loop.index0}}` | 0-based iteration number |
| `{{loop.first}}` | `true` on first iteration |
| `{{loop.last}}` | `true` on last iteration |
| `{{loop.length}}` | Total item count |

## 7. Scope — `{{scope}}`

Only in review templates (via `aiki review`).

| Variable | Description |
|----------|-------------|
| `{{scope}}` | Scope identifier (task ID or "session") |
| `{{scope.id}}` | Same as scope |
| `{{scope.name}}` | Human-readable ("task abc123" or "current session") |

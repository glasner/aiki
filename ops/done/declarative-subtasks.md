# Declarative Subtasks from Templates

**Status**: Implemented
**Related**: [Review and Fix](review-and-fix.md), [Task Templates](../done/task-templates.md)

---

## Summary

Extend the template system to support declarative subtask creation, including iteration over data sources.

## Motivation

Some workflows need to create a parent task with multiple subtasks atomically:

- **Review followup**: One subtask per review comment
- **Multi-file tasks**: One subtask per file to process
- **Batch operations**: One subtask per item in a list

Currently templates can define static subtasks, but can't iterate over dynamic data.

## Changes

### Template Syntax: `subtasks`

New frontmatter field to iterate over a data source:

```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {source.name}

Fix all issues identified in review.

# Subtasks

## Fix: {item.file}:{item.line}

**Severity**: {item.severity}
**Category**: {item.category}

{item.text}
```

**How it works:**
- `subtasks: source.comments` tells template to iterate over comments array
- Each comment becomes one subtask
- The `# Subtasks` section is the template for each item
- Current item exposes `{item.text}` (comment body) and `{item.*}` (structured fields)
- Parent context available via `parent.*` prefix

### `create_task_from_template()`

New function in template resolver for atomic parent+subtask creation:

```rust
// tasks/templates/resolver.rs
pub fn create_task_from_template(
    template_name: &str,
    variables: HashMap<String, String>,
) -> Result<String> {
    // 1. Load and parse template
    // 2. Resolve variables in parent task
    // 3. If subtasks specified, iterate over data source
    // 4. Create parent + all subtasks atomically
    // 5. Return parent task ID
}
```

### Data Sources

Initial supported sources:

| Source | Description |
|--------|-------------|
| `source.comments` | Comments from a task (via `--source task:<id>`) |

Future sources could include:
- `source.files` - Files from a task's changes
- `source.changes` - JJ changes from a revset
- Custom data passed via CLI

## Implementation

### Template Resolution Flow

1. Parse template frontmatter for `subtasks`
2. If present, load the data source (e.g., fetch comments from task)
3. For each item in data source:
   - Clone subtask template section
   - Resolve `{text}`, `{data.*}` variables from current item
   - Resolve `{parent.*}` variables from parent context
4. Create parent task
5. Create all subtasks as children
6. Return parent task ID

### Files

- `cli/src/tasks/templates/resolver.rs` - Implement `create_task_from_template()` with iteration
- `cli/src/tasks/templates/types.rs` - Add `subtasks` field to template frontmatter

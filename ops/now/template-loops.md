# Template Loops

**Date**: 2026-02-05
**Status**: Draft
**Purpose**: Replace frontmatter-based subtask iteration with inline Tera-style `{% for %}` loops

**Related Documents**:
- [Template Conditionals](../done/template-conditionals.md) - Implemented `{% if %}` syntax
- [Declarative Subtasks](../done/declarative-subtasks.md) - Current frontmatter-based iteration (to be deprecated)
- [Task Templates](../done/task-templates.md) - Base template system

---

## Executive Summary

Replace the frontmatter `subtasks: source.comments` declaration with inline `{% for %}` loops, completing Aiki's adoption of Tera-style template syntax. This provides a more intuitive, flexible, and consistent templating experience.

**Current approach (frontmatter declaration):**
```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {{source.name}}

# Subtasks

## Fix: {{item.file}}
{{item.text}}
```

**Proposed approach (inline loops):**
```markdown
---
version: 1.0.0
---

# Followup: {{source.name}}

# Subtasks

{% for item in source.comments %}
## Fix: {{item.file}}
{{item.text}}
{% endfor %}
```

---

## Motivation

The current `subtasks:` frontmatter approach has limitations:

1. **Hidden magic** - The iteration happens implicitly; you must know the `# Subtasks` section is special
2. **Single loop only** - Can't iterate over multiple collections in one template
3. **No loop nesting** - Can't have loops inside loops
4. **Inconsistent with conditionals** - We have inline `{% if %}` but frontmatter-based iteration
5. **Limited scope** - Only works for subtask generation, not general content

Inline `{% for %}` loops solve all of these while maintaining Tera syntax consistency.

---

## Design

### Loop Syntax

Standard Tera/Jinja2 for-loop syntax:

```markdown
{% for item in collection %}
  Content using {{item.field}}
{% endfor %}
```

**Components:**
- `item` - Loop variable name (user-defined)
- `collection` - Data source to iterate over
- Content between tags is repeated for each item

### Loop Variable Naming

The loop variable name is user-defined:

```markdown
{% for comment in source.comments %}
  {{comment.text}}
{% endfor %}

{% for file in data.files %}
  {{file.path}}
{% endfor %}

{% for c in source.comments %}
  {{c.text}}
{% endfor %}
```

**Naming rules:**
- Must be a valid identifier: `[a-z_][a-z0-9_]*`
- Validated by `validation::is_valid_template_identifier()` (centralized validation)
- Shadows any outer variable with the same name
- Only valid within the loop body

### Data Sources

**Currently supported:**

| Source | Description | Available Fields |
|--------|-------------|------------------|
| `source.comments` | Comments from source task | `text`, `file`, `line`, `severity`, `category`, `id` |

**Planned sources:**

| Source | Description | Available Fields |
|--------|-------------|------------------|
| `source.files` | Files changed in source task | `path`, `status`, `lines_added`, `lines_removed` |
| `data.<array>` | Array passed via `--data` | User-defined |

### Loop with Index

Support loop metadata via special variables:

```markdown
{% for item in source.comments %}
## {{loop.index}}. Fix: {{item.file}}

{{item.text}}

{% if loop.first %}
Start with this one.
{% endif %}

{% if loop.last %}
This is the final item.
{% endif %}
{% endfor %}
```

**Loop metadata variables:**

| Variable | Type | Description |
|----------|------|-------------|
| `loop.index` | int | 1-based index (1, 2, 3, ...) |
| `loop.index0` | int | 0-based index (0, 1, 2, ...) |
| `loop.first` | bool | True for first iteration |
| `loop.last` | bool | True for last iteration |
| `loop.length` | int | Total number of items |

### Nesting Loops

Loops can be nested:

```markdown
{% for category in data.categories %}
## {{category.name}}

{% for item in category.items %}
- {{item.description}}
{% endfor %}

{% endfor %}
```

**Scope rules:**
- Inner loops can access outer loop variables
- Inner loop variables shadow outer ones with the same name
- `loop.*` variables refer to the innermost loop

### Loops with Conditionals

Loops integrate naturally with the existing conditional system:

```markdown
{% for item in source.comments %}
{% if item.severity == "high" %}
## CRITICAL: {{item.file}}

**Priority fix required.**

{{item.text}}
{% elif item.severity == "medium" %}
## Fix: {{item.file}}

{{item.text}}
{% endif %}
{% endfor %}
```

**Evaluation order:**
1. Parse loop structure
2. For each item in collection:
   a. Set loop variable
   b. Evaluate conditionals
   c. Substitute variables
   d. Emit content (if not empty)

### Empty Collections

When a collection is empty, the loop body is skipped entirely:

```markdown
{% for item in source.comments %}
## Fix: {{item.file}}
{% endfor %}

{% if not source.comments %}
No issues found!
{% endif %}
```

**Alternative: `{% else %}` on loops (Jinja2 style):**

```markdown
{% for item in source.comments %}
## Fix: {{item.file}}
{% else %}
No issues found!
{% endfor %}
```

The `{% else %}` block executes only when the collection is empty.

---

## Subtask Generation

### How Loops Create Subtasks

When a `## ` heading appears inside a loop in the `# Subtasks` section, each iteration creates a separate subtask:

```markdown
# Subtasks

{% for item in source.comments %}
## Fix: {{item.category}} in {{item.file}}

{{item.text}}
{% endfor %}
```

If `source.comments` has 3 items, this creates 3 subtasks.

### Conditional Subtask Skipping

Subtasks can be conditionally skipped:

```markdown
# Subtasks

{% for item in source.comments %}
{% if item.severity != "low" %}
## Fix: {{item.file}}

{{item.text}}
{% endif %}
{% endfor %}
```

**Rule:** If the `## ` heading is inside a conditional that evaluates to false, that subtask is not created.

### Multiple Loops in Subtasks

Unlike the frontmatter approach, you can have multiple loops:

```markdown
# Subtasks

## Review high-severity issues

{% for item in source.comments %}
{% if item.severity == "high" %}
### {{item.file}}:{{item.line}}
{{item.text}}
{% endif %}
{% endfor %}

## Address medium-severity issues

{% for item in source.comments %}
{% if item.severity == "medium" %}
### {{item.file}}:{{item.line}}
{{item.text}}
{% endif %}
{% endfor %}
```

This creates two static subtasks, each containing filtered content from the loop.

### Nested Headings in Loops

Only `## ` headings at the subtask level create subtasks. `### ` and below are content:

```markdown
# Subtasks

{% for group in data.groups %}
## Process: {{group.name}}

{% for item in group.items %}
### {{item.title}}
{{item.description}}
{% endfor %}
{% endfor %}
```

This creates one subtask per group, with nested content inside each.

---

## Examples

### Fix Template (Updated)

```markdown
---
version: 1.0.0
---

# Followup: {{source.name}} (task:{{source.id}})

Please close all subtasks once they are either fixed or declined.

# Subtasks

{% for item in source.comments %}
## Fix: {{item.category}} - {{item.severity}} Severity

**File**: {{item.file}}:{{item.line}}
**Category**: {{item.category}}

{{item.text}}
{% endfor %}
```

### Review Template with Conditional Loops

```markdown
---
version: 1.0.0
---

# Review: {{data.target_name}}

# Subtasks

## Understand the changes

{% if data.target_type == "file" %}
Read `{{data.path}}` to understand the document.
{% else %}
Run `aiki task diff {{data.task_id}}` to see the changes.
{% endif %}

## Evaluate quality

{% if data.target_type == "file" %}
Check for completeness, clarity, and implementability.
{% else %}
Check for correctness, security, and performance issues.
{% endif %}

{% for focus in data.review_focuses %}
## {{focus.name}} Review

{{focus.instructions}}

{% for check in focus.checklist %}
- [ ] {{check}}
{% endfor %}
{% endfor %}
```

### Batch File Processing

```markdown
---
version: 1.0.0
---

# Process Files: {{data.operation}}

Processing {{source.files | length}} files.

# Subtasks

{% for file in source.files %}
{% if file.status != "deleted" %}
## {{data.operation}}: {{file.path}}

Apply {{data.operation}} to this file.

**Status**: {{file.status}}
**Lines changed**: +{{file.lines_added}} / -{{file.lines_removed}}
{% endif %}
{% endfor %}
```

---

## Migration

Remove the `subtasks:` frontmatter field and update built-in templates to use `{% for %}` loops.

**Templates to update:**
- `.aiki/templates/aiki/fix.md` - Wrap subtask section in `{% for item in source.comments %}`

**Code changes:**
- Remove `subtasks` field from `TemplateFrontmatter` struct
- Remove `subtasks_source` and `subtask_template` from `TaskTemplate` struct
- Remove special-case handling in resolver for frontmatter-based iteration
- Delete `data_source.rs` module (no longer needed)

---

## Implementation

### Phase 1: Parser Extension

**Files:**
- `cli/src/tasks/templates/conditionals.rs` - Add loop parsing to existing tokenizer

**New AST nodes:**
```rust
enum TemplateNode {
    Text(String),
    Variable(String),
    Conditional { ... },  // Existing
    Loop {
        variable: String,           // "item"
        collection: String,         // "source.comments"
        body: Vec<TemplateNode>,
        else_body: Option<Vec<TemplateNode>>,  // For {% else %} on empty
    },
}
```

**Tokenizer additions:**
- `{% for <var> in <collection> %}` → `ControlBlock::For`
- `{% endfor %}` → `ControlBlock::EndFor`

### Phase 2: Evaluator Extension

**Loop evaluation:**
```rust
fn evaluate_loop(
    variable: &str,
    collection: &str,
    body: &[TemplateNode],
    else_body: Option<&[TemplateNode]>,
    context: &mut Context,
) -> Result<String> {
    let items = resolve_collection(collection, context)?;

    if items.is_empty() {
        return match else_body {
            Some(nodes) => evaluate_nodes(nodes, context),
            None => Ok(String::new()),
        };
    }

    let mut output = String::new();
    let len = items.len();

    for (index, item) in items.iter().enumerate() {
        // Set loop variables
        context.set("loop.index", index + 1);
        context.set("loop.index0", index);
        context.set("loop.first", index == 0);
        context.set("loop.last", index == len - 1);
        context.set("loop.length", len);
        context.set(variable, item);

        output.push_str(&evaluate_nodes(body, context)?);
    }

    Ok(output)
}
```

### Phase 3: Integration

**Update template resolver:**
- Remove special handling for `subtasks:` frontmatter
- Process loops during normal template evaluation
- Subtask extraction happens after loop expansion

**Processing order:**
1. Parse frontmatter (no loop processing yet)
2. Tokenize template body
3. Parse into AST (loops, conditionals, text, variables)
4. Evaluate AST with context (expands loops, evaluates conditionals)
5. Substitute remaining variables
6. Extract subtasks from `# Subtasks` section

---

## Error Handling

| Error | Message |
|-------|---------|
| Unclosed `{% for %}` | `Error: Unclosed loop starting at line N. Expected {% endfor %}` |
| `{% endfor %}` without `{% for %}` | `Error: Unexpected {% endfor %} without matching {% for %}` |
| Invalid loop syntax | `Error: Invalid loop syntax at line N. Expected: {% for var in collection %}` |
| Unknown collection | `Error: Unknown collection 'source.foo'. Available: source.comments` |
| Invalid variable name | `Error: Invalid loop variable 'for'. Must match [a-z_][a-z0-9_]*` |

---

## Comparison with Full Tera

This implements a subset of Tera's loop functionality:

| Feature | Supported | Notes |
|---------|-----------|-------|
| `{% for x in y %}` | Yes | Basic iteration |
| `{% endfor %}` | Yes | Close loop |
| `{% else %}` on loops | Yes | Empty collection fallback |
| `loop.index`, `loop.first`, etc. | Yes | Loop metadata |
| `{% break %}` | No | Keep templates simple |
| `{% continue %}` | No | Use conditionals instead |
| Filters in loops | No | `{% for x in y \| filter %}` not supported |
| Destructuring | No | `{% for k, v in dict %}` not supported |

---

## Summary

Inline `{% for %}` loops provide:

1. **Consistency** - Same syntax as conditionals (`{% %}` for control flow)
2. **Flexibility** - Multiple loops, nesting, loops inside conditionals
3. **Clarity** - Iteration is visible in the template, not hidden in frontmatter
4. **Power** - Loop metadata, empty collection handling, conditional filtering
5. **Simplicity** - Natural Tera syntax familiar to Rust developers

This completes Aiki's template system evolution from simple variable substitution to a capable (but intentionally limited) templating language.

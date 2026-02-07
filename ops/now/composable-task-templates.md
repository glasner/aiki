# Composable Task Templates

**Date**: 2026-02-07
**Status**: Draft
**Purpose**: Allow task templates to include other templates as nested subtasks

**Related Documents**:
- [Task Templates](../done/task-templates.md) - Base template system
- [Template Conditionals](../done/template-conditionals.md) - `{% if %}` syntax
- [Template Loops](../done/template-loops.md) - `{% for %}` syntax
- [Plan and Build Commands](../done/plan-and-build-commands.md) - Current composition via CLI
- [Review and Fix Non-Task Targets](review-and-fix-files.md) - Adaptive review workflows using specialized templates

**Future Enhancements**:
- [Custom Variables for Subtask Templates](../future/custom-variables-for-subtask-templates.md) - `with` clause for passing variables to composed templates

---

## Executive Summary

Templates today are standalone: each defines a flat parent + subtasks structure. Composition happens through CLI commands (e.g., `build.md` shells out to `aiki plan` which uses `plan.md`). This is fragile and opaque.

Composable templates let a template reference another complete template as a subtask, creating a nested task tree. The child template inherits the parent's variable context and produces its own sub-subtasks within the tree.

---

## Motivation

Templates today are standalone: each defines a flat parent + subtasks structure. To reuse template logic, you need to compose templates together.

**Composable templates** let a template reference another complete template as a subtask, creating a nested task tree:

```markdown
# Subtasks

{% subtask aiki/plan %}

## Execute Subtasks
Run through each plan subtask.
```

The template engine handles creating the nested task tree. The agent sees a subtask called "Plan: feature.md" with its own sub-subtasks already created, ready to work through.

**Conditional inclusion** lets templates adapt based on context:

```markdown
{% subtask aiki/review/spec if data.file_type == "spec" %}
{% subtask aiki/review/code if data.file_type == "code" %}
```

This enables a single `aiki review` command to work across different content types by delegating to specialized templates.

---

## User Experience

### Syntax

Tera-style block tag, consistent with `{% if %}` and `{% for %}`:

```
{% subtask <template-name> %}
```

With optional inline conditional (Jinja2-style):

```
{% subtask <template-name> if condition %}
```

**Grammar:**
```
subtask      := "{% subtask" template_name [if_clause] "%}"
template_name := identifier ("/" identifier)*
if_clause    := "if" condition
condition    := <same as {% if %} conditions>
```

Composed subtasks inherit all variables from the parent template's context.

### Example: Specialized Review Templates

Different types of content need different review approaches. Using conditional subtask inclusion, a unified review workflow can delegate to specialized templates:

```markdown
---
version: 2.0.0
type: review
---

# Review: {{data.target_name}}

Review the target and provide feedback.

# Subtasks

## Understand what you're reviewing
Identify the target type and review approach.

{% subtask aiki/review/spec if data.file_type == "spec" %}
{% subtask aiki/review/code if data.file_type == "code" %}
{% subtask aiki/review/task if data.target_type == "task" %}

## Provide feedback
Leave comments on issues found.
```

**How it works:**
- If reviewing a spec file (`data.file_type == "spec"`), include `aiki/review/spec` subtask (which has spec-specific evaluation criteria)
- If reviewing code (`data.file_type == "code"`), include `aiki/review/code` subtask (which checks for bugs, style, tests)
- If reviewing a task (`data.target_type == "task"`), include `aiki/review/task` subtask (which examines task changes)

Each specialized template brings its own subtasks with domain-specific evaluation steps, but they all fit into the same parent review workflow.

---

## How It Works

### Processing Order

1. Parse frontmatter
2. Process conditionals (`{% if %}`)
3. Expand loops (`{% for %}`)
4. **Resolve subtask references** (`{% subtask %}`) — NEW
5. Substitute remaining variables
6. Extract subtasks from `# Subtasks` section

### Subtask Resolution

When the template engine encounters `{% subtask aiki/plan %}`:

1. Load the referenced template (`aiki/plan`)
2. Build a variable context:
   - Start with the **parent template's full context** (all variables inherited)
   - Apply any `with` overrides
3. Resolve the child template's parent name (this becomes the subtask name)
4. Create the subtask in the task tree
5. Recursively create the child template's subtasks as sub-subtasks

### Task Tree Structure

Given a `build.md` template:
```markdown
---
version: 1.0.0
type: build
---

# Build: {{data.spec}}

# Subtasks

{% subtask aiki/plan %}

## Execute Subtasks
Run each plan subtask.
```

And `plan.md` has subtasks "Read spec" and "Create subtasks", the result is:

```
Build: feature.md                    (parent)
├── Plan: feature.md                 (subtask from {% subtask aiki/plan %})
│   ├── Read spec                    (plan's subtask .1)
│   └── Create subtasks              (plan's subtask .2)
└── Execute Subtasks                 (static subtask)
```

Task IDs follow the existing convention:
- `<build-id>` — Build parent
- `<build-id>.1` — Plan (child template subtask)
- `<build-id>.1.1` — Read spec (grandchild)
- `<build-id>.1.2` — Create subtasks (grandchild)
- `<build-id>.2` — Execute Subtasks (static subtask)

### Variable Inheritance

Child templates inherit the parent's full `VariableContext`:

| Variable namespace | Behavior |
|---|---|
| `data.*` | Inherited from parent |
| `source.*` | Inherited from parent |
| `parent.*` | Rebound to the **composed subtask**, not the top-level parent |
| `id` | Set to the child subtask's generated ID |
| `builtins.*` | Inherited (assignee, priority, type) |

**Example:**
```markdown
# Parent template sets data.spec = "feature.md"
{% subtask aiki/plan %}
# Plan template sees: data.spec = "feature.md" (inherited)
```

### Recursion Protection

Templates can compose other templates, which could compose more. To prevent infinite recursion:

- **Max depth**: 4 levels of nesting (configurable)
- **Cycle detection**: Track template names in the composition stack; error if a template tries to include itself (directly or transitively)

Error: `Template cycle detected: aiki/build → aiki/plan → aiki/build`

---

## Use Case: Adaptive Review Workflows

The primary use case is **specialized review templates** that adapt based on what's being reviewed:

**Problem:** Different content types (specs, code, tasks) need different review approaches, but you want a unified `aiki review` command that works for all of them.

**Solution:** Use conditional subtask inclusion to delegate to specialized templates:

```bash
# Review a spec document
aiki review ops/now/feature.md
# → Creates review with aiki/review/spec subtask

# Review task changes
aiki review xqrmnpst
# → Creates review with aiki/review/task subtask

# Review code files
aiki review src/auth.rs
# → Creates review with aiki/review/code subtask
```

The unified `aiki/review` template uses `{% subtask %}` with `if` conditions to include the appropriate specialized template based on `data.file_type` or `data.target_type` set by the CLI.

---

## Implementation Plan

### Phase 1: Parser — `{% subtask %}` tag

**Files:** `cli/src/tasks/templates/conditionals.rs`

- Add `SubtaskRef` token: `{% subtask <name> %}` and `{% subtask <name> if condition %}`
- Add `TemplateNode::SubtaskRef { template_name, condition }` AST node
- Emit marker during conditional processing: `<!-- AIKI_SUBTASK:template_name -->`

### Phase 2: Resolver — expand subtask markers

**Files:** `cli/src/tasks/templates/resolver.rs`, `cli/src/commands/task.rs`

- Add `expand_subtask_refs()` function that processes `<!-- AIKI_SUBTASK:... -->` markers
- Load referenced templates, build child variable contexts
- Create nested task events (subtask + sub-subtasks)
- Add recursion depth tracking and cycle detection

### Phase 3: Update `create_from_template`

**Files:** `cli/src/commands/task.rs`

- After expanding loops and conditionals, scan for subtask markers
- For each marker, recursively call template resolution
- Generate proper nested task IDs (`parent.N.M`)

### Phase 4: Update built-in templates

**Files:** `.aiki/templates/aiki/build.md`

- Replace CLI composition (`aiki plan ...`) with `{% subtask aiki/plan %}`
- Test that build workflow creates correct nested task tree

---

## Error Handling

| Error | Message |
|-------|---------|
| Template not found | `Template 'myorg/missing' not found in {% subtask %} at line N` |
| Cycle detected | `Template cycle detected: aiki/build → aiki/plan → aiki/build` |
| Max depth exceeded | `Template composition depth limit (4) exceeded at 'aiki/deep'` |
| Invalid `if` syntax | `Invalid {% subtask %} syntax at line N. Expected: {% subtask name if condition %}` |
| Missing required variable | Standard variable-not-found error, but with composition context in the message |

---

## Design Decisions

1. **`{% subtask %}` only works in `# Subtasks` sections** — not for general content inclusion. This keeps the feature focused: it creates nested task trees, not text partials.
2. **No name overrides** — the child template's resolved name (e.g., "Plan: feature.md") is used as-is. No `as "..."` syntax.
3. **`parent.*` points to the composed subtask** — sub-subtasks inside a composed template see their immediate parent (the composed subtask), not the top-level root. This is consistent with how `parent.*` works for static subtasks today.

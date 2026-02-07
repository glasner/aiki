# Composable Task Templates

**Date**: 2026-02-07
**Status**: Draft
**Purpose**: Allow task templates to include other templates as nested subtasks

**Related Documents**:
- [Task Templates](../done/task-templates.md) - Base template system
- [Template Conditionals](../done/template-conditionals.md) - `{% if %}` syntax
- [Template Loops](../done/template-loops.md) - `{% for %}` syntax
- [Plan and Build Commands](../done/plan-and-build-commands.md) - Current composition via CLI

---

## Executive Summary

Templates today are standalone: each defines a flat parent + subtasks structure. Composition happens through CLI commands (e.g., `build.md` shells out to `aiki plan` which uses `plan.md`). This is fragile and opaque.

Composable templates let a template reference another complete template as a subtask, creating a nested task tree. The child template inherits the parent's variable context and produces its own sub-subtasks within the tree.

---

## Motivation

### Current workaround: CLI-level composition

`build.md` currently composes with `plan.md` by shelling out:

```markdown
{% if not data.plan %}
### Create Plan
No existing plan. Create one using the plan command:
aiki plan {{data.spec}}
{% endif %}
```

Problems with this approach:
1. **Fragile** — relies on agents correctly parsing CLI output and extracting task IDs
2. **Opaque** — the template doesn't declare its dependencies; you must read instructions to discover them
3. **No type safety** — mismatched variables between templates aren't caught until runtime
4. **Agent-dependent** — the composition logic runs inside the agent's session, not in the template engine

### What composable templates enable

```markdown
# Subtasks

{% subtask aiki/plan %}

### Execute Subtasks
...
```

The template engine handles creating the nested task tree. The agent sees a subtask called "Plan: feature.md" with its own sub-subtasks already created, ready to work through.

---

## User Experience

### Syntax

Tera-style block tag, consistent with `{% if %}` and `{% for %}`:

```
{% subtask <template-name> %}
```

With optional variable overrides:

```
{% subtask <template-name> with key=value, key2=value2 %}
```

**`with` clause grammar:**
```
with_clause  := "with" assignment ("," assignment)*
assignment   := identifier "=" value
value        := quoted_string | variable_ref
quoted_string := '"' ... '"'
variable_ref  := identifier ("." identifier)*
```

Values can be string literals (`"auth module"`) or variable references (`data.task_id`, `module.name`). Variable references are resolved against the current context at composition time.

### Examples

**Simple composition:**
```markdown
# My Workflow

# Subtasks

{% subtask aiki/plan %}

## Execute the plan
Run through each plan subtask.

{% subtask aiki/review %}
```

Creates:
```
My Workflow
├── Plan: feature.md            (from aiki/plan, with its own subtasks)
├── Execute the plan            (static subtask)
└── Review: ...                 (from aiki/review, with its own subtasks)
```

**With variable overrides:**
```markdown
{% subtask aiki/review with scope="auth module", scope_id=data.task_id %}
```

**Inside loops:**
```markdown
{% for module in data.modules %}
{% subtask aiki/review with scope=module.name %}
{% endfor %}
```

**Inside conditionals:**
```markdown
{% if data.needs_review %}
{% subtask aiki/review %}
{% endif %}
```

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
| `data.*` | Inherited. `with` overrides take precedence. |
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

```markdown
{% subtask aiki/plan with spec="different.md" %}
# Plan template sees: data.spec = "different.md" (overridden)
```

### Recursion Protection

Templates can compose other templates, which could compose more. To prevent infinite recursion:

- **Max depth**: 4 levels of nesting (configurable)
- **Cycle detection**: Track template names in the composition stack; error if a template tries to include itself (directly or transitively)

Error: `Template cycle detected: aiki/build → aiki/plan → aiki/build`

---

## Use Cases

### 1. Build workflow (current pain point)

**Before** (CLI composition):
```markdown
### Create Plan
aiki plan {{data.spec}}
# Agent must parse output, extract ID, store it...
```

**After** (template composition):
```markdown
{% subtask aiki/plan %}
```

### 2. Full development workflow

```markdown
---
version: 1.0.0
---

# Dev: {{data.spec}}

Complete development workflow: plan, build, review.

# Subtasks

{% subtask aiki/plan %}
{% subtask aiki/build %}
{% subtask aiki/review %}
```

### 3. Organization-specific workflows

```markdown
---
version: 1.0.0
---

# Feature: {{data.name}}

# Subtasks

{% subtask aiki/spec %}

## Design review
Get team sign-off on the spec.

{% subtask aiki/plan %}
{% subtask aiki/build %}
{% subtask myorg/security-review %}
{% subtask aiki/review %}
```

### 4. Conditional composition

```markdown
# Subtasks

{% subtask aiki/plan %}

{% subtask aiki/build %}

{% if data.needs_security_review %}
{% subtask myorg/security-review %}
{% endif %}

{% subtask aiki/review %}
```

### 5. Loop-based composition

```markdown
# Subtasks

{% for component in data.components %}
{% subtask aiki/build with spec=component.spec %}
{% endfor %}
```

---

## Implementation Plan

### Phase 1: Parser — `{% subtask %}` tag

**Files:** `cli/src/tasks/templates/conditionals.rs`

- Add `SubtaskRef` token: `{% subtask <name> %}` and `{% subtask <name> with k=v, ... %}`
- Add `TemplateNode::SubtaskRef { template_name, overrides }` AST node
- Emit marker during conditional processing: `<!-- AIKI_SUBTASK:template_name:json_overrides -->`

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
| Invalid `with` syntax | `Invalid {% subtask %} syntax at line N. Expected: {% subtask name with key=value %}` |
| Missing required variable | Standard variable-not-found error, but with composition context in the message |

---

## Design Decisions

1. **`{% subtask %}` only works in `# Subtasks` sections** — not for general content inclusion. This keeps the feature focused: it creates nested task trees, not text partials.
2. **No name overrides** — the child template's resolved name (e.g., "Plan: feature.md") is used as-is. No `as "..."` syntax.
3. **`parent.*` points to the composed subtask** — sub-subtasks inside a composed template see their immediate parent (the composed subtask), not the top-level root. This is consistent with how `parent.*` works for static subtasks today.

# Custom Variables for Subtask Templates

**Date**: 2026-02-07
**Status**: Future
**Purpose**: Allow passing custom variables when composing subtask templates

**Related Documents**:
- [Composable Task Templates](../now/composable-task-templates.md) - Base composition feature

---

## Overview

Currently, composed subtask templates inherit all variables from the parent template's context. This document describes adding a `with` clause to override or add variables when including a subtask template.

## Syntax

```markdown
{% subtask <template-name> with key=value, key2=value2 %}
```

Combined with conditional:
```markdown
{% subtask <template-name> with key=value if condition %}
```

## Grammar

```
subtask      := "{% subtask" template_name [with_clause] [if_clause] "%}"
with_clause  := "with" assignment ("," assignment)*
assignment   := identifier "=" value
value        := quoted_string | variable_ref
quoted_string := '"' ... '"'
variable_ref  := identifier ("." identifier)*
```

## Examples

### Override inherited variable

```markdown
# Parent template sets data.spec = "feature.md"

{% subtask plan with spec="different.md" %}

# Plan template sees: data.spec = "different.md" (overridden)
```

### Add new variable

```markdown
{% subtask review with scope="auth module", scope_id=data.task_id %}
```

### In loops with dynamic values

```markdown
{% for module in data.modules %}
{% subtask review with scope=module.name %}
{% endfor %}
```

## Variable Resolution

Values can be:
1. **String literals**: `"auth module"` → `data.key = "auth module"`
2. **Variable references**: `module.name` → resolved from current context, then set as `data.key`

Variables set by `with` are added to the `data.*` namespace of the child template's context.

## Implementation Notes

### Phase 1: Parser
- Parse `with` clause after template name
- Extract key-value assignments
- Store in AST node

### Phase 2: Resolver
- Resolve variable references against current context
- Merge with inherited `data.*` namespace (overrides take precedence)
- Pass merged context to child template

## Use Cases

### 1. Same template, different targets

```markdown
{% for file in data.modified_files %}
{% subtask review with target=file.path %}
{% endfor %}
```

### 2. Override defaults

```markdown
# Default review scope is the whole task
{% subtask review %}

# But for security-critical code, narrow the scope
{% subtask review with scope="auth module" if data.touches_auth %}
```

### 3. Parameterized workflows

```markdown
{% subtask myorg/deploy with environment="staging", version=data.release_version %}
```

## Design Decisions

1. **`with` overrides only affect `data.*` namespace** - doesn't affect `source.*`, `parent.*`, or builtins
2. **Values are resolved at composition time** - not deferred until the child template runs
3. **Order matters**: `with` comes before `if` in the syntax

## Future Enhancements

- Support for nested objects: `with config.timeout=30`
- Support for arrays: `with files=["a.rs", "b.rs"]`
- Support for expressions: `with score=data.bugs * 10`

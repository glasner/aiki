# Template Inheritance

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Allow templates to extend other templates, inheriting their structure and instructions while adding customizations.

## Syntax

File: `.aiki/templates/myorg/custom-security.md`

```markdown
---
extends: aiki/security  # Inherit from built-in security template
---

# Custom Security Review: {data.scope}

${parent.instructions}  # Include base instructions

Additional company-specific checks:
- Check for banned libraries
- Verify compliance requirements

# Subtasks

## Digest code changes

${subtasks.0.instructions}  # Inherit first subtask

## Security analysis

${subtasks.1.instructions}  # Inherit second subtask

## Compliance check

Additional subtask for company-specific compliance.
```

## Usage

```bash
# Use custom template that extends built-in
aiki review --template myorg/custom-security
```

## Behavior

1. Load parent template (`aiki/security`)
2. Merge frontmatter (child overrides parent)
3. Replace `${parent.instructions}` with parent's instruction section
4. Replace `${subtasks.N.instructions}` with parent's Nth subtask
5. Append any additional subtasks from child template

## Implementation Considerations

- **Resolution order**: Built-in templates (`aiki/`) → user templates (`.aiki/templates/`)
- **Circular dependency detection**: Prevent `A extends B, B extends A`
- **Deep nesting**: Should we limit inheritance depth? (e.g., max 3 levels)
- **Variable substitution**: Run after inheritance resolution

## Benefits

- **Reusability**: Build on existing templates without duplication
- **Customization**: Add company-specific requirements to standard templates
- **Maintenance**: Update base template, all children inherit changes

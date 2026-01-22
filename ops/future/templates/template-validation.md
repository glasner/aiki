# Template Validation Command

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Add a command to validate template syntax without creating a task.

## Syntax

```bash
# Validate a template without creating a task
aiki task template validate .aiki/templates/myorg/my-template.md

# Validate all templates in a directory
aiki task template validate .aiki/templates/myorg/

# Validate during development with watch mode
aiki task template validate --watch .aiki/templates/myorg/custom.md
```

## Output

### Valid Template

```
✓ Template is valid: .aiki/templates/myorg/my-template.md
  - Name: Custom Review
  - Assignee: codex
  - Priority: p2
  - Subtasks: 3
  - Variables: {data.scope}, {assignee}, {priority}
```

### Invalid Template

```
✗ Template validation failed: .aiki/templates/myorg/broken.md

Errors:
  Line 5: Invalid YAML frontmatter
    Expected key-value pair, found: "name Custom Review"
    
  Line 23: Unknown variable: {invalid.var}
    Valid variables: {assignee}, {priority}, {id}, {type}, {created}, {source}, {data.*}
    
  Line 45: Subtask missing instructions
    Subtask "## Empty subtask" has no body text

Warnings:
  Line 12: Variable {data.scope} used but not documented in template
    Consider adding a comment explaining required --data flags
```

## Validation Checks

### Syntax Checks

- **YAML frontmatter**: Valid YAML with required fields
- **Markdown structure**: Proper heading hierarchy
- **Variable syntax**: All `{var}` references are valid

### Semantic Checks

- **Required fields**: `name`, `assignee` present
- **Subtask structure**: All subtasks have instructions
- **Variable usage**: No undefined variables
- **Priority values**: Valid priority (p0-p3)

### Best Practice Warnings

- **Undocumented data variables**: `{data.*}` used but not explained
- **Missing examples**: Template has no usage examples
- **Long subtasks**: Subtask body exceeds 500 lines (consider splitting)

## Benefits

- **Early error detection**: Catch issues before using templates
- **Better error messages**: More context than runtime errors
- **CI integration**: Validate templates in CI pipelines
- **Development workflow**: Validate while editing templates

## Note

`aiki task template list` and `aiki task template show` are part of the main implementation (Phase 4), not future enhancements. Only the `validate` subcommand is a future addition.

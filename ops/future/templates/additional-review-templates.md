# Additional Review Templates

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Specialized built-in templates can be added to `.aiki/templates/`:

- **`security`** (`.aiki/templates/security.md`) - Deep security analysis (SQL injection, XSS, auth, crypto)
- **`performance`** (`.aiki/templates/performance.md`) - Performance bottlenecks, algorithm efficiency
- **`style`** (`.aiki/templates/style.md`) - Code style, naming conventions, documentation

## Usage

```bash
# Use built-in security template
aiki review --template security

# Create task with security template
aiki task add --template security
```

## Implementation Notes

- Templates should be bundled with the CLI
- Installed to `.aiki/templates/` during `aiki init`
- Can be customized by users after installation
- Should cover common code review scenarios

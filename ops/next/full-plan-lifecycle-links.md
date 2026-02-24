---
status: draft
---

# Full Plan Lifecycle Links

**Date**: 2026-02-23
**Status**: Future

---

## Overview

Beyond the core `adds-plan` link currently implemented, we may want to track the full lifecycle of plan files through additional link types.

## Proposed Link Types

### edits-plan

Track when a task edits an existing plan file.

**Example:**
```
refine_task ──edits-plan──→ file:ops/now/feature.md
```

**Use cases:**
- Find all tasks that modified a specific plan
- Track plan evolution over time
- Understand who/what changed a plan document

### moves-plan

Track when a task moves or renames a plan file.

**Example:**
```
reorganize_task ──moves-plan──→ file:ops/done/feature.md
                                (from ops/now/feature.md)
```

**Use cases:**
- Track plan file relocations (e.g., now → done, now → next)
- Understand plan organization changes
- Find what happened to a plan that moved

### deletes-plan

Track when a task deletes a plan file.

**Example:**
```
cleanup_task ──deletes-plan──→ file:ops/now/obsolete-feature.md
```

**Use cases:**
- Track removed/cancelled plans
- Audit plan deletions
- Understand why a plan was removed

---

## Implementation Notes

These links would be derived from JJ change events, similar to `adds-plan`:

1. **ChangeCompleted event** → find tasks with matching `working_copy`
2. **JJ diff analysis** → determine operation type (add/edit/move/delete)
3. **Filter to plan files** → only track files matching plan patterns (e.g., `ops/**/*.md`)
4. **Emit LinkAdded events** → create appropriate plan lifecycle link

**Advantage:** Tracks plan file operations without duplicating all JJ file tracking in the task graph.

**Scope limitation:** Only plan files are tracked, avoiding graph bloat from tracking every source file change.

---

## Decision: Future Work

For the initial cleanup-links implementation, we're focusing on:
- `adds-plan` - tracks plan creation (most important for workflow)
- `implements-plan` - links epic to plan (1:1 relationship)
- `decomposes-plan` - links decompose_task to plan it reads

The edit/move/delete lifecycle links can be added later if needed for plan management workflows.

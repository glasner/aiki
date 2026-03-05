---
status: draft
---

# Multiple File Review

**Date**: 2026-02-13
**Status**: Idea
**Priority**: P3
**Depends on**: `ops/now/review-and-fix-files.md`

---

## Problem

`aiki review` currently targets a single file (e.g., `aiki review ops/now/feature.md`). There's no way to review multiple files in one pass, which means reviewing a set of related specs requires separate review tasks for each.

## Desired Behavior

```bash
# Review multiple specific files
aiki review ops/now/feature-a.md ops/now/feature-b.md

# Review files matching a glob pattern
aiki review "ops/now/*.md"
```

## Scope Model Impact

The current `ReviewScope` uses `scope.id` as a single file path. Multi-file review would need either:
- A `scope.file_ids` field (list of paths), or
- Multiple `scope.id` entries with a delimiter

The `scope.task_ids` pattern (used for session reviews) could serve as a model.

## Open Questions

- Should multi-file reviews produce one review task per file, or one review task covering all files?
- How should fix tasks work — one fix per file, or one fix covering all?
- Should glob patterns be expanded at detection time or passed through to the template?

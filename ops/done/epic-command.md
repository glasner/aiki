# Epic Command Restructuring

**Date**: 2026-02-23
**Status**: Draft

---

## Problem

`aiki decompose` is primarily used internally by `aiki build` and doesn't need to be a top-level convenience command. The name "decompose" is also somewhat abstract - it doesn't clearly communicate that it's creating/managing epics.

## Proposal

Restructure the decompose command into `aiki epic add` as a more direct, resource-oriented command.

---

## Current State

```bash
# Current usage
aiki decompose ops/now/feature.md

# What it does:
# 1. Finds or creates epic for plan file
# 2. Creates decompose_task that reads plan
# 3. decompose_task populates epic with subtasks (epic.1, epic.2, etc.)
# 4. Epic is blocked until decompose_task completes
```

**Internal usage:**
```bash
# aiki build calls decompose internally
aiki build ops/now/feature.md
  → find_or_create_epic(plan_file)
  → aiki decompose (if epic doesn't exist)
```

---

## Proposed State

```bash
# New command structure
aiki epic add ops/now/feature.md

# What it does (same as decompose):
# 1. Finds or creates epic for plan file
# 2. Creates decompose_task that reads plan
# 3. decompose_task populates epic with subtasks
# 4. Epic is blocked until decompose_task completes
```

**Internal usage:**
```bash
# aiki build calls epic add internally
aiki build ops/now/feature.md
  → find_or_create_epic(plan_file)
  → aiki epic add (if epic doesn't exist)
```

---

## Command Structure

```
aiki epic
├── add <plan-file>          # Create epic for plan (replaces decompose)
├── show <plan-file|epic-id> # Show epic and subtasks
└── list                     # List all epics
```

**Future possibilities:**
```
aiki epic
├── remove <epic-id>         # Delete epic
├── sync <plan-file>         # Re-sync epic with plan changes
└── status <plan-file>       # Show epic status (draft/planned/implementing/done)
```

---

## Migration

### Code Changes

1. **Delete decompose command:**
   - Delete `cli/src/commands/decompose.rs`
   - Remove from command registration in `cli/src/commands/mod.rs`

2. **Create epic command:**
   - Create `cli/src/commands/epic.rs` with subcommands: `add`, `show`, `list`
   - Register in `cli/src/commands/mod.rs`

3. **Update internal calls:**
   - `aiki build` → call `epic add` instead of `decompose`
   - Any other internal decompose calls

### Documentation Changes

- Update CLAUDE.md references
- Update cleanup-links.md (this doc)
- Update any example workflows

---

## Benefits

1. **Clearer intent** - "epic add" clearly says "create an epic"
2. **Resource-oriented** - Follows REST-like patterns (noun + verb)
3. **Extensible** - Natural place for other epic operations
4. **Less abstract** - "decompose" is conceptual, "epic" is concrete
5. **Better UX** - User thinks "I want an epic" not "I want to decompose"

---

## Open Questions

1. Should `aiki epic add` be idempotent (find existing) or error if epic exists?
   - **Proposal**: Idempotent (same as current decompose behavior)

2. Should we expose `aiki epic show` for debugging or keep it internal?
   - **Proposal**: Expose it - useful for checking epic status

3. Keep `aiki decompose show` or move to `aiki epic show`?
   - **Proposal**: Move to `aiki epic show`

---

## Timeline

1. Delete `cli/src/commands/decompose.rs`
2. Create `cli/src/commands/epic.rs` with `add`, `show`, `list` subcommands
3. Update `aiki build` and other internal calls to use `epic add`
4. Update documentation (CLAUDE.md, etc.)

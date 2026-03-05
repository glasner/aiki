# Simplify Template Directory Names

**Date**: 2026-03-04
**Status**: Draft
**Purpose**: Rename `plan/epic` → `plan` and `plan/fix` → `fix` so template names match CLI commands

---

## Summary

The `.aiki/templates/aiki/plan/` subdirectory currently holds two templates (`epic.md` and `fix.md`) that were created during the loop refactor. Now that these templates are stable, the `plan/` nesting adds confusion without value — `aiki/plan/epic` doesn't match any command, but `aiki/plan` maps directly to `aiki plan`, and `aiki/fix` maps directly to `aiki fix`.

---

## Changes

### 1. Move template files

```
.aiki/templates/aiki/plan/epic.md  →  .aiki/templates/aiki/plan.md
.aiki/templates/aiki/plan/fix.md   →  .aiki/templates/aiki/fix.md
```

Delete the now-empty `.aiki/templates/aiki/plan/` directory.

### 2. Update Rust source references

Every string literal `"aiki/plan/epic"` becomes `"aiki/plan"`, and `"aiki/plan/fix"` becomes `"aiki/fix"`.

| File | What changes |
|------|-------------|
| `cli/src/main.rs:122` | Help text default: `aiki/plan/fix` → `aiki/fix` |
| `cli/src/main.rs:170` | Help text default: `aiki/plan/epic` → `aiki/plan` |
| `cli/src/commands/plan.rs:450` | `unwrap_or("aiki/plan/epic")` → `unwrap_or("aiki/plan")` |
| `cli/src/commands/plan.rs:695` | `unwrap_or("aiki/plan/fix")` → `unwrap_or("aiki/fix")` |
| `cli/src/commands/fix.rs` (6 sites) | All `"aiki/plan/fix"` literals → `"aiki/fix"` |
| `cli/src/commands/build.rs:79` | `default_missing_value = "aiki/plan/fix"` → `"aiki/fix"` |
| `cli/src/commands/build.rs` (2 test sites) | `"aiki/plan/fix"` → `"aiki/fix"` |
| `cli/src/commands/review.rs:444` | Comment: `"aiki/plan/fix"` → `"aiki/fix"` |

### 3. Update documentation

| File | What changes |
|------|-------------|
| `cli/docs/sdlc.md` | All `plan/epic` and `plan/fix` references |
| `cli/docs/sdlc/fix.md` | `aiki/plan/fix` reference |

### 4. Update ops docs (non-blocking, nice-to-have)

References in `ops/done/` are historical context. Update if convenient, skip if not — they describe past decisions and won't confuse anyone.

---

## Non-goals

- No behavioral changes — only string renames and file moves
- No changes to template content (the `.md` files themselves stay identical)
- No changes to template resolution logic in `resolver.rs` (it already handles any depth of `/`-separated names)

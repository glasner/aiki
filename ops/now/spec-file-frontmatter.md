# Spec File Frontmatter

**Date**: 2026-02-14
**Status**: Draft
**Purpose**: Add YAML frontmatter to spec/plan files to track lifecycle state and link to the implementation task.

---

## Executive Summary

Spec files (the markdown documents authored via `aiki spec` / `aiki plan`) currently have no metadata. This makes it impossible to know a file's lifecycle state (is it a draft? being built? done?) or which task implements it without running `aiki build show`.

This spec adds YAML frontmatter to spec files with two fields: `state` (lifecycle) and `plan_task` (implementation task ID). Commands that create or process spec files update the frontmatter at each lifecycle transition.

---

## Frontmatter Format

```yaml
---
state: draft
plan_task: null
---
```

| Field | Type | Description |
|-------|------|-------------|
| `state` | string | Lifecycle state of the spec file |
| `plan_task` | string \| null | Full 32-char task ID of the implementation epic, set by `aiki build` |

### States

```
draft → ready → building → reviewing → done
                    ↓          ↓
                 failed      fixing
                               ↓
                           reviewing
```

| State | Set By | Meaning |
|-------|--------|---------|
| `draft` | `aiki spec` (creation) | Spec is being authored |
| `ready` | `aiki spec` (session completes) | Spec is complete, ready to build |
| `building` | `aiki build` (start) | Implementation in progress |
| `reviewing` | `aiki review` (start) | Code review in progress |
| `fixing` | `aiki fix` (start) | Applying review fixes |
| `done` | `aiki review` (no issues / approved) | Lifecycle complete |
| `failed` | `aiki build` (error) | Build failed |

Notes:
- `done` → `building` is valid (rebuild). The `plan_task` is overwritten.
- `failed` → `building` is valid (retry after failure).
- `reviewing` → `fixing` → `reviewing` can cycle until all issues are resolved.
- `reviewing` → `done` when the review finds no issues (approved).

---

## State Transitions by Command

### `aiki spec <path>` — File Creation

When `aiki spec` creates a new spec file, write frontmatter instead of an empty file.

**Current behavior** (`spec.rs:454`):
```rust
fs::write(&spec_path, "").map_err(...)?;
```

**New behavior**:
```rust
let frontmatter = "---\nstate: draft\n---\n\n";
fs::write(&spec_path, frontmatter).map_err(...)?;
```

Only set `state`. `plan_task` is omitted (not `null`) — it appears only after `aiki build` runs.

### `aiki spec` — Session Completion

When the spec session finishes successfully, update `state: draft` → `state: ready`.

**Where**: After the Claude session exits with success in `run_spec()` (around spec.rs:500-520).

**How**: Read file, update frontmatter state, write file.

### `aiki build <spec.md>` — Build Start

When `aiki build` starts processing a spec file:

1. **Read frontmatter** from the spec file
2. **Create frontmatter** if missing (for files created without `aiki spec`)
3. **Set `state: building`**
4. **Write updated frontmatter** back to the file

**Where**: In `run_build_spec()` (build.rs), after validating the spec path and before creating the build task (~line 142).

**Handling files without frontmatter**: If the spec file has no `---` frontmatter block, prepend one:
```yaml
---
state: building
---

(existing file content unchanged)
```

### `aiki build` — Plan Task Created

When the build's planning subtask creates the implementation (plan/epic) task, write its ID back to the spec file frontmatter.

**How**: `build.rs` already has `find_plan_for_spec()` (line ~413) which finds a plan task by matching `data.spec`. After the planning subtask completes, `build.rs` calls this function to get the plan task ID, then writes it to frontmatter via the Rust utility.

**Where**: In `run_build_spec()`, after the planning subtask completes but before executing implementation subtasks. The build orchestrator can detect plan creation by checking whether `find_plan_for_spec()` returns a result after the planning phase.

```rust
// After planning subtask completes:
if let Some(plan_task_id) = find_plan_for_spec(cwd, &spec_path)? {
    set_frontmatter_field(&spec_path, "plan_task", Value::String(plan_task_id))?;
}
```

No template changes or new CLI commands needed — `build.rs` handles this internally.

### `aiki build` — Build Completion

When build finishes successfully, set `state: reviewing` (next step is review, not done).

**Where**: In `run_build_spec()` (build.rs), after `task_run()` returns success (~line 215).

**On failure**: Set `state: failed`.

### Resolving the Spec File from Review/Fix

Review and fix operate on tasks or files, not spec paths directly. To update frontmatter, they need to resolve the spec file path from their scope:

| `ReviewScopeKind` | How to find spec file |
|--------------------|-----------------------|
| `Spec` | `scope.id` IS the spec file path |
| `Implementation` | `scope.id` IS the spec file path |
| `Task` | Look up `data.spec` on the target task (`scope.id`) |
| `Session` | Multiple tasks — skip frontmatter update |

This resolution logic should be a shared helper:

```rust
/// Resolve the spec file path from a review scope, if one exists.
fn resolve_spec_path(scope: &ReviewScope, tasks: &TaskMap) -> Option<PathBuf> {
    match scope.kind {
        ReviewScopeKind::Spec | ReviewScopeKind::Implementation => {
            Some(PathBuf::from(&scope.id))
        }
        ReviewScopeKind::Task => {
            let task = tasks.get(&scope.id)?;
            task.data.get("spec").map(|s| PathBuf::from(s))
        }
        ReviewScopeKind::Session => None, // No single spec file
    }
}
```

### `aiki review` — Review Start

When `aiki review` starts, resolve the spec file and set `state: reviewing`.

**Where**: In `create_review()` (review.rs:380), after creating the review task.

```rust
if let Some(spec_path) = resolve_spec_path(&scope, &tasks) {
    set_frontmatter_field(&spec_path, "state", Value::String("reviewing".into()))?;
}
```

### `aiki review` — Review Completion (No Issues)

When a review completes with no issues (approved), set `state: done`.

**Where**: In `run_review()` (review.rs), after the review task completes and the result indicates no issues.

### `aiki fix` — Fix Start

When `aiki fix` starts, resolve the spec file from the review's scope and set `state: fixing`.

**Where**: In `run_fix()` (fix.rs:90), after creating the fix task.

```rust
let scope = ReviewScope::from_data(&review_task.data)?;
if let Some(spec_path) = resolve_spec_path(&scope, &tasks) {
    set_frontmatter_field(&spec_path, "state", Value::String("fixing".into()))?;
}
```

### `aiki fix` — Fix Completion

When fix completes, set `state: reviewing` (next step is re-review).

**Where**: In `run_fix()` (fix.rs), after the fix task completes successfully.

---

## Frontmatter Read/Write Utility

**New file**: `cli/src/frontmatter.rs`

All frontmatter updates are done internally by `spec.rs` and `build.rs` — no new CLI commands. This module provides the shared read/write functions.

```rust
/// Parsed frontmatter as key-value pairs
pub type Frontmatter = BTreeMap<String, serde_yaml::Value>;

/// Read frontmatter from a markdown file. Returns (frontmatter, body).
/// Returns empty map if no frontmatter block exists.
pub fn read_frontmatter(path: &Path) -> Result<(Frontmatter, String)>;

/// Write frontmatter + body to a markdown file.
/// If frontmatter is empty, writes body only (no `---` block).
pub fn write_frontmatter(path: &Path, fm: &Frontmatter, body: &str) -> Result<()>;

/// Update a single field in a file's frontmatter, preserving body.
/// Creates frontmatter block if none exists.
pub fn set_frontmatter_field(path: &Path, key: &str, value: serde_yaml::Value) -> Result<()>;
```

**Key behavior for `write_frontmatter`**:
- If file has existing `---` block: replace it, keep body
- If file has no `---` block: prepend frontmatter + blank line before body
- Preserve all non-frontmatter content exactly as-is

### No template changes needed

- `aiki/spec` template: frontmatter is written by `spec.rs` before the session starts
- `aiki/plan` template: no changes — `build.rs` writes `plan_task` after planning completes
- `aiki/build` template: no changes — `build.rs` handles all state transitions directly

---

## Existing Infrastructure

The codebase already has what we need:

| Component | Location | Reusable? |
|-----------|----------|-----------|
| YAML frontmatter parser | `cli/src/tasks/templates/parser.rs:111-146` | Pattern yes, code partially (typed to `TemplateFrontmatter`) |
| `serde_yaml` dependency | `cli/Cargo.toml:26` | Yes, no new deps |
| `serde` with derive | `cli/Cargo.toml:24` | Yes |

The existing `extract_yaml_frontmatter<T>()` in `parser.rs` parses frontmatter into a typed struct. The new utility should use `BTreeMap<String, serde_yaml::Value>` for flexibility — spec file frontmatter has a different schema than template frontmatter and we don't want tight coupling.

---

## Implementation Plan

### Phase 1: Frontmatter Utility

1. Add `cli/src/frontmatter.rs` with `read_frontmatter()`, `write_frontmatter()`, `set_frontmatter_field()`
2. Unit tests for: no frontmatter, existing frontmatter, empty file, frontmatter-only file

### Phase 2: Spec Integration

3. Update `spec.rs` file creation to write `state: draft` frontmatter
4. Update `spec.rs` session completion to set `state: ready`

### Phase 3: Build Integration

5. Update `build.rs` start to read/create frontmatter and set `state: building`
6. After planning subtask completes, call `find_plan_for_spec()` and write `plan_task` to frontmatter
7. Update `build.rs` completion to set `state: reviewing` (or `state: failed` on error)

### Phase 4: Review/Fix Integration

8. Add `resolve_spec_path()` helper (shared between review.rs and fix.rs)
9. Update `review.rs` to set `state: reviewing` on start, `state: done` on approved (no issues)
10. Update `fix.rs` to set `state: fixing` on start, `state: reviewing` on completion

### Phase 5: Validation

11. `cargo test` — all unit tests pass
12. Manual smoke test: full lifecycle `aiki spec` → `aiki build` → `aiki review` → `aiki fix` — verify frontmatter state at each step

---

## Example Lifecycle

```bash
$ aiki spec dark-mode.md "Add dark mode toggle"
# File created: ops/now/dark-mode.md
```

```yaml
---
state: draft
---

```

```bash
# Claude session runs, fills in spec content...
# Session completes successfully
```

```yaml
---
state: ready
---

# Dark Mode Toggle
...spec content...
```

```bash
$ aiki build ops/now/dark-mode.md
# Build starts, frontmatter updated
```

```yaml
---
state: building
---

# Dark Mode Toggle
...spec content...
```

```bash
# Planning subtask creates implementation task...
# build.rs calls find_plan_for_spec() and writes plan_task to frontmatter
```

```yaml
---
state: building
plan_task: xtuttnyvykpulsxzqnznsxylrzkkqssy
---

# Dark Mode Toggle
...spec content...
```

```bash
# Build completes successfully
```

```yaml
---
state: reviewing
plan_task: xtuttnyvykpulsxzqnznsxylrzkkqssy
---

# Dark Mode Toggle
...spec content...
```

```bash
$ aiki review xtuttnyvykpulsxzqnznsxylrzkkqssy
# Review starts — state already "reviewing" (set by build completion)
# Review finds 2 issues
```

```bash
$ aiki fix <review-task-id>
# Fix starts
```

```yaml
---
state: fixing
plan_task: xtuttnyvykpulsxzqnznsxylrzkkqssy
---

# Dark Mode Toggle
...spec content...
```

```bash
# Fix completes — state goes back to "reviewing" for re-review
```

```yaml
---
state: reviewing
plan_task: xtuttnyvykpulsxzqnznsxylrzkkqssy
---

# Dark Mode Toggle
...spec content...
```

```bash
$ aiki review xtuttnyvykpulsxzqnznsxylrzkkqssy
# Review finds no issues — approved
```

```yaml
---
state: done
plan_task: xtuttnyvykpulsxzqnznsxylrzkkqssy
---

# Dark Mode Toggle
...spec content...
```

---

## Open Questions

1. **Should `aiki build show` read from frontmatter?** Currently it finds the plan by scanning task events for `data.spec` matches. It could also/instead read `plan_task` from frontmatter for a faster lookup.

2. **Rebuild behavior**: When running `aiki build` on a file that's already `done` with a `plan_task`, should it warn? Require `--force`? Or just overwrite silently?

3. **Manual files**: Users might create spec files by hand (no `aiki spec`). `aiki build` handles this by creating frontmatter if missing. Is that sufficient, or should there be a way to manually set state?

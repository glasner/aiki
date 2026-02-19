# Subtask Slugs: Stable References for Subtasks

**Date**: 2026-02-16
**Status**: Draft
**Priority**: P2

---

## Problem

Subtasks are currently referenced by **position** (creation order) or by their full 32-character task ID. Neither is suitable for automation:

1. **Positional references are fragile** — If a template adds, removes, or reorders subtasks, all positional references break. A workflow that says "start subtask 3" silently does the wrong thing when the template changes.

2. **Task IDs are opaque** — A 32-char random ID like `mvslrspmoynoxyyywqyutmovxpvztkls` carries no meaning. Automation that needs to reference "the review step" or "the deploy step" must discover the ID at runtime by querying and filtering.

3. **Templates can't express inter-subtask relationships** — A template that defines `build`, `test`, `deploy` subtasks has no way to say "deploy depends-on test" because subtask IDs don't exist until creation time. Position-based references (`subtask.2 depends-on subtask.1`) are fragile and unreadable.

4. **No stable handle for hooks/conditions** — Autorun rules, spawn conditions, and hooks all need a stable way to say "when the review subtask closes, start the fix subtask." Today this requires knowing the task ID ahead of time, which is impossible for template-created tasks.

**Example of the problem:**

```yaml
# Template wants to express: deploy depends-on test
# But there's no way to reference "test" from "deploy"
subtasks:
  - name: Build the feature
  - name: Test the feature
  - name: Deploy the feature  # How does this reference "Test"?
```

---

## Summary

Add an optional **`slug`** field to subtasks — a user-defined slug (`[a-z0-9]([a-z0-9-]*[a-z0-9])?`, max 48 chars) that provides a stable, human-readable handle for referencing subtasks within their parent scope.

**Key properties:**
- **Optional** — Not required. Most ad-hoc subtasks won't have one.
- **Scoped to parent** — Slugs are unique within a parent task, not globally. Two different parent tasks can each have a subtask with slug `review`.
- **Immutable** — Once set, a subtask's slug cannot be changed (prevents broken references).
- **Slug format** — Lowercase alphanumeric with hyphens, like `build`, `run-tests`, `deploy-staging`. No dots (reserved for ID notation), no underscores (pick one convention).

**Benefits:**
- Templates can express inter-subtask dependencies: `depends-on: siblings.test`
- Hooks and automation get stable references: `on: subtasks.review.closed`
- Humans can reason about subtask references: `parent-id:review` vs `xtuttnyvykpulsxz...`
- CLI becomes more ergonomic: `aiki task start <parent>:deploy`

---

## Design

### The `slug` Field

Add `slug: Option<String>` to the `Task` struct, alongside the existing `id` and `name` fields:

```rust
pub struct Task {
    pub id: String,              // Full 32-char unique ID (existing)
    pub name: String,            // Human-readable title like "Fix the auth bug" (existing)
    pub slug: Option<String>,    // User-defined slug for automation (new, optional)
    // ... rest unchanged
}
```

The slug is set at creation time and never changes. It is stored via a new field on `TaskEvent::Created`:

```rust
TaskEvent::Created {
    task_id: String,
    name: String,           // Human-readable display name (existing — this is the task title)
    slug: Option<String>,   // Stable slug for automation references (NEW)
    // ... rest unchanged
}
```

**Field naming note:** The existing `name` field on `Task` and `TaskEvent::Created` is actually the task's **title** (free-form text like "Fix the auth bug"). The new slug field should be called `slug` in the event schema to avoid confusion. On the materialized `Task` struct, it maps to `slug: Option<String>`.

### Slug Format

```
slug = [a-z0-9] ( [a-z0-9-]* [a-z0-9] )?
```

Rules:
- Lowercase letters, digits, and hyphens only
- Must start and end with alphanumeric character
- 1–48 characters
- No dots (reserved for subtask ID notation `parent.N`)
- No underscores (single convention: hyphens)

Valid: `build`, `run-tests`, `deploy-staging`, `phase-2`, `a`
Invalid: `-build`, `build-`, `Build`, `run_tests`, `deploy.staging`, `my slug`

### Scoped Uniqueness

Slugs are unique **within their parent task**. Enforced at creation time:

```rust
fn validate_slug(graph: &TaskGraph, parent_id: &str, slug: &str) -> Result<()> {
    // Check slug format
    if !is_valid_slug(slug) {
        return Err(AikiError::InvalidSlug(slug.to_string()));
    }
    // Check uniqueness within parent
    let siblings = graph.children_of(parent_id);
    if siblings.iter().any(|t| t.slug.as_deref() == Some(slug)) {
        return Err(AikiError::DuplicateSlug {
            slug: slug.to_string(),
            parent_id: parent_id.to_string(),
        });
    }
    Ok(())
}
```

### Addressing Syntax: `<parent>:<slug>`

Subtasks with slugs can be referenced using `<parent-id-or-prefix>:<slug>`:

```bash
# Reference by slug
aiki task show mvslrsp:review
aiki task start mvslrsp:deploy
aiki task close mvslrsp:build --summary "Built successfully"

# Slug resolution: find child of parent where slug == "review"
```

The `:` separator is chosen because:
- `.` is already taken by positional notation (`parent.1`)
- `/` implies hierarchy/paths
- `#` has shell escaping issues
- `:` is clean, unambiguous, and familiar (Docker `image:tag`, Git `remote:branch`)

Both forms coexist:
| Form | Meaning | When to use |
|------|---------|-------------|
| `mvslrsp.1` | Positional (legacy/backward compat) | Ad-hoc subtasks without slugs |
| `mvslrsp:review` | Slug-based reference | Template-created or explicitly slugged subtasks |
| `xtuttny...` | Full task ID | Direct reference (always works) |

### Resolution Order

When resolving a task reference, try in order:

1. **Full 32-char task ID** — exact match
2. **ID prefix** — existing prefix resolution (3+ chars of k-z)
3. **Parent:slug** — if input contains `:`, split and resolve
4. **Positional** — if input contains `.`, existing dot-notation

```rust
fn resolve_task_ref(graph: &TaskGraph, input: &str) -> Result<&Task> {
    // 1. Full ID
    if is_task_id(input) {
        return graph.tasks.get(input).ok_or(/* not found */);
    }
    // 2. Slug reference (contains ':')
    if let Some((parent_ref, slug)) = input.split_once(':') {
        let parent = resolve_task_ref(graph, parent_ref)?;  // Recurse for parent
        return find_child_by_slug(graph, &parent.id, slug);
    }
    // 3. ID prefix (existing logic)
    if is_task_id_prefix(input) {
        return find_by_prefix(graph, input);
    }
    Err(/* not found */)
}
```

### CLI Surface

#### Setting a slug on `task add`

```bash
# New --slug flag
aiki task add "Run the test suite" --parent <parent-id> --slug run-tests

# Quick-start with slug
aiki task start "Deploy to staging" --parent <parent-id> --slug deploy
```

#### Referencing by slug

```bash
# All task commands accept slug references
aiki task show <parent>:review
aiki task start <parent>:deploy
aiki task close <parent>:build --summary "Done"
aiki task comment <parent>:test "All 42 tests pass"
```

### Templates: Named Subtasks and Reference Model

Templates can assign slugs to subtasks via frontmatter on each subtask section:

```markdown
---
version: 1.0.0
---

# Review: {source.name}

Review changes for quality and correctness.

# Subtasks

## Build the feature
---
slug: build
---
Compile and verify the build succeeds.

## Run tests
---
slug: test
depends-on: siblings.build
---
Execute the full test suite.

## Deploy to staging
---
slug: deploy
depends-on: siblings.test
---
Push to staging environment.
```

#### Reference Model

The reference syntax depends on **context** (where the reference appears):

| Context | Syntax | Example | Description |
|---------|--------|---------|-------------|
| **Parent template frontmatter** | `subtasks.{slug}` | `subtasks.review.approved` | Direct reference from parent to its subtasks |
| **Subtask frontmatter** | `siblings.{slug}` | `siblings.build` | Shorthand for sibling references |
| **Subtask frontmatter** | `parent.subtasks.{slug}` | `parent.subtasks.build` | Explicit parent reference (equivalent to `siblings.{slug}`) |

**Key insight:** `siblings.{slug}` is **syntactic sugar** available only in subtask frontmatter. It resolves to `parent.subtasks.{slug}`. Everywhere else, use the full path.

**Examples:**

```markdown
# Parent template frontmatter
---
spawns:
  - template: aiki/fix
    condition: "!subtasks.review.approved"   # Direct: subtasks.{slug}
---

# Subtask frontmatter
---
slug: deploy
depends-on: siblings.build                   # Sugar: siblings.{slug}
blocks: parent.subtasks.test                 # Explicit: parent.subtasks.{slug}
---
```

At template instantiation time, the system:

1. Creates all subtasks (each with full 32-char IDs)
2. Sets slugs from frontmatter
3. Resolves `siblings.{slug}` references to `parent.subtasks.{slug}` in subtask frontmatter
4. Resolves slug references to actual task IDs
5. Creates the appropriate link events

This is the primary motivating use case: templates can now express the full dependency graph between their subtasks without relying on position or runtime IDs.

### Display

When displaying subtasks, show the slug alongside the title if present:

```
In Progress:
- mvslrspmoynoxyyywqyutmovxpvztkls — Deploy pipeline
  ├─ ◆ build — Build the feature (in_progress)
  ├─ ○ test — Run tests (blocked by: build)
  └─ ○ deploy — Deploy to staging (blocked by: test)
```

The slug appears as a terse label before the dash, giving the subtask a recognizable handle at a glance. When no slug is set, fall back to the current behavior (just the title).

### Storage

Slugs are stored as part of the `TaskEvent::Created` event and materialized onto the `Task` struct during graph construction. No new event types needed — just a new optional field on the existing `Created` variant.

On the graph side, add a reverse index for efficient slug lookup:

```rust
pub struct TaskGraph {
    pub tasks: FastHashMap<String, Task>,
    pub edges: EdgeStore,
    /// Reverse index: (parent_id, slug) → task_id for O(1) slug resolution
    slug_index: FastHashMap<(String, String), String>,
}
```

Populated during `process_event` when a `Created` event has a slug and the task has a `subtask-of` link.

---

## Implementation Plan

### Phase 1: Core slug field

1. Add `slug: Option<String>` to `TaskEvent::Created` and `Task`
2. Add slug validation function (`is_valid_slug`)
3. Materialize slug in `process_event` during graph construction
4. Add slug uniqueness check (scoped to parent)
5. Add `slug_index` to `TaskGraph` for O(1) lookup
6. Tests: slug validation, uniqueness enforcement, materialization

**Files:**
- `cli/src/tasks/types.rs` — Add field to `Task` and `TaskEvent::Created`
- `cli/src/tasks/graph.rs` — Add `slug_index`, populate during `process_event`
- `cli/src/tasks/id.rs` — Add `is_valid_slug()` validator

### Phase 2: Resolution and CLI

1. Add `parent:slug` resolution to `find_task()` / task resolution
2. Add `--slug` flag to `task add`
3. Accept `parent:slug` syntax in all task commands (show, start, close, comment, etc.)
4. Tests: resolution with prefix, full ID, and slug forms

**Files:**
- `cli/src/tasks/manager.rs` — Update `find_task()` with slug resolution
- `cli/src/commands/task.rs` — Add `--slug` flag, wire through to `Created` event

### Phase 3: Template support

1. Add `slug` field to `SubtaskFrontmatter`
2. Add `depends-on` field to `SubtaskFrontmatter` (resolves sibling slugs)
3. Two-pass template instantiation: create subtasks first, then resolve slug references for links
4. Tests: template with slugs, inter-subtask dependencies via slug

**Files:**
- `cli/src/tasks/templates/types.rs` — Add `slug` to `SubtaskFrontmatter` and `TaskDefinition`
- `cli/src/tasks/templates/parser.rs` — Parse slug from subtask frontmatter
- `cli/src/commands/task.rs` — Two-pass subtask creation in `create_static_subtasks` and `create_dynamic_subtasks`

### Phase 4: Display

1. Show slug in `task show` output
2. Show slug in status monitor tree view
3. Show slug in `task list` output (when in subtask scope)

**Files:**
- `cli/src/tasks/status_monitor.rs` — Include slug in tree display
- `cli/src/commands/task.rs` — Include slug in show/list formatters

---

## Event Schema Change

The `Created` event gains one new optional field:

```
Before:
Created { task_id, name, task_type, priority, assignee, sources, template, working_copy, instructions, data, timestamp }

After:
Created { task_id, name, slug, task_type, priority, assignee, sources, template, working_copy, instructions, data, timestamp }
```

**Backward compatibility:** `slug` is `Option<String>` and defaults to `None`. Existing events without the field deserialize correctly. No migration needed.

---

## Examples

### Ad-hoc subtasks with slugs

```bash
# Create parent
aiki task start "Release v2.0"

# Add named subtasks
aiki task add "Write migration guide" --parent <id> --slug docs
aiki task add "Run regression suite" --parent <id> --slug regression
aiki task add "Tag and publish" --parent <id> --slug release

# Reference by slug
aiki task start <id>:regression
aiki task close <id>:regression --summary "All green"
aiki task start <id>:release
```

### Template with inter-subtask dependencies

```markdown
---
version: 1.0.0
type: review
---

# Review: {source.name}

## Code review
---
slug: review
---
Review the code changes for quality.

## Fix issues
---
slug: fix
depends-on: siblings.review
---
Address any issues found during review.

## Final check
---
slug: final
depends-on: siblings.fix
---
Verify all fixes are correct.
```

Instantiation creates:
```
Review: auth-module
├─ review — Code review (ready)
├─ fix — Fix issues (blocked by: review)
└─ final — Final check (blocked by: fix)
```

### Hooks referencing slugs

```yaml
# .aiki/hooks.yaml (future)
on:
  task.closed:
    match:
      slug: review
    run: aiki task start {parent}:fix
```

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Slug collision within parent | Error at creation: "Slug 'build' already exists under parent X" |
| Slug on root task (no parent) | Allowed but not useful for `:` resolution (no parent to scope to). Stored for metadata. |
| Slug on task created without `--parent` | Stored but `:` resolution won't find it (no parent scope) |
| Same slug under different parents | Allowed — slugs are scoped to parent |
| Rename a slug | Not allowed — slugs are immutable once set |
| Delete subtask with slug | Slug is freed — a new subtask can reuse it |
| Template subtask without slug | No slug set, referenced by position or full ID (existing behavior) |
| `:` in task title (not slug) | No conflict — `:` resolution only applies to the ID resolution path, not to the title field |
| Slug that looks like a number (`3`) | Allowed by format rules. `parent:3` resolves as slug, `parent.3` resolves as position |

---

## Open Questions

1. **Should `slug` be settable after creation via `task set`?** — Current design says immutable. But users might want to add a slug to an existing subtask. Risk: references to the old (absent) slug silently fail. Mitigation: allow `set` only if no slug was previously set (add-once semantics).

2. **Should slugs be allowed on root tasks?** — The primary use case is subtask references, but root-level slugs could be useful for bookmarks or well-known tasks. No harm in allowing it.

3. **Should the colon syntax work with positional parent references?** — E.g., `mvslrsp.1:deploy` meaning "child of subtask 1 of mvslrsp with slug deploy". Probably yes for consistency, but adds parsing complexity.

4. **Interaction with declarative subtask iteration** — When `subtasks: source.comments` generates subtasks dynamically, can the template assign slugs? The slug would need to be derived from item data (e.g., `slug: {item.file}`). Worth supporting but adds template variable resolution complexity.

---

## What This Does NOT Change

- **Task IDs** — Full 32-char IDs remain the primary key. Slugs are an alias, not a replacement.
- **Positional notation** — `parent.1`, `parent.2` continues to work for backward compatibility.
- **Link system** — Links still use full task IDs internally. Slugs are resolved to IDs at the CLI/template layer.
- **Task lifecycle** — No changes to start/stop/close behavior.
- **Event storage** — Same file-per-event model. Just one new optional field on `Created`.

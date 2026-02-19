# Fix: `parent.subtasks.*` Variable Resolution

**Date**: 2026-02-19
**Status**: Draft
**Priority**: P1
**Source**: subtask-slugs spec (Phase 3, partial)

---

## Problem

Templates that reference sibling subtasks by slug via `{{parent.subtasks.{slug}}}` fail with:

```
Error: Variable '{parent.subtasks.criteria}' referenced but not provided
  In template: aiki/review@2.0.0 (line 4)
  Variable 'parent.subtasks.criteria' is only available in subtask templates
```

The subtask-slugs spec defined this variable namespace but it was never implemented. Three things are missing:

1. **No `parent.subtasks.*` variable population** — The variable resolver doesn't build a slug→taskID map for sibling subtasks
2. **Composed templates can't declare slugs** — `TemplateFrontmatter` has no `slug` field, so `{% subtask %}` references can't contribute slugs
3. **`parse_expanded_subtasks` doesn't parse inline subtask frontmatter** — In the entry-based path (templates with `{% subtask %}` refs), inline `## heading` subtasks' frontmatter (slug, priority, etc.) is silently ignored

---

## Root Cause

Variable substitution happens eagerly in `parse_expanded_subtasks` (resolver.rs:1076-1077) before task IDs are generated. Since `parent.subtasks.*` resolves to sibling task IDs, it can only be populated AFTER all sibling subtask IDs exist — but substitution happens before the IDs are generated in `create_subtasks_from_entries` (task.rs).

The fix requires a **two-phase approach**: collect slugs and generate IDs first, then substitute variables.

---

## Design

### Placeholder-based deferred resolution

Follow the existing `PARENT_ID_PLACEHOLDER` pattern (resolver.rs:1142-1165):

1. **Pre-scan** the template for `{{parent.subtasks.*}}` references
2. Populate those variables with **placeholder values** (`__AIKI_SUBTASK_SLUG_{slug}__`)
3. Run normal template resolution (placeholders pass through substitution)
4. **Plan phase**: generate all sibling task IDs, build slug→taskID map
5. **Execute phase**: replace placeholders with actual task IDs, then write events

This avoids deep changes to the variable resolution system — placeholders are just strings.

### Template-level slug for composed subtasks

Add `slug: Option<String>` to `TemplateFrontmatter` so composed templates (loaded via `{% subtask %}`) can declare what slug they should receive when composed into a parent:

```markdown
---
slug: criteria
---

# Understand Criteria: Code

Evaluate the implementation...
```

When the composition flow in `create_subtasks_from_entries` loads this template, it reads `slug` from the frontmatter and assigns it to the composed subtask's Created event.

### Inline subtask frontmatter in entry-based path

`parse_expanded_subtasks` currently ignores `---...---` frontmatter blocks after `## heading` in the entry-based path. Fix this to parse frontmatter and extract slug, priority, assignee, sources, and data — matching what `parse_single_subtask` does in the static path.

---

## Implementation

### Phase 1: Template-level slug

**`cli/src/tasks/templates/types.rs`**

Add `slug` to `TemplateFrontmatter`:

```rust
pub struct TemplateFrontmatter {
    pub slug: Option<String>,    // NEW — slug for this template when composed
    pub version: Option<String>,
    // ... rest unchanged
}
```

**`cli/src/tasks/templates/parser.rs`**

In `parse_template`, propagate slug from frontmatter to the parent TaskDefinition:

```rust
template.parent.slug = frontmatter.slug;
```

**`.aiki/templates/aiki/review/criteria/code.md`** and **`spec.md`**

Move slug to template-level frontmatter (top of file, before `# heading`):

```markdown
---
slug: criteria
---

# Understand Criteria: Code

Evaluate the implementation...
```

### Phase 2: Parse inline subtask frontmatter in entry path

**`cli/src/tasks/templates/resolver.rs`** — `parse_expanded_subtasks`

After finding a `## heading` and collecting body lines, parse frontmatter:

```rust
// Current (broken):
let instructions = body_lines.join("\n").trim().to_string();
entries.push(SubtaskEntry::Static(TaskDefinition {
    name, slug: None, instructions, ..Default::default()
}));

// Fixed:
let raw_body = body_lines.join("\n");
let (frontmatter, body) = extract_yaml_frontmatter::<SubtaskFrontmatter>(&raw_body)
    .map_err(|e| AikiError::TemplateFrontmatterInvalid { ... })?;
let fm = frontmatter.unwrap_or_default();
entries.push(SubtaskEntry::Static(TaskDefinition {
    name,
    slug: fm.slug,
    instructions: body.trim().to_string(),
    priority: fm.priority,
    assignee: fm.assignee,
    sources: fm.sources,
    data: fm.data,
}));
```

### Phase 3: Pre-scan and placeholder population

**`cli/src/commands/task.rs`** — in `create_from_template`

Before calling `create_subtask_entries_from_template`, scan for `parent.subtasks.*` references and populate placeholders:

```rust
use crate::tasks::templates::find_variables;

const SUBTASK_SLUG_PLACEHOLDER_PREFIX: &str = "__AIKI_SUBTASK_SLUG_";
const SUBTASK_SLUG_PLACEHOLDER_SUFFIX: &str = "__";

// Pre-scan for parent.subtasks.* references and add placeholders
if let Some(ref raw_content) = template.raw_content {
    for var_name in find_variables(raw_content) {
        if let Some(slug) = var_name.strip_prefix("parent.subtasks.") {
            ctx.set_parent(
                &format!("subtasks.{}", slug),
                &format!("{}{}{}", SUBTASK_SLUG_PLACEHOLDER_PREFIX, slug, SUBTASK_SLUG_PLACEHOLDER_SUFFIX),
            );
        }
    }
}
```

This makes `{{parent.subtasks.criteria}}` resolve to `__AIKI_SUBTASK_SLUG_criteria__` during template processing, avoiding the "not provided" error.

### Phase 4: Two-phase subtask creation

**`cli/src/commands/task.rs`** — `create_subtasks_from_entries`

Restructure into two phases:

**Phase A — Plan:** Iterate all entries, generate task IDs, collect slug→taskID map:

```rust
struct PlannedSubtask {
    task_id: String,
    slug: Option<String>,
}

let mut planned: Vec<PlannedSubtask> = Vec::new();
let mut slug_map: HashMap<String, String> = HashMap::new();

for (i, entry) in entries.iter().enumerate() {
    let subtask_id = generate_task_id(&format!("subtask-{}", i + 1));
    let slug = match entry {
        SubtaskEntry::Static(def) => def.slug.clone(),
        SubtaskEntry::Composed { template_name, .. } => {
            let child = load_template(template_name, &templates_dir)?;
            child.parent.slug.clone()
        }
    };
    if let Some(ref s) = slug {
        slug_map.insert(s.clone(), subtask_id.clone());
    }
    planned.push(PlannedSubtask { task_id: subtask_id, slug });
}
```

**Phase B — Execute:** Create events, replacing slug placeholders with actual task IDs:

```rust
// Build replacement function
fn replace_slug_placeholders(text: &str, slug_map: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (slug, task_id) in slug_map {
        let placeholder = format!("{}{}{}", SUBTASK_SLUG_PLACEHOLDER_PREFIX, slug, SUBTASK_SLUG_PLACEHOLDER_SUFFIX);
        result = result.replace(&placeholder, task_id);
    }
    result
}

// In the execute phase, after substituting variables:
let subtask_instructions = replace_slug_placeholders(&subtask_instructions, &slug_map);
```

The execute phase also needs to pass the slug to each subtask's Created event:
- Static entries: `slug: planned[i].slug.clone()`
- Composed entries: `slug: planned[i].slug.clone()` (from child template's frontmatter)

### Phase 5: Propagate composed template slug

**`cli/src/commands/task.rs`** — in the `Composed` branch of `create_subtasks_from_entries`

Change from:
```rust
let composed_event = TaskEvent::Created {
    slug: None,  // Always None currently
    ...
};
```

To:
```rust
let composed_event = TaskEvent::Created {
    slug: planned[i].slug.clone(),  // From child template's frontmatter
    ...
};
```

Also update the in-memory graph entry and slug_index.

---

## Files Changed

| File | Change |
|------|--------|
| `cli/src/tasks/templates/types.rs` | Add `slug` to `TemplateFrontmatter` |
| `cli/src/tasks/templates/parser.rs` | Propagate frontmatter slug to `template.parent.slug` |
| `cli/src/tasks/templates/resolver.rs` | Parse inline subtask frontmatter in `parse_expanded_subtasks` |
| `cli/src/commands/task.rs` | Pre-scan placeholders, two-phase creation, composed slug propagation |
| `.aiki/templates/aiki/review/criteria/code.md` | Move slug to template-level frontmatter |
| `.aiki/templates/aiki/review/criteria/spec.md` | Move slug to template-level frontmatter |

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| `parent.subtasks.{slug}` where slug doesn't match any sibling | Error: "Subtask slug '{slug}' referenced in template but no sibling has that slug" — detected when placeholder remains after replacement |
| Composed template with no slug in frontmatter | `slug: None` on the Created event (current behavior preserved) |
| Inline subtask with no frontmatter | No slug set, no entry in slug_map (current behavior) |
| Circular slug reference (subtask A references subtask B which references A) | Not possible — slugs resolve to task IDs, not to instructions. No circular dependency. |
| `parent.subtasks.*` in non-entry-based path (static subtasks without `{% subtask %}`) | Works via the existing `create_static_subtasks` in task.rs, which would need the same pre-scan + post-replace logic |

---

## Testing

1. **Unit test**: `parse_expanded_subtasks` with inline subtask frontmatter extracts slug, priority, assignee
2. **Unit test**: Template-level slug round-trips through parse → `TaskTemplate.parent.slug`
3. **Integration test**: Template with `{% subtask aiki/review/criteria/code %}` (composed with slug) + inline subtasks referencing `{{parent.subtasks.criteria}}` resolves correctly
4. **Integration test**: `parent.subtasks.{slug}` resolves to the 32-char task ID of the sibling with that slug
5. **Edge case test**: Unresolved slug placeholder (no matching sibling) produces clear error

---

## What This Does NOT Change

- **Slug validation and uniqueness** — Already implemented
- **`:` resolution syntax** — `parent:slug` addressing is separate
- **`siblings.*` sugar** — Not implemented here (future work)
- **Dynamic subtask slugs** — Slugs on `{% for %}` iteration items are out of scope

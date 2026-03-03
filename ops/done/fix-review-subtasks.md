# Plan: Add attribute support to `{% subtask %}` tag

## Context

The review template uses `{% subtask %}` to compose child templates (criteria, fix/loop), but there's no way to set `needs-context` on composed subtasks. With lane-based execution, subtasks need to declare context dependencies so they execute sequentially within a lane. The parent template (review.md) knows the ordering — not the child templates — so attributes must be set at the composition site.

## Changes

### 1. Extend `{% subtask %}` syntax to accept key:value attributes

**File:** `cli/src/tasks/templates/conditionals.rs`

New syntax: `{% subtask template/name key:value key2:"quoted value" if condition %}`

- Update `TemplateNode::SubtaskRef` to include `attributes: HashMap<String, String>`
- Update `parse_subtask_ref()` to parse key:value pairs between template name and `if` clause
  - Support both unquoted values (`key:value`) and quoted values (`key:"value with spaces"`)
  - Strip quotes from quoted values before storing in HashMap
- Update `node_to_template()` serialization to round-trip attributes (quote values with spaces)
- Update marker emission format: `<!-- AIKI_SUBTASK_REF:name:line:key1:val1;key2:val2 -->`

### 2. Propagate attributes through `SubtaskEntry::Composed`

**File:** `cli/src/tasks/templates/resolver.rs`

- Add `attributes: HashMap<String, String>` field to `SubtaskEntry::Composed`
- Update `parse_expanded_subtasks()` to extract attributes from the marker regex
- Update marker regex to capture the optional attributes segment

### 3. Handle `needs-context` for Composed entries in Phase C

**File:** `cli/src/commands/task.rs`

- In Phase C of `create_subtasks_from_entries()`, extend the loop to also process `SubtaskEntry::Composed` entries for `needs-context`
- Read `needs-context` from `entry.attributes` instead of `subtask_def.needs_context`

### 4. Update review template

**File:** `.aiki/templates/aiki/review.md`

- Add slugs to H2 subtasks (`explore`, `review`)
- Add `needs-context:subtasks.explore` to criteria subtask tags
- Add `needs-context:subtasks.criteria` to Review H2 frontmatter
- Add `needs-context:subtasks.review` to fix/loop subtask tag

### 5. Update tests

**Files:** `cli/src/tasks/templates/conditionals.rs` (tests), `cli/src/tasks/templates/resolver.rs` (tests)

- Add tests for parsing attributes in subtask refs
- Add tests for round-trip serialization with attributes
- Add tests for marker parsing with attributes
- Add tests for conditional subtask refs with attributes
- Add tests for quoted values with spaces

## Verification

1. `cargo test -p aiki-cli` — all existing tests pass, new tests pass
2. `cargo build` — no warnings
3. Manual: create a review task and verify needs-context links are created between subtasks

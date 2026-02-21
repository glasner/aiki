# Cleanup: Remove Frontmatter-Declared Subtasks

**Date**: 2026-02-19
**Status**: Planning

## Goal

Remove the two frontmatter-based subtask creation mechanisms, keeping only subtasks defined in the markdown template body (H2 sections, {%% subtask %%} directives, {%% for %%} loops).

## What is Removed

- **subtasks: source.comments** (string) in frontmatter -- dynamic subtasks via create_dynamic_subtasks
- **subtasks: list of objects** in frontmatter -- static subtasks via create_static_subtasks (resolver path)

## What is Kept

- Markdown body H2 sections parsed as subtasks (parse_subtasks(), parse_single_subtask())
- subtask aiki/template directives
- for loops
- The entire create_subtasks_from_entries path in task.rs

---

## Step 1: Delete data_source.rs

**File**: cli/src/tasks/templates/data_source.rs

Delete the entire file -- it only serves the dynamic subtasks path (DataSource enum, parse_data_source, resolve_data_source, resolve_comments).

Update cli/src/tasks/templates/mod.rs to remove the data_source module declaration and any re-exports.

---

## Step 2: Remove frontmatter fields from types

**File**: cli/src/tasks/templates/types.rs

Remove from TaskTemplate:
- pub subtasks: Vec<TaskDefinition>
- pub subtasks_source: Option<String>
- pub subtask_template: Option<String>

And their initializations in TaskTemplate::new().

Remove from TemplateFrontmatter:
- pub subtasks: Option<String>

Remove structs entirely:
- TaskDefinition -- only used by frontmatter-declared subtasks
- SubtaskFrontmatter -- only used by frontmatter-declared subtasks

---

## Step 3: Simplify parser

**File**: cli/src/tasks/templates/parser.rs

Remove:
- has_subtasks_source variable and mode-based routing
- parse_markdown_body_with_mode (or collapse to always parse body)
- extract_subtask_template_section -- extracts raw Subtasks section for dynamic iteration

Keep (these handle H2 body subtasks):
- parse_subtasks() -- parses H2 sections from markdown body
- parse_single_subtask() -- parses individual H2 subtask with optional frontmatter
- extract_subtask_frontmatter() -- extracts per-subtask frontmatter (slug, priority, etc.)

The parser should always parse the markdown body for H2 subtask sections -- no mode flag needed.

---

## Step 4: Remove resolver functions

**File**: cli/src/tasks/templates/resolver.rs

Remove:
- create_static_subtasks() -- resolves from template.subtasks Vec (frontmatter list)
- create_dynamic_subtasks() -- resolves from subtasks_source string
- ParsedSubtaskTemplate struct and parse_subtask_template()

Simplify routing in create_tasks_from_template() and create_subtask_entries_from_template():

Before:
  if template.subtasks_source.is_some() -> create_dynamic_subtasks
  else if has_inline_loops || has_subtask_refs -> create_subtasks_from_inline_loops
  else -> create_static_subtasks (reads template.subtasks Vec)

After:
  if has_inline_loops || has_subtask_refs -> create_subtasks_from_inline_loops
  else -> vec![] (H2 subtasks are parsed at task.rs level via create_subtasks_from_entries)

---

## Step 5: Remove command-level functions

**File**: cli/src/commands/task.rs

Remove:
- create_static_subtasks() function (~lines 5163-5289) -- iterates template.subtasks Vec
- create_dynamic_subtasks() function (~lines 5049-5161) -- calls parse_data_source, resolve_data_source
- The call site block (~lines 4987-5043) that checks subtasks_source and dispatches

The remaining path always goes through create_subtasks_from_entries.

---

## Step 6: Remove error variant

**File**: cli/src/error.rs

Remove MissingSourceTask(String) -- only thrown by the removed create_dynamic_subtasks.

---

## Step 7: Clean up tests

**File**: cli/tests/test_declarative_subtasks.rs

Keep:
- test_variable_substitution_in_parent_task -- uses H2 body sections

Remove (test removed mechanisms):
- All test_parse_data_source_* and test_resolve_data_source_*
- test_parse_template_with_subtasks_source*
- test_variable_substitution_in_static_subtasks
- test_full_declarative_subtask_workflow
- test_dynamic_subtasks_with_text_only
- test_empty_data_source_produces_no_subtasks
- test_none_data_source_produces_no_subtasks
- test_missing_subtask_template_section
- test_static_subtasks_not_affected_by_data_source
- test_complex_subtask_template_multiline
- test_variable_context_merges_parent_and_item_data
- test_item_data_overrides_parent_data
- test_parent_prefixed_variables_in_dynamic_subtasks
- test_parent_builtin_variables_in_dynamic_subtasks
- test_dynamic_subtask_with_frontmatter
- test_dynamic_subtask_frontmatter_with_data_field
- Helper functions create_test_task, create_comment, create_comment_with_data

Also remove inline tests in parser.rs and resolver.rs for the removed functions.

---

## Step 8: Build and test

  cargo build --manifest-path cli/Cargo.toml
  cargo test --manifest-path cli/Cargo.toml

---

## Step 9: Validate remaining subtask creation methods

After the removal, validate that the three remaining subtask creation mechanisms still work:

### 9a: Manual subtask creation (aiki task add --parent)

```bash
# Create parent task
PARENT=$(aiki task add "Test parent" | awk '{print $2}')

# Create subtasks manually
aiki task add "Subtask 1" --parent $PARENT
aiki task add "Subtask 2" --parent $PARENT --slug step-two

# Verify
aiki task show $PARENT
# Should show 2 subtasks with full 32-char IDs, linked via subtask-of
```

### 9b: H2 body subtasks (markdown sections in template)

Create a test template at `.aiki/templates/test/h2-subtasks.md`:

```markdown
---
version: 1.0.0
---

# Test: {data.name}

Parent instructions here.

## Step 1: First thing
---
slug: first
---

Do the first thing.

## Step 2: Second thing

Do the second thing.
```

Test:

```bash
# Create task from template
TASK=$(aiki task add --template test/h2-subtasks --data name=MyTest | awk '{print $2}')

# Verify
aiki task show $TASK
# Should show 2 subtasks (Step 1 with slug "first", Step 2 without slug)
```

### 9c: Composable subtasks ({% subtask %} directive)

Create a test template at `.aiki/templates/test/compose.md`:

```markdown
---
version: 1.0.0
---

# Composed Task

Parent task.

{% subtask aiki/plan if data.needs_plan %}
```

Test:

```bash
# Create task with composition
TASK=$(aiki task add --template test/compose --data needs_plan=true --data spec=ops/now/example.md | awk '{print $2}')

# Verify
aiki task show $TASK
# Should show the plan subtask (aiki/plan template instantiated)
```

### 9d: Loop-based subtasks ({% for %})

Create a test template at `.aiki/templates/test/loop-subtasks.md`:

```markdown
---
version: 1.0.0
---

# Batch Task

Parent task.

{% for item in data.items %}
## Process {item.name}

Handle {item.name} with priority {item.priority}.
{% endfor %}
```

Test:

```bash
# Create task with loop-generated subtasks
TASK=$(aiki task add --template test/loop-subtasks \
  --data 'items=[{"name":"A","priority":"p0"},{"name":"B","priority":"p2"}]' \
  | awk '{print $2}')

# Verify
aiki task show $TASK
# Should show 2 subtasks: "Process A" and "Process B"
```

### 9e: Spawn-based subtasks (spawns: in template)

Verify the spawn codepath (updated in earlier fix to use full IDs):

Create a test template at `.aiki/templates/test/spawn.md`:

```markdown
---
version: 1.0.0
spawns:
  - template: test/h2-subtasks
    when: closed.outcome == "done"
    as_subtask: true
    data:
      name: "Spawned"
---

# Spawn Parent

Close this task to spawn a subtask.
```

Test:

```bash
# Create task
TASK=$(aiki task add --template test/spawn | awk '{print $2}')

# Start and close to trigger spawn
aiki task start $TASK
aiki task close $TASK --summary "Done"

# Verify
aiki task show $TASK
# Should show 1 spawned subtask with full 32-char ID
```

If all 5 mechanisms work, the removal is complete and backward-compatible.

---

## Files Modified

| File | Change |
|------|--------|
| cli/src/tasks/templates/data_source.rs | Delete |
| cli/src/tasks/templates/mod.rs | Remove data_source module |
| cli/src/tasks/templates/types.rs | Remove 3 TaskTemplate fields, 2 structs, 1 TemplateFrontmatter field |
| cli/src/tasks/templates/parser.rs | Remove mode routing, extract_subtask_template_section, associated tests |
| cli/src/tasks/templates/resolver.rs | Remove create_static_subtasks, create_dynamic_subtasks, ParsedSubtaskTemplate, simplify routing |
| cli/src/commands/task.rs | Remove 2 functions + call site block |
| cli/src/error.rs | Remove MissingSourceTask variant |
| cli/tests/test_declarative_subtasks.rs | Remove ~30 test functions, keep H2-body tests |

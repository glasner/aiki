# Rename task.children to task.subtasks

## Motivation

The AGENTS.md documentation already uses "subtasks" terminology:
- "Parent + Subtasks (Example)"
- "Add subtasks under the parent"
- "A `.0` subtask auto-starts"
- "Subtask IDs are `<parent-id>.1`, `<parent-id>.2`"

But the code uses "children" throughout. This creates a terminology mismatch. "Subtasks" is the natural way to talk about nested tasks in a task management context, while "children" is a more generic tree/graph term.

## Scope

### Files to Modify

| File | Changes |
|------|---------|
| `cli/src/tasks/manager.rs` | Rename 4 functions + ~25 test references |
| `cli/src/tasks/id.rs` | Rename 1 function + ~10 test/comment references |
| `cli/src/tasks/mod.rs` | Update 4 exports |
| `cli/src/commands/task.rs` | Update function calls + XML element `<children>` → `<subtasks>` |
| `cli/src/tasks/xml.rs` | Update 2 doc comments |

### Function Renames

| Old Name | New Name |
|----------|----------|
| `has_children(tasks, parent_id)` | `has_subtasks(tasks, parent_id)` |
| `get_children(tasks, parent_id)` | `get_subtasks(tasks, parent_id)` |
| `all_children_closed(tasks, parent_id)` | `all_subtasks_closed(tasks, parent_id)` |
| `get_unclosed_children(tasks, parent_id)` | `get_unclosed_subtasks(tasks, parent_id)` |
| `get_next_child_number(parent_id, ids)` | `get_next_subtask_number(parent_id, ids)` |

### Test Function Renames

| Old Name | New Name |
|----------|----------|
| `test_has_children()` | `test_has_subtasks()` |
| `test_get_children()` | `test_get_subtasks()` |
| `test_all_children_closed()` | `test_all_subtasks_closed()` |
| `test_get_unclosed_children()` | `test_get_unclosed_subtasks()` |
| `test_get_children_excludes_grandchildren()` | `test_get_subtasks_excludes_grandsubtasks()` |
| `test_has_children_with_only_grandchildren()` | `test_has_subtasks_with_only_grandsubtasks()` |

### XML Output Change

```xml
<!-- Before -->
<children>
  <task id="abc.1" status="pending" name="First subtask"/>
</children>

<!-- After -->
<subtasks>
  <task id="abc.1" status="pending" name="First subtask"/>
</subtasks>
```

### NOT in Scope

- **JJ's `children()` function** in storage.rs (line 79): This is JJ's native revset function for querying revision children, not our terminology. Leave unchanged.
- **"grandchildren" terminology**: Will become "grandsubtasks" to stay consistent.

## Implementation Plan

1. **Update manager.rs** - Rename 4 functions and update all test function names/references
2. **Update id.rs** - Rename `get_next_child_number` and update tests/comments
3. **Update mod.rs** - Change exports to new names
4. **Update task.rs** - Update all call sites + XML element
5. **Update xml.rs** - Update doc comments
6. **Run tests** - Verify nothing breaks
7. **Build** - Ensure clean compilation

## Risk Assessment

**Low risk** - This is a pure rename refactor with no behavioral changes. All call sites are internal (not public API), so this is a straightforward find-and-replace operation.

## Verification

```bash
# After implementation
cargo build
cargo test
cargo clippy

# Verify no stray "children" references (except JJ's children())
rg "children" cli/src/tasks/ --ignore-case | grep -v "jj.*children" | grep -v "grandchildren"
```

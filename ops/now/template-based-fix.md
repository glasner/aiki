# Template-Based Fix Command

**Date**: 2026-01-21
**Status**: Design Exploration
**Related**: [Code Review Task-Based Design](code-review-task-native.md), [Task Templates](task-templates.md)

---

## Overview

Currently, `aiki fix <task_id>` is hardcoded logic that loops through comments and creates followup tasks. This explores making `fix` template-driven instead.

## Current Approach (Hardcoded)

```rust
// In cli/src/commands/review.rs
pub fn fix(task_id: String) -> Result<()> {
    let comments = get_task_comments(&task_id)?;
    
    if comments.is_empty() {
        println!("✓ Review approved - no issues found");
        return Ok(());
    }
    
    // Create parent followup task
    let followup = Task::new("Followup: Fix review issues");
    followup.add_source(&format!("task:{}", task_id));
    
    // Create one subtask per comment
    for comment in comments {
        let subtask = Task::new(&comment.text);
        subtask.add_source(&format!("comment:{}", comment.id));
        followup.add_child(subtask);
    }
    
    task_add_with_children(followup)?;
    Ok(())
}
```

## Template-Based Approach

### Option 1: Dynamic Subtask Generation

Template with special `for_each` directive:

```markdown
---
# cli/src/tasks/templates/builtin/fix.md
version: 1.0.0
description: Create followup tasks from review comments
---

# Followup: {source.name}

Fix all issues identified in review.

# Subtasks

<!-- for_each: comment in source.comments -->
## {comment.text}

**File**: {comment.file}:{comment.line}
**Severity**: {comment.severity}
**Category**: {comment.category}

{comment.description}

<!-- /for_each -->
```

**Usage:**
```bash
# Instead of hardcoded fix command:
aiki fix xqrmnpst

# Use template:
aiki task add --template aiki/fix --source task:xqrmnpst
```

**Template Variables:**

*Parent template (fix.md):*
- `{source.name}` - Name of source task
- `{source.comments}` - Array of comments from source task

*Subtask template (each comment becomes current context):*
- `{text}` - Comment summary (current item)
- `{file}` - File path from comment (current item)
- `{line}` - Line number (current item)
- `{severity}` - error|warning|info (current item)
- `{category}` - functionality|quality|security|performance (current item)
- `{description}` - Full comment description (current item)
- `{root.source.*}` - Access parent template variables (e.g., `{root.source.name}`)
- `{root.data.*}` - Access parent template data variables

### Option 2: Template + Command Wrapper

Keep `aiki fix` as convenience command, but it uses template internally:

```rust
pub fn fix(task_id: String) -> Result<()> {
    // Load fix template and populate with task data
    let template = load_template("aiki/fix")?;
    let source_task = get_task(&task_id)?;
    
    // Template system handles comment iteration
    create_task_from_template(
        template,
        HashMap::from([
            ("source", format!("task:{}", task_id)),
        ])
    )?;
    
    Ok(())
}
```

```

**Approach B: Lazy evaluation**
```rust
// Template references {source.comments}
// System recognizes "source.*" prefix and fetches task data on-demand
```

### 3. Empty Comments Handling

What if review has no comments?

**Template solution:**
```markdown
# Subtasks

<!-- if: source.comments.length > 0 -->
<!-- for_each: comment in source.comments -->
## Fix: {comment.text}
<!-- /for_each -->
<!-- else -->
## Review approved
No issues found - review completed successfully.
<!-- /if -->
```

### 4. Backward Compatibility

**Keep `aiki fix` command?**
- **Yes**: Convenience wrapper around template (Option 2 above)
- **No**: Users learn `aiki task add --template aiki/fix --source task:ID`

**Recommendation**: Keep `aiki fix` as convenience command for better UX.

## Implementation Phases

### Phase 1: Template Loop Support

Add loop directives to template parser:
- `<!-- for_each: var in collection -->` 
- Access loop variables: `{var.field}`
- Conditional support: `<!-- if: condition -->`

**Files:**
- `cli/src/tasks/templates/parser.rs` - Add loop parsing
- `cli/src/tasks/templates/renderer.rs` - Render loops

### Phase 2: Source Data Resolution

Enable templates to reference source task data:
- `{source.name}` - Source task name
- `{source.comments}` - Array of comments
- Auto-fetch task data when `--source task:ID` is provided

**Files:**
- `cli/src/tasks/templates/context.rs` - Resolve `source.*` variables

### Phase 3: Built-in Fix Template

Create bundled `aiki/fix` template:

**Files:**
- `cli/src/tasks/templates/builtin/fix.md`

### Phase 4: Update Fix Command

Change `aiki fix` to use template:

**Files:**
- `cli/src/commands/review.rs` - Call template system instead of hardcoded logic

## Benefits

1. **Customizable**: Users can override fix template for custom workflows
2. **Consistent**: Fix uses same template system as reviews
3. **Readable**: Template shows structure clearly vs imperative code
4. **Flexible**: Can add custom fields to fix template without code changes

## Drawbacks

1. **Complexity**: Template system needs loop/conditional support
2. **Learning curve**: Users need to understand template syntax
3. **Debugging**: Harder to debug template logic vs code
4. **Performance**: Template parsing overhead (minimal, but exists)

## Recommendation

**Start simple, iterate later:**

1. **Phase 1-3**: Keep hardcoded `aiki fix` command (ship faster)
2. **Phase 4+**: Add template loop support as enhancement
3. **Migration**: Once templates support loops, make `fix` template-based

This lets us ship the review system sooner while keeping the door open for template-based fix later.

## Example: Full Fix Template

```markdown
---
# cli/src/tasks/templates/builtin/fix.md
version: 1.0.0
description: Fix issues from code review
---

# Followup: {source.name}

Address all issues identified during code review.

**Review task**: {source.id}
**Issues found**: {source.comments.length}

# Subtasks

<!-- for_each: comment in source.comments -->
## {comment.text}

**Location**: {comment.file}:{comment.line}
**Severity**: {comment.severity}
**Category**: {comment.category}

{comment.description}

**Review comment**: {comment.id}

<!-- /for_each -->

<!-- if: source.comments.length == 0 -->
## Review approved

No issues found - code review passed all checks.
<!-- /if -->
```

## Open Questions

1. Should loop variables support nested access? (e.g., `{comment.metadata.author}`)
2. How to handle comments without file/line metadata?
3. Should fix template be required, or fall back to hardcoded logic if template missing?
4. Performance: Loop 50+ comments - is template rendering fast enough?
5. Should templates support filters? (e.g., `{comment.text | truncate:50}`)

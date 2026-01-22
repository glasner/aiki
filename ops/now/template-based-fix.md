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

## Template-Based Approach (Chosen)

**Implicit Subtask Looping with YAML Frontmatter**

Template at `cli/src/tasks/templates/builtin/fix.md`:

```markdown
---
version: 1.0.0
description: Create followup tasks from review comments
subtasks:
  from: source.comments
---

# Followup: {source.name}

Fix all issues identified in review.

# Subtasks

## {text}

**Review**: {parent.source.name}
**File**: {file}:{line}
**Severity**: {severity}
**Category**: {category}

{description}
```

**How it works:**
- `subtasks.from: source.comments` tells template system to iterate
- Each comment in the array becomes one subtask
- The `# Subtasks` section is used as the template for each item
- Current item becomes root context (access fields directly: `{text}`, `{file}`)
- Parent context via `parent.*` prefix (e.g., `{parent.source.name}`)

**Usage:**

```bash
# Convenience command (wrapper around template)
aiki fix xqrmnpst

# Or direct template usage
aiki task add --template aiki/fix --source task:xqrmnpst
```

**Implementation:**
```rust
pub fn fix(task_id: String) -> Result<()> {
    // aiki fix is just a wrapper around the template system
    create_task_from_template(
        "aiki/fix",
        HashMap::from([
            ("source", format!("task:{}", task_id)),
        ])
    )?;
    
    Ok(())
}
```

The template system:
1. Loads `aiki/fix` template
2. Sees `subtasks.from: source.comments` in frontmatter
3. Fetches source task and extracts comments

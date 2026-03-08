---
status: draft
---

# Review Uncommitted Changes

**Date**: 2026-02-08
**Status**: Idea
**Priority**: P3

**Related Documents**:
- [Review and Fix Non-Task Targets](review-and-fix-files.md) - File-targeted review (superset feature)
- [Composable Task Templates](../now/composable-task-templates.md) - Template composition

---

## Problem

Currently `aiki review` only reviews work associated with tasks (`aiki review <task-id>`) or closed tasks in a session (`aiki review`). There's no way to review uncommitted changes in the git working copy — edits that exist but haven't been committed yet.

Common scenarios where this matters:
1. **Agent finishes work** — before committing, you want a quality check on the actual diff
2. **Manual edits** — user made changes directly and wants a review before committing
3. **Mid-work check** — agent wants to self-review progress on a complex task
4. **Mixed changes** — working copy has changes from multiple sources (some tracked by tasks, some not)

The gap: there's no `aiki review` target type that says "review whatever is currently changed in the git working copy."

---

## Proposed Approach

Add three mutually exclusive flags to `aiki review` for reviewing git working copy changes:

| Flag | Git command | What it reviews |
|------|------------|-----------------|
| `--staged` | `git diff --staged` | Only staged changes |
| `--unstaged` | `git diff` | Only unstaged changes |
| `--uncommitted` | Both | All uncommitted changes (staged + unstaged) |

### Command Syntax

```bash
# Review only staged changes
aiki review --staged

# Review only unstaged changes
aiki review --unstaged

# Review all uncommitted changes (staged + unstaged)
aiki review --uncommitted

# Can combine with existing execution options
aiki review --uncommitted --start          # Start review, return control
aiki review --staged --async               # Run in background
aiki review --unstaged --agent codex       # Assign specific reviewer
```

### What Gets Reviewed

Each flag maps to specific git commands:

```bash
# --staged
git diff --staged --no-color

# --unstaged
git diff --no-color

# --uncommitted (combines both)
git diff --staged --no-color
git diff --no-color
```

If the selected diff is empty, the command outputs a "nothing to review" message and exits.

### How It Works

1. **Compute diff** — Run the appropriate `git diff` command(s) from the repo root
2. **Create review task** — Use a review template with the diff embedded as context
3. **Run review** — Agent reads the diff, leaves comments on the review task
4. **Output** — Review task ID to stdout (for piping to `aiki fix`)

### New Review Target

Extend the review scope detection in `create_review`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum DiffScope {
    Staged,
    Unstaged,
    Uncommitted,  // both
}

// In CreateReviewParams:
pub diff_scope: Option<DiffScope>,

// In create_review:
if let Some(scope) = params.diff_scope {
    let diff = get_git_diff(cwd, scope)?;
    if diff.is_empty() {
        output_nothing_to_review_diff(scope)?;
        return Err(AikiError::NothingToReview);
    }
    let scope_name = match scope {
        DiffScope::Staged => "staged changes",
        DiffScope::Unstaged => "unstaged changes",
        DiffScope::Uncommitted => "uncommitted changes",
    };
    // scope_id = "staged" | "unstaged" | "uncommitted"
} else if let Some(ref id) = params.task_id {
    // Existing: review specific task
} else {
    // Existing: review session
}
```

### Template

A new template `review-diff` provides the review instructions. The template adapts its title based on the scope:

```markdown
---
version: 1.0.0
type: review
---

# Review: {{scope.name}}

Review the {{scope.name}} in the git working copy.

When all subtasks are complete, close this task with a comment of "Review complete (n issues found)"

# Subtasks

## Understand the changes

The following diff shows the {{scope.name}}:

```diff
{{diff}}
```

Read the changed files in full to understand context around the diff.
Leave a comment on {{parent.id}} to explain your understanding of the changes.

## Review

Review the changes for:
- Correctness: logic errors, edge cases, incorrect assumptions
- Quality: error handling, code clarity, resource management
- Security: injection, auth issues, data exposure
- Completeness: missing tests, incomplete implementations

Track each issue found using `aiki review issue add {{parent.id}} "Description" --file path/to/file:line`.
```

### Diff Retrieval

New helper function:

```rust
fn get_git_diff(cwd: &Path, scope: DiffScope) -> Result<String> {
    use std::process::Command;

    let mut diff = String::new();

    if matches!(scope, DiffScope::Staged | DiffScope::Uncommitted) {
        let staged = Command::new("git")
            .current_dir(cwd)
            .args(["diff", "--staged", "--no-color"])
            .output()
            .map_err(|e| AikiError::GitDiffFailed(format!("git diff --staged failed: {}", e)))?;

        let staged_str = String::from_utf8_lossy(&staged.stdout);
        if !staged_str.trim().is_empty() {
            if scope == DiffScope::Uncommitted {
                diff.push_str("## Staged changes\n\n");
            }
            diff.push_str(&staged_str);
            diff.push('\n');
        }
    }

    if matches!(scope, DiffScope::Unstaged | DiffScope::Uncommitted) {
        let unstaged = Command::new("git")
            .current_dir(cwd)
            .args(["diff", "--no-color"])
            .output()
            .map_err(|e| AikiError::GitDiffFailed(format!("git diff failed: {}", e)))?;

        let unstaged_str = String::from_utf8_lossy(&unstaged.stdout);
        if !unstaged_str.trim().is_empty() {
            if scope == DiffScope::Uncommitted {
                diff.push_str("## Unstaged changes\n\n");
            }
            diff.push_str(&unstaged_str);
        }
    }

    Ok(diff)
}
```

---

## Template Variable Injection

The review command passes the diff content into the template as a builtin variable:

```rust
builtins.insert("diff".to_string(), diff_content);
```

The template references `{{diff}}` directly. This works with the current template engine. No diff size limit — trust the reviewing agent to handle whatever size diff exists.

---

## Integration with `aiki fix`

The output follows the existing review pattern — review task ID to stdout when piped:

```bash
# Review and fix pipeline
aiki review --uncommitted | aiki fix

# Or manual two-step
aiki review --staged --start
# ... agent reviews, leaves comments ...
aiki fix <review-task-id>
```

Since there's no originating task, `aiki fix` creates standalone fix tasks (same as file-targeted reviews in `review-and-fix-files.md`).

---

## Output Format

**stdout (piped):**
```
xqrmnpst
```

**stderr:**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" target="staged" target_type="working_copy" comments="3">
    Review completed with 3 comments.
  </completed>
</aiki_review>
```

**Nothing to review:**
```xml
<aiki_review cmd="review" status="ok">
  <approved>
    Nothing to review - no staged changes.
  </approved>
</aiki_review>
```

---

## Implementation

### Files to Modify

1. **`cli/src/commands/review.rs`**
   - Add `--staged`, `--unstaged`, `--uncommitted` flags to `ReviewArgs`
   - Add `DiffScope` enum and `diff_scope` field to `CreateReviewParams`
   - Handle diff scope in `create_review`
   - Add `get_git_diff()` helper
   - Validate mutual exclusivity of flags

2. **`cli/src/commands/review.rs` (run_review)**
   - Pass diff scope through to `create_review`

3. **`cli/src/tasks/templates/resolver.rs`**
   - Update `create_review_task_from_template` to accept optional diff content as a builtin variable

4. **`.aiki/tasks/review-diff.md`** (new)
   - Review template for working copy changes with `{{diff}}` placeholder

### Implementation Steps

1. Add `DiffScope` enum and three flags to `ReviewArgs` struct
2. Add validation: flags are mutually exclusive, cannot combine with task ID
3. Add `get_git_diff()` function
4. Create `review-diff` template
5. Update `create_review` to handle diff scope (compute diff, pass as builtin)
6. Wire flags through `run_review` → `create_review`
7. Update XML output for working copy target type
8. Add tests

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| No changes for selected scope | "Nothing to review" message, exit success |
| Not in a git repo | Error: `"Not in a git repository"` |
| Diff flag with task ID | Error: `"--staged/--unstaged/--uncommitted cannot be combined with a task ID"` |
| Multiple diff flags | Error: `"--staged, --unstaged, and --uncommitted are mutually exclusive"` |
| `git diff` fails | Error: propagate git error |

---

## Open Questions

1. **Relationship to task diffs**: If uncommitted changes are from an in-progress task, should the review link back to that task?

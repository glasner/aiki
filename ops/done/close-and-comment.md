# Task Close with Comment

**Status**: ✅ Complete
**Priority**: Medium
**Depends On**: Milestone 1.4 Task System
**Related**: `run-task.md`, `code-review-task-native.md`

---

## Overview

Add a `--comment` flag to `aiki task close` to leave a comment before closing a task. This enables agents and users to document why a task was closed, provide context, or leave final notes. For agent-initiated closes, `--comment` is required; if missing, the error should prompt the agent to summarize their work.

**Key Use Case**: Review tasks where the reviewer wants to leave feedback before closing (e.g., "Looks good, approved!" or "Closing as won't fix - out of scope").

---

## Table of Contents

1. [Motivation](#motivation)
2. [CLI Command Design](#cli-command-design)
3. [Event Model](#event-model)
4. [Use Cases](#use-cases)
5. [Implementation](#implementation)

---

## Motivation

### Current State

Today, closing a task is separate from commenting:

```bash
# Two separate commands
aiki task comment xqrmnpst "Review complete, all issues addressed"
aiki task close xqrmnpst --outcome done
```

### Problem

1. **Extra step** - Requires two commands for a common pattern
2. **Ordering matters** - Comment should come before close (chronologically)
3. **Atomic operation** - Comment + close should happen together
4. **Agent friction** - Agents need to remember to comment before closing

### Solution

Allow commenting as part of the close operation:

```bash
# Single atomic operation
aiki task close xqrmnpst --outcome done --comment "Review complete, all issues addressed"
```

---

## CLI Command Design

### Current Command

```bash
aiki task close <task_id> [--outcome <outcome>] [--reason <text>]
```

**Options:**
- `--outcome` - done, wontfix, blocked, duplicate (default: done)
- `--reason` - Short reason for closing (stored in close event)

### Enhanced Command

```bash
aiki task close <task_id> [--outcome <outcome>] [--reason <text>] [--comment <text>]
```

**New Option:**
- `--comment` - Leave a comment before closing (supports multiline via heredoc)

**Behavior:**
1. If `--comment` provided, emit `comment_added` event first
2. Then emit `closed` event with outcome and reason
3. Both events have same timestamp (or comment timestamp is 1ms earlier)
4. If caller is an agent session, `--comment` is required; otherwise error and prompt to summarize work

### Examples

**Basic close with comment:**
```bash
aiki task close xqrmnpst --comment "All issues addressed, looks good!"
```

**Close with heredoc comment:**
```bash
aiki task close xqrmnpst --comment - <<EOF
Review complete. All security issues resolved:
- Fixed null pointer check in auth.ts
- Added JWT validation in middleware.ts

Code is ready to merge.
EOF
```

**Close as won't fix with explanation:**
```bash
aiki task close xqrmnpst --outcome wontfix --comment "Out of scope for this sprint, will revisit in Q2"
```

**Close as duplicate:**
```bash
aiki task close lpqrstwo --outcome duplicate --comment "Duplicate of task:mxsl"
```

**Close with comment (multiline on command line):**
```bash
aiki task close xqrmnpst --comment "All issues from review:xqrmnpst have been fixed. Code quality looks good."
```

### Difference: --reason vs --comment

| Field | Purpose | Storage | Visibility |
|-------|---------|---------|------------|
| `--reason` | Short label for close | In `closed` event | Summary views |
| `--comment` | Full explanation/notes | Separate `comment_added` event | Task details, history |

**Example:**
```bash
aiki task close xqrmnpst --comment "Fixed all 3 issues from security review. Added null checks and JWT validation."
```

The comment is stored as a separate event and visible in task history.

---

## Event Model

### Event Sequence

When `--comment` is provided:

```yaml
# Event 1: Comment added
---
aiki_task_event: v1
task_id: xqrmnpst
event: comment_added
timestamp: 2025-01-15T10:05:10.000Z
body: |
  All issues addressed, looks good!
---

# Event 2: Task closed
---
aiki_task_event: v1
task_id: xqrmnpst
event: closed
outcome: done
reason: Review approved
timestamp: 2025-01-15T10:05:10.001Z
---
```

**Note**: Comment timestamp is 1ms before close to maintain correct chronological order.

### Without --comment

Behavior unchanged:

```yaml
# Single event
---
aiki_task_event: v1
task_id: xqrmnpst
event: closed
outcome: done
reason: Review approved
timestamp: 2025-01-15T10:05:10Z
---
```

---

## Use Cases

### Use Case 1: Review Task Closure

**Scenario**: Codex agent completes a review and wants to close with approval comment.

```bash
# Agent perspective
aiki task close xqrmnpst --comment "Code review complete. No issues found. Ready to merge."
```

**Result**:
- Review task closed
- Approval comment visible in task history
- User sees both outcome and detailed feedback

### Use Case 2: Won't Fix with Explanation

**Scenario**: Developer decides not to fix an issue.

```bash
aiki task close lpqrstwo \
  --outcome wontfix \
  --comment "After discussion, we've decided this is acceptable behavior. The null case is handled upstream."
```

**Result**:
- Task closed as won't fix
- Explanation preserved for future reference
- Team understands the decision

### Use Case 3: Followup Task Closure

**Scenario**: Agent completes all subtasks and closes parent followup task.

```bash
aiki task close lpqrstwo --comment "All review issues have been addressed:
- ✅ lpqrstwo.1: Null pointer check added
- ✅ lpqrstwo.2: JWT validation implemented

Changes ready for re-review."
```

**Result**:
- Parent task closed
- Summary of completed work documented
- Review loop can proceed

### Use Case 4: Duplicate Task

**Scenario**: User realizes a task is a duplicate.

```bash
aiki task close newrstu \
  --outcome duplicate \
  --comment "This is a duplicate of task:mxsl. Closing in favor of the original task."
```

**Result**:
- Task marked as duplicate
- Reference to original task preserved
- Prevents duplicate work

### Use Case 5: Review Loop Approval

**Scenario**: Final review iteration finds no issues.

```bash
aiki task close xqrmnpst --comment "✅ Review iteration 3: No issues found. All previous concerns addressed. Code approved."
```

**Result**:
- Review task closed with approval
- Iteration context preserved
- Review loop ends successfully

---

## Implementation

### Command Handler

```rust
pub struct TaskCloseOptions {
    pub task_id: String,
    pub outcome: TaskOutcome,
    pub reason: Option<String>,
    pub comment: Option<String>,
}

pub fn task_close(opts: TaskCloseOptions) -> Result<()> {
    // Load and validate task
    let task = load_task(&opts.task_id)?;
    
    if task.is_closed() {
        return Err(AikiError::TaskAlreadyClosed(opts.task_id.clone()));
    }
    
    let timestamp = Utc::now();

    // Require comment for agent-initiated closes
    if is_agent_session()? && opts.comment.is_none() {
        return Err(AikiError::TaskCommentRequired(
            "Closing tasks requires a comment. Please summarize your work with --comment."
                .to_string(),
        ));
    }
    
    // If comment provided, emit comment event first
    if let Some(comment) = &opts.comment {
        emit_task_event(TaskEvent::CommentAdded {
            task_id: opts.task_id.clone(),
            timestamp: timestamp - Duration::milliseconds(1), // 1ms earlier
            instructions: comment.clone(),
        })?;
    }
    
    // Emit close event
    emit_task_event(TaskEvent::Closed {
        task_id: opts.task_id.clone(),
        outcome: opts.outcome,
        reason: opts.reason,
        timestamp,
    })?;
    
    // Output
    if let Some(comment) = &opts.comment {
        eprintln!("Added comment to task {}", opts.task_id);
    }
    eprintln!("✅ Closed task {} ({})", opts.task_id, opts.outcome);
    
    Ok(())
}
```

### CLI Parsing

```rust
// In cli/src/commands/task.rs

#[derive(Parser)]
pub struct CloseCommand {
    /// Task ID to close
    task_id: String,
    
    /// Outcome of the task
    #[arg(long, default_value = "done")]
    outcome: TaskOutcome,
    
    /// Short reason for closing (optional)
    #[arg(long)]
    reason: Option<String>,
    
    /// Comment to add before closing (use "-" for stdin/heredoc)
    #[arg(long)]
    comment: Option<String>,
}

impl CloseCommand {
    pub fn run(&self) -> Result<()> {
        let comment = if self.comment.as_deref() == Some("-") {
            // Read from stdin for heredoc support
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            Some(buffer.trim().to_string())
        } else {
            self.comment.clone()
        };
        
        let opts = TaskCloseOptions {
            task_id: self.task_id.clone(),
            outcome: self.outcome.clone(),
            reason: self.reason.clone(),
            comment,
        };
        
        task_close(opts)
    }
}
```

### Agent Integration

Agents can use this when closing tasks:

```rust
// In agent task execution
fn complete_review_task(task_id: &str, issues_found: usize) -> Result<()> {
    let comment = if issues_found == 0 {
        "Code review complete. No issues found. Ready to merge."
    } else {
        &format!("Code review complete. Found {} issue(s). Followup tasks created.", issues_found)
    };
    
    task_close(TaskCloseOptions {
        task_id: task_id.to_string(),
        outcome: if issues_found == 0 { TaskOutcome::Done } else { TaskOutcome::Rejected },
        reason: Some(if issues_found == 0 { 
            "Review approved".to_string() 
        } else { 
            "Issues found".to_string() 
        }),
        comment: Some(comment.to_string()),
    })?;
    
    Ok(())
}
```

---

## Implementation Plan

### Phase 1: CLI Support (2 days)

**Deliverables:**
- Add `--comment` flag to `aiki task close`
- Heredoc support via `--comment -`
- Emit comment event before close event
- Enforce comment for agent-initiated closes with a "summarize your work" error message
- Add `TaskCommentRequired` error variant under task system errors
- Basic validation and error handling

**Files:**
- `cli/src/commands/task.rs` - Update `CloseCommand`
- `cli/src/tasks/manager.rs` - Update `task_close()` function

### Phase 2: Agent Integration (1 day)

**Deliverables:**
- Update review task closure to use `--comment`
- Update agent task execution helpers
- Documentation for agent usage

**Files:**
- `cli/src/commands/review.rs` - Use comment on close
- `cli/src/tasks/runner.rs` - Agent helpers

### Phase 3: Documentation (1 day)

**Deliverables:**
- Update task system docs
- Add examples to help text
- Update CLAUDE.md guidelines
- Update `.aiki/AGENTS.md` template (used by `aiki init`) to include instructions for agents to always use `--comment` when closing tasks

**Files:**
- Template file used by `aiki init` command
- Example: "When closing tasks, always include a `--comment` to summarize your work. Example: `aiki task close <id> --comment 'Completed X, Y, and Z'`"

**Timeline:** 4 days total

---

## Success Criteria

### Must Have
- ✅ `--comment` flag works with single-line text
- ✅ Heredoc support via `--comment -`
- ✅ Events emitted in correct order
- ✅ Comment visible in `aiki task show`

### Should Have
- ✅ Agent integration in review system
- ✅ Documentation and examples
- ✅ Help text updated
- ✅ AGENTS.md template includes `--comment` instructions

### Nice to Have
- ✅ `--comment` works with `--outcome` and `--reason`
- ✅ Error messages for invalid combinations
- ✅ Preview mode (show what would be added)

---

## Examples in Context

### Review System Integration

In `code-review-task-native.md`, the review command would close like this:

```rust
// After creating followup tasks
if !comments.is_empty() {
    let followup_task_id = create_followup_from_comments(followup_opts)?;
    eprintln!("Created followup task: {}", followup_task_id);
    
    // Close review task with comment
    task_close(TaskCloseOptions {
        task_id: task_id.clone(),
        outcome: TaskOutcome::Rejected,
        reason: Some(format!("{} issues found", comments.len())),
        comment: Some(format!(
            "Review completed with {} issue(s) found.\n\nFollowup task created: {}",
            comments.len(),
            followup_task_id
        )),
    })?;
} else {
    // Approved - close with success comment
    task_close(TaskCloseOptions {
        task_id: task_id.clone(),
        outcome: TaskOutcome::Done,
        reason: Some("Review approved".to_string()),
        comment: Some("✅ Code review complete. No issues found. Ready to merge.".to_string()),
    })?;
}
```

### Flow Action Support (Future)

Could also add to flow actions:

```yaml
# Future: task.close flow action
task.completed:
  - if: $event.task.discovered_from | contains("review:")
    then:
      - task.close:
          task_id: $event.task.id
          outcome: done
          comment: "Review followup completed successfully"
```

---

## References

- Milestone 1.4: Task System
- `ops/now/run-task.md` - Task execution model
- `ops/now/code-review-task-native.md` - Review system using task close
- Task event schema in task system docs

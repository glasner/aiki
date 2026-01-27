/// Shared template for the <aiki> block in AGENTS.md
///
/// This template is used by both `aiki init` and `aiki doctor` to ensure
/// consistent agent instructions across the codebase.

/// Current version of the AIKI block template
pub const AIKI_BLOCK_VERSION: &str = "1.8";

/// Template for the <aiki> block in AGENTS.md
pub const AIKI_BLOCK_TEMPLATE: &str = r#"<aiki version="1.8">

## ⛔ STOP - Read This First

**Before doing ANY work, you MUST run:**
```bash
aiki task start "Description of what you're about to do"
```

This creates and starts a task in one atomic command (quick-start).

**"Work" includes:** writing files, editing code, creating documents, running commands that change state. Reading files to understand the task is OK before starting.

**FORBIDDEN:** Do NOT use `TodoWrite`, the `Task` tool, or mental checklists. These do not persist.

### ❌ WRONG (don't do this)
```bash
# Reading the codebase, writing the plan file...
# Oh, I should track this
aiki task start "Already done task"
aiki task close <id> --comment "..."  # Immediately closing - defeats the purpose!
```

### ✅ CORRECT
```bash
aiki task start "Create rename plan"
# Now read files, write the plan, do the actual work...
aiki task close <id> --comment "Created plan at ops/now/rename.md"
```

**Leave progress comments during long tasks:**
```bash
aiki task comment --id <task-id> "Completed phase 1, now working on phase 2"
```

**When closing tasks, summarize your work:**
```bash
aiki task close <task-id> --comment "What you did"
```

---

## Aiki Task System

**IMPORTANT: Use `aiki task` for ALL task management.** Do not use built-in todo tools (TodoWrite, task lists, etc.). Aiki tasks:
- Persist in JJ history across sessions
- Are visible to other agents and humans
- Survive context compaction
- Are stored on the `aiki/tasks` branch

### TL;DR (First-Time Use)

```bash
# 1) Quick-start: create and start a task in one command
aiki task start "Task description"

# 2) Close it when done (with comment describing your work)
aiki task close <task-id> --comment "What I did to fix this"
```

Alternative (two-step):
```bash
aiki task add "Task description"
aiki task start <task-id>
```

### First Action Rule

**Before doing any substantive work, create and start a task.** This includes:
- Code reviews (`review @file`)
- Document reviews (`review @doc.md`)
- Bug investigations
- Feature implementations
- Refactoring

```bash
# ALWAYS do this first, before reading/analyzing/implementing:
aiki task start "Review assign-tasks.md design"
# ... now do the work ...
aiki task close <task-id> --comment "Reviewed, found 3 issues: ..."
```

### When to Use Tasks

- Any work beyond a quick one-liner or immediate response
- Any multi-step change, investigation, or review
- Anything that could carry over across sessions

### Progress Updates

**For multi-step or long-running tasks, leave comments to track progress:**

```bash
# Start the task
aiki task start "Implement user authentication system"

# As you make progress, add comments
aiki task comment --id <task-id> "Completed database schema design"
aiki task comment --id <task-id> "Implemented password hashing"
aiki task comment --id <task-id> "Added login endpoint, now testing"

# Close with final summary
aiki task close <task-id> --comment "Completed: authentication with JWT tokens, password hashing, and session management"
```

**Benefits:**
- Other agents can see what's been done if they take over
- User can track progress on long tasks
- Creates a record of your thought process and approach

### Quick Reference

```bash
# See what's ready to work on
aiki task

# Quick-start: create and start a new task (RECOMMENDED)
aiki task start "Task description"

# Quick-start with priority
aiki task start "Urgent fix" --p0

# Start existing task by ID
aiki task start <task-id>

# Start multiple existing tasks for batch work
aiki task start <id1> <id2> <id3>

# Stop current task (with optional reason)
aiki task stop --reason "Blocked on X"

# Add a comment (without closing)
aiki task comment --id <task-id> "Progress update: ..."

# Show task details including comments
aiki task show <task-id>

# Close with comment (preferred - atomic operation)
aiki task close <task-id> --comment "Fixed by updating X to do Y"

# Close as won't-do (skipped, not needed, or deliberately declined)
aiki task close <task-id> --wont-do --comment "Already handled by existing code"

# Close multiple tasks
aiki task close <id1> <id2> <id3> --comment "All done"
```

### Parent + Subtasks (Example)

```bash
# Create a parent task
aiki task add "Review prompt-history findings"

# Add subtasks under the parent
aiki task add --parent <parent-id> "Check attribution range collisions"
aiki task add --parent <parent-id> "Define intent summary field"
aiki task add --parent <parent-id> "Add privacy redaction rules"

# Start the parent - this reveals subtasks
aiki task start <parent-id>

# Work through subtasks, closing each with a comment
aiki task start <parent-id>.1
# ... do the work ...
aiki task close <parent-id>.1 --comment "Fixed by ..."
```

### Parent Task Behavior

When you start a parent task with subtasks:
1. A `.0` subtask auto-starts: "Review all subtasks and start first batch"
2. `aiki task` now shows only subtasks (scoped view)
3. Subtask IDs are `<parent-id>.1`, `<parent-id>.2`, etc.
4. When all subtasks are closed, the parent auto-starts for final review
5. Close the parent task when everything is complete

### When Planning Work

Instead of creating a mental todo list or using built-in tools:

```bash
# Break down the work
aiki task add "Research existing implementation"
aiki task add "Design the solution"
aiki task add "Implement changes"
aiki task add "Add tests"

# Start the first task
aiki task start <id>
```

### Task Output Format

Commands return XML showing current state:

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress>
      <task id="abc" name="Current task"/>
    </in_progress>
    <list ready="3">
      <task id="def" priority="p0" name="Next task"/>
    </list>
  </context>
</aiki_task>
```

**Reading the output:**
- `<in_progress>` - Tasks you're currently working on
- `<list ready="N">` - Tasks ready to be started
- `scope="<id>"` attribute means you're inside a parent task (only subtasks shown)

### Task IDs

**Format:** Task IDs are exactly 32 lowercase letters (a-z only), e.g., `xtuttnyvykpulsxzqnznsxylrzkkqssy`

**Recognizing task IDs:** When a user provides a 32-character lowercase alphabetic string, it's almost certainly a task ID. Examples:
- `fix luppzupttoslmupvtsromtrytsqsqmxp` → User wants you to work on task `luppzupttoslmupvtsromtrytsqsqmxp`
- `show oorznprsukkomwtnolrrqspllrywxznv` → User wants to see task details
- `close tnslzmpqpzypnymnzlroorzvxkqtulml` → User wants to close that task

**When you see a task ID:**
1. Run `aiki task show <id>` to see what the task is about
2. If the user wants work done, run `aiki task start <id>` (if not already started)
3. Do the work described in the task
4. Close with `aiki task close <id> --comment "What you did"`

**Subtask IDs:** Append a dot and number to parent ID: `<parent-id>.1`, `<parent-id>.2`

### Workflow

1. **Start before working** - Run `aiki task start` before implementation
2. **Comment on progress** - Use `aiki task comment` during long/multi-step tasks
3. **Stop when blocked** - Use `aiki task stop --reason` to document blockers
4. **Close with comment** - Use `aiki task close --comment` to document your work
5. **Close as won't-do when appropriate** - Use `aiki task close --wont-do --comment` for tasks you skip or decline (not needed, already done, disagree with approach)
6. **Close immediately** - Don't leave tasks open after finishing
7. **Report what you did** - Include completed tasks when replying to user

### Reporting Completed Tasks

**When replying to the user, always include a summary of tasks completed.**

At the end of your response, list the tasks you worked on:

```
## Tasks Completed
- `<task-id>` - Task name: Brief summary of what was done
- `<task-id>` - Task name: Brief summary of what was done
```

Example:
```
## Tasks Completed
- `abc123...` - Fix login bug: Updated auth handler to validate tokens before redirect
- `def456...` - Add unit tests: Added 5 tests covering edge cases in token validation
```

**Why this matters:**
- User sees exactly what work was accomplished
- Creates clear audit trail linking responses to tasks
- Helps user understand scope of changes made
- Makes it easy to review or revert specific work

**When to include:**
- Always include when you've closed one or more tasks
- Include task IDs so user can run `aiki task show <id>` for details
- Keep summaries brief (one line per task)

### Common Pitfalls

- **Using TodoWrite instead of `aiki task`** ← Most common mistake!
- **Not leaving progress comments on long tasks** ← Easy to forget!
- **Not reporting completed tasks to user** ← User can't see what was done!
- Forgetting to `start` a task before you begin work
- Closing tasks without `--comment` to describe what you did
- Leaving tasks open after finishing
- Creating long tasks without subtasks for multi-step work
- Not updating progress with comments during multi-step work
- Trying to `start` a task that's already in progress
- Forgetting to close the parent task after all subtasks are done

### Task Priorities

`p0` (urgent) → `p1` (high) → `p2` (normal, default) → `p3` (low)
</aiki>
"#;

<aiki version="1.13">

## ⛔ STOP - Read This First

**Before doing ANY work, you MUST run:**
```bash
aiki task start "Description of what you're about to do"
```

This creates and starts a task in one atomic command (quick-start).

**"Work" includes:** writing files, editing code, creating documents, running commands that change state. Reading files to understand the task is OK before starting.

**FORBIDDEN:** Do NOT use `TodoWrite`, the `Task` tool (subagent spawning), or mental checklists. These do not persist. Use `aiki task` for tracking and `aiki task run` for delegation.

### ❌ WRONG (don't do this)
```bash
# Reading the codebase, writing the plan file...
# Oh, I should track this
aiki task start "Already done task"
aiki task close <id> --summary "..."  # Immediately closing - defeats the purpose!
```

### ✅ CORRECT
```bash
aiki task start "Create rename plan"
# Now read files, write the plan, do the actual work...
aiki task close <id> --summary "Created plan at ops/now/rename.md"
```

**Leave progress comments during long tasks:**
```bash
aiki task comment <task-id> "Completed phase 1, now working on phase 2"
```

**When closing tasks, summarize your work:**
```bash
aiki task close <task-id> --summary "What you did"
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

# 2) Close it when done (with summary describing your work)
aiki task close <task-id> --summary "What I did to fix this"
```

Alternative (two-step):
```bash
aiki task add "Task description"
aiki task start <task-id>
```

### First Action Rule

**Before modifying any files, create and start a task.** This includes:
- Code reviews (`review @file`)
- Document reviews (`review @doc.md`)
- Bug investigations
- Feature implementations
- Refactoring

```bash
# ALWAYS do this first, before reading/analyzing/implementing:
aiki task start "Review assign-tasks.md design" --source prompt
# ... now do the work ...
aiki task close <task-id> --summary "Reviewed, found 3 issues: ..."
```

Using `--source prompt` links the task to the user's request that triggered it.

### When to Use Tasks

- **Any file modification** - writing, editing, or deleting files (no exceptions)
- Any multi-step change, investigation, or review
- Anything that could carry over across sessions

**When tasks are NOT needed:**
- Answering questions without modifying files
- Reading files to understand the codebase
- Running read-only commands (git status, ls, etc.)

### Progress Updates

**For multi-step or long-running tasks, leave comments to track progress:**

```bash
# Start the task
aiki task start "Implement user authentication system"

# As you make progress, add comments
aiki task comment <task-id> "Completed database schema design"
aiki task comment <task-id> "Implemented password hashing"
aiki task comment <task-id> "Added login endpoint, now testing"

# Close with final summary
aiki task close <task-id> --summary "Completed: authentication with JWT tokens, password hashing, and session management"
```

**Benefits:**
- Other agents can see what's been done if they take over
- User can track progress on long tasks
- Creates a record of your thought process and approach

### Code Reviews

**When asked to review a task's changes, use `aiki review --start`:**

```bash
# Review a specific task's changes (you perform the review)
aiki review <task-id> --start
```

**When to use `aiki review --start`:**
- User asks you to review work done on a task
- User says "review task X" or provides a task ID to review
- You want to check the code changes associated with a completed task

**How it works:**
1. `aiki review <task-id> --start` creates a review task and you perform the review
2. You'll see instructions to run `aiki task diff` and examine the changes
3. Add comments for any issues found using `aiki task comment`
4. Close the review task when done

**The `--start` flag means you perform the review yourself** (vs. spawning a background agent).

**After reviewing**, if you found issues, run `aiki fix` to create followup tasks:
```bash
aiki fix <review-task-id>
```

**Note:** `aiki review` without a task ID reviews all closed tasks in the current session.

### Delegating Work to Subagents

**Do NOT use native subagent tools to spawn agents.** Use `aiki task run` instead — it spawns a separate agent session with full aiki context (task tracking, provenance, hooks).

Native subagent tools include:
- **Claude Code**: `Task` tool (subagent spawning)
- **Codex**: `spawn_agent`, `spawn_agents_parallel`
- **Cursor**: Subagents (`/explore`, `/bash`, etc.) and Background Agents

**Why:** Native subagents run without aiki context. Their work isn't tracked, isn't visible to other agents/humans, and doesn't persist. `aiki task run` gives the spawned agent the same aiki integration you have.

**⚠️ CRITICAL: Always wait for results before responding.** After running `aiki task run`, you MUST wait for the task to finish and inspect its results BEFORE responding to the user. Never just fire off tasks and tell the user "they're running in the background" — the whole point of delegation is to get results back and synthesize them.

**Scenario 1: User asks you to delegate an existing task**
```bash
# Run synchronously (wait for agent to finish, then inspect results)
aiki task run <task-id>
aiki task show <task-id>
```

**Scenario 2: User asks you to have a subagent do something new**
```bash
# 1. Create a task describing the work
aiki task add "Description of the work to delegate"

# 2. Run it with a subagent (waits for completion)
aiki task run <task-id>

# 3. Inspect results before responding
aiki task show <task-id>
```

**Scenario 3: User asks you to run multiple things in parallel**
```bash
# Create tasks for each piece of work
aiki task add "First piece of work"
aiki task add "Second piece of work"

# Launch them concurrently with --async
aiki task run <id1> --async
aiki task run <id2> --async

# IMPORTANT: Wait for ALL tasks to complete, then inspect results
aiki task wait <id1> <id2>
aiki task show <id1>
aiki task show <id2>
# Now synthesize the results and respond to the user
```

**The `--async` flag is ONLY for launching tasks in parallel** — it does NOT mean "fire and forget". You still must wait for all async tasks to finish (using `aiki task wait`) and read their results before responding to the user.

### ❌ WRONG: Using native subagents
```
# Claude Code - Don't use the Task tool
Task(prompt="Go fix the tests", subagent_type="general-purpose")

# Codex - Don't use spawn_agent
spawn_agent(role="fixer", prompt="Go fix the tests")

# Cursor - Don't use subagents or background agents directly
/bash fix the failing tests
```

### ✅ CORRECT: Using aiki task run
```bash
aiki task add "Fix failing tests in auth module"
aiki task run <task-id>
```

### Quick Reference

```bash
# See what's ready to work on
aiki task

# Quick-start: create and start a new task (RECOMMENDED)
aiki task start "Task description"

# Quick-start with priority
aiki task start "Urgent fix" --p0

# Quick-start with source (track where task came from)
aiki task start "Implement feature X" --source file:ops/now/design.md

# Start existing task by ID
aiki task start <task-id>

# Start multiple existing tasks for batch work
aiki task start <id1> <id2> <id3>

# Stop current task (with optional reason)
aiki task stop --reason "Blocked on X"

# Add a comment (without closing)
aiki task comment <task-id> "Progress update: ..."

# Show task details including comments
aiki task show <task-id>

# Close with comment (preferred - atomic operation)
aiki task close <task-id> --summary "Fixed by updating X to do Y"

# Close as won't-do (skipped, not needed, or deliberately declined)
aiki task close <task-id> --wont-do --summary "Already handled by existing code"

# Close multiple tasks
aiki task close <id1> <id2> <id3> --summary "All done"

# Delegate task to a subagent (waits for completion)
aiki task run <task-id>

# Delegate multiple tasks in parallel
aiki task run <id1> --async
aiki task run <id2> --async
aiki task wait <id1> <id2>    # block until all finish
aiki task show <id1>          # inspect results
aiki task show <id2>          # inspect results

# Filter tasks by source
aiki task list --source file:ops/now/design.md
aiki task list --source ops/now/design.md  # partial match works
```

### Handling Multiple Requests (Subtasks)

**When a user asks you to do multiple things at once, create a parent task with subtasks.**

This is common when:
- User provides a list of fixes or changes ("fix X, Y, and Z")
- A review produces multiple issues to address
- User pastes a list of items to work through
- Any request with 2+ distinct pieces of work

**How to do it:**

```bash
# 1. Create a parent task for the overall request
aiki task add "Fix issues from code review" --source prompt

# 2. Add a subtask for each item
aiki task add --parent <parent-id> "Fix null check in auth handler"
aiki task add --parent <parent-id> "Add missing error handling in API client"
aiki task add --parent <parent-id> "Remove unused import in utils.rs"

# 3. Start the parent to begin work
aiki task start <parent-id>

# 4. Work through subtasks one by one
aiki task start <parent-id>.1
# ... do the work ...
aiki task close <parent-id>.1 --summary "Added null check before token access"

aiki task start <parent-id>.2
# ... do the work ...
aiki task close <parent-id>.2 --summary "Wrapped API calls in try/catch"
```

### ❌ WRONG: One big task for multiple items
```bash
# Don't lump everything into one task
aiki task start "Fix all review issues"
# ... do 5 different things ...
aiki task close <id> --summary "Fixed everything"  # No granularity!
```

### ✅ CORRECT: Parent + subtasks
```bash
aiki task add "Fix review issues" --source prompt
aiki task add --parent <id> "Fix null check in auth"
aiki task add --parent <id> "Add error handling in API"
aiki task add --parent <id> "Remove unused import"
aiki task start <id>
# Work through each subtask individually
```

### Parent Task Behavior

When you start a parent task with subtasks:
1. A `.0` subtask auto-starts: "Review all subtasks and start first batch"
2. `aiki task` now shows only subtasks (scoped view)
3. Subtask IDs are `<parent-id>.1`, `<parent-id>.2`, etc.
4. **After all subtasks are done**, review the work to make sure nothing was missed, then close the parent with a summary comment:
   ```bash
   aiki task close <parent-id> --summary "All 3 subtasks done: fixed null check, added error handling, removed unused import"
   ```

### When Planning Work

Instead of creating a mental todo list or using built-in tools:

```bash
# Break down the work (with source for traceability)
aiki task add "Research existing implementation" --source file:ops/now/design.md
aiki task add "Design the solution" --source file:ops/now/design.md
aiki task add "Implement changes" --source file:ops/now/design.md
aiki task add "Add tests" --source file:ops/now/design.md

# Start the first task
aiki task start <id>
```

### Task Sources

The `--source` flag tracks where tasks originate from. This enables:
- **Plan lineage** - Link tasks to design docs or plans that spawned them
- **Review lineage** - Link followup tasks to code reviews
- **Prompt lineage** - Link tasks to the user prompt that triggered them
- **Traceability** - Answer "why does this task exist?"

**Source prefixes:**
| Prefix | Meaning | Example |
|--------|---------|---------|
| `file:` | File path (design doc, plan) | `file:ops/now/design.md` |
| `task:` | Another task (follow-up, review) | `task:xqrmnpst` |
| `comment:` | Specific comment within a task | `comment:c1a2b3c4` |
| `issue:` | External issue tracker | `issue:GH-123` |
| `prompt:` | User prompt that triggered work | `prompt:nzwtoqqr` |

**Linking tasks to prompts:**

Use `--source prompt` to automatically link a task to the user's latest prompt:

```bash
# Quick-start with prompt source (RECOMMENDED for agents)
aiki task start "Implement feature X" --source prompt

# The system resolves "prompt" to the actual change_id, e.g., "prompt:nzwtoqqr"
```

This creates traceability from tasks back to the user request that triggered them.

```bash
# Task from a design document
aiki task add "Implement session tracking" --source file:ops/now/design.md

# Task with multiple sources (from another task + specific comment)
aiki task add "Fix auth bug" --source task:abc123 --source comment:c1a2b3c4

# Quick-start with source
aiki task start "Implement X" --source file:ops/now/design.md

# Query tasks from a specific source
aiki task list --source file:ops/now/design.md
aiki task list --source ops/now/design.md  # partial match works
```

### Task Output Format

**Action commands** (add, start, comment) return slim single-line confirmations:
```
Added: qotysworupowzkxyknzkworuwlyksmls — Fix auth bug
Started: qotysworupowzkxyknzkworuwlyksmls
Commented: qotysworupowzkxyknzkworuwlyksmls
```

**State-transition commands** (stop, close) return confirmation + context footer:
```
Closed: qotysworupowzkxyknzkworuwlyksmls

In Progress:
(none)

Ready (2):
- anothertwentycharsofidpadding01 [p0] Urgent fix
- anothertwentycharsofidpadding02 [p2] Add tests
```

**Read commands** (list, show) return full context:
```
In Progress:
- qotysworupowzkxyknzkworuwlyksmls — Fix auth bug

Ready (3):
- anothertwentycharsofidpadding01 [p0] Urgent fix
- anothertwentycharsofidpadding02 [p2] Add tests
```

**Reading the output:**
- `In Progress:` - Tasks you're currently working on
- `Ready (N):` - Tasks ready to be started
- Error responses: `Error: message`

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
4. Close with `aiki task close <id> --summary "What you did"`

**Subtask IDs:** Append a dot and number to parent ID: `<parent-id>.1`, `<parent-id>.2`

### Workflow

1. **Start before working** - Run `aiki task start` before implementation
2. **Comment on progress** - Use `aiki task comment` during long/multi-step tasks
3. **Stop when blocked** - Use `aiki task stop --reason` to document blockers
4. **Close with summary** - Use `aiki task close --summary` to document your work
5. **Close as won't-do when appropriate** - Use `aiki task close --wont-do --summary` for tasks you skip or decline (not needed, already done, disagree with approach)
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
- **Using the Task tool instead of `aiki task run`** ← Native subagents lack aiki context!
- **Responding before delegated tasks finish** ← Always wait for `aiki task run` results and inspect them before replying!
- **Not leaving progress comments on long tasks** ← Easy to forget!
- **Not reporting completed tasks to user** ← User can't see what was done!
- Forgetting to `start` a task before you begin work
- Closing tasks without `--summary` to describe what you did
- Leaving tasks open after finishing
- Creating long tasks without subtasks for multi-step work
- Not updating progress with comments during multi-step work
- Trying to `start` a task that's already in progress
- Forgetting to review and close the parent task after all subtasks are done

### Task Priorities

`p0` (urgent) → `p1` (high) → `p2` (normal, default) → `p3` (low)
</aiki>

# Review and Fix Commands

**Date**: 2026-01-10
**Updated**: 2026-01-29
**Status**: Ready for Implementation
**Purpose**: Code review workflow via `aiki review` and `aiki fix` commands

**Related Documents**:
- [Task Execution: aiki task run](../done/run-task.md) - Agent runtime and task execution
- [Task-to-Change Linkage](../done/task-change-linkage.md) - Bidirectional task/change tracking
- [Task Diff Command](task-diff.md) - Native `aiki task diff` for viewing task changes
- [Task Source Context](with-source.md) - `--with-source` flag for understanding task intent

**Prerequisites** (all implemented):
- [Background Task Execution](background-run.md) - `--async`, `wait`, `stop` ✅
- [Structured Comment Metadata](comment-metadata.md) - `--data key=value` on comments ✅
- [Declarative Subtasks](declarative-subtasks.md) - `subtasks` template iteration ✅
- [Task Lifecycle Events](task-events.md) - `task.started`, `task.closed` events + sugar triggers ✅
- [Lazy Loading](lazy-load-payloads.md) - Lazy variable resolution for event payloads ✅

---

## Executive Summary

This design uses **tasks for everything** with an **async, composable CLI**:

1. **Review tasks** - Regular tasks with subtasks that agents execute to analyze code
2. **Followup tasks** - Tasks created by `aiki fix` from review comments, with flags controlling execution level
3. **Single storage branch** - All tasks on `aiki/tasks` branch
4. **Async workflow** - `--async` returns immediately, `wait` blocks until done
5. **Consistent task lifecycle** - Reviews use same event structure as other tasks
6. **Pipeable commands** - `aiki review | aiki fix` and `aiki review --async | aiki wait | aiki fix`
7. **Task-change linkage** - Changes made during reviews include `task=` in provenance

---

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Data Model](#data-model)
3. [CLI Commands](#cli-commands)
4. [Flow Integration](#flow-integration)
5. [Review Loop Pattern](#review-loop-pattern)
6. [Use Cases](#use-cases)
7. [Implementation](#implementation)
8. [Benefits](#benefits-of-task-based-approach)
9. [Implementation Plan](#implementation-plan)

---

## Core Concepts

### How Reviews Work

1. **Pipeable workflow commands**: `review`, `wait`, `fix` are composable via Unix pipes
2. **aiki fix addresses findings**: Reads review comments, creates followup tasks, and runs them to completion (flags control execution level: `--start`, `--async`)
3. **Provenance tracking**: Changes made by agents include `task=` field linking to the active task

### Pipeable Commands

The workflow commands (`review`, `wait`, `fix`) follow a Unix pipe convention:

- **stdout (piped/non-TTY)**: Task ID only — machine-readable for pipe consumption
- **stdout (interactive/TTY)**: Empty — nothing printed to stdout
- **stderr**: Structured XML output (always, visible to user in both modes)
- **stdin**: Accepts task ID if no argument provided

This enables two natural patterns:

```bash
# Blocking: review waits for completion, then fix processes findings
aiki review | aiki fix

# Async: review returns immediately, wait blocks, then fix processes
aiki review --async | aiki wait | aiki fix
```

**Flag parity across commands:**

| | Default | `--async` | `--start` |
|---|---|---|---|
| `aiki task run` | Runs to completion | Runs async | — |
| `aiki review` | Creates + runs to completion | Creates + runs async | Creates + starts (agent takes over) |
| `aiki fix` | Creates + runs to completion | Creates + runs async | Creates + starts (agent takes over) |

All commands follow the same pattern: default runs to completion, `--async` runs async, `--start` creates and starts but returns control to the calling agent.

**`--start` implementation:** The `--start` flag always calls `aiki task start <id>` internally — same codepath, same behavior, same error messages. The calling agent takes over the task regardless of the task's `assignee` field. This means `aiki review --start` lets you perform a review yourself even though review tasks default to `assignee: codex`.

In interactive (TTY) mode, stdout is empty and users see only the structured XML on stderr. In piped mode, stdout emits the task ID for the next command while stderr remains visible to the user (since pipes only capture stdout). TTY detection uses `std::io::stdout().is_terminal()` (Rust 1.70+).

**Empty stdin vs empty output:** These are different cases with different behaviors:

| Scenario | Example | Exit Code | stdout |
|----------|---------|-----------|--------|
| No task ID provided | `echo "" | aiki fix` | 1 (error) | empty |
| Task ID provided, no issues found | `aiki review | aiki fix` (review approved) | 0 (success) | empty |
| Task ID provided, issues found | `aiki review | aiki fix` (issues found) | 0 (success) | followup task ID |

**Error propagation:** Unix pipes don't short-circuit by default. For agents that want short-circuit behavior, use `set -o pipefail` or `&&` chaining:

```bash
# Pipe (data flow) — natural for success path
aiki review | aiki fix

# && (error handling) — stops on failure
ID=$(aiki review) && aiki fix "$ID"
```

### Async Review Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Flow hook triggers async review                            │
│     • Creates review task with subtasks                         │
│     • Runs: aiki review --async                             │
│     • Hook sends autoreply to requesting agent                  │
└─────────────────────────────────────────────────────────────────┘
                          ↓                        ↓
         ┌────────────────────────────┐  ┌────────────────────────┐
         │  2a. Background: Codex     │  │  2b. Requesting agent  │
         │      executes review       │  │      receives autoreply│
         │                            │  │                        │
         │  • Digests code changes    │  │  • Gets instructions   │
         │  • Reviews for issues      │  │  • Runs: aiki wait     │
         │  • Adds comments           │  │  • Blocks until done   │
         │  • Closes task when done   │  │                        │
         └────────────────────────────┘  └────────────────────────┘
                          ↓                        ↓
                          └────────────┬───────────┘
                                       ↓
         ┌─────────────────────────────────────────────────────────┐
         │  3. Requesting agent processes findings                 │
         │     • Runs: aiki wait <review_id> | aiki fix            │
         │     • Creates followup tasks from comments              │
         └─────────────────────────────────────────────────────────┘
```

**Note on autoreply**: When a flow hook triggers `--async` review, it should
use autoreply to send instructions back to the requesting agent. The autoreply
tells the agent to run `aiki wait` to block until the review completes:

```yaml
turn.completed:
  - review:
      task_id: $event.task.id
  - autoreply: |
      Review started in background (task:${review.task_id}).
      
      Run this command to wait for completion and address findings:
      aiki wait ${review.task_id} | aiki fix
```

The requesting agent receives this autoreply immediately and runs the `aiki wait`
command, which blocks until the background review task completes. Meanwhile, the
codex agent works on the review in parallel.

### Review Scopes

Reviews can target different scopes of changes:

- **session** - All closed tasks in current session (default)
  - `aiki review` - Review all closed tasks in session
  - If no closed tasks exist: succeeds with "nothing to review" message, exits 0 (consistent with `fix` graceful handling)
- **task** - Changes associated with a specific task
  - `aiki review <task-id>` - Review specific task by ID

**Future scope ideas:**
- **changes** - JJ changes by revset (see [review-jj-changes.md](../future/review-jj-changes.md))
- **files** - Specific file paths (`--files <paths>`)
- **active** - Active task currently being worked on (`--active`)
- **staged** - Files staged for commit (Git interop)

### Task-Aware Review Commands

Reviewing agents use task-aware commands instead of raw jj/git commands:

| Command | Purpose |
|---------|---------|
| `aiki task show <id> --with-source` | Understand task + why it exists (expands source references) |
| `aiki task diff <id>` | View net code changes for the task (pure diff, no XML wrapper) |

**Benefits over raw jj commands:**
- **Task-centric**: Shows all changes for a task, not just working copy
- **Automatic correlation**: No manual work to find which changes belong to the task
- **Subtask aggregation**: Parent task diffs include all subtask changes automatically
- **Intent visibility**: `--with-source` shows the original task/review that created this work

**Review workflow:**
```bash
# Step 1: Understand what you're reviewing and why
aiki task show xqrmnpst --with-source

# Step 2: Read the actual code changes
aiki task diff xqrmnpst
```

See [task-diff.md](task-diff.md) and [with-source.md](with-source.md) for implementation details.

### Task Templates

#### Review Template (`aiki/review`)

**Location:** `.aiki/templates/aiki/review.md`

**Purpose:** Creates a review task with two sequential subtasks (digest, review)

**Template structure:**
```markdown
---
version: 1.0.0
type: review
---

# Review: Work from ${scope.name} (task:${scope.id}) 

This task coordinates review steps as subtasks.

# Subtasks

## Digest code changes

Examine the code changes to understand what was modified.

Commands to use:
1. `aiki task show ${scope} --with-source` - Understand the task and its intent
2. `aiki task diff ${scope}` - View all code changes for the task

The `--with-source` flag expands source references to show why the task exists.
The `diff` command shows the net result of all task work (not individual changes).

## Review code

Review the code changes for bugs, quality, security, performance, and user experience.

If you haven't already, run:
- `aiki task diff ${scope}` - View all code changes for the task

Focus on:
- **Bugs**: Logic errors, edge cases, correctness
- **Quality**: Error handling, resource leaks, null checks, code clarity
- **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
- **Performance**: Inefficient algorithms, unnecessary operations, resource usage
- **User Experience**: UI/UX, accessibility, usability

For each issue found, add a comment using `aiki task comment` with structured data:

aiki task comment <parent.id> \
  --data file=<path> --data line=<line> \
  --data severity=high|medium|low \
  --data category=bug|quality|security|performance|ux \
  "<description of issue, impact, and suggested fix>"

Add comments as you find issues, don't wait until the end.
```

**Note:** The template does not specify an `assignee`. The `aiki review` command sets the assignee dynamically when creating the task (see Review Command implementation below).

**Custom templates:**
- Users can create custom templates in `.aiki/templates/{namespace}/` (e.g., `.aiki/templates/myorg/security-review.md`)
- Example: `aiki review --template myorg/security-review`
- Future: Additional specialized built-in templates (security, performance, style)

#### Fix Template (`aiki/fix`)

**Location:** `.aiki/templates/aiki/fix.md`

**Purpose:** Creates followup tasks from review comments using `subtasks` iteration

**Template structure:**
```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {source.name} (task:{source.id})

Please close all subtasks once they are either fixed or declined. It is fine to mark the as "wont_do" if you think the proposal is either out of scope or introdued to much complexity for the benefits gained. 

# Subtasks

## Fix: {data.category} - {data.severity} Severity Finding

**File**: {data.file}:{data.line}
**Category**: {data.category}

{text}
```

**Variables available:**

The `source` field (stored as `task:xqrmnpst`) is expanded into a full task object during template rendering:

- `source.id` - ID of the review task (e.g., `xqrmnpst`)
- `source.name` - Name of the review task
- `source.comments[]` - Array of comments from the review task
  - `text` - Comment text
  - `data.file` - File path from comment metadata
  - `data.line` - Line number from comment metadata
  - `data.severity` - Severity level (high|medium|low)
  - `data.category` - Issue category (bug|quality|security|performance|ux)

**Priority derivation:**
- `severity=high` → Same priority as reviewed task
- `severity=medium` → One level lower (p+1)
- `severity=low` → Two levels lower (p+2)

**Note:** The template does not specify an `assignee`. The `aiki fix` command sets the assignee dynamically when creating the task (see Fix Command implementation below).

**Custom templates:**
- Users can create custom fix templates: `.aiki/templates/{namespace}/custom-fix.md`
- Example: `aiki fix --template myorg/security-fix`

See [task-templates.md](task-templates.md) and [declarative-subtasks.md](declarative-subtasks.md) for template system details.

### Task Types

**Review tasks** (`type: review`):
- Created by `aiki review` with 2 subtasks (digest, review)
- Task `type` field set to `review` — enables sugar triggers like `review.started`, `review.completed`
- Assigned to reviewer agent (e.g., `codex`)
- Agent adds comments during review
- Closed automatically when agent completes review (parent auto-closes when all subtasks done)
- Supports different scopes and prompts

**Followup tasks** (generic tasks):
- Created by `aiki fix` from review comments (parent + all children in one operation)
- Regular tasks — no special `type` field (uses default or inherits from template)
- Each child task has `source` field referencing specific comment (e.g., `source: task:xqrmnpst`, `source: comment:c1a2b3c4`)
- Visible immediately (no draft flag needed)
- `aiki fix` (default) creates + runs followup tasks to completion
- `aiki fix --async` creates + runs async, returns immediately
- `aiki fix --start` creates + starts, returns control to calling agent
- Changes made while working on these tasks include `task=` in provenance (see [task-change-linkage](../done/task-change-linkage.md))

---

## Data Model: Review Tasks

### Review Task Lifecycle

**User/flow runs:**
```bash
aiki review <task-id>
```

This internally executes:
1. `create_task_from_template()` - Creates parent task + 2 subtasks atomically from the review template (digest, review)
2. `aiki task run xqrmnpst` - Runs the review task to completion (blocking)
3. Returns task ID to caller

**Then agent processes:**
```bash
aiki fix xqrmnpst
```

This:
1. Reads comments from the completed review task
2. Creates followup task with subtasks (if issues found)
3. Runs followup task to completion

**Note:** The simple pipe `aiki review | aiki fix` works because blocking `review` waits for completion before outputting the task ID. Use `--async` only when you want async execution: `aiki review --async | aiki wait | aiki fix`.

**1. Review Task Created** (parent task with children)
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: created
type: review
timestamp: 2025-01-15T10:04:50Z
name: "Review: changes @"
assignee: codex  # Set by aiki review command (opposite of agent who completed task)
instructions: |
  Code review orchestration task
  
  This task coordinates review steps.
data:
  task_id: xqrmnpst
  changes: [zxywtuvs]
---
```

**Note**: 
- This is a parent orchestration task with sequential subtasks
- The `assignee` is set dynamically by `aiki review` (not in template)
- Can be started immediately after creation
- Agent only handles the review work; followup task creation is done by `aiki fix` after agent completes

**Subtask 1: Digest Code Changes**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.1
event: created
type: review
timestamp: 2025-01-15T10:04:51Z
name: "Digest code changes"
assignee: codex  # Inherits from parent task
instructions: |
  Examine the code changes to understand what was modified.

  Commands to use:
  1. `aiki task show xqrmnpst --with-source` - Understand the task and its intent
  2. `aiki task diff xqrmnpst` - View all code changes for the task

  The `--with-source` flag expands source references to show why the task exists.
  The `diff` command shows the net result of all task work (not individual changes).
data:
  task_id: xqrmnpst
  changes: [zxywtuvs]
---
```

**Subtask 2: Review Code**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: created
type: review
timestamp: 2025-01-15T10:04:51Z
name: "Review code"
assignee: codex  # Inherits from parent task
instructions: |
  Review the code changes for bugs, quality, security, performance, and user experience.

  If you haven't already, run:
  - `aiki task diff xqrmnpst` - View all code changes for the task

  Focus on:
  - **Bugs**: Logic errors, edge cases, correctness
  - **Quality**: Error handling, resource leaks, null checks, code clarity
  - **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
  - **Performance**: Inefficient algorithms, unnecessary operations, resource usage
  - **User Experience**: UI/UX, accessibility, usability

  For each issue found, add a comment using `aiki task comment` with structured data:

  aiki task comment <task-id> \
    --data file=<path> --data line=<line> \
    --data severity=high|medium|low \
    --data category=bug|quality|security|performance|ux \
    "<description of issue, impact, and suggested fix>"

  Add comments as you find issues, don't wait until the end.
data:
  task_id: xqrmnpst
  changes: [zxywtuvs]
---
```

**2. Task Run Starts Agent Session**

The `aiki task run --async` command spawns a background agent session to execute the review task.

**Session event** (recorded in history):
```yaml
---
event: session.started
session: codex-session-review-xqrmnpst
agent: codex
parent_session: claude-session-abc123
timestamp: 2025-01-15T10:04:56Z
context:
  task_id: xqrmnpst
  mode: task_execution
---
```

**Task context** (loaded from `tasks/{task_id}/context.yaml` per [run-task.md](../done/run-task.md)):
```yaml
type: review
instructions: |
  Code review orchestration task
data:
  task_id: xqrmnpst
  template: review
scope_files:
  - src/auth.ts
  - src/middleware.ts
```

The agent (codex) receives the review task context and begins executing subtasks sequentially.

**3. Subtask 1 Started and Completed**

```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.1
event: started
timestamp: 2025-01-15T10:04:52Z
---
aiki_task_event: v1
task_id: xqrmnpst.1
event: closed
outcome: done
timestamp: 2025-01-15T10:04:55Z
---
```

**4. Subtask 2 Started - Agent Begins Review**

The codex agent starts executing the review task:

```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: started
timestamp: 2025-01-15T10:04:56Z
---
```

The agent analyzes the code changes and adds comments for each issue found.

**5. Comments Added During Review**

Comments include both human-readable `text` and structured `data` fields. The `data` field enables template iteration without brittle markdown parsing.

```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: comment_added
timestamp: 2025-01-15T10:05:00Z
text: |
  Potential null pointer dereference when accessing user.name.
  Runtime crash if user object is null from auth middleware.
  Suggested fix: check user && user.name before access.
data:
  file: src/auth.ts
  line: 42
  severity: high
  category: quality
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: comment_added
timestamp: 2025-01-15T10:05:03Z
text: |
  JWT expiration not validated before use.
  Expired tokens may be accepted.
  Suggested fix: check exp claim before accepting token.
data:
  file: src/middleware.ts
  line: 28
  severity: medium
  category: security
---
```

**CLI for adding structured comments:**
```bash
aiki task comment xqrmnpst.2 \
  --data file=src/auth.ts --data line=42 \
  --data severity=high --data category=quality \
  "Potential null pointer dereference when accessing user.name."
```

**6. Subtask 2 Completed**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: closed
outcome: done
timestamp: 2025-01-15T10:05:10Z
---
```

**Comment IDs:** Each comment is stored as a JJ change on the `aiki/tasks` branch. The comment ID is the change_id of that change — stable, unique, and consistent with how all other identifiers work in Aiki. Referenced via `source: comment:<change_id>`.

**Note**: Comments are added as issues are discovered during the review process.

**7. Parent Task Auto-Closes (Synchronous)**

When the last subtask completes, the parent task automatically closes. This happens synchronously inside `task_close()`: when closing a child task, the function checks if all siblings are now closed and, if so, closes the parent in the same operation. This is deterministic—agents can rely on the parent being closed immediately after the last child closes.

```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: closed
outcome: done
timestamp: 2025-01-15T10:05:10Z
---
```

**8. Agent Session Ends**

After the review task completes, the agent session ends:

```yaml
---
event: session.ended
session: codex-session-review-xqrmnpst
timestamp: 2025-01-15T10:05:11Z
---
```

The review task is now in a terminal state, ready to be processed.

**9. Requesting Agent Calls aiki fix**

The requesting agent (which triggered the review) runs:
```bash
aiki wait xqrmnpst | aiki fix
```

The `aiki fix` command reads all comments from the completed task xqrmnpst and creates the followup task + all child tasks in one operation:

```yaml
---
aiki_task_event: v1
task_id: lpqrstwo
event: created
timestamp: 2025-01-15T10:05:12Z
name: "Followup: Review xqrmnpst"
priority: p2  # Inherits from blocked task mxsl (or p1 if no blocked task)
assignee: claude-code  # Set by aiki fix command (opposite of agent who performed review)
instructions: |
  Fix all issues identified in review.
scope:
  files:
    - path: src/auth.ts
    - path: src/middleware.ts
source: task:xqrmnpst
data:
  blocks: [mxsl]  # Blocks originating task if review was of task changes
---
```

**Note**: 
- The `assignee` is set dynamically by `aiki fix` (not in template)
- Task name comes from template: `Followup: {source.name}`
- Uses the `source` field format from [task-change-linkage](../done/task-change-linkage.md). Supports multiple values (one per line), with prefixes: `file:`, `task:`, `comment:`.

**Child task 1:**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo.1
event: created
timestamp: 2025-01-15T10:05:12Z
name: "Fix: quality in src/auth.ts"
priority: p2  # severity=high → same priority as reviewed task
assignee: claude-code  # Inherits from parent task
instructions: |
  **File**: src/auth.ts:42
  **Severity**: high
  **Category**: quality

  Potential null pointer dereference when accessing user.name.
  Runtime crash if user object is null from auth middleware.
  Suggested fix: check user && user.name before access.
data:
  file: src/auth.ts
  line: 42
  severity: high
  category: quality
scope:
  files:
    - path: src/auth.ts
      lines: [42]
source: task:xqrmnpst
source: comment:c1a2b3c4
---
```

**Child task 2:**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo.2
event: created
timestamp: 2025-01-15T10:05:12Z
name: "Fix: security in src/middleware.ts"
priority: p3  # severity=medium → one level lower than reviewed task (p2)
assignee: claude-code  # Inherits from parent task
instructions: |
  **File**: src/middleware.ts:28
  **Severity**: medium
  **Category**: security

  JWT expiration not validated before use.
  Expired tokens may be accepted.
  Suggested fix: check exp claim before accepting token.
data:
  file: src/middleware.ts
  line: 28
  severity: medium
  category: security
scope:
  files:
    - path: src/middleware.ts
      lines: [28]
source: task:xqrmnpst
source: comment:d5e6f7g8
---
```

**Note**:
- Parent task + all child tasks created atomically in one operation via the template system (`create_task_from_template()`)
- No draft flag needed since all tasks are created together
- Each child task carries structured `data` from the original comment (file, line, severity, category)
- `instructions` contains the human-readable comment text
- Priority inherits from blocked task (`mxsl` is p2), defaults to p1 if no blocked task
- If the originating task (`mxsl`) was closed, it should be reopened with reason "Review found issues (task:lpqrstwo)"
- When agent works on these tasks, changes include `task=lpqrstwo.1` in provenance (see [task-change-linkage](../done/task-change-linkage.md))

**Note**:
- Review task was already closed in step 7 (auto-closed when subtasks completed)
- `aiki fix` reads the completed task and creates followup tasks from comments
- If no comments, `aiki fix` still succeeds (exits 0) with "approved" message
- **Issues found**: Count children of followup task

---

## CLI Commands

### Fix Command (Create and Run Followup Tasks)

```bash
aiki fix [<task_id>] [--async] [--start] [--template <name>]
```

**Arguments:**
- `<task_id>` - Review task to process (reads from stdin if not provided)

**Naming rationale:** `fix` is intentionally short for agent ergonomics. By default, it creates followup tasks and runs them to completion — the same pattern as `aiki review` (which creates a review task and runs it). Flags control the execution level. Alternative names considered: `followup` (verbose), `task followup` (too deep).

**Options:**
- `--async` - Create + run followup task asynchronously, return task ID immediately
- `--start` - Create task, then call `aiki task start` — calling agent takes over
- `--template <name>` - Task template to use for followup tasks (default: aiki/fix)
- `--agent <name>` - Agent for task assignment (default: claude-code). Only affects default/`--async` modes.

**Flag progression (mirrors `aiki task` lifecycle):**

| Command | Action | Returns |
|---------|--------|---------|
| `aiki fix` | Creates + runs to completion | Result |
| `aiki fix --async` | Creates + runs async | Task ID |
| `aiki fix --start` | Creates + starts | Control to caller |

**Behavior (default — creates + runs to completion):**
1. Reads task ID from argument or stdin (for piping: `aiki review | aiki fix`)
2. Reads all comments from the specified task
3. If no comments found: prints success message ("approved"), exits 0
4. If comments found:
   - Creates followup task with one subtask per comment
   - **Runs the followup task to completion** (spawns agent or runs inline)
   - Outputs followup task ID to stdout when complete
5. Each subtask has `source` field linking to original comment

**Behavior (`--async` — creates + runs async):**
1-3. Same as default
4. If comments found:
   - Creates followup task with one subtask per comment
   - **Starts the followup task in background**, returns immediately
   - Outputs followup task ID to stdout
5. Each subtask has `source` field linking to original comment

**Behavior (`--start` — creates + starts, agent takes over):**
1-3. Same as default
4. If comments found:
   - Creates followup task with one subtask per comment
   - Calls `aiki task start <followup_id>` — same codepath as running `aiki task start` directly
   - The calling agent takes over the task (regardless of task's `assignee` field)
   - Outputs followup task ID to stdout immediately
5. Each subtask has `source` field linking to original comment


**Output (no issues, stdout):** _(empty — no followup task created, nothing to act on)_

This is a **success case** (exit 0) — the review was processed and approved. Empty stdout follows the `grep` pattern: no match = no output. This is different from "no task ID provided" which is an error (exit 1).

**Output (no issues, stderr):**
```xml
<aiki_fix cmd="fix" status="ok">
  <approved task_id="xqrmnpst">
    Review approved - no issues found.
  </approved>
</aiki_fix>
```

**Output (issues found, stdout when piped):**
```
lpqrstwo
```

**Output (issues found, stderr):**
```xml
<aiki_fix cmd="fix" status="ok">
  <followup task_id="lpqrstwo" issues_found="2" status="started">
    Created and started followup task with 2 subtasks.

    1. Fix: quality in src/auth.ts (p2)
       File: src/auth.ts:42

    2. Fix: security in src/middleware.ts (p3)
       File: src/middleware.ts:28
  </followup>

  <context>
    <in_progress>
      <task id="lpqrstwo" name="Followup: Review xqrmnpst" priority="p2"/>
    </in_progress>
    <list ready="3">
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_fix>
```

### Primary Review Command

```bash
aiki review [<task-id>] [options]
```

**Arguments:**
- `<task-id>` - Optional task ID to review (default: all closed tasks in session)

**Options:**
- `--agent <name>` - Agent for task assignment (default: codex). Only affects default/`--async` modes.
- `--template <name>` - Task template (default: review)
- `--async` - Create + run review task async, return task ID immediately
- `--start` - Create task, then call `aiki task start` — calling agent takes over regardless of `--agent`

**Examples:**
```bash
# Fully autonomous: review + fix completed
aiki review | aiki fix

# Autonomous fix runs in background, returns immediately
aiki review | aiki fix --async

# Agent takes over fixing in current session
aiki review | aiki fix --start

# Agent takes over reviewing in current session
aiki review --start

# Async review, then autonomous fix
aiki review --async | aiki wait | aiki fix

# Review specific task
aiki review xqrmnpst | aiki fix

# Interactive use (no pipe, XML on stderr)
aiki review

# Custom template
aiki review --template myorg/custom-review | aiki fix
```

**Pipeline pattern — build > test > review > fix:**
The `fix` command works the same way regardless of what produced the findings:
```bash
# Build finds errors → fix addresses them
aiki build | aiki fix

# Tests find failures → fix addresses them
aiki test | aiki fix

# Review finds issues → fix addresses them
aiki review | aiki fix
```

**Behavior (default, blocking):**
1. Creates review task with subtasks via `create_task_from_template()` using the review template (assignee: codex)
2. Calls `aiki task run <task_id>` to start the review (waits for completion)
3. Outputs task ID to stdout, structured result to stderr

**Behavior (--async):**
1. Creates review task with subtasks via `create_task_from_template()` using the review template (assignee: codex)
2. Calls `aiki task run <task_id> --async` to start the review
3. Returns immediately — outputs task ID to stdout, status to stderr

**Behavior (--start):**
1. Creates review task with subtasks via `create_task_from_template()` using the review template
2. Calls `aiki task start <task_id>` — same codepath as running `aiki task start` directly
3. The calling agent takes over the task (regardless of task's `assignee` field)
4. Returns control to calling agent — agent performs review itself in current session
5. Outputs task ID to stdout, status to stderr

In all cases, `aiki review` never calls `fix`. The agent pipes or chains `fix` separately.

**Note:** `aiki review | aiki wait | aiki fix` works but `wait` is redundant — blocking `review` already waits for completion before outputting the task ID. Use `wait` only with `--async`.

**Output (stdout when piped):**
```
xqrmnpst
```

**Output (stderr, blocking):**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" comments="2">
    Review completed with 2 comments.
  </completed>
</aiki_review>
```

**Output (stderr, --async):**
```xml
<aiki_review cmd="review" status="ok">
  <started task_id="xqrmnpst">
    Review started in background.
  </started>
</aiki_review>
```

### Review History

```bash
aiki review list
```

**Behavior:** Convenience command that queries review tasks. Equivalent to `aiki task list --all --filter 'name~"^Review:"'` with specialized output format.

```xml
<aiki_review cmd="list" status="ok">
  <reviews>
    <!-- outcome and issues_found derived from task relationships -->
    <review task_id="xqrmnpst" outcome="rejected" issues_found="2" 
            timestamp="2025-01-15T10:05:00Z" reviewer="codex"/>
    <review task_id="pqrstuv" outcome="approved" issues_found="0"
            timestamp="2025-01-14T14:20:00Z" reviewer="codex"/>
  </reviews>
</aiki_review>
```

### Review Details

```bash
aiki review show xqrmnpst
```

**Behavior:** Shows review task details, comments, and links to followup task (equivalent to `aiki task show xqrmnpst`)

```xml
<aiki_review cmd="show" status="ok">
  <review task_id="xqrmnpst">
    <reviewer>codex</reviewer>
    <requested_by>claude-code</requested_by>  <!-- Derived from change metadata -->
    <changes>[zxywtuvs]</changes>
    <outcome>rejected</outcome>  <!-- Derived: followup_task present -->
    <issues_found>2</issues_found>  <!-- Derived: count children of followup_task -->
    <duration_ms>9000</duration_ms>
    <comments>
      <comment timestamp="2025-01-15T10:04:55Z">**File**: src/auth.ts:42...</comment>
      <comment timestamp="2025-01-15T10:04:58Z">**File**: src/middleware.ts:28...</comment>
    </comments>
    <followup_task id="lpqrstwo" name="Followup: Review xqrmnpst">
      <children>
        <task id="lpqrstwo.1" name="Fix: quality in src/auth.ts" priority="p2"/>
        <task id="lpqrstwo.2" name="Fix: security in src/middleware.ts" priority="p3"/>
      </children>
    </followup_task>
  </review>
</aiki_review>
```

---

## Task List Behavior

```bash
aiki task list
```

Shows open tasks (review tasks appear if open/in-progress):

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="4">
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_task>
```

Include closed tasks:

```bash
aiki task list --all
```

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="6">
      <task id="xqrmnpst" name="Review: changes @" assignee="codex" status="closed"/>
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
      <task id="pqrstuv" name="Review: main..@" assignee="codex" status="closed"/>
    </list>
  </context>
</aiki_task>
```

---

## Flow Integration

### review: Flow Action

The `review:` flow action is a thin wrapper around the `aiki review` CLI command. Internally, `review: { task_id: X, template: Y }` is equivalent to `aiki review X --template Y --async`. Flows always use background mode since they can't block the agent. This means flow actions can be tested by running the equivalent CLI command directly.

Flows trigger reviews and return a prompt to the agent:

```yaml
# Background review triggered on task completion
# Reviews the completed task
task.closed:
  - review:
      task_id: $event.task.id
      agent: codex
      template: review
```

**What the flow does:**
1. Creates review task with subtasks
2. Runs `aiki review <id>` (creates + starts review task)
3. Returns prompt to agent with instructions

**Prompt returned to agent:**
```
A code review has been started (task: xqrmnpst).

When ready to process review findings, run:
aiki wait xqrmnpst | aiki fix
```

### Flow Action Options

```yaml
# Review completed task
task.closed:
  - review:
      task_id: $event.task.id

# Security review for auth-related tasks
task.closed:
  - if: $event.files | contains("src/auth.ts")
    then:
      - review:
          task_id: $event.task.id
          template: security
```

### Agent Workflow After Review Trigger

When a flow triggers a review, the agent decides when to wait and fix:

```bash
# Pipe: wait for review, then process findings
aiki wait xqrmnpst | aiki fix

# Check review status without blocking
aiki task show xqrmnpst

# Stop a running task
aiki task stop xqrmnpst
```

### Task Lifecycle Events with Sugar

Review and fix tasks use the **syntactic sugar system** (see [task-events.md](task-events.md)) for concise, readable triggers:

#### `review.started`

Sugar for `task.started` where `type == "review"`:

```yaml
review.started:
  - log: "Review started: ${event.task.name}"
  - prompt: |
      A code review is running (task: ${event.task.id}).

      When ready to process findings:
      aiki wait ${event.task.id} | aiki fix
```

#### `review.completed`

Sugar for `task.closed` where `type == "review"` AND `outcome == "done"`:

```yaml
review.completed:
  - log: "Review completed: ${event.task.name}"
  - run: aiki fix ${event.task.id}
```

#### Tracking Followup Tasks

Followup tasks created by `aiki fix` are generic tasks — they don't have a special type. To react to them in flows, filter by their `source` field:

```yaml
task.closed:
  - if: $event.task.source | startswith("task:")
    then:
      - log: "Followup task completed: ${event.task.name}"
```

#### Handling Non-Success Outcomes

For rare cases where you need to react to any closure (including `wont_do`), use the base `task.closed` event with an explicit filter:

```yaml
task.closed:
  - if: $event.task.type == "review" && $event.task.outcome == "wont_do"
    then:
      - log: "Review was declined: ${event.task.name}"
```

**Event payload** (standard task event — see [task-events.md](task-events.md)):
```json
{
  "task": {
    "id": "xqrmnpst",
    "name": "Review: changes @",
    "type": "review",
    "status": "closed",
    "outcome": "done",
    "assignee": "codex"
  }
}
```

### Custom Flow for Security Reviews

```yaml
# Trigger security review on auth file changes
task.closed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - review:
          task_id: $event.task.id
          template: security
      - prompt: |
          ⚠️ Security-sensitive files changed. A security review is running.

          You MUST wait for and address the review before continuing:
          aiki wait ${review.task_id} | aiki fix
```


---

## Review Loop Pattern

The **review-loop** pattern enables iterative review-fix-review cycles until code is approved. This is **not a special mode** — it's simply a hook wiring pattern that users create themselves.

### How It Works

```
┌──────────────────────────────────────────────────────────────┐
│  1. Review triggers on turn.completed                        │
│     • Flow runs: aiki review $task_id --async                │
│     • Autoreply instructs agent to wait and fix              │
│     • If issues found, fix creates followup task             │
│     • Followup task has source: task:<review_id>             │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  2. Agent fixes issues                                       │
│     • Works on followup task                                 │
│     • Closes followup task when done                         │
│     • Triggers turn.completed event                          │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  3. Flow triggers re-review                                  │
│     • Same hook fires again on turn.completed                │
│     • Cycle repeats until review finds no issues             │
└──────────────────────────────────────────────────────────────┘
```

### Task Relationships via JJ

No special metadata needed — relationships are queryable from jj:

- **Followup → Review**: The `source: task:<id>` field links followup to its review
- **Changes → Task**: Provenance includes `task=<id>` in change descriptions
- **Task → Changes**: Query with `jj log -r 'description("task=<id>")'`

The flow can use the `source` field to find the original task being reviewed, rather than duplicating that info in custom metadata.

### Example Hook Configuration

Using sugar triggers for clean, readable flows:

```yaml
# .aiki/hooks/review-loop.yml
name: "review-loop"
version: "1"

# Trigger async review on turn completion
turn.completed:
  - if: $event.task.type != "review"
    then:
      - review:
          task_id: $event.task.id
          agent: codex
      - autoreply: |
          Review started (task: ${review.task_id}).
          
          Run: aiki wait ${review.task_id} | aiki fix
```

The agent waits for the review, runs fix, works on followup tasks, and when the turn completes, the cycle repeats. The loop terminates naturally when a review finds no issues (no followup task created).

### Preventing Infinite Loops

Add conditions to limit iterations:

```yaml
turn.completed:
  - if: $event.task.source | length < 5  # Max 5 levels of followup
    then:
      - review:
          task_id: $event.task.id
          agent: codex
      - autoreply: |
          Review started. Run: aiki wait ${review.task_id} | aiki fix
```

Or track iteration count in the flow's condition logic based on the `source` chain depth.

---

## Use Cases

### Use Case 1: Pre-Commit Review

```yaml
# .aiki/flows/default.yml
name: "default"
version: "1"

commit_message.started:
  - log: "Running pre-commit review of session..."
  - review:
      template: review  # Reviews all closed tasks in session by default
  
  - if: $review.issues_found > 0
    then:
      - block: |
          ❌ Review failed with ${review.issues_found} issue(s).
          
          Followup tasks created. Run `aiki task list` to see them.
```

### Use Case 2: Security Review on Auth Changes

```yaml
# .aiki/hooks/security-review.yml
name: "security-review"
version: "1"

task.closed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - log: "Security-sensitive task completed, triggering review..."
      - review:
          task_id: $event.task.id
          template: security
      - if: $review.issues_found > 0
        then:
          - block: |
              🚨 SECURITY REVIEW FOUND ISSUES

              Found ${review.issues_found} security issue(s).
              Run `aiki fix ${review.task_id}` to address them.
```

### Use Case 3: Session Review Before Commit

```yaml
# .aiki/flows/session-review.yml
name: "session-review"
version: "1"

turn.completed:
  - if: $event.session.modified_files_count > 5
    then:
      - log: "Running session review before commit..."
      - review:
          template: review  # Reviews all closed tasks in session by default
      
      - if: $review.issues_found > 0
        then:
          - log: "⚠️  Found ${review.issues_found} issue(s) in session review"
```

### Use Case 4: Pre-Push Review Gate

```yaml
# .aiki/hooks/pre-push.yml
name: "pre-push"
version: "1"

shell.permission_asked:
  - if: $event.command | contains("git push")
    then:
      - log: "Running pre-push review..."
      - review:
          template: review  # Reviews all closed tasks in session

      - if: $review.issues_found > 0
        then:
          - block: |
              ❌ Cannot push - review found issues

              Run `aiki fix ${review.task_id}` to address findings.
```

---

## Implementation

### Fix Command

Wrapper around the template system. Uses `subtasks` iteration (see [declarative-subtasks.md](declarative-subtasks.md)).

**Behavior:**
1. Read comments from the review task specified by `<task_id>`
2. If no comments: print "approved" message and exit 0
3. Determine assignee for followup task:
   - If `--agent` provided: use that agent
   - Otherwise: determine agent that performed the review
   - Default to opposite agent: `claude-code` → `codex`, `codex` → `claude-code`
   - Fallback if unable to determine: `claude-code`
4. Create followup task from template (default: `aiki/fix`, overridable with `--template`)
   - Template receives `source.comments` array for `subtasks` iteration
   - Command sets `assignee` on created task to the determined agent (step 3)
5. Run followup task (default: to completion, `--async`: async, `--start`: hand off)

**Template support:**
- Default template: `aiki/fix` (resolved from `.aiki/templates/aiki/fix.md`)
- Custom templates: `--template myorg/custom-fix` (resolved from `.aiki/templates/myorg/custom-fix.md`)
- Template receives `source.comments` array with comment data for `subtasks` iteration

**Template:** `.aiki/templates/aiki/fix.md`
```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {source.name}

Fix all issues identified in review.

# Subtasks

## {text}

**File**: {data.file}:{data.line}
**Severity**: {data.severity}

{text}
```

### Review Command

**Behavior:**
1. Determine reviewer agent:
   - If `--agent` provided: use that agent
   - Otherwise: determine agent that completed the task being reviewed
   - Default to opposite agent: `claude-code` → `codex`, `codex` → `claude-code`
   - Fallback if unable to determine: `codex`
2. Determine scope to review:
   - If task-id provided: review that specific task
   - Otherwise: review all closed tasks in current session (default)
3. Build metadata (task_id or session, changes, template)
4. Load task template (user custom or aiki: default is `review`)
5. Create review task from template (parent + subtasks defined in template)
6. Set `assignee` on created task to the determined reviewer agent (step 1)
7. If `--async`: start task in background and return immediately
8. Otherwise: start task and wait for completion, then return result
9. In both cases, agent calls `aiki fix` separately to process findings

**Template Loading:**
- Templates use namespace prefixes: `aiki/review` or `myorg/custom-review`
- Built-in templates in `.aiki/templates/aiki/{name}.md`
- Custom templates in `.aiki/templates/{namespace}/{name}.md` (e.g., `.aiki/templates/myorg/custom-review.md`)
- Default template is `aiki/review` (resolved from `.aiki/templates/aiki/review.md`)

**Task-aware commands for templates:**
- Templates instruct agents to use `aiki task show --with-source` and `aiki task diff`
- See [task-diff.md](task-diff.md) for diff command details
- See [with-source.md](with-source.md) for source expansion details

**Helper Functions:**
- `create_task_from_template()` - See [declarative-subtasks.md](declarative-subtasks.md)

---

## Task-Change Linkage Integration

The review system integrates with the bidirectional task-change linkage design (see [task-change-linkage.md](../done/task-change-linkage.md)):

### Direction 1: Change → Task (Provenance)

When agents make code changes while working on review followup tasks, the changes include task context in provenance:

```
[aiki]
author=claude
session=abc123
tool=Edit
task=lpqrstwo.1
[/aiki]
```

This enables:
- **Query by task**: `jj log -r 'description("task=lpqrstwo.1")'` shows all changes for that fix
- **Context in history**: See which task a change was made for
- **Audit trail**: Track which work was done for which review issue

### Direction 2: Task → Source

Review followup tasks include `source` fields to track lineage:

| Source Prefix | Meaning | Example |
|---------------|---------|---------|
| `task:` | Parent task (e.g., code review) | `source: task:xqrmnpst` |
| `comment:` | Specific comment within a task | `source: comment:c1a2b3c4` |

This enables:
- **Traceability**: Answer "why does this task exist?"
- **Review lineage**: Link followup tasks to the code review that found issues
- **Querying**: `aiki task list --source task:xqrmnpst` shows all followup tasks from a review

---

## Benefits of Task-Based Approach

### 1. Single Storage System

**Everything on `aiki/tasks`**:
- Review tasks with `assignee: codex` (ready to run immediately)
- Followup tasks created atomically with all children (no draft flag needed)
- User tasks with `draft: false` (or absent)
- Single event structure for all tasks
- No separate review event schema

### 2. Consistent Task Lifecycle

**Reviews use standard task events**:
- `created`, `started`, `comment_added`, `closed`
- Same infrastructure as regular tasks
- No special-case handling needed

### 3. Natural Visibility Control

**Status-based filtering (same for all tasks)**:
- `aiki task list` - Shows open tasks (including open review tasks)
- `aiki task list --all` - Shows all tasks (including closed review tasks)
- `aiki review list` - Convenience command for review-specific output format
- No special-case filtering based on task type

### 4. Simpler Implementation

**Single code path**:
- Reuse existing task storage/query infrastructure
- Reuse task comment system
- Reuse task relationship tracking
- No special filtering logic needed

### 5. Future-Proof

**Draft tasks can be used for**:
- Code reviews (current use case)
- Work-in-progress task creation (multi-step atomic operations)
- Automated testing workflows
- Documentation generation tasks
- Any task that shouldn't be visible until ready

All with the same infrastructure.

---

## Implementation Plan

### Prerequisites

**Implemented infrastructure:**

| Component | Document | Implementation |
|-----------|----------|----------------|
| `AgentRuntime` trait | [run-task.md](../done/run-task.md) | `agents/runtime/mod.rs` |
| `aiki task run <id>` | [run-task.md](../done/run-task.md) | `commands/task.rs` |
| `sources` field on tasks | [task-change-linkage.md](../done/task-change-linkage.md) | `tasks/types.rs` |
| `task=` in provenance | [task-change-linkage.md](../done/task-change-linkage.md) | `jj/mod.rs` |
| Template resolver | [task-templates.md](../done/task-templates.md) | `tasks/templates/resolver.rs` |
| Background task execution (`--async`, `wait`, `stop`) | [background-run.md](background-run.md) | `commands/wait.rs`, `tasks/runner.rs:298` |
| Structured comment metadata (`--data key=value`) | [comment-metadata.md](comment-metadata.md) | `tasks/types.rs:177`, `commands/task.rs:109` |
| Declarative subtasks (`subtasks`) | [declarative-subtasks.md](declarative-subtasks.md) | `tasks/templates/resolver.rs:234`, `tasks/templates/data_source.rs` |
| Task lifecycle events (`task.started`, `task.closed`) | [task-events.md](task-events.md) | `events/task_started.rs`, `events/task_closed.rs` |
| Sugar triggers (`{type}.started`, `{type}.completed`) | [task-events.md](task-events.md) | `flows/sugar.rs` |
| Lazy loading for event payloads | [lazy-load-payloads.md](lazy-load-payloads.md) | `flows/variables.rs:69`, `flows/engine.rs:365` |

**Planned infrastructure (needed for review templates):**

| Component | Document | Status |
|-----------|----------|--------|
| `aiki task diff <id>` | [task-diff.md](task-diff.md) | Not implemented |
| `<files_changed>` in `aiki task show` | [task-diff.md](task-diff.md) | Not implemented |
| `aiki task show --with-source` | [with-source.md](with-source.md) | Not implemented |

### Phase 1: Fix Command

**Deliverables:**
- `aiki fix <task_id>` command - create followup tasks and run to completion (default)
- `--async` flag - create and run async, return task ID immediately
- `--start` flag - create and start, return control to calling agent
- `--template <name>` flag - use custom template (default: aiki/fix)
- `--agent <name>` flag - agent for task assignment (default: claude-code)
- Read comments from task, create followup task with subtasks
- Graceful handling when no comments (success, not error)

**Files:**
- `cli/src/commands/fix.rs` - Fix command implementation
- `cli/src/commands/mod.rs` - Export fix module

### Phase 2: Review Command

**Deliverables:**
- `aiki review [<task-id>]` CLI command with all options
- Review scope support (session (default), task)
- `--template` flag for review templates (default: `review`)
- `--async` flag for async review
- `--start` flag for agent-takes-over review
- Add `type` field to TaskEvent schema (to identify review tasks)

**Files:**
- `cli/src/commands/review.rs` - Review command implementation
- `cli/src/tasks/types.rs` - Add `type` field

### Phase 3: Flow Integration

**Deliverables:**
- `review:` flow action (wraps `aiki review` CLI)
- Set `type: review` when creating review tasks

**Note:** Sugar triggers (`review.started`, `review.completed`) are already implemented in `flows/sugar.rs`.

**Files:**
- `cli/src/flows/types.rs` - Add `Review` action variant
- `cli/src/flows/engine.rs` - Add `execute_review()` handler
- `cli/src/commands/review.rs` - Set `type: review` on created tasks

### Phase 4: Review Queries

**Deliverables:**
- `aiki review list` - List review tasks (convenience wrapper)
- `aiki review show <id>` - Show review details with comments

**Files:**
- `cli/src/commands/review.rs` - Add list/show subcommands

### Phase 5: Documentation

**Deliverables:**
- Update AGENTS.md template
- Document `aiki review` and `aiki fix` commands
- Add pipeline examples

---

## Summary

This task-based design unifies reviews and regular tasks with a composable, async CLI:

- **Single storage system** - All tasks on `aiki/tasks` branch
- **Consistent lifecycle** - Reviews use same events as regular tasks
- **Pipeable CLI** - `aiki review | aiki fix` or `aiki review --async | aiki wait | aiki fix`
- **Async option** - `--async` lets reviews run without blocking the agent
- **Simple orchestration** - Agent pipes: `aiki review | aiki fix` (autonomous), `--async` (async), or `--start` (agent takes over)
- **Graceful handling** - `aiki fix` succeeds with no error when no issues found
- **Bidirectional linkage** - Changes include `task=` in provenance, tasks include `source` for lineage

### Key Commands

| Command | Purpose | stdin | stdout |
|---------|---------|-------|--------|
| `aiki review [<task-id>] [--async] [--start]` | Create + run review task (`--async` async, `--start` hand off) | — | task ID |
| `aiki wait [<id>]` | Block until task completes | task ID | task ID (passthrough) |
| `aiki fix [<id>] [--async] [--start]` | Create + run followup tasks (`--async` async, `--start` hand off) | task ID | result / followup task ID |
| `aiki task run <id> --async` | Start task, return immediately | — | — |
| `aiki task stop <id>` | Stop a running task | — | — |

**Pipe patterns:**
```bash
aiki review | aiki fix                            # fully autonomous: review + fix completed
aiki review | aiki fix --async               # autonomous fix runs async, returns immediately
aiki review | aiki fix --start                    # review complete, agent takes over fixing
aiki review --start                               # agent performs review itself
aiki review --async | aiki wait | aiki fix   # async review + wait + autonomous fix
```

### Related Documents

| Document | Purpose |
|----------|---------|
| [run-task.md](../done/run-task.md) | `AgentRuntime` trait, `aiki task run` command |
| [task-change-linkage.md](../done/task-change-linkage.md) | Provenance `task=` field, task `source` field |

This composable design lets agents control when to wait for reviews and process findings with a simple, consistent workflow.

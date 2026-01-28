# Code Review System: Task-Based Design

**Date**: 2026-01-10
**Updated**: 2026-01-24
**Status**: Proposed Architecture
**Purpose**: Reviews as system tasks on `aiki/tasks`

**Related Documents**:
- [Task Execution: aiki task run](../done/run-task.md) - Agent runtime and task execution
- [Task-to-Change Linkage](../done/task-change-linkage.md) - Bidirectional task/change tracking

---

## Executive Summary

This design uses **tasks for everything** with an **async, composable CLI**:

1. **Review tasks** - Regular tasks with subtasks that agents execute to analyze code
2. **Followup tasks** - User-visible tasks created by `aiki fix` from review comments
3. **Single storage branch** - All tasks on `aiki/tasks` branch
4. **Async workflow** - `--background` returns immediately, `wait` blocks until done
5. **Consistent task lifecycle** - Reviews use same event structure as other tasks
6. **Pipeable commands** - `aiki review | aiki fix` and `aiki review --background | aiki wait | aiki fix`
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
2. **aiki fix creates followups**: Reads review comments and creates followup tasks atomically
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
aiki review --background | aiki wait | aiki fix
```

In interactive (TTY) mode, stdout is empty and users see only the structured XML on stderr. In piped mode, stdout emits the task ID for the next command while stderr remains visible to the user (since pipes only capture stdout). TTY detection uses `std::io::stdout().is_terminal()` (Rust 1.70+).

**Empty stdin:** If `wait` or `fix` receives no task ID (neither argument nor stdin), it exits with code 1 and prints "no task ID provided" to stderr.

**Error propagation:** Unix pipes don't short-circuit by default — if `wait` times out (exit 2), `fix` still runs and receives empty stdin, then errors. For agents that want short-circuit behavior, use `set -o pipefail` or `&&` chaining instead:

```bash
# Pipe (data flow) — natural for success path
aiki review | aiki fix

# && (error handling) — stops on failure
ID=$(aiki review) && aiki fix "$ID"

# || (fallback) — handle timeout
aiki wait xqrmnpst --timeout 60s || aiki task stop xqrmnpst
```

### Async Review Flow

```
┌──────────────────────────────────────────────────────────────┐
│  1. Flow hook triggers review                                 │
│     • Creates review task with subtasks                       │
│     • Runs: aiki review --background                           │
│     • Returns prompt to agent immediately                     │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  2. Background: Codex agent executes review                   │
│     • Digests code changes                                    │
│     • Reviews for issues                                      │
│     • Adds comments via aiki task comment                     │
│     • Closes task when done                                   │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  3. Requesting agent waits and fixes                          │
│     • aiki wait <id>  - blocks until complete                 │
│     • aiki fix <id>   - creates followup from comments        │
└──────────────────────────────────────────────────────────────┘
```

### Review Scopes

Reviews can target different scopes of changes:

- **session** - All closed tasks in current session (default)
  - `aiki review` - Review all closed tasks in session
  - If no closed tasks exist: succeeds with "nothing to review" message, exits 0 (consistent with `fix` graceful handling)
- **task** - Changes associated with a specific task
  - `aiki review <task-id>` - Review specific task by ID
- **changes** - JJ changes by revset
  - `aiki review --changes @` - Review working copy change
  - `aiki review --changes trunk()..@` - Review range of changes

**Future scope ideas:**
- **files** - Specific file paths (`--files <paths>`)
- **active** - Active task currently being worked on (`--active`)
- **staged** - Files staged for commit (Git interop)

### Review Templates

The default review template covers general code quality:

- **`review`** (default) - General code quality, functionality, security, and performance
  - Subtasks: Digest code changes, Review code
  - Resolved from `.aiki/templates/aiki/review.md`

**Custom templates:**
- Users can create custom templates in `.aiki/templates/{namespace}/` (e.g., `.aiki/templates/myorg/`)
- Future: Additional specialized templates (security, performance, style)

Templates define the full task structure (parent + subtasks + instructions). See [task-templates.md](task-templates.md) for details.

### Task Types

**Review tasks**:
- Created by `aiki review --changes @` with 2 subtasks (digest, review)
- Assigned to reviewer agent (e.g., `codex`)
- Agent adds comments during review
- Closed automatically when agent completes review (parent auto-closes when all subtasks done)
- Supports different scopes and prompts

**Followup tasks**:
- Created atomically from review comments (parent + all children in one operation)
- Each child task has `source` field referencing specific comment (e.g., `source: task:xqrmnpst`, `source: comment:c1a2b3c4`)
- Visible immediately (no draft flag needed)
- Normal user tasks that can be worked on
- Changes made while working on these tasks include `task=` in provenance (see [task-change-linkage](../done/task-change-linkage.md))

---

## Data Model: Review Tasks

### Review Task Lifecycle

**User/flow runs:**
```bash
aiki review --changes @
```

This internally executes:
1. `create_task_from_template()` - Creates parent task + 2 subtasks atomically from the review template (digest, review)
2. `aiki task run xqrmnpst --background` - Starts the review task and returns immediately
3. Returns task ID to caller for later processing

**Then agent processes:**
```bash
aiki wait xqrmnpst | aiki fix
```

This:
1. Waits for review task to complete (review task auto-closes when all subtasks done)
2. Reads comments from the completed review task
3. Creates followup task with subtasks (if issues found)

**1. Review Task Created** (parent task with children)
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: created
type: review
timestamp: 2025-01-15T10:04:50Z
name: "Review: changes @"
assignee: codex
instructions: |
  Code review orchestration task
  
  This task coordinates review steps.
data:
  task_id: xqrmnpst
  changes: [zxywtuvs]
---
```

**Note**: 
- Created with 2 subtasks atomically using `--subtasks` pattern (digest, review)
- This is a parent orchestration task with sequential subtasks
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
assignee: codex
instructions: |
  Examine the code changes to understand what was modified.
  
  Commands to use:
  - `jj diff --revision @` - Show full diff of working copy
  - `jj show @` - Show change description and summary
  - `jj log -r @` - Show change in log context
  
  Summarize:
  - What files were changed
  - What functionality was added/modified
  - The scope and intent of the changes
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
assignee: codex
instructions: |
  Review the code changes for functionality, quality, security, and performance.
  
  Focus on:
  - **Functionality**: Logic errors, edge cases, correctness
  - **Quality**: Error handling, resource leaks, null checks, code clarity
  - **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
  - **Performance**: Inefficient algorithms, unnecessary operations, resource usage
  
  For each issue found, add a comment using `aiki task comment` with structured data:

  aiki task comment --id <task-id> \
    --data file=<path> --data line=<line> \
    --data severity=error|warning|info \
    --data category=functionality|quality|security|performance \
    "<description of issue, impact, and suggested fix>"

  Add comments as you find issues, don't wait until the end.
data:
  task_id: xqrmnpst
  changes: [zxywtuvs]
---
```

**2. Task Run Starts Agent Session**

The `aiki task run --background` command spawns a background agent session to execute the review task.

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
  severity: error
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
  severity: warning
  category: security
---
```

**CLI for adding structured comments:**
```bash
aiki task comment --id xqrmnpst.2 \
  --data file=src/auth.ts --data line=42 \
  --data severity=error --data category=quality \
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
name: "Followup: JWT authentication review"
priority: p2  # Inherits from blocked task mxsl (or p1 if no blocked task)
assignee: claude-code
instructions: |
  Code review completed by codex (task:xqrmnpst)

  Found 2 issues requiring fixes.
  Start this task to scope to review issues only.
scope:
  files:
    - path: src/auth.ts
    - path: src/middleware.ts
source: task:xqrmnpst
data:
  blocks: [mxsl]  # Blocks originating task if review was of task changes
---
```

**Note on source field**: Uses the `source` field format from [task-change-linkage](../done/task-change-linkage.md). Supports multiple values (one per line), with prefixes: `file:`, `task:`, `comment:`.

**Child task 1:**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo.1
event: created
timestamp: 2025-01-15T10:05:12Z
name: "Fix: Null pointer check in auth.ts"
priority: p0
assignee: claude-code
instructions: |
  Potential null pointer dereference when accessing user.name.
  Runtime crash if user object is null from auth middleware.
  Suggested fix: check user && user.name before access.
data:
  file: src/auth.ts
  line: 42
  severity: error
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
name: "Fix: JWT expiration validation"
priority: p1
assignee: claude-code
instructions: |
  JWT expiration not validated before use.
  Expired tokens may be accepted.
  Suggested fix: check exp claim before accepting token.
data:
  file: src/middleware.ts
  line: 28
  severity: warning
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

### Task Run with Background Flag

```bash
aiki task run <task_id> [--background]
```

**Options:**
- `--background` - Start task execution and return immediately (don't wait for completion)

**Behavior:**
- Without `--background`: Blocks until task completes (existing behavior)
- With `--background`: Spawns agent, prints task ID, returns immediately

**Output (--background):**
```xml
<aiki_task cmd="run" status="ok">
  <started task_id="xqrmnpst" background="true">
    Review task started in background.

    To wait for completion and process findings:
    aiki wait xqrmnpst | aiki fix
  </started>
</aiki_task>
```

### Wait Command

```bash
aiki wait [<task_id>] [--timeout <duration>]
```

**Arguments:**
- `<task_id>` - Task ID to wait for (reads from stdin if not provided)

**Options:**
- `--timeout <duration>` - Maximum time to wait (e.g., `30s`, `5m`). Default: no limit.

**Behavior:**
- Reads task ID from argument or stdin (for piping: `aiki review --background | aiki wait`)
- Blocks until the task reaches a terminal state (closed, stopped, or failed)
- Uses exponential backoff polling: 100ms → 200ms → 400ms → ... → 2s max
- Outputs task ID to stdout (passes through for next pipe stage)
- Exit code 0 if task completed successfully
- Exit code 1 if task failed or was stopped
- Exit code 2 if timeout reached

**Output (stdout when piped):**
```
xqrmnpst
```

**Output (stderr):**
```xml
<aiki_wait cmd="wait" status="ok">
  <completed task_id="xqrmnpst" outcome="done" duration_ms="45000">
    Task completed successfully.
  </completed>
</aiki_wait>
```

### Fix Command (Create and Start Followup Tasks)

```bash
aiki fix [<task_id>]
```

**Arguments:**
- `<task_id>` - Review task to process (reads from stdin if not provided)

**Naming rationale:** `fix` is intentionally short for agent ergonomics. It doesn't perform fixes—it creates and starts followup tasks that describe fixes needed. Think of it as "create the fix list and begin work." Alternative names considered: `followup` (verbose), `task followup` (too deep).

**Behavior:**
1. Reads task ID from argument or stdin (for piping: `aiki review | aiki fix`)
2. Reads all comments from the specified task
3. If no comments found: prints success message ("approved"), exits 0
4. If comments found:
   - Creates followup task with one subtask per comment
   - **Automatically starts the followup task**
   - Outputs followup task ID to stdout
5. Each subtask has `source` field linking to original comment

**Output (no issues, stdout):** _(empty — no followup task created, nothing to act on)_

Empty stdout follows the `grep` pattern: no match = no output. Downstream commands receiving empty stdin should exit 0 as a no-op.

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

    1. Fix: Null pointer check in auth.ts (p0)
       File: src/auth.ts:42

    2. Fix: JWT expiration validation (p1)
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
- `--changes <revset>` - Review JJ changes by revset (e.g., `@`, `trunk()..@`)
- `--from <agent>` - Reviewer agent (default: codex)
- `--template <name>` - Task template (default: review)
- `--background` - Start review and return immediately (don't wait for completion)

**Examples:**
```bash
# Review and fix in one pipeline (blocking)
aiki review | aiki fix

# Background review with async wait
aiki review --background | aiki wait | aiki fix

# Review specific task
aiki review xqrmnpst | aiki fix

# Review working copy change
aiki review --changes @ | aiki fix

# Review range of changes
aiki review --changes "trunk()..@" | aiki fix

# Interactive use (no pipe, XML on stderr)
aiki review

# Custom template
aiki review --template myorg/custom-review | aiki fix
```

**Behavior (default, blocking):**
1. Creates review task with subtasks via `create_task_from_template()` using the review template (assignee: codex)
2. Calls `aiki task run <task_id>` to start the review (waits for completion)
3. Outputs task ID to stdout, structured result to stderr

**Behavior (--background):**
1. Creates review task with subtasks via `create_task_from_template()` using the review template (assignee: codex)
2. Calls `aiki task run <task_id> --background` to start the review
3. Returns immediately — outputs task ID to stdout, status to stderr

In both cases, `aiki review` never calls `fix`. The agent pipes or chains `fix` separately.

**Note:** `aiki review | aiki wait | aiki fix` works but `wait` is redundant — blocking `review` already waits for completion before outputting the task ID. Use `wait` only with `--background`.

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

**Output (stderr, --background):**
```xml
<aiki_review cmd="review" status="ok">
  <started task_id="xqrmnpst">
    Review started in background.
  </started>
</aiki_review>
```

**Note**: Completed review tasks are hidden by default (filtered by type). Use `aiki review list` to see review history.

### Review History

```bash
aiki review list
```

**Behavior:** Queries completed review tasks (filters by task name pattern or discovered_from links)

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
    <followup_task id="lpqrstwo" name="Followup: JWT auth review">
      <children>
        <task id="lpqrstwo.1" name="Fix: Null pointer check" priority="p0"/>
        <task id="lpqrstwo.2" name="Fix: JWT validation" priority="p1"/>
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

Shows user tasks only (completed review tasks filtered by default):

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

Include all tasks (review tasks):

```bash
aiki task list --all
```

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="6">
      <task id="xqrmnpst" name="Review: changes @" type="review" assignee="codex" status="closed"/>
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
      <task id="pqrstuv" name="Review: main..@" type="review" assignee="codex" status="closed"/>
    </list>
  </context>
</aiki_task>
```

---

## Flow Integration

### review: Flow Action

The `review:` flow action is a thin wrapper around the `aiki review` CLI command. Internally, `review: { task_id: X, template: Y }` is equivalent to `aiki review X --template Y --background`. Flows always use background mode since they can't block the agent. This means flow actions can be tested by running the equivalent CLI command directly.

Flows trigger reviews and return a prompt to the agent:

```yaml
# Background review triggered on task completion
# Reviews the completed task
task.closed:
  - review:
      task_id: $event.task.id
      from: codex
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

Continue with your current work - the review runs in parallel.
```

### Flow Action Options

```yaml
# Simple session review (default: all closed tasks)
turn.completed:
  - review: {}

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

# Wait with timeout, stop if too slow
aiki wait xqrmnpst --timeout 60s || aiki task stop xqrmnpst
```

### review.started Event

Fires when a review starts:

```yaml
review.started:
  - log: "Review started: ${event.review.task_id}"
  - prompt: |
      A code review is running (task: ${event.review.task_id}).

      When ready to process findings:
      aiki wait ${event.review.task_id} | aiki fix
```

**Event payload:**
```json
{
  "review": {
    "task_id": "xqrmnpst",
    "reviewer": "codex",
    "reviewed_task_id": "lpqrstwo",
    "template": "default"
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

The **review-loop** pattern enables iterative review-fix-review cycles until code is approved.

### How Review Loops Work

```
┌──────────────────────────────────────────────────────────────┐
│  1. Initial Review (aiki review --changes @ --loop)                    │
│     • Creates review task xqrmnpst                           │
│     • Finds 2 issues                                         │
│     • Creates followup task lpqrstwo with metadata:          │
│       review_loop_enabled: true                              │
│       review_loop_parent: xqrmnpst                           │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  2. Developer Fixes Issues (aiki task start lpqrstwo)        │
│     • Completes child tasks lpqrstwo.1, lpqrstwo.2           │
│     • Closes followup task lpqrstwo                          │
│     • Triggers task.closed event                             │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  3. Automatic Re-review (triggered by flow)                  │
│     • Flow detects review_loop_enabled in metadata           │
│     • Runs: aiki review --changes @ --from codex                       │
│     • Reviews only the fixed files                           │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  4. Outcome                                                  │
│     a) APPROVED: No issues → loop ends                       │
│     b) REJECTED: New issues → create followup, repeat        │
└──────────────────────────────────────────────────────────────┘
```

### Review Loop Metadata

Followup tasks created from `--loop` reviews include:

```yaml
# Top-level fields (not nested in metadata)
source: task:xqrmnpst

# Data for loop control
data:
  review_loop_enabled: true
  review_loop_parent: xqrmnpst  # Original review task
  review_loop_iteration: 1
  review_loop_task_id: lpqrstwo  # Task being reviewed
  review_loop_template: security
```

**Iteration tracking:** The `review_loop_iteration` is incremented by the flow action when it triggers re-review. The flow reads the current iteration from the completed followup task's metadata and passes `iteration + 1` to the next `review:` action. This keeps iteration counting in one place (the flow engine) rather than spreading it across commands.

**Note**: The `source` field uses the format defined in [task-change-linkage](../done/task-change-linkage.md) and is stored at the top level of the task event, not inside metadata.

### Flow Configuration for Review Loop

```yaml
# .aiki/flows/review-loop.yml
name: "review-loop"
version: "1"

task.closed:
  - if: $event.task.data.review_loop_enabled == true
    then:
      - log: "Review loop followup completed, re-reviewing..."
      - review:
          task_id: $event.task.data.review_loop_task_id
          template: $event.task.data.review_loop_template
          from: codex

review.closed:
  - if: $event.review.loop_enabled == true
    then:
      - if: $event.review.issues_found == 0
        then:
          - log: "✅ Review loop approved - no more issues!"
        else:
          - log: "🔄 Review loop iteration ${event.review.loop_iteration} found ${event.review.issues_found} issue(s)"
```

### Review Loop Iteration Limit

To prevent infinite loops:

```yaml
review.closed:
  - if: $event.review.loop_iteration >= 5
    then:
      - log: "⚠️  Review loop exceeded 5 iterations, manual review required"
      - block: "Too many review iterations, please review manually"
```

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
          
          Fix tasks created. Run `aiki task list` to see them.
```

### Use Case 2: Security Review Loop

```yaml
# .aiki/flows/security-review.yml
name: "security-review"
version: "1"

task.closed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - log: "Security-sensitive task completed, starting review loop..."
      - review:
          task_id: $event.task.id
          template: security
          loop: true

review.closed:
  - if: $event.review.template == "security" && $event.review.issues_found > 0
    then:
      - block: |
          🚨 SECURITY REVIEW FAILED
          
          Found ${event.review.issues_found} security issue(s).
          Fix followup task ${event.review.followup_task_id} to continue.
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

### Use Case 4: Pre-Push Review with Loop

```yaml
# .aiki/flows/pre-push.yml
name: "pre-push"
version: "1"

shell.permission_asked:
  - if: $event.command | contains("git push")
    then:
      - log: "Running pre-push review of session with loop..."
      - review:
          template: review  # Reviews all closed tasks in session
          loop: true
      
      - if: $review.issues_found > 0
        then:
          - block: |
              ❌ Cannot push - review found issues
              
              Fix followup task ${review.followup_task_id} and review will re-run automatically.
```

---

## Implementation

### Agent Runtime Integration

The task run command uses the `AgentRuntime` abstraction from [run-task.md](../done/run-task.md) to spawn agent sessions.

**AgentRuntime Interface** (see run-task.md for implementation):
- `spawn_blocking(options)` - Run agent and wait for completion (implemented)
- `spawn_background(options)` - Start agent and return immediately (not yet implemented — needs to be added to trait)
- Returns session result (completed/stopped/failed) or background task handle

### Task Run Command (with --background)

**Behavior:**
- Get task and determine assigned agent runtime
- If `--background`: spawn agent process and return immediately with task ID
- If blocking: spawn agent and wait for completion, then return result
- Output includes instructions for piping to `wait` and `fix`

### Wait Command

**Behavior:**
- Poll task status with exponential backoff: starts at 100ms, doubles up to 2s max interval
- Exit immediately if task is already in terminal state (closed/failed)
- Exit code 0 on success, 1 on failure
- Supports `--timeout <duration>` (e.g., `--timeout 60s`). Default: no timeout (waits indefinitely)
- Suitable for piping: `aiki wait <id> | aiki fix`
- On timeout: exits with code 2, prints "Task did not complete within timeout"

### Fix Command

The `aiki fix` command is **template-driven** rather than hardcoded logic. This makes it extensible and consistent with the task template system.

**Design Evolution:**
- **Old approach**: Hardcoded loop through comments, manually creating tasks
- **New approach**: Template-driven with `subtasks.from: source.comments` iteration

**Behavior:**
1. Wrapper around template system: `aiki fix <task_id>` → `aiki task add --template aiki/fix --source task:<task_id>`
2. Template system loads `aiki/fix` template (resolved from `.aiki/templates/aiki/fix.md`)
3. If no comments: print "approved" message and exit 0
4. If comments found:
   - Template uses `subtasks.from: source.comments` to iterate over comments
   - Creates parent followup task with one child task per comment
   - Each child task includes comment content (file, severity, issue, fix suggestion)
   - Comment IDs are the change_id of the JJ change storing the comment event
   - Parent task has `source` field linking to review task
   - Each child has `source` fields linking to review task and specific comment
   - **Automatically start the followup task**
   - Print list of issues with instructions for agent to complete the work
5. Show updated task context with followup task in_progress

**How Template Iteration Works:**
- `subtasks.from: source.comments` tells template system to iterate over comments array
- Each comment becomes one subtask
- The `# Subtasks` section is used as the template for each item
- Current item exposes `{text}` (comment body) and `{data.*}` (structured fields)
- Parent context via `parent.*` prefix (e.g., `{parent.source.name}`)

**Template:** `.aiki/templates/aiki/fix.md`
```markdown
---
version: 1.0.0
subtasks:
  from: source.comments
---

# Followup: {source.name}

Fix all issues identified in review.

# Subtasks

## {text}

**Review**: {parent.source.name}
**File**: {data.file}:{data.line}
**Severity**: {data.severity}
**Category**: {data.category}

{text}
```

**Implementation:**
```rust
pub fn fix(task_id: String) -> Result<()> {
    let followup_id = create_task_from_template(
        "aiki/fix",
        HashMap::from([("source", format!("task:{}", task_id))]),
    )?;
    task_start(&followup_id)?;
    Ok(())
}
```

**Note:** Review task is already closed (auto-closed when agent completed subtasks)

### Review Command

**Behavior:**
1. Determine reviewer agent (from `--from` option, default: codex)
2. Determine scope to review:
   - If task-id provided: review that specific task
   - Otherwise: review all closed tasks in current session (default)
3. Build metadata (task_id or session, changes, template)
4. Load task template (user custom or aiki: default is `review`)
5. Create review task from template (parent + subtasks defined in template)
6. If `--background`: start task in background and return immediately
7. Otherwise: start task and wait for completion, then return result
8. In both cases, agent calls `aiki fix` separately to process findings

**Template Loading:**
- Templates use namespace prefixes: `aiki/review` or `myorg/custom-review`
- Built-in templates in `.aiki/templates/aiki/{name}.md`
- Custom templates in `.aiki/templates/{namespace}/{name}.md` (e.g., `.aiki/templates/myorg/custom-review.md`)
- Default template is `aiki/review` (resolved from `.aiki/templates/aiki/review.md`)

**Helper Functions:**
- `create_task_from_template()` - Load template, resolve variables, and atomically create parent + all child tasks. Templates define subtasks in markdown with `# Subtasks` sections, and the template resolver creates them atomically. There is no separate `task_add_with_children()` function.

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

**Task type filtering controls display**:
- `aiki task list` - Shows user tasks (excludes completed review tasks by default)
- `aiki task list --all` - Shows all tasks including reviews
- `aiki review list` - Shows review task history
- No confusion about where to look

### 4. Simpler Implementation

**Single code path**:
- Reuse existing task storage/query infrastructure
- Reuse task comment system
- Reuse task relationship tracking
- Use `type` field for filtering (review vs user tasks)

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

### Prerequisites (Implemented)

The following infrastructure from [run-task.md](../done/run-task.md), [task-change-linkage.md](../done/task-change-linkage.md), and [task-templates.md](../done/task-templates.md) is required:

| Component | Status | Document |
|-----------|--------|----------|
| `AgentRuntime` trait | Implemented | run-task.md |
| `ClaudeCodeRuntime` | Implemented | run-task.md |
| `CodexRuntime` | Implemented | run-task.md |
| `aiki task run <id>` | Implemented | run-task.md |
| `sources` field on tasks | Implemented | task-change-linkage.md |
| `task=` in provenance | Implemented | task-change-linkage.md |
| `Template resolver` | Implemented | task-templates.md |
| `Template types/parsing` | Implemented | task-templates.md |
| `.aiki/templates/` discovery | Implemented | task-templates.md |
| `spawn_background()` on AgentRuntime | Not implemented | (this plan, Phase 2) |
| `task.closed` flow event | Not implemented | (this plan) |
| `review.started` flow event | Not implemented | (this plan) |
| `review.closed` flow event | Not implemented | (this plan) |

### Phase 1: Task Infrastructure

**Deliverables:**
- Add `--children` flag to `aiki task add` (accepts heredoc input) — **Note:** May be superseded by template system which handles parent+subtask creation via `aiki task add --template <name>`. Consider whether direct `--children` flag is still needed or if templates cover all use cases.
- Implement `create_task_from_template()` in the template resolver (atomic parent+subtask creation is handled by the template system, not a separate helper)
- Atomically create parent + all child tasks in one operation
- Add `type` field to TaskEvent schema
- Add `sources` field to TaskEvent::Created (for lineage tracking)
- Add `data: HashMap<String, String>` field to `TaskComment` struct
- Add `--data key=value` flag to `aiki task comment` command
- Store structured data alongside comment text in CommentAdded events

> **Why `data` on TaskComment?** The review comment workflow (Phase 3+) needs structured
> fields like `file`, `line`, `severity`, and `category` on comments. Currently `TaskComment`
> only has `text: String` and `timestamp: DateTime<Utc>` — there is no structured data field.
> Adding `data: HashMap<String, String>` here is a prerequisite for `aiki fix` to extract
> actionable review findings from comments.

**Files:**
- `cli/src/tasks/types.rs` - Add `type`, `instructions`, `sources` fields; add `data: HashMap<String, String>` to `TaskComment`
- `cli/src/tasks/templates/resolver.rs` - Implement `create_task_from_template()` (template system handles atomic parent+subtask creation)
- `cli/src/commands/task.rs` - Add `--children` flag, add `--data key=value` to `comment` subcommand

### Phase 2: Async Task Execution

**Deliverables:**
- `aiki task run --background` flag - spawn agent and return immediately
- `aiki wait <id>` command - block until task completes (exponential backoff 100ms→2s)
- `aiki wait --timeout <duration>` - exit code 2 on timeout
- `aiki task stop` terminates background agent process (existing command, extended behavior)
- Background task tracking (PID, status polling)
- Exit codes for chaining (`wait` exits 0 on success, 1 on failure, 2 on timeout)

**Files:**
- `cli/src/commands/task.rs` - Add `--background` flag, extend `stop` for background tasks
- `cli/src/commands/wait.rs` - Wait command implementation
- `cli/src/tasks/runner.rs` - Background execution logic
- `cli/src/agents/runtime/mod.rs` - Add `spawn_background()` method

### Phase 3: Fix Command

**Prerequisite:** Requires template iteration support (`subtasks.from`) from task-templates.md. If template iteration is not yet implemented, Phase 3 should implement a minimal version (hardcoded comment-to-subtask mapping) and migrate to templates once available.

**Deliverables:**
- `aiki fix <task_id>` command - create and start followup from comments
- Read comments from task, create followup task with subtasks, start it
- Graceful handling when no comments (success, not error)
- Comment IDs derived from JJ change_id (no generation needed)

**Files:**
- `cli/src/commands/fix.rs` - Fix command implementation
- `cli/src/commands/mod.rs` - Export fix module

### Phase 4: Review Command

**Deliverables:**
- `aiki review [<task-id>]` CLI command with all options
- Review scope support (session (default), task)
- Session scope = all closed tasks in current session
- `--template` flag for review templates (default: `review`)
- `--background` flag for async review (flows use this by default)
- Review task creation from templates with review-specific data

**Files:**
- `cli/src/commands/review.rs` - Review command implementation
- Integration with template system from [task-templates.md](task-templates.md)

**Dependencies:**
- Requires template infrastructure from task-templates.md Phase 1-2
- Review command populates `{data.scope}` and `{data.files}` variables
- Uses built-in `aiki/review` template (resolved from `.aiki/templates/aiki/review.md`)

**Implementation Notes:**
- `aiki review --changes @` creates task from template with `data.scope="@"`, `data.files="..."`
- Template system handles variable substitution and task creation
- Review command is a specialized wrapper around `aiki task create --template`

### Phase 5: Flow Integration

**Deliverables:**
- `review:` flow action (wraps `aiki review` CLI)
- `review.started` event with task ID
- `prompt:` action to return instructions to agent
- Agent workflow: `aiki wait <id> | aiki fix`

**Files:**
- `cli/src/flows/types.rs` - Add `Review` action variant to `Action` enum
- `cli/src/flows/engine.rs` - Add `execute_review()` handler
- `cli/src/events/mod.rs` - Add `ReviewStarted` event variant

**Note: Missing flow events.** The flow actions in this plan reference `task.closed`, `review.started`, and `review.closed` as event triggers, but none of these exist in the current `AikiEvent` enum (`cli/src/events/mod.rs`). Before the review flow actions can work, these events must be:
1. Added as variants to the `AikiEvent` enum (e.g., `TaskClosed`, `ReviewStarted`, `ReviewClosed`)
2. Wired into the flow engine's event routing so that flow YAML triggers like `task.closed:` and `review.started:` are matched to the corresponding enum variants
3. Emitted at the appropriate points: `task.closed` when `task_close()` runs, `review.started` when `aiki review` creates and starts a review task, `review.closed` when a review task reaches a terminal state

### Phase 6: Review Queries

**Deliverables:**
- `aiki review list` - List review tasks
- `aiki review show <id>` - Show review details with comments
- Review outcome derivation from task relationships

**Files:**
- `cli/src/commands/review.rs` - Add list/show subcommands

**Critical Path:**
1. Task infrastructure → Async execution → Fix command → Review command → Flow integration
2. Review queries can be done in parallel with flow integration

---

## Summary

This task-based design unifies reviews and regular tasks with a composable, async CLI:

- **Single storage system** - All tasks on `aiki/tasks` branch
- **Consistent lifecycle** - Reviews use same events as regular tasks
- **Pipeable CLI** - `aiki review | aiki fix` or `aiki review --background | aiki wait | aiki fix`
- **Async option** - `--background` lets reviews run without blocking the agent
- **Simple orchestration** - Agent pipes: `aiki review | aiki fix`
- **Graceful handling** - `aiki fix` succeeds with no error when no issues found
- **Bidirectional linkage** - Changes include `task=` in provenance, tasks include `source` for lineage

### Key Commands

| Command | Purpose | stdin | stdout |
|---------|---------|-------|--------|
| `aiki review [<task-id>] [--background]` | Create and run review task | — | task ID |
| `aiki wait [<id>] [--timeout <dur>]` | Block until task completes | task ID | task ID (passthrough) |
| `aiki fix [<id>]` | Create and start followup tasks | task ID | followup task ID |
| `aiki task run <id> --background` | Start task, return immediately | — | — |
| `aiki task stop <id>` | Stop a running task | — | — |

**Pipe patterns:**
```bash
aiki review | aiki fix                          # blocking review + fix
aiki review --background | aiki wait | aiki fix  # async review + wait + fix
```

### Related Documents

| Document | Purpose |
|----------|---------|
| [run-task.md](../done/run-task.md) | `AgentRuntime` trait, `aiki task run` command |
| [task-change-linkage.md](../done/task-change-linkage.md) | Provenance `task=` field, task `source` field |

This composable design lets agents control when to wait for reviews and process findings with a simple, consistent workflow.

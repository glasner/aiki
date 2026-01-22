# Code Review System: Task-Based Design

**Date**: 2026-01-10
**Updated**: 2026-01-18
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
4. **Async workflow** - `aiki task run --background` returns immediately, agent chains `wait && fix`
5. **Consistent task lifecycle** - Reviews use same event structure as other tasks
6. **Composable commands** - `aiki wait` + `aiki fix` can be chained by agents
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

1. **Async task execution**: `aiki task run --background` starts review and returns immediately
2. **Agent waits and fixes**: Agent chains `aiki wait <id> && aiki fix <id>`
3. **aiki fix creates followups**: Reads review comments and creates followup tasks atomically
4. **Composable CLI**: Each command does one thing well, agents compose them
5. **Provenance tracking**: Changes made by agents include `task=` field linking to the active task

### Async Review Flow

```
┌──────────────────────────────────────────────────────────────┐
│  1. Flow hook triggers review                                 │
│     • Creates review task with subtasks                       │
│     • Runs: aiki task run <id> --background                   │
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
- **task** - Changes associated with a specific task
  - `aiki review <task-id>` - Review specific task by ID

**Future scope ideas:**
- **files** - Specific file paths (`--files <paths>`)
- **active** - Active task currently being worked on (`--active`)
- **working_copy** (`@`) - Single working copy change only
- **range** - JJ revset (e.g., `trunk()..@`, `main..@`)
- **staged** - Files staged for commit (Git interop)

### Review Templates

The default review template covers general code quality:

- **`review`** (default) - General code quality, functionality, security, and performance
  - Subtasks: Digest code changes, Review code
  - Location: `.aiki/templates/aiki/review.md`

**Custom templates:**
- Users can create custom templates in `.aiki/templates/{namespace}/` (e.g., `.aiki/templates/myorg/`)
- Future: Additional specialized templates (security, performance, style)

Templates define the full task structure (parent + subtasks + instructions). See [task-templates.md](task-templates.md) for details.

### Task Types

**Review tasks**:
- Created by `aiki review @` with 2 subtasks (digest, review)
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
aiki review @ --background
```

This internally executes:
1. `task_add_with_children()` - Creates parent task + 2 subtasks atomically (digest, review)
2. `aiki task run xqrmnpst --background` - Starts the review task and returns immediately
3. Returns task ID to caller for later processing

**Then agent processes:**
```bash
aiki wait xqrmnpst && aiki fix xqrmnpst
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
name: "Review: @ (working copy)"
assignee: codex
instructions: |
  Code review orchestration task
  
  This task coordinates review steps.
metadata:
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
metadata:
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
  
  For each issue found, add a comment using `aiki task comment` with:
  **File**: <path>:<line>
  **Severity**: error|warning|info
  **Category**: functionality|quality|security|performance
  
  <description of issue>
  
  **Impact**: <what could go wrong>
  
  **Suggested Fix**:
  <how to fix it>
  
  Add comments as you find issues, don't wait until the end.
metadata:
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
metadata:
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
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: comment_added
timestamp: 2025-01-15T10:05:00Z
instructions: |
  **File**: src/auth.ts:42
  **Severity**: error
  **Category**: quality
  
  Potential null pointer dereference when accessing user.name
  
  **Impact**: Runtime crash if user object is null from auth middleware
  
  **Suggested Fix**:
  ```typescript
  if (user && user.name) {
    return user.name;
  }
  throw new Error("User not authenticated");
  ```
---
aiki_task_event: v1
task_id: xqrmnpst.2
event: comment_added
timestamp: 2025-01-15T10:05:03Z
instructions: |
  **File**: src/middleware.ts:28
  **Severity**: warning
  **Category**: security
  
  JWT expiration not validated before use
  
  **Impact**: Expired tokens may be accepted
  
  **Suggested Fix**:
  Check exp claim before accepting token
---
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

**Note**: Comments are added as issues are discovered during the review process.

**7. Parent Task Auto-Closes**

When the last subtask completes, the parent task automatically closes:

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
aiki task wait xqrmnpst && aiki fix xqrmnpst
```

The `aiki fix` command reads all comments from the completed task xqrmnpst and creates the followup task + all child tasks in one operation:

```yaml
---
aiki_task_event: v1
task_id: lpqrstwo
event: created
timestamp: 2025-01-15T10:05:01Z
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
metadata:
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
timestamp: 2025-01-15T10:05:01Z
name: "Fix: Null pointer check in auth.ts"
priority: p0
assignee: claude-code
instructions: |
  **File**: src/auth.ts:42
  **Severity**: error
  **Category**: quality

  Potential null pointer dereference when accessing user.name

  **Impact**: Runtime crash if user object is null from auth middleware

  **Suggested Fix**:
  ```typescript
  if (user && user.name) {
    return user.name;
  }
  throw new Error("User not authenticated");
  ```
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
timestamp: 2025-01-15T10:05:01Z
name: "Fix: JWT expiration validation"
priority: p1
assignee: claude-code
instructions: |
  **Review**: task:xqrmnpst
  **File**: src/middleware.ts:28
  **Severity**: warning

  ## Issue
  JWT expiration not validated before use

  ## Impact
  Expired tokens may be accepted

  ## Suggested Fix
  Check exp claim before accepting token
scope:
  files:
    - path: src/middleware.ts
      lines: [28]
source: task:xqrmnpst
source: comment:d5e6f7g8
---
```

**Note**:
- Parent task + all child tasks created atomically in one operation using `task_add_with_children()`
- No draft flag needed since all tasks are created together
- Each child task preserves the comment structure (file, severity, issue, impact, suggested fix)
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
    aiki wait xqrmnpst && aiki fix xqrmnpst
  </started>
</aiki_task>
```

### Wait Command

```bash
aiki wait <task_id>
```

**Behavior:**
- Blocks until the task reaches a terminal state (closed, stopped, or failed)
- Exit code 0 if task completed successfully
- Exit code 1 if task failed
- Useful for chaining: `aiki wait <id> && aiki fix <id>`

**Output:**
```xml
<aiki_task cmd="wait" status="ok">
  <completed task_id="xqrmnpst" outcome="done" duration_ms="45000">
    Task completed successfully.
  </completed>
</aiki_task>
```

### Fix Command (Create and Start Followup Tasks)

```bash
aiki fix <task_id>
```

**Behavior:**
1. Reads all comments from the specified task
2. If no comments found: prints success message, exits 0 (no error)
3. If comments found:
   - Creates followup task with one subtask per comment
   - **Automatically starts the followup task**
   - Returns instructions for agent to complete the work
4. Each subtask has `source` field linking to original comment

**Output (no issues):**
```xml
<aiki_fix cmd="fix" status="ok">
  <approved task_id="xqrmnpst">
    Review approved - no issues found.
  </approved>
</aiki_fix>
```

**Output (issues found):**
```xml
<aiki_fix cmd="fix" status="ok">
  <followup task_id="lpqrstwo" issues_found="2" status="started">
    Created and started followup task with 2 subtasks.

    Please complete the following fixes:
    
    1. Fix: Null pointer check in auth.ts (p0)
       File: src/auth.ts:42
       
    2. Fix: JWT expiration validation (p1)
       File: src/middleware.ts:28
    
    Work on these issues and close each subtask when complete.
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
- `--from <agent>` - Reviewer agent (default: codex)
- `--template <name>` - Task template (default: review)
- `--background` - Return immediately after starting review (agent chains wait && fix)

**Examples:**
```bash
# Review all closed tasks in session (uses default template)
aiki review

# Review specific task by ID
aiki review xqrmnpst

# Background review of session
aiki review --background

# Explicit template
aiki review --template aiki/review

# Custom template (if user creates one)
aiki review --template myorg/custom-review
```

**Behavior (--background):**
1. Creates review task with children using `task_add_with_children()` (assignee: codex)
2. Calls `aiki task run <task_id> --background` to start the review
3. Returns immediately with task ID and suggested follow-up commands

**Output (--background):**
```xml
<aiki_review cmd="review" status="ok">
  <started task_id="xqrmnpst" background="true">
    Review started in background.

    To wait for completion and process findings:
    aiki wait xqrmnpst && aiki fix xqrmnpst
  </started>
</aiki_review>
```

**Behavior (blocking, default):**
1. Creates review task with children
2. Calls `aiki task run <task_id>` (blocks until complete)
3. Calls `aiki fix <task_id>` to process comments
4. Returns with summary

**Output (blocking):**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" outcome="rejected" issues_found="2" duration_ms="9000">
    Review completed: Found 2 issues

    Followup task created: lpqrstwo
    Start with: aiki task start lpqrstwo
  </completed>

  <context>
    <in_progress/>
    <list ready="4">
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
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
      <task id="xqrmnpst" name="Review: @ (working copy)" type="review" assignee="codex" status="completed"/>
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
      <task id="pqrstuv" name="Review: main..@" type="review" assignee="codex" status="completed"/>
    </list>
  </context>
</aiki_task>
```

---

## Flow Integration

### review: Flow Action (Background by Default)

Flows trigger background reviews and return a prompt to the agent:

```yaml
# Background review triggered on task completion
# Reviews the completed task
task.completed:
  - review:
      task_id: $event.task.id
      from: codex
      template: review
```

**What the flow does:**
1. Creates review task with subtasks
2. Runs `aiki task run <id> --background`
3. Returns prompt to agent with instructions

**Prompt returned to agent:**
```
A code review has been started (task: xqrmnpst).

When ready to process review findings, run:
aiki task wait xqrmnpst && aiki fix xqrmnpst

Continue with your current work - the review runs in parallel.
```

### Flow Action Options

```yaml
# Simple session review (default: all closed tasks)
response.received:
  - review: {}

# Review completed task
task.completed:
  - review:
      task_id: $event.task.id

# Security review for auth-related tasks
task.completed:
  - if: $event.files | contains("src/auth.ts")
    then:
      - review:
          task_id: $event.task.id
          template: security

# Blocking review (waits for completion)
response.received:
  - review:
      background: false  # Explicitly block
```

### Agent Workflow After Review Trigger

When a flow triggers background reviews, the agent decides when to wait and fix:

```bash
# Single review - wait and fix
aiki wait xqrmnpst && aiki fix xqrmnpst

# Check review status without blocking
aiki task show xqrmnpst
```

### Agent Workflow After Review

When a review completes, the agent processes findings:

```bash
# Wait for review, then process findings
aiki wait xqrmnpst && aiki fix xqrmnpst
```

### review.started Event

Fires when a background review starts:

```yaml
review.started:
  - log: "Review started: ${event.review.task_id}"
  - prompt: |
      A code review is running in background (task: ${event.review.task_id}).

      When ready to process findings:
      aiki wait ${event.review.task_id} && aiki fix ${event.review.task_id}
```

**Event payload:**
```json
{
  "review": {
    "task_id": "xqrmnpst",
    "reviewer": "codex",
    "reviewed_task_id": "lpqrstwo",
    "template": "default",
    "background": true
  }
}
```

### Custom Flow for Security Reviews

```yaml
# Trigger security review on auth file changes
task.completed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - review:
          task_id: $event.task.id
          prompt: security
      - prompt: |
          ⚠️ Security-sensitive files changed. A security review is running.

          You MUST wait for and address the review before continuing:
          aiki wait ${review.task_id} && aiki fix ${review.task_id}
```

### Triggering Reviews from Flows

```yaml
# Trigger security review on task completion
task.completed:
  - if: $event.files | any(f => f.path | contains("auth"))
    then:
      - review:
          task_id: $event.task.id
          template: security
      - prompt: |
          Security review started: ${review.task_id}

          To process when ready:
          aiki wait ${review.task_id} && aiki fix ${review.task_id}
```



---

## Review Loop Pattern

The **review-loop** pattern enables iterative review-fix-review cycles until code is approved.

### How Review Loops Work

```
┌──────────────────────────────────────────────────────────────┐
│  1. Initial Review (aiki review @ --loop)                    │
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
│     • Triggers task.completed event                          │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  3. Automatic Re-review (triggered by flow)                  │
│     • Flow detects review_loop_enabled in metadata           │
│     • Runs: aiki review @ --from codex                       │
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

# Metadata for loop control
metadata:
  review_loop_enabled: true
  review_loop_parent: xqrmnpst  # Original review task
  review_loop_iteration: 1
  review_loop_task_id: lpqrstwo  # Task being reviewed
  review_loop_template: security
```

**Note**: The `source` field uses the format defined in [task-change-linkage](../done/task-change-linkage.md) and is stored at the top level of the task event, not inside metadata.

### Flow Configuration for Review Loop

```yaml
# .aiki/flows/review-loop.yml
name: "review-loop"
version: "1"

task.completed:
  - if: $event.task.metadata.review_loop_enabled == true
    then:
      - log: "Review loop followup completed, re-reviewing..."
      - review:
          task_id: $event.task.metadata.review_loop_task_id
          template: $event.task.metadata.review_loop_template
          from: codex

review.completed:
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
review.completed:
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

task.completed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - log: "Security-sensitive task completed, starting review loop..."
      - review:
          task_id: $event.task.id
          template: security
          loop: true

review.completed:
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

response.received:
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
- `spawn_blocking(options)` - Run agent and wait for completion
- `spawn_background(options)` - Start agent and return immediately
- Returns session result (completed/stopped/failed) or background task handle

### Task Run Command (with --background)

**Behavior:**
- Get task and determine assigned agent runtime
- If `--background`: spawn agent process and return immediately with task ID
- If blocking: spawn agent and wait for completion, then return result
- Output includes instructions for chaining `wait` and `fix` commands

### Task Wait Command

**Behavior:**
- Poll task status in a loop with configurable interval
- Exit immediately if task is in terminal state (closed/failed)
- Exit code 0 on success, 1 on failure
- Suitable for command chaining: `aiki wait <id> && aiki fix <id>`

### Fix Command

**Behavior:**
1. Read all comments from the completed review task
2. If no comments: print "approved" message and exit 0
3. If comments found:
   - Determine assignee (default to agent that authored the reviewed changes)
   - Create parent followup task with one child task per comment
   - Each child task includes comment content (file, severity, issue, fix suggestion)
   - Parent task has `source` field linking to review task
   - Each child has `source` fields linking to review task and specific comment
   - **Automatically start the followup task**
   - Print list of issues with instructions for agent to complete the work
4. Show updated task context with followup task in_progress

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
6. If `--background`: start task and return immediately with instructions
7. If blocking: run task, wait for completion, then call `fix` to create followup tasks

**Template Loading:**
- Templates use namespace prefixes: `aiki/review` or `myorg/custom-review`
- Built-in templates in `.aiki/templates/aiki/{name}.md`
- Custom templates in `.aiki/templates/{namespace}/{name}.md` (e.g., `.aiki/templates/myorg/custom-review.md`)
- Default template is `aiki/review` (at `.aiki/templates/aiki/review.md`)

**Helper Functions:**
- `task_add_with_children()` - Atomically create parent + all child tasks (see task-change-linkage.md)
- `create_followup_from_comments()` - Build followup task with one child per comment, including source lineage

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
- `created`, `started`, `comment_added`, `completed`
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

The following infrastructure from [run-task.md](../done/run-task.md) and [task-change-linkage.md](../done/task-change-linkage.md) is required:

| Component | Status | Document |
|-----------|--------|----------|
| `AgentRuntime` trait | Implemented | run-task.md |
| `ClaudeCodeRuntime` | Implemented | run-task.md |
| `CodexRuntime` | Implemented | run-task.md |
| `aiki task run <id>` | Implemented | run-task.md |
| `sources` field on tasks | Implemented | task-change-linkage.md |
| `task=` in provenance | Implemented | task-change-linkage.md |

### Phase 1: Task Infrastructure

**Deliverables:**
- Add `--children` flag to `aiki task add` (accepts heredoc input)
- Implement `task_add_with_children()` helper function
- Atomically create parent + all child tasks in one operation
- Add `type` field to TaskEvent schema
- Add `sources` field to TaskEvent::Created (for lineage tracking)

**Files:**
- `cli/src/tasks/types.rs` - Add `type`, `instructions`, `sources` fields
- `cli/src/tasks/manager.rs` - Implement `task_add_with_children()`
- `cli/src/commands/task.rs` - Add `--children` flag

### Phase 2: Async Task Execution

**Deliverables:**
- `aiki task run --background` flag - spawn agent and return immediately
- `aiki wait <id>` command - block until task completes
- Background task tracking (PID, status polling)
- Exit codes for chaining (`wait` exits 0 on success, 1 on failure)

**Files:**
- `cli/src/commands/task.rs` - Add `--background` flag
- `cli/src/commands/wait.rs` - Wait command implementation
- `cli/src/tasks/runner.rs` - Background execution logic
- `cli/src/agents/runtime/mod.rs` - Add `spawn_background()` method

### Phase 3: Fix Command

**Deliverables:**
- `aiki fix <task_id>` command - create followup from comments
- Read comments from task, create followup task with subtasks
- Automatically start the followup task
- Graceful handling when no comments (success, not error)

**Files:**
- `cli/src/commands/fix.rs` - Fix command implementation
- `cli/src/commands/mod.rs` - Export fix module

### Phase 4: Review Command

**Deliverables:**
- `aiki review [<task-id>]` CLI command with all options
- Review scope support (session (default), task)
- Session scope = all closed tasks in current session
- `--template` flag for review templates (default: `review`)
- `--background` flag (default for flow integration)
- Review task creation from templates with review-specific data

**Files:**
- `cli/src/commands/review.rs` - Review command implementation
- Integration with template system from [task-templates.md](task-templates.md)

**Dependencies:**
- Requires template infrastructure from task-templates.md Phase 1-2
- Review command populates `{data.scope}` and `{data.files}` variables
- Uses `.aiki/templates/aiki/review.md` as default template

**Implementation Notes:**
- `aiki review @` creates task from template with `data.scope="@"`, `data.files="..."`
- Template system handles variable substitution and task creation
- Review command is a specialized wrapper around `aiki task create --template`

### Phase 5: Flow Integration

**Deliverables:**
- `review:` flow action (background by default)
- `review.started` event with task ID
- `prompt:` action to return instructions to agent
- Agent workflow: `aiki wait <id> && aiki fix <id>`

**Files:**
- `cli/src/flows/actions/review.rs` - Review flow action
- `cli/src/flows/actions/prompt.rs` - Prompt action for agent instructions
- `cli/src/flows/events.rs` - Add `review.started` event

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
- **Composable CLI** - `task run --background`, `wait`, `fix` can be chained
- **Async by default** - Background reviews don't block the requesting agent
- **Simple orchestration** - Agent chains: `aiki wait <id> && aiki fix <id>`
- **Graceful handling** - `aiki fix` succeeds with no error when no issues found
- **Bidirectional linkage** - Changes include `task=` in provenance, tasks include `source` for lineage

### Key Commands

| Command | Purpose |
|---------|---------|
| `aiki task run <id> --background` | Start task, return immediately |
| `aiki wait <id>` | Block until task completes |
| `aiki fix <id>` | Create followup tasks from review comments |
| `aiki review [<task-id>] --background` | Create and start review task (async) |

### Related Documents

| Document | Purpose |
|----------|---------|
| [run-task.md](../done/run-task.md) | `AgentRuntime` trait, `aiki task run` command |
| [task-change-linkage.md](../done/task-change-linkage.md) | Provenance `task=` field, task `source` field |

This composable design lets agents control when to wait for reviews and process findings with a simple, consistent workflow.

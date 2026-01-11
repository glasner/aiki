# Code Review System: Task-Based Design

**Date**: 2026-01-10  
**Status**: Proposed Architecture  
**Purpose**: Reviews as system tasks on `aiki/tasks`

---

## Executive Summary

This design uses **tasks for everything**:

1. **Review tasks** - Regular tasks with subtasks that agents execute to analyze code
2. **Followup tasks** - User-visible tasks created atomically from review comments
3. **Single storage branch** - All tasks on `aiki/tasks` branch
4. **Simple workflow** - `aiki review` creates task, runs agent, processes comments, closes review
5. **Consistent task lifecycle** - Reviews use same event structure as other tasks

---

## Core Concepts

### How Reviews Work

1. **Agent does the work**: Agent executes review subtasks (digest, review) and adds comments
2. **aiki review orchestrates**: Creates tasks, runs agent, processes comments into followup tasks
3. **Atomic followup creation**: All followup tasks created together using `--children` flag
4. **Clean separation**: Agent doesn't need to know about task management

### Task Types

**Review tasks**:
- Created by `aiki review @` with 2 subtasks (digest, review)
- Assigned to reviewer agent (e.g., `codex`)
- Agent adds comments during review
- Closed automatically after followup tasks created

**Followup tasks**:
- Created atomically from review comments (parent + all children in one operation)
- Each child task references specific comment via `discovered_from`
- Visible immediately (no draft flag needed)
- Normal user tasks that can be worked on

---

## Data Model: Review Tasks

### Review Task Lifecycle

**User runs:**
```bash
aiki review @
```

This internally executes:
1. `task_add_with_children()` - Creates parent task + 2 subtasks atomically (digest, review)
2. `aiki task run xqrmnpst` - Starts the review task and delegates to codex agent
3. After agent completes, `aiki review` processes comments into followup tasks
4. Closes the review task

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
  
  This task coordinates review steps and creates followup tasks from findings.
metadata:
  revset: "@"
  changes: [zxywtuvs]
---
```

**Note**: 
- Created with 2 subtasks atomically using `--children` pattern (digest, review)
- Review tasks are normal tasks (not draft) since structure is complete
- This is a parent orchestration task with sequential subtasks
- Can be started immediately after creation
- Agent only handles the review work; followup task creation is done by aiki review process after agent completes

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
  revset: "@"
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
  revset: "@"
  changes: [zxywtuvs]
---
```

**2. Task Run Starts Agent Session**

The `aiki task run` command spawns a background agent session to execute the review task:

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

The agent (codex) receives the review task in its initial context and begins executing subtasks sequentially.

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

**7. Agent Session Ends - Control Returns to aiki review**

After the codex agent completes both review subtasks, the agent session ends:

```yaml
---
event: session.ended
session: codex-session-review-xqrmnpst
timestamp: 2025-01-15T10:05:10Z
---
```

Control returns to the `aiki review` process, which now reads all comments from task xqrmnpst.2 and creates followup tasks.

**8. Followup Task Created with All Children** (atomically using --children)

The `aiki review` process collects all comments, then creates the followup task + all child tasks in one operation:

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
  Code review completed by codex (review:xqrmnpst)
  
  Found 2 issues requiring fixes.
  Start this task to scope to review issues only.
scope:
  files:
    - path: src/auth.ts
    - path: src/middleware.ts
discovered_from: review:xqrmnpst
blocks: [mxsl]  # Blocks originating task if review was of task changes
---
```

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
discovered_from: review:xqrmnpst
discovered_from: comment:c1a2b3c4
---
```

**Child task 2:**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo.2
event: created
timestamp: 2025-01-15T10:05:01Z
name: "Fix: JWT token validation"
priority: p0
assignee: claude-code
instructions: |
  **Review**: review:xqrmnpst
  **File**: src/auth.ts:42
  **Severity**: error
  
  ## Issue
  Potential null pointer dereference when accessing user.name
  
  ## Impact
  Runtime crash if user object is null from auth middleware
  
  ## Suggested Fix
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
discovered_from: review:xqrmnpst
discovered_from: comment:d5e6f7g8
---
```

**Note**: 
- Parent task + all child tasks created atomically in one operation using `task_add_with_children()`
- No draft flag needed since all tasks are created together
- Each child task preserves the comment structure (file, severity, issue, impact, suggested fix)
- Priority inherits from blocked task (`mxsl` is p2), defaults to p1 if no blocked task
- If the originating task (`mxsl`) was closed, it should be reopened with reason "Review found issues (task:lpqrstwo)"

**9. aiki review Process Closes Review Task**

After creating followup tasks, the `aiki review` process closes the review task:
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: closed
outcome: rejected
timestamp: 2025-01-15T10:05:10Z
---
```

**Note**: 
- Review task is now complete and hidden (system task)
- Review outcome derived from task relationships:
  - **Outcome**: Followup task exists = `rejected`, absent = `approved`
  - **Issues found**: Count children of followup task

---

## CLI Commands

### Primary Review Command

```bash
aiki review <revset> [--from <reviewer>]
```

**Behavior:**
1. Creates review task with children using `task_add_with_children()` (assignee: codex)
2. Calls `aiki task run <task_id>` to execute the review
3. Task runs headless reviewer agent (blocking/synchronous)
4. Agent adds comments to task as issues found
5. When agent completes, creates followup tasks from comments
6. Marks review task as completed
7. Returns with summary

**Output:**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" outcome="rejected" issues_found="2" duration_ms="9000">
    Review completed: Found 2 issues
    
    Followup task created: lpqrstwo
    Start with: aiki task start lpqrstwo
  </completed>
  
  <!-- outcome and issues_found derived from task relationships:
       outcome = followup_task exists ? "rejected" : "approved"
       issues_found = count_children(followup_task) -->
  
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

**Note**: Draft tasks (like review tasks) are hidden from this list by default. Use `aiki task list --draft` to include them.

### Review History

```bash
aiki review list
```

**Behavior:** Queries completed review tasks (filters by task name pattern or discovered_from links)

```xml
<aiki_review cmd="list" status="ok">
  <reviews>
    <!-- outcome and issues_found derived from task relationships -->
    <review task_id="xqrmnpst" revset="@" outcome="rejected" issues_found="2" 
            timestamp="2025-01-15T10:05:00Z" reviewer="codex"/>
    <review task_id="pqrstuv" revset="main..@" outcome="approved" issues_found="0"
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
    <revset>@</revset>
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

Shows ready tasks only (drafts hidden by default):

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

Include draft tasks:

```bash
aiki task list --draft
```

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="6">
      <task id="xqrmnpst" name="Review: @ (working copy)" assignee="codex" draft="true" status="completed"/>
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p2"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
      <task id="pqrstuv" name="Review: main..@" assignee="codex" draft="true" status="completed"/>
    </list>
  </context>
</aiki_task>
```

---

## Implementation

### Review Command

```rust
pub fn review(revset: &str, from: Option<String>) -> Result<()> {
    let reviewer = from.unwrap_or_else(|| "codex".to_string());
    let changes = resolve_revset(revset)?;
    
    // Build metadata for all review tasks
    let mut metadata = HashMap::new();
    metadata.insert("revset".to_string(), json!(revset));
    metadata.insert("changes".to_string(), json!(changes));
    
    // Create review task with 2 subtasks (digest, review)
    // All tasks get same metadata
    let children = vec![
        ChildTask {
            name: "Digest code changes".to_string(),
            metadata: Some(metadata.clone()),
        },
        ChildTask {
            name: "Review code for functionality/quality/security/performance".to_string(),
            metadata: Some(metadata.clone()),
        },
    ];
    
    let task_id = task_add_with_children(
        format!("Review: {}", revset),
        reviewer,
        false,  // draft = false (visible, ready to run)
        children,
        Some(metadata),  // Parent metadata
    )?;
    
    // Run the review task - agent executes digest and review subtasks
    task_run(&task_id)?;
    
    // After agent completes, process comments into followup tasks
    let comments = get_task_comments(&task_id)?;
    if !comments.is_empty() {
        let followup_task_id = create_followup_from_comments(&task_id, &comments)?;
        eprintln!("Created followup task: {}", followup_task_id);
    }
    
    // Close the review task
    close_task(&task_id, format!("Review completed: {} issues found", comments.len()))?;
    
    Ok(())
}

/// Child task creation info
struct ChildTask {
    name: String,
    metadata: Option<HashMap<String, Value>>,
}

/// Create parent task with children atomically (implements --children flag)
fn task_add_with_children(
    name: String,
    assignee: String,
    draft: bool,
    children: Vec<ChildTask>,
    parent_metadata: Option<HashMap<String, Value>>,
) -> Result<String> {
    let parent_id = generate_task_id();
    let timestamp = Utc::now();
    
    // Build all events (parent + children)
    let mut events = Vec::new();
    
    // Parent event
    events.push(TaskEvent::Created {
        task_id: parent_id.clone(),
        name,
        assignee: assignee.clone(),
        draft,
        metadata: parent_metadata,
        timestamp,
    });
    
    // Child events
    for (i, child) in children.iter().enumerate() {
        let child_id = format!("{}.{}", parent_id, i + 1);
        events.push(TaskEvent::Created {
            task_id: child_id,
            name: child.name.clone(),
            assignee: assignee.clone(),
            draft,
            metadata: child.metadata.clone(),
            timestamp,
        });
    }
    
    // Store all events atomically in one write
    store_task_events(&events)?;
    
    Ok(parent_id)
}

fn create_followup_from_comments(review_task_id: &str, comments: &[TaskComment]) -> Result<String> {
    // Build parent metadata for followup task
    let mut parent_metadata = HashMap::new();
    parent_metadata.insert("discovered_from".to_string(), json!(format!("review:{}", review_task_id)));
    if let Some(blocked_tasks) = find_blocked_tasks(review_task_id)? {
        parent_metadata.insert("blocks".to_string(), json!(blocked_tasks));
    }
    
    // Extract files from comments for scope
    let files = extract_files_from_comments(comments);
    if !files.is_empty() {
        parent_metadata.insert("files".to_string(), json!(files));
    }
    
    // Build child tasks with per-child metadata
    let children: Vec<ChildTask> = comments.iter()
        .map(|comment| {
            let mut child_metadata = HashMap::new();
            child_metadata.insert("discovered_from".to_string(), json!([
                format!("review:{}", review_task_id),
                format!("comment:{}", comment.id),
            ]));
            
            ChildTask {
                name: format!("Fix: {}", extract_issue_title(comment)),
                metadata: Some(child_metadata),
            }
        })
        .collect();
    
    // Create parent + all children atomically with metadata
    let followup_task_id = task_add_with_children(
        format!("Followup: Review {}", review_task_id),
        "claude-code".to_string(),
        false,  // Not draft - ready immediately
        children,
        Some(parent_metadata),
    )?;
    
    Ok(followup_task_id)
}
```

---

## Benefits of Task-Based Approach

### 1. Single Storage System

**Everything on `aiki/tasks`**:
- Review tasks with `assignee: codex` (ready to run immediately)
- Followup tasks start as `draft: true`, then marked ready after children added
- User tasks with `draft: false` (or absent)
- Single event structure for all tasks
- No separate review event schema

### 2. Consistent Task Lifecycle

**Reviews use standard task events**:
- `created`, `started`, `comment_added`, `completed`
- Same infrastructure as regular tasks
- No special-case handling needed

### 3. Natural Visibility Control

**`draft` flag controls display**:
- `aiki task list` - Shows only ready tasks (excludes drafts by default)
- `aiki task list --draft` - Shows draft tasks
- `aiki review list` - Convenience wrapper for draft tasks
- No confusion about where to look

### 4. Simpler Implementation

**Single code path**:
- Reuse existing task storage/query infrastructure
- Reuse task comment system
- Reuse task relationship tracking
- Just add `draft` field to task schema

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

### Phase 1: Children Flag Support

- Add `--children` flag to `aiki task add` (accepts heredoc input)
- Implement `task_add_with_children()` helper function
- Atomically create parent + all child tasks in one operation

### Phase 2: Review Task Creation

- `aiki review @` uses `task_add_with_children()` internally
- Creates review task with `assignee: codex` with 2 subtasks (digest, review)
- Uses `aiki task run` to execute review
- After agent completes, collects comments and creates followup tasks atomically
- Closes review task

### Phase 3: Review Commands

- `aiki review list` - List review tasks
- `aiki review show <id>` - Show review details (wrapper for `aiki task show`)
- Integration with task context in XML output

---

## Summary

This task-based design unifies reviews and regular tasks:

- **Single storage system** - All tasks on `aiki/tasks` branch
- **Consistent lifecycle** - Reviews use same events as regular tasks
- **Clean separation** - Agent does review work, aiki review handles orchestration
- **Code reuse** - Leverage existing task infrastructure (task run, comments, hierarchy)
- **Atomic task creation** - Parent + children created together using `--children` flag
- **Simple workflow** - No draft flags or complex state management needed

This is simpler than maintaining separate storage systems and keeps the agent focused on what it does best: reviewing code.

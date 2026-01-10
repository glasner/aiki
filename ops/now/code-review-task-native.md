# Code Review System: Task-Native Design

**Date**: 2026-01-10  
**Status**: Proposed Architecture  
**Purpose**: Design reviews as tasks with autonomous agent execution

---

## Executive Summary

This design makes **tasks the orchestration primitive for agent work**. Reviews become tasks assigned to autonomous agents, creating a unified system where:

1. **Reviews are tasks** - No separate `aiki/reviews` branch
2. **Agents are assignees** - `autonomous(codex)` vs `codex` distinguishes execution modes
3. **Followup tasks created on completion** - Same pattern as current design
4. **Headless execution is reusable** - Any task can be autonomous
5. **Single event storage** - All work tracked on `aiki/tasks` branch

---

## Core Concepts

### Autonomous Agent Assignee

Tasks can be assigned to **autonomous agents** using the pattern: `autonomous(agent-name)`

```yaml
assignee: autonomous(codex)  # Autonomous execution
assignee: codex              # Interactive execution
assignee: claude-code        # Interactive execution
```

**Key insight**: The assignee string distinguishes execution modes, ensuring autonomous tasks don't appear in interactive agent queues.

### Task Type Field

Tasks have a top-level `task_type` field to distinguish different kinds of work:

```rust
pub enum TaskType {
    Work,    // Default: regular task
    Review,  // Autonomous code review
}
```

Reviews use `task_type: Review`:

```yaml
task_id: xqrmnpst
name: "Review: JWT authentication"
assignee: autonomous(codex)
task_type: Review
metadata:
  prompt: security
  scope: working_copy
```

---

## Data Model: Task-Native Reviews

### Review Task Lifecycle

**1. Review Task Created**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: created
timestamp: 2025-01-15T10:04:50Z
name: "Review: JWT authentication"
priority: p1
assignee: autonomous(codex)
task_type: Review
body: |
  Code review of current changes.
  
  **Scope**: working_copy
  **Prompt**: security
  **Context**: Ready for merge
metadata:
  prompt: security
  scope: working_copy
blocks: [mxsl]  # Blocks the task that was in progress when review was run
---
```

**Note**: The agent that requested the review (e.g., `claude-code`) can be derived from the JJ change metadata in the `[aiki]` block of the change description being reviewed.

**2. Review Task Started (Autonomous Execution Begins)**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: started
timestamp: 2025-01-15T10:04:51Z
stopped: []
metadata:
  execution_mode: autonomous
---
```

**3. Review Task Completed (Results in Body)**
```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: closed
timestamp: 2025-01-15T10:05:00Z
body: |
  # Review Results
  
  **Duration**: 9s
  
  ## Issues Found
  
  1. **src/auth.ts:42** - Potential null pointer dereference (p0)
  2. **src/auth.ts:67-69** - JWT token validation missing (p1)
  
  ## Followup
  
  Created followup task: lpqrstwo
  - lpqrstwo.1: Fix null pointer check
  - lpqrstwo.2: Fix JWT validation
metadata:
  duration_ms: 9000
  followup_task_id: lpqrstwo
---
```

**Note**: Review outcome is derived from task relationships:
- **Outcome**: `rejected` if followup task exists, `approved` otherwise
- **Issues found**: Count of children of followup task

**4. Followup Task Created**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo
event: created
timestamp: 2025-01-15T10:05:01Z
name: "Followup: JWT authentication review"
priority: p1
assignee: claude-code  # Back to interactive
task_type: Work  # Regular work task
body: |
  Code review completed by codex (review:xqrmnpst)
  
  Found 2 issues requiring fixes.
  Start this task to scope to review issues only.
scope:
  files:
    - path: src/auth.ts
    - path: src/middleware.ts
discovered_from: task:xqrmnpst
blocks: [mxsl]
---
```

**5. Child Tasks Created**
```yaml
---
aiki_task_event: v1
task_id: lpqrstwo.1
event: created
timestamp: 2025-01-15T10:05:02Z
name: "Fix: Potential null pointer dereference"
priority: p0
assignee: claude-code
task_type: Work
body: |
  **Review**: task:xqrmnpst
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
discovered_from: task:xqrmnpst
---
```

---

## CLI Commands

### Review Command (Creates Autonomous Task)

```bash
aiki review @
```

**Behavior:**
1. Creates review task with `assignee: autonomous(codex)`
2. Immediately starts task (autonomous execution begins)
3. Codex runs review, updates task body with results
4. If issues found, creates followup task with children
5. Closes review task

**Output:**
```xml
<aiki_review cmd="review" status="ok">
  <task_created id="xqrmnpst" name="Review: current changes" assignee="autonomous(codex)">
    Review task created and started.
    Running autonomous review...
  </task_created>
  
  <completed review_task="xqrmnpst" outcome="rejected" issues_found="2" duration_ms="9000">
    Review completed: Found 2 issues
    
    Followup task created: lpqrstwo
    Start with: aiki task start lpqrstwo
  </completed>
  
  <!-- outcome and issues_found derived from task graph:
       outcome = followup_task exists ? "rejected" : "approved"
       issues_found = count_children(followup_task) -->
  
  <context>
    <in_progress/>
    <list ready="4">
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p1"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_review>
```

### Review History

```bash
aiki review list
```

**Behavior:** Queries tasks with `task_type: Review`

```xml
<aiki_review cmd="list" status="ok">
  <reviews>
    <!-- outcome/issues_found derived from task graph -->
    <review task_id="xqrmnpst" name="Review: JWT auth" outcome="rejected" issues_found="2" 
            timestamp="2025-01-15T10:05:00Z" assignee="autonomous(codex)"/>
    <review task_id="pqrstuv" name="Review: Login flow" outcome="approved" issues_found="0"
            timestamp="2025-01-14T14:20:00Z" assignee="autonomous(codex)"/>
  </reviews>
</aiki_review>
```

### Review Details

```bash
aiki review show xqrmnpst
```

**Behavior:** Shows task details with review-specific formatting (derives `requested_by` from change metadata)

```xml
<aiki_review cmd="show" status="ok">
  <review task_id="xqrmnpst">
    <name>Review: JWT authentication</name>
    <assignee>autonomous(codex)</assignee>
    <requested_by>claude-code</requested_by>  <!-- Derived from change metadata -->
    <scope>working_copy</scope>
    <prompt>security</prompt>
    <outcome>rejected</outcome>  <!-- Derived: followup_task exists -->
    <issues_found>2</issues_found>  <!-- Derived: count_children(followup_task) -->
    <duration_ms>9000</duration_ms>
    <followup_task id="lpqrstwo" name="Followup: JWT auth review">
      <child id="lpqrstwo.1" name="Fix: Null pointer check" priority="p0"/>
      <child id="lpqrstwo.2" name="Fix: JWT validation" priority="p1"/>
    </followup_task>
    <body>
      # Review Results
      
      **Duration**: 9s
      
      ## Issues Found
      
      1. src/auth.ts:42 - Null pointer (p0)
      2. src/auth.ts:67 - JWT validation (p1)
      ...
    </body>
  </review>
</aiki_review>
```

---

## Task List Filtering

### Default Behavior

```bash
aiki task list
```

**Excludes autonomous tasks by default** - only shows interactive tasks:

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="4">
      <!-- No xqrmnpst (autonomous review task) -->
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p1"/>
      <task id="mxsl" name="Implement user auth" priority="p2" blocked_by="lpqrstwo"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_task>
```

### Include Autonomous Tasks

```bash
aiki task list --all
```

Shows both interactive and autonomous:

```xml
<list ready="5">
  <task id="xqrmnpst" name="Review: JWT auth" assignee="autonomous(codex)" status="closed"/>
  <task id="lpqrstwo" name="Followup: JWT auth review" priority="p1"/>
  <task id="mxsl" name="Implement user auth" priority="p2"/>
  ...
</list>
```

### Only Autonomous Tasks

```bash
aiki task list --autonomous
```

Shows only autonomous tasks:

```xml
<list ready="1">
  <task id="xqrmnpst" name="Review: JWT auth" assignee="autonomous(codex)" status="closed"/>
</list>
```

---

## Implementation

### Autonomous Execution Primitive

```rust
pub enum TaskAssignee {
    Interactive(String),      // "claude-code", "codex", etc.
    Autonomous(String),        // "autonomous(codex)", etc.
}

impl TaskAssignee {
    pub fn parse(s: &str) -> Self {
        if let Some(agent) = s.strip_prefix("autonomous(").and_then(|s| s.strip_suffix(")")) {
            Self::Autonomous(agent.to_string())
        } else {
            Self::Interactive(s.to_string())
        }
    }
    
    pub fn is_autonomous(&self) -> bool {
        matches!(self, Self::Autonomous(_))
    }
    
    pub fn agent_name(&self) -> &str {
        match self {
            Self::Interactive(name) | Self::Autonomous(name) => name,
        }
    }
}

// Task filtering
pub fn get_ready_queue(tasks: &HashMap<String, Task>, include_autonomous: bool) -> Vec<&Task> {
    tasks.values()
        .filter(|t| t.status == TaskStatus::Ready)
        .filter(|t| include_autonomous || !t.assignee.is_autonomous())
        .sorted_by_key(|t| &t.priority)
        .collect()
}
```

### Review Command Implementation

```rust
pub fn review(scope: ReviewScope, from: Option<String>) -> Result<()> {
    let agent = from.unwrap_or_else(|| "codex".to_string());
    
    // 1. Get originating task (if any)
    let originating_task = get_in_progress_task()?;
    
    // 2. Create review task
    let review_task_id = generate_task_id("review");
    let create_event = TaskEvent::Created {
        task_id: review_task_id.clone(),
        name: format!("Review: {}", get_scope_description(&scope)),
        priority: Priority::P1,
        assignee: TaskAssignee::Autonomous(agent.clone()),
        body: format!("Code review of {}", scope),
        metadata: hashmap! {
            "task_type" => "review",
            "scope" => scope.to_string(),
            "originating_task_id" => originating_task.as_ref().map(|t| &t.id),
        },
        blocks: originating_task.map(|t| vec![t.id]),
        timestamp: Utc::now(),
    };
    store_task_event(&create_event)?;
    
    // 3. Start task (begins autonomous execution)
    let start_event = TaskEvent::Started {
        task_ids: vec![review_task_id.clone()],
        stopped: vec![],
        agent_type: agent.clone(),
        timestamp: Utc::now(),
    };
    store_task_event(&start_event)?;
    
    // 4. Launch autonomous agent
    let result = launch_autonomous_agent(&agent, &review_task_id, &scope)?;
    
    // 5. Update task with results
    let close_event = TaskEvent::Closed {
        task_id: review_task_id.clone(),
        timestamp: Utc::now(),
        body: format_review_results(&result),
        metadata: hashmap! {
            "duration_ms" => result.duration_ms,
            "followup_task_id" => result.followup_task_id,
            // outcome and issues_found derived from task graph, not stored
        },
    };
    store_task_event(&close_event)?;
    
    // 6. Create followup tasks if issues found
    if !result.issues.is_empty() {
        create_followup_tasks(&review_task_id, &result, originating_task)?;
    }
    
    Ok(())
}

fn launch_autonomous_agent(agent: &str, task_id: &str, scope: &ReviewScope) -> Result<ReviewResult> {
    // Launch agent in headless mode
    // Agent reads task from task system
    // Agent performs review
    // Agent returns results
    // This is the reusable "autonomous task execution" primitive
    
    todo!("Implement autonomous agent launcher")
}
```

---

## Benefits of Task-Native Approach

### 1. Unified System

**One event storage branch** (`aiki/tasks`):
- Reviews
- Interactive work
- Followup tasks
- All tracked together

**One query interface**:
- `aiki task list --type Review` shows reviews
- `aiki task show xqrmnpst` shows review details
- All task queries work on reviews

### 2. Reusable Autonomous Execution

**Not just for reviews**:
```yaml
# Autonomous testing
task_id: abc123
name: "Run test suite"
assignee: autonomous(test-runner)

# Autonomous documentation
task_id: def456
name: "Generate API docs"
assignee: autonomous(doc-writer)

# Autonomous refactoring
task_id: ghi789
name: "Refactor auth module"
assignee: autonomous(claude-code)
```

### 3. Composable Agent Workflows

**Agents can create autonomous subtasks**:
```yaml
# Human creates task
task: "Implement feature X"
assignee: claude-code

# Agent creates review subtask
subtask: "Review feature X"
assignee: autonomous(codex)

# Review creates followup
followup: "Fix review issues"
assignee: claude-code
```

### 4. Natural Blocking

**Task dependencies work automatically**:
- Followup blocks review task
- Review task blocks originating task
- All visible in task graph

### 5. Simplified Architecture

**No need for**:
- Separate `aiki/reviews` branch
- Review-specific storage format
- Review-specific query logic
- Synchronization between systems

---

## Migration from Current Design

### Phase 1: Add Autonomous Assignee Support

- Add `TaskAssignee` enum to task types
- Update task creation to parse `autonomous(agent)` syntax
- Update task list filtering to exclude autonomous by default

### Phase 2: Implement Autonomous Execution

- Add `launch_autonomous_agent()` function
- Agent reads task from task system
- Agent updates task body with results
- Agent can create subtasks

### Phase 3: Implement Task-Native Reviews

- `aiki review` creates autonomous task
- Results stored in task body
- Followup tasks created from results

### Phase 4: Migrate Existing Reviews (Optional)

- If `aiki/reviews` branch exists, can keep for history
- Or migrate old reviews to task format
- New reviews use task-native approach

---

## Open Questions

1. **Autonomous vs Headless vs Auto terminology?**
   - Current proposal: `autonomous(agent)`
   - Alternatives: `headless(agent)`, `auto(agent)`
   - Need to decide on final naming

2. **Task metadata vs structured fields?**
   - Current: Review results in task `body` (markdown)
   - Alternative: Add structured review fields to task schema
   - Body is more flexible, fields are more queryable

3. **Should `aiki review` be a separate command or `aiki task` subcommand?**
   - Current: `aiki review @` for convenience
   - Alternative: `aiki task create --type review --autonomous codex`
   - Keep `aiki review` as sugar over task creation

4. **How to handle long-running autonomous tasks?**
   - Reviews should be quick (seconds/minutes)
   - But what if autonomous task takes hours?
   - Need progress updates, ability to cancel, etc.

---

## Summary

This design makes **tasks the universal orchestration layer** for agent work:

- **Reviews become tasks** assigned to autonomous agents
- **No separate storage system** - unified on `aiki/tasks` branch  
- **Autonomous execution is reusable** - any task can be autonomous
- **Natural composition** - agents can create autonomous subtasks
- **Simpler architecture** - one system, one query interface

The `autonomous(agent)` assignee pattern ensures autonomous tasks don't interfere with interactive workflows while keeping them observable and queryable.

This positions tasks as the foundation for **human-agent collaboration**, not just task tracking.

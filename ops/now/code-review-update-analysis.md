# Code Review System: Integration with Full Task System

**Date**: 2026-01-10  
**Status**: Design Complete  
**Purpose**: Design review system integration assuming ALL task system phases (1-5) are implemented

---

## Executive Summary

The task system is now fully implemented with all Phase 1-5 features. The review system can leverage:

1. **Hierarchical Tasks** - Parent review task with child issues for scoped workflows
2. **Rich Metadata** - Full `body`, `scope`, `discovered_from` fields for task context
3. **Native Flow Actions** - `review:` action integrated with task system
4. **Self Functions** - `self.review_pending()`, `self.review_last_outcome()` for flow logic
5. **AGENTS.md Integration** - Review commands in agent system prompts
6. **Scoped Queues** - Start parent review task to see only review issues
7. **Comments & Updates** - Add review context to tasks, update metadata
8. **Task-to-Change Linking** - Query review history via JJ metadata

This document provides the complete integration design for the review system.

---

## Data Model: Full Integration

### Review Execution and Followup Task Creation

**`aiki review` runs immediately and creates followup tasks if issues found:**

```yaml
# Review started event
---
aiki_review_event: v1
review_id: xqrmnpst
event: started
timestamp: 2025-01-15T10:04:50Z
requested_by: claude-code
reviewed_by: codex
scope: working_copy
originating_task_id: mxsl  # Task that was in progress when review was run (if any)
---

# Review completed event
---
aiki_review_event: v1
review_id: xqrmnpst
event: completed
timestamp: 2025-01-15T10:05:00Z
requested_by: claude-code
reviewed_by: codex
outcome: rejected
issues_found: 2
followup_task_id: lpqrstwo
---

# Followup parent task created (if issues found)
---
aiki_task_event: v1
task_id: lpqrstwo
event: created
timestamp: 2025-01-15T10:05:01Z
name: "Followup: JWT authentication review"
priority: p1
body: |
  Code review completed by codex (review:xqrmnpst)
  
  Found 2 issues requiring fixes.
  Start this task to scope to review issues only.
scope:
  files:
    - path: src/auth.ts
    - path: src/middleware.ts
discovered_from: "review:xqrmnpst"
blocks: ["mxsl"]  # Blocks originating task if review was run from task context
assignee: claude-code
---

# Child task 1
---
aiki_task_event: v1
task_id: lpqrstwo.1
event: created
timestamp: 2025-01-15T10:05:02Z
name: "Fix: Potential null pointer dereference"
priority: p0
body: |
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
discovered_from: "review:xqrmnpst"
blocks: []
assignee: claude-code
---

# Child task 2
---
aiki_task_event: v1
task_id: lpqrstwo.2
event: created
timestamp: 2025-01-15T10:05:03Z
name: "Fix: JWT token validation missing"
priority: p1
body: |
  **Review**: review:xqrmnpst
  **File**: src/auth.ts:67-69
  **Severity**: warning
  
  ## Issue
  JWT token signature is not validated before use
  
  ## Security Impact
  Attacker could forge tokens with modified claims
  
  ## Suggested Fix
  ```typescript
  const decoded = jwt.verify(token, process.env.JWT_SECRET);
  ```
scope:
  files:
    - path: src/auth.ts
      lines: [67, 68, 69]
discovered_from: "review:xqrmnpst"
blocks: []
assignee: claude-code
---
```

**Key Features Used:**
- ✅ Hierarchical task IDs (parent + children)
- ✅ Rich `body` field with formatted content
- ✅ File/line `scope` for precise targeting
- ✅ `discovered_from` linking back to review
- ✅ `blocks` for task dependencies - followup blocks originating task
- ✅ Priority mapping from severity
- ✅ `originating_task_id` captures task context when review is run

---

## CLI Output: XML with Context

### Review Command (Immediate Execution)

```bash
aiki review @ --from codex
```

**Output (XML with context):**
```xml
<aiki_review cmd="review" status="ok">
  <completed review_id="xqrmnpst" from="codex" scope="working_copy" outcome="rejected">
    Review completed: Found 2 issues
    
    Followup task created: lpqrstwo
    Start with: aiki task start lpqrstwo
  </completed>
  
  <context>
    <in_progress/>
    <list ready="4">
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p1"/>
      <task id="mxsl" name="Implement user auth" priority="p2"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_review>
```

### Review Show Command

```bash
aiki review show xqrmnpst
```

**Output:**
```xml
<aiki_review cmd="show" status="ok">
  <review id="xqrmnpst" status="completed">
    <requested_by>claude-code</requested_by>
    <reviewed_by>codex</reviewed_by>
    <scope type="working_copy">
      <file>src/auth.ts</file>
      <file>src/middleware.ts</file>
    </scope>
    <outcome>rejected</outcome>
    <issues_found>2</issues_found>
    <parent_task id="lpqrstwo" name="Review: JWT auth implementation"/>
    <child_tasks>
      <task id="lpqrstwo.1" name="Fix: Null pointer check" priority="p0"/>
      <task id="lpqrstwo.2" name="Fix: JWT validation" priority="p1"/>
    </child_tasks>
    <followup_task id="lpqrstwo" name="Followup: JWT auth review"/>
  </review>
  
  <context>
    <in_progress/>
    <list ready="5">
      <task id="lpqrstwo" name="Followup: JWT auth review" priority="p1"/>
      <task id="mxsl" name="Implement user auth" priority="p2"/>
      <task id="npts" name="Add tests" priority="p2"/>
      <task id="oqru" name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_review>
```

---

## Flow Integration: Native Review Action

### Review Action Definition

```rust
pub enum Action {
    // ... existing actions
    Review {
        scope: ReviewScope,
        from: Option<String>,           // Agent name (default: codex)
        prompt: Option<String>,         // Prompt template name
        context: Option<String>,        // Additional context
        create_followup_tasks: bool,    // Create followup tasks if issues found (default: true)
        followup_name: Option<String>,  // Followup parent task name
        assign_to: Option<String>,      // Assignee (default: current agent)
        quick: bool,                    // Quick mode (faster, less thorough)
    },
    // ... rest
}
```

### Flow Examples

**Simple review (creates followup tasks automatically):**
```yaml
response.received:
  - review:
      scope: session
```

**Advanced review with full options:**
```yaml
change.completed:
  - review:
      scope: working_copy
      from: codex
      prompt: security
      context: "Pre-merge security audit"
      create_followup_tasks: true
      followup_name: "Security Followup: ${event.change_description}"
      assign_to: $event.author
      quick: false
```

**Conditional review:**
```yaml
response.received:
  - let needs_review = $event.session.modified_files_count > 5
  - if $needs_review
    then:
      - review:
          scope: session
      
      - log: "Running review due to large changeset"
```

### Review Completed Event

```yaml
review.completed:
  - log: "Review ${event.review.id}: ${event.review.outcome}"
  
  - if: $event.review.issues_found > 0
    then:
      - log: |
          ⚠️  Review found ${event.review.issues_found} issue(s)
          
          Followup task: ${event.review.followup_task_id}
          Start with: aiki task start ${event.review.followup_task_id}
      
      - autoreply:
          append: |
            Review found ${event.review.issues_found} issue(s). Created followup task with subtasks.
            
            To work on review issues:
            ```bash
            aiki task start ${event.review.followup_task_id}
            ```
            This will scope you to review tasks only.
  
  - if: $event.review.outcome == "approved"
    then:
      - log: "✅ Review approved - no issues found"
```

### Self Functions for Reviews

**Available functions:**
```rust
self.review_list(limit: n)     // List recent reviews (most recent first)
self.review_last()             // Last completed review
self.review_last_outcome()     // "approved" | "rejected" | etc
self.review_last_id()          // Review ID
self.review_get(id)            // Get specific review by ID
```

**Usage in flows:**
```yaml
session.started:
  - let last = self.review_last()
  - if $last && $last.issues_found > 0
    then:
      - context:
          append: |
            ⚠️  Recent review found ${last.issues_found} issue(s)
            Followup task: ${last.followup_task_id}
            
            View details: `aiki review show ${last.id}`

prompt.submitted:
  - let last_review = self.review_last()
  - if $last_review && $last_review.outcome == "rejected"
    then:
      - context:
          append: |
            Note: Last review (${last_review.id}) found ${last_review.issues_found} issues.
            Consider addressing review tasks before new work.
```

---

## Review Task Lifecycle: Complete Workflow

### Step-by-Step with Scoped Queues

**1. Run Review (from task context)**
```bash
# While working on task "mxsl"
aiki review @
```

Review executes immediately. If issues found, creates:
- Review event on `aiki/reviews` branch
- Followup parent task: `lpqrstwo` (blocks originating task "mxsl")
- Child tasks: `lpqrstwo.1`, `lpqrstwo.2`, etc.

**Task "mxsl" is now blocked** - must fix review issues first!

**2. Start Followup Task (Enters Review Scope)**
```bash
aiki task start lpqrstwo
```

```xml
<aiki_task cmd="start" status="ok" scope="lpqrstwo">
  <started>
    <task id="lpqrstwo" name="Followup: JWT review" priority="p1"/>
  </started>
  
  <context>
    <in_progress>
      <task id="lpqrstwo" name="Followup: JWT review"/>
    </in_progress>
    <list ready="2" scope="lpqrstwo">
      <!-- Only shows children of lpqrstwo -->
      <task id="lpqrstwo.1" name="Fix: Null pointer" priority="p0"/>
      <task id="lpqrstwo.2" name="Fix: JWT validation" priority="p1"/>
    </list>
  </context>
</aiki_task>
```

**Scope is now `lpqrstwo`** - only see review issues!

**3. Work on First Issue**
```bash
aiki task start lpqrstwo.1
```

Work on the fix...

```bash
aiki task close lpqrstwo.1
```

**4. Work on Second Issue**
```bash
aiki task start lpqrstwo.2
```

Work on the fix...

```bash
aiki task close lpqrstwo.2
```

**5. All Children Closed → Parent Auto-Starts**

When last child closes, parent automatically starts for verification:

```xml
<aiki_task cmd="close" status="ok" scope="lpqrstwo">
  <closed>
    <task id="lpqrstwo.2" name="Fix: JWT validation"/>
  </closed>
  
  <notice>
    All children complete. Followup task (lpqrstwo) auto-started for verification.
  </notice>
  
  <context>
    <in_progress>
      <task id="lpqrstwo" name="Followup: JWT review"/>
    </in_progress>
    <list ready="0" scope="lpqrstwo"/>
  </context>
</aiki_task>
```

**6. Verify Fixes and Close Followup**
```bash
# Verify all fixes are good
aiki task close lpqrstwo
```

**Scope returns to root** - back to normal task queue!

---

## Task Comments and Updates from Reviews

### Adding Review Context to Tasks

```yaml
review.completed:
  - if: $event.review.issues_found > 0
    then:
      # Add comment to each task with review details
      - for issue in $event.review.issues:
          - task:
              action: comment
              task_id: $issue.task_id
              text: |
                📋 Review Details
                
                **Review ID**: ${event.review.id}
                **Reviewer**: ${event.review.reviewed_by}
                **Severity**: ${issue.severity}
                
                ${issue.suggestion}
```

### Updating Task Metadata

```yaml
review.completed:
  - for issue in $event.review.issues:
      - task:
          action: update
          task_id: $issue.task_id
          fields:
            priority: $issue.severity_to_priority
            scope: $issue.scope
            blocks: $issue.related_tasks
```

---

## AGENTS.md Integration

### Agent System Prompt

Add to `.aiki/AGENTS.md` during `aiki init`:

```markdown
## Code Reviews

Run reviews of your work to catch issues:

- `aiki review @` - Review current changes
- `aiki review @ --from codex --prompt security` - Security review
- `aiki review list` - List recent reviews
- `aiki review show <review-id>` - View review details

### Review Workflow

1. **Run review**: `aiki review @`
   - Review executes immediately
   - If issues found, creates followup task with subtasks
2. **Work on issues**: `aiki task start <followup-task-id>` (scopes to review tasks only)
3. **Fix each issue**: Start and close child tasks
4. **Verify**: Followup task auto-starts when all children complete
5. **Finalize**: Close followup to return to normal scope

Reviews that find issues create scoped followup tasks. Starting the followup task shows only review issues.
```

### Context Injection in Flows

```yaml
# .aiki/flows/bundled.yml
name: "default"
version: "1"

session.started:
  - let recent = self.review_list(limit: 3)
  - if $recent.length > 0
    then:
      - context:
          append: |
            📋 Recent Reviews:
            
            ${self.review_list(limit: 3) | map(r => "- " + r.id + ": " + r.outcome + " (" + r.issues_found + " issues)") | join("\n")}
            
            View: `aiki review show <id>`

prompt.submitted:
  - let last = self.review_last()
  - if $last.outcome == "rejected"
    then:
      - context:
          append: |
            ⚠️  Note: Recent review (${last.id}) found issues.
            Review tasks may need attention.

response.received:
  - let last = self.review_last()
  - if $last.issues_found > 0
    then:
      - autoreply:
          append: |
            ⚠️  Note: Recent review found ${last.issues_found} issues.
            Followup task: ${last.followup_task_id}
            
            Start with: aiki task start ${last.followup_task_id}
```

---

## Advanced Patterns

### Multi-Agent Review

```yaml
response.received:
  - review:
      scope: session
      from: [codex, claude-code]  # Multiple reviewers
      merge_strategy: union       # Combine all issues
  
  - log: "Running multi-agent review"
```

### Conditional Review Prompts

```yaml
change.completed:
  - let has_auth = $event.files | any(f => f.path | contains("auth"))
  - let has_crypto = $event.files | any(f => f.path | contains("crypto"))
  
  - let prompt = if $has_auth || $has_crypto
                 then "security"
                 else "default"
  
  - review:
      scope: working_copy
      prompt: $prompt
      followup_name: |
        ${if $prompt == "security" then "🔐 Security" else "📋"} Followup: ${event.files | join(", ")}
```

### Review History Queries

```yaml
session.started:
  - let last_review = self.review_last()
  
  - if $last_review && $last_review.outcome == "rejected"
    then:
      - let unresolved = $last_review.child_tasks 
                         | filter(t => t.status != "closed")
                         | length
      
      - if $unresolved > 0
        then:
          - context:
              append: |
                ⚠️  Previous review (${last_review.id}) has ${unresolved} unresolved issues.
                
                View: `aiki task list --scope ${last_review.followup_task_id}`
```

### Pre-Push Review Gate

```yaml
# .aiki/flows/pre-push.yml
name: "pre-push"
version: "1"

shell.permission_asked:
  - if: $event.command | contains("git push")
    then:
      - log: "Running pre-push review..."
      
      - review:
          scope: staged
          prompt: default
          create_followup_tasks: false  # Don't create tasks, just check
      
      # Review runs synchronously
      - if: $review.outcome == "rejected"
        then:
          - block: |
              ❌ Cannot push - review found ${review.issues_found} issue(s)
              
              Fix issues first, then push.
              Issues:
              ${review.issues | map(i => "- " + i.message) | join("\n")}
      
      - if: $review.outcome == "approved"
        then:
          - log: "✅ Review passed - proceeding with push"
```

---

## Task Queries and History

### Query Tasks from Review

```bash
# Find all tasks from specific review
aiki task list --discovered-from review:xqrmnpst

# Show task with source review
aiki task show lpqrstwo.1 --show-source
```

### JJ Metadata Queries

```bash
# Find changes linked to review tasks
jj log -r 'description(glob:"*review:xqrmnpst*")'

# Find all changes with review task metadata
jj log -r 'description(glob:"*task=rev-*")'
```

### Flow-Based Queries

```yaml
session.started:
  - let review_tasks = self.task_list() 
                       | filter(t => t.discovered_from | starts_with("review:"))
  
  - if $review_tasks.length > 0
    then:
      - context:
          append: |
            🔍 You have ${review_tasks.length} review task(s) pending:
            ${review_tasks | map(t => "- " + t.id + ": " + t.name) | join("\n")}
```

---

## Implementation Checklist

### Review System V1.0 (Complete)

**Core Features:**
- ✅ Event storage on `aiki/reviews` branch
- ✅ Native `review:` flow action
- ✅ XML output with `<context>` elements
- ✅ Multiple agent support (codex, claude-code, cursor)

**Task Integration:**
- ✅ Full task metadata (body, scope, discovered_from)
- ✅ Hierarchical task creation (parent + children)
- ✅ Scoped queues (start parent → see only children)
- ✅ Auto-start parent when all children closed
- ✅ Task comments from review results
- ✅ Task updates from review metadata

**Flow Integration:**
- ✅ `review.completed` event
- ✅ Self functions: `review_pending()`, `review_last()`, etc.
- ✅ Flow composition with `before`/`after`
- ✅ Conditional review logic

**Agent Integration:**
- ✅ AGENTS.md documentation
- ✅ Context injection in flows
- ✅ Review commands in system prompts
- ✅ Workflow guidance

---

## Summary

The review system fully leverages the complete task system (Phases 1-5):

**Hierarchical Tasks**: Reviews create parent + child tasks for scoped workflows  
**Rich Metadata**: Tasks include body, scope, discovered_from for full context  
**Native Actions**: `review:` action integrated with task and flow systems  
**Self Functions**: Flow logic based on review state  
**AGENTS.md**: Full agent integration with context injection  
**Scoped Queues**: Start parent review task to focus on review issues only  
**Task Operations**: Comments and updates link reviews to ongoing work  
**Query Integration**: Find review tasks via metadata and JJ history  

This provides a complete, production-ready code review system that works seamlessly with the task system's advanced features.

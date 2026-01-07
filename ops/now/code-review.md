# Aiki Review Feature Implementation Plan

## Executive Summary

Implement a multi-model AI code review system with longitudinal tracking, enabling developers to get specialized review feedback from different LLMs via both CLI commands and flow actions. Unlike stateless review tools, Aiki review creates a causal graph connecting reviews to changes, fixes, and re-reviews—making review a first-class operation in the change history.

## Design Philosophy

**Oracle is a snapshot. Aiki is a memory.**

Oracle-style review (Amp Code, Cursor) provides stateless judgment: run a review, get feedback, done. Aiki review is longitudinal reasoning: reviews create tasks, issues become subtasks, and fixing is normal task completion tracked in the event-sourced task system.

**Core Principles:**
1. **Reviews are tasks** - A review creates a parent task with subtasks for each issue found
2. **Issues are subtasks** - Each review issue becomes a subtask with file/line context
3. **Fixing is task completion** - Fixing issues = completing subtasks, tracked with full provenance
4. **Read-only enforcement** - Review agents are cryptographically isolated from write operations
5. **Task system integration** - Leverages existing task system from milestone-1.4 for persistence and querying

**The Architecture:**

```
Review (Parent Task)
├─ Issue 1 (Subtask) → Fixed → JJ change with [aiki] metadata
├─ Issue 2 (Subtask) → Fixed → JJ change with [aiki] metadata
└─ Issue 3 (Subtask) → Fixed → JJ change with [aiki] metadata
```

All stored on the `aiki/tasks` branch using the event-sourced task system. When all issue subtasks are fixed, the review task is complete.

**Key Benefits:**
- **Unified system** - Reviews, issues, and fixes all use the same task infrastructure
- **Full provenance** - Every fix links back to the issue task and review task
- **Queryable history** - Use task queries: `aiki task list --type review`, `aiki task show review-123`
- **Natural workflow** - Agents see "ready tasks" which include review issues to fix
- **Composable** - Review issues can block other tasks, be assigned, etc.

This is what makes Aiki review fundamentally different from Oracle: **reviews create trackable work units** that integrate with the broader development workflow, not just ephemeral feedback.

---

## Research Summary

### What Makes Oracle Valuable

**Amp Code's Oracle Design:**
- Read-only subagent powered by GPT-5 (previously o3)
- Specialized for: code review, debugging, analysis, architectural decisions
- Explicitly invoked rather than automatic (cost/speed management)
- Works alongside main agent (Sonnet 4) - "one writing, one reviewing"
- Slower and more expensive but "impressively good at reviewing, debugging, analyzing"

**Developer Feedback (Mitchell Hashimoto & others):**
- "Amp Code freaking cooks" - strong endorsement from HashiCorp founder
- Oracle enables developers to separate coding speed from review depth
- Multi-model approach: right tool for the right job
- Explicit invocation prevents cost bloat while enabling deep analysis when needed

**Key Use Cases:**
- "Use the oracle to review the last commit's changes"
- "Work with the oracle to figure out refactoring while keeping backwards compatibility"
- Debugging complex, non-working code
- Understanding existing code behavior before modifications

---

## Core Design Principles

### 1. **Headless CLI-First Architecture**
All LLM interactions happen via headless CLI commands (not API clients). This provides:
- Process isolation and security
- Easy model switching (just change the binary)
- Language-agnostic (works with any CLI tool)
- Familiar integration pattern (matches existing `shell:` and `jj:` actions)

**Security: Read-Only Enforcement**
- Review agents are cryptographically isolated from write operations
- Cannot modify files, create commits, or execute shell commands
- Receives diff content via stdin, returns review comments via stdout
- If an adapter cannot enforce read-only semantics, reviews are marked `advisory_only: true`
- Advisory reviews are tracked but excluded from blocking operations
- Prevents accidental or malicious modifications during review

### 2. **Multiple Agent Options with Thinking Modes**
Support for multiple review agents via native headless CLIs, each with optional deep-thinking mode:

**Agents:**
- **claude** - Opus (default) / Sonnet (--quick)
- **codex** - o3 (default) / o4-mini (--quick)
- **gemini** - Gemini Thinking (default) / Flash (--quick)

**Thinking Modes:**
- Default: Deep-thinking models for thorough analysis (Opus, o3, Thinking)
- `--quick`: Fast models for rapid feedback (Sonnet, o4-mini, Flash)

### 3. **Flexible Invocation**
Two ways to invoke review:
- **CLI command**: `aiki review [options]` - Manual reviews, one-off checks
- **Flow action**: `review:` action in flow YAML - Automated workflows, policy enforcement

### 4. **Context-Aware Scoping**
Review different scopes automatically:
- Working copy changes (`@` in JJ)
- Staged changes (Git staging area)
- Specific JJ changes (via change ID or revset like `@-`, `main`)
- Specific Git commits (via commit SHA)
- File-specific reviews
- Diff-based reviews (between any two references)

---

## Architecture Overview

### CLI Command - `aiki review`

**Command Interface:**

`aiki review` produces a judgment: analyzes changes, records findings, and emits a `review.completed` event.

- Basic usage: `aiki review` (reviews current changes with one non-authoring agent)
- JJ changes: `aiki review <change-id-or-revset>` (e.g., `@-`, `main`)
- Git staged: `aiki review git --staged` (staged changes)
- Git commits: `aiki review git <commit-sha>` (specific commit)
- File targeting: `aiki review --files <file1> <file2>...`
- Quick mode: `aiki review --quick` (uses fast models for rapid feedback)
- Self-review: `aiki review --self` (uses the authoring agent to review its own work)

**What happens after review:**
The `review.completed` event is emitted with review results. The `aiki/default` flow responds to this event by attempting auto-remediation (see Default Remediation Flow section below).

**Smart Agent Selection:**
By default, `aiki review` automatically selects a different agent than the one that authored the changes:
- Reads `[aiki]` metadata from JJ change description to identify authoring agent
- Selects one non-authoring agent for independent review
- Example: If claude authored, might use codex or gemini to review
- Falls back to configured default agent if no authorship metadata found

**Human-Authored Changes:**
For changes without `[aiki]` metadata (human-authored):
- **For review**: Uses the configured default review agent
- **For auto-remediation fixes**: Uses the last coding agent from session history
- Falls back to default configured agent if no session history available

**Self-Review Mode:**
`--self` mode uses the authoring agent to review its own work:
- Identifies the authoring agent from `[aiki]` metadata
- Uses the same agent that wrote the code to review it
- Useful for iterative improvement and catching the agent's own mistakes
- Example: If claude authored, self-review uses claude
- **No-op for human changes**: Ignored if no `[aiki]` metadata found (falls back to default single-agent mode)

**Prompt History Integration:**

Review leverages the `aiki/conversations` branch (see `ops/now/prompt-history.md`) to provide context about *why* code exists:

1. **Intent from current change**: If reviewing a JJ change with conversation history, include the intent summary
2. **Historical context**: Include recent changes to the target files (last 5 turns) with their intents
3. **Reviewer context**: Pass this context to the review agent so it understands:
   - What the developer was trying to accomplish
   - Recent changes to the same files
   - Evolution of the code

**Example review context:**

```yaml
target:
  type: "change"
  change_id: "xyz789"
  intent: "add JWT validation middleware"  # From conversation history
  context:
    - turn: 2
      intent: "core auth service"
      files: ["src/auth.ts"]
    - turn: 1
      intent: "JWT authentication service"
      files: ["src/auth.ts", "src/routes/login.ts"]
```

This allows the reviewer to understand "we're adding middleware for the JWT auth system we just built" rather than just seeing code changes in isolation.

**Task System Integration:**

The `aiki review` command creates a single task on the `aiki/tasks` branch:

1. **Review task** - The review itself (type: `review`)
   - **Subtasks** - Each issue found becomes a subtask (type: `error`/`warning`/`suggestion`)

The review task ID is included in the `review.completed` event. Query subtasks with: `aiki task list --parent review-abc123`

**Events:**

After a review completes, the `review.completed` event is emitted:

```rust
review.completed {
  review: {
    id: String,              // UUID for this review
    task_id: String,         // Task ID on aiki/tasks branch (e.g., "review-abc123")
                             // Subtasks (issues) can be queried: aiki task list --parent review-abc123
    agent: String,           // Agent that performed review (e.g., "codex")
    mode: String,            // "default" or "self"
    thinking_mode: String,   // "deep" or "quick"
    prompt: String,          // Prompt template used ("default", "security", "performance")
    issues_found: usize,     // Number of issues detected (= number of subtasks created)
    target: {
      type: String,          // "change" | "git_commit" | "git_staged" | "files"
      change_id: Option<String>,     // JJ change ID (if type="change")
      commit_sha: Option<String>,    // Git commit SHA (if type="git_commit")
      files: Vec<PathBuf>,           // File paths (always populated, may be all files in scope)
      author: Option<String>,        // Agent that authored (from [aiki] metadata, if available)
      intent: Option<String>,        // Intent summary from prompt history (if available)
      context: Vec<HistoryEntry>,    // Recent changes to target files (from aiki/conversations branch)
    },
  },
  session: AikiSession,      // Standard session context
  cwd: PathBuf,              // Working directory
  timestamp: DateTime<Utc>,  // When review completed
}

// HistoryEntry provides context about why code exists
struct HistoryEntry {
  turn: usize,               // Turn number in session
  session_id: String,        // Session that made the change
  timestamp: DateTime<Utc>,  // When the change was made
  intent: String,            // Why the change was made
  files: Vec<PathBuf>,       // Files modified in this turn
}
```

**Task Definitions Created:**

When `aiki review` runs, it creates tasks using the task system from milestone-1.4. **Issue subtasks are automatically assigned to the authoring agent** (the agent that wrote the code being reviewed).

**Task Status and Events:**

Review integration extends the task system with new statuses and events:

```rust
pub enum TaskStatus {
    Open,          // Task created but not started
    InProgress,    // Task being worked on
    NeedsReview,   // Work done, awaiting review
    NeedsFix,      // Review found issues, needs rework
    NeedsHuman,    // Max retries exceeded, needs human attention
    Closed,        // Task finished (approved or abandoned)
}

pub enum EventType {
    Created { task: TaskDefinition },
    Started,                           // Open → InProgress
    CompletedWork,                     // InProgress → NeedsReview
    ReviewCompleted { 
        issues_found: usize,
        review_id: String 
    },                                 // NeedsReview → NeedsFix or Closed(Approved)
    FixStarted,                        // NeedsFix → InProgress
    NeedsHuman { 
        reason: NeedsHumanReason,
        attempts: u32 
    },                                 // NeedsFix → NeedsHuman (max retries)
    Closed { reason: ClosureReason },  // Any → Closed
    
    // ... existing assignment/dependency events
}

pub enum NeedsHumanReason {
    MaxRetriesExceeded,    // Too many fix iterations
    ReviewerDisagreement,  // Multiple reviewers can't agree
    ComplexityThreshold,   // Task too complex for automation
}

pub enum ClosureReason {
    Approved,              // Review passed
    Abandoned,             // Given up by human
    Fixed,                 // Successfully fixed
}
```

```yaml
# Parent review task
---
aiki_task_event: v1
task_id: review-abc123
event: created
timestamp: 2025-01-15T10:30:00Z
agent_type: codex
task:
  objective: "Review changes in @"
  type: review
  scope:
    files:
      - path: src/auth.ts
  evidence:
    source: review
    message: "Code review of JWT authentication changes"
---

# Issue subtask 1 - Auto-assigned to authoring agent
---
aiki_task_event: v1
task_id: review-abc123.1
event: created
timestamp: 2025-01-15T10:30:01Z
agent_type: codex
task:
  objective: "Fix null pointer dereference at src/auth.ts:42"
  type: error
  parent_id: review-abc123
  assignee: claude-code  # Authoring agent from JJ [aiki] metadata
  scope:
    files:
      - path: src/auth.ts
        lines: [42]
  evidence:
    source: codex_review
    message: "Potential null pointer dereference. Add null check before accessing user.name"
    code: "REVIEW_NULL_CHECK"
---

# Issue subtask 2 - Auto-assigned to authoring agent
---
aiki_task_event: v1
task_id: review-abc123.2
event: created
timestamp: 2025-01-15T10:30:02Z
agent_type: codex
task:
  objective: "Add input validation for JWT token"
  type: warning
  parent_id: review-abc123
  assignee: claude-code  # Authoring agent from JJ [aiki] metadata
  scope:
    files:
      - path: src/auth.ts
        lines: [67, 68, 69]
  evidence:
    source: codex_review
    message: "JWT token should be validated before use"
    code: "REVIEW_VALIDATION"
---
```

**Fixing Issues:**

When an agent fixes an issue, it's just a normal code change that closes a task. The existing `change.completed` event already includes task information from milestone-1.4:

```rust
change.completed {
  // ... standard change.completed fields ...
  tasks: {
    works_on: Vec<String>,   // Task IDs being worked on
    closes: Vec<String>,     // Task IDs closed by this change (e.g., ["review-abc123.1"])
  },
}
```

When a change closes a review issue task:
1. `change.completed` event fires with the issue task ID in `tasks.closes`
2. Task system marks the subtask as closed
3. When all subtasks of a review are closed, the review task auto-completes
4. Flows can detect this: `aiki task show review-abc123` → status: `Closed`

**Nested Structure Rationale:**

Following GitHub's webhook pattern, events use nested objects for logical grouping:
- `review.*` - All review data: who reviewed (`agent`), what was reviewed (`target.*`), and results (`issues_found`, `issues`)
- `review.target.*` - What was reviewed: type, identifiers, `files`, `author`, **intent**, and historical **context**
  - Includes intent from prompt history to understand *why* the change was made
  - Includes recent changes to same files for continuity
- `fix.*` - All fix data: who fixed (`agent`), what was fixed (`changes`, `files_modified`), outcome (`issues_fixed`), and what it addresses (`review.id`)
- `fix.review.*` - The review this fix addresses (`id`)
- Top-level: Standard session context (`session`, `cwd`, `timestamp`)

This provides clear namespacing and natural ownership - all data about an entity lives under that entity. The inclusion of prompt history context enables reviewers to understand developer intent, not just see code changes.

Flows can hook into these events to handle review results and fixes however they want.

**Default Remediation Flow (`aiki/default`):**

The `aiki/default` flow provides batteries-included auto-remediation using the `fix:` action for headless task completion.

**Two Remediation Modes:**

1. **Interactive Agent Mode** - Agent sees tasks in their work queue naturally
2. **Headless Auto-Remediation** - Flow triggers `fix:` action to start work headlessly

**Headless Auto-Remediation with Iterations:**

```yaml
# aiki/flows/default.yml

review.completed:
  - if: $event.review.issues_found > 0
    then:
      - log: "Review found {{$event.review.issues_found}} issues, fixing..."
      - fix: 
          review: $event.review.id
          max_iterations: 3  # Try up to 3 times per issue
          quick: true        # Use quick mode for first 2 iterations
      # If all issues fixed, task status is Closed(Approved)
      # If max iterations reached, task status is NeedsHuman(MaxRetriesExceeded)
      - log: "Fix cycle complete"
      
      # Check for tasks that need human attention
      - shell: "aiki task list --status needs-human --parent review-*"
        alias: needs_human
      - if: $needs_human | length > 0
        then:
          - log: "⚠️  {{$needs_human | length}} issue(s) need human review"
```

**How Iterations Work:**

For each issue subtask, the `fix:` action will:
1. **Iteration 1**: Fix with quick mode → re-review
2. **Iteration 2**: If still failing, fix with quick mode → re-review
3. **Iteration 3**: If still failing, fix with deep-thinking → re-review
4. **Result**: Either `Closed(Approved)` or `NeedsHuman(MaxRetriesExceeded)`

The iteration happens **inside** the `fix:` action, so the flow just calls it once and waits for all retries to complete.

**Tasks That Need Human Attention:**
- Status: `NeedsHuman` (not `Closed`)
- Still appear in task queries
- Can be worked on by humans
- When human fixes and closes: `Closed(Fixed)`
- Query these tasks: `aiki task list --status needs-human`

**How the `fix:` Action Works:**

The `fix:` action is a convenience wrapper around `task: { start: ..., headless: true }` with smart agent selection and iteration support for review remediation.

1. Takes a review ID and looks up the corresponding task ID
2. Iterates through all issue subtasks (which are already assigned to the authoring agent)
3. For each subtask (blocking operation):
   - Loads task details (objective, scope, evidence, assignee)
   - Uses the assigned agent (authoring agent from task metadata)
   - Marks task as `InProgress` (fires `Started` event)
   - Invokes agent headlessly to fix the issue
   - Creates JJ change with `[aiki]` metadata
   - Marks task as `NeedsReview` (fires `CompletedWork` event)
   - Runs quick re-review of the specific file/lines from the task
   - If review passes: marks subtask as `Closed(Approved)` (fires `Closed` event)
   - If review fails: marks subtask as `NeedsFix` and retries (up to max iterations)
   - If max iterations exceeded: marks subtask as `NeedsHuman(MaxRetriesExceeded)` (fires `NeedsHuman` event)
4. When all subtasks are completed, parent review task auto-completes
5. Flow continues to next action

**Task Status Flow:**
```
Open → InProgress → NeedsReview → NeedsFix → InProgress → NeedsReview → Closed(Approved)
        ↑______________|             |___________|            |
                                     ↓                        
                                  NeedsHuman (max retries)
                                     ↓
                              Human fixes → Closed(Fixed)
```

**Note:** Issue tasks are auto-assigned to the authoring agent when created, so `fix:` doesn't need to read JJ metadata - it just uses the task's `assignee` field.

**Equivalent generic form:**
```yaml
# fix: is syntactic sugar for:
- task:
    start: <review.task_id>  # Looked up from review ID
    headless: true
    # Agent comes from task.assignee (set when issue task was created)
```

### Fix Action Lifecycle: JJ Changes and Events

This section documents the complete lifecycle of a `fix:` action, showing every JJ change created and every event emitted.

**Initial State:**
```
JJ changes from session (reviewed scope: session):
  @ (change_id: abc123) - Add JWT token validation
    [aiki] agent=claude-code, session=xyz, tool=Edit
  ○ (change_id: def456) - Implement auth middleware  
    [aiki] agent=claude-code, session=xyz, tool=Edit
  ○ (change_id: ghi789) - Create user model
    [aiki] agent=claude-code, session=xyz, tool=Edit

Review found issues across these changes.

aiki/tasks branch:
  - review-456 (parent task, status: Open, scope: session xyz)
    - review-456.1 (issue task, status: Open, assignee: claude-code)
      scope: src/auth.ts:42 (in change abc123)
    - review-456.2 (issue task, status: Open, assignee: claude-code)
      scope: src/middleware.ts:67 (in change def456)
```

**Flow triggers fix:**
```yaml
- fix:
    review: review-456
    max_iterations: 3
    quick: true
```

**Fix Task Created:**

1. **Create Parent Fix Task with Structured Subtasks**
   ```yaml
   # aiki/tasks branch - new task created
   ---
   aiki_task_event: v1
   task_id: fix-789
   event: created
   timestamp: 2025-01-15T10:35:00Z
   agent_type: claude-code
   task:
     objective: "Address all issues from review-456"
     type: fix_review
     assignee: claude-code  # Inherited from issue tasks
     works_on:
       - review-456.1  # Links to all issue subtasks
       - review-456.2
     context:
       review_id: review-456  # Can fetch full issue details from these tasks on demand
       session_id: xyz  # Original session that created the reviewed changes
     subtasks:
       - fix-789.1:
           objective: "Analyze context and create implementation plan"
           description: |
             1. Digest prompt/intent from session xyz (understand why code was written)
             2. Digest code from reviewed changes (abc123, def456, ghi789)
             3. Digest review findings and all issue subtasks
             4. Create solution plan with implementation subtasks
           status: open
       - fix-789.2:
           objective: "Implement solution"
           status: open
       - fix-789.3:
           objective: "Verify all issues resolved"
           status: open
   ---
   ```

2. **Invoke Agent Headlessly to Execute Task**
   - Agent: `claude-code` (from fix task assignee)
   - Mode: `--quick` (first 2 iterations use quick mode)
   - Command: "Execute task fix-789"
   - Input: Task structure with all subtasks
   - Agent sees the structured process and works through it sequentially

**Agent Execution - Step 1: Analyze Context and Create Plan**

3. **Agent Works on fix-789.1**
   ```rust
   task.event {
     task_id: "fix-789.1",
     event: Started,
     agent: "claude-code",
   }
   ```
   
   Agent performs all analysis steps:
   - **Digests prompt/intent**: Reads `aiki/conversations` branch, fetches session xyz history
   - **Digests code**: Reads changes abc123, def456, ghi789 to understand current structure
   - **Digests review**: Fetches issue tasks review-456.1 and review-456.2, reads Gerrit JSON
   - **Creates plan**: Determines implementation approach
   - **Dynamically creates implementation subtasks**
   
   ```yaml
   # Agent creates new subtasks on aiki/tasks branch
   ---
   aiki_task_event: v1
   task_id: fix-789.1.1
   event: created
   parent_id: fix-789
   task:
     objective: "Add null check before user.name access"
     type: implementation
     works_on: [review-456.1]  # Maps to specific review issue
     scope:
       files:
         - path: src/auth.ts
           lines: [42]
   ---
   
   ---
   aiki_task_event: v1
   task_id: fix-789.1.2
   event: created
   parent_id: fix-789
   task:
     objective: "Add JWT token validation in middleware"
     type: implementation
     works_on: [review-456.2]  # Maps to specific review issue
     scope:
       files:
         - path: src/middleware.ts
           lines: [67]
   ---
   ```
   
   ```rust
   task.event {
     task_id: "fix-789.1",
     event: Closed {
       reason: Completed,
     },
     analysis: "User wanted JWT auth. Code has user model, middleware, token validation. Review found: null check needed in auth.ts:42, validation needed in middleware.ts:67",
     subtasks_created: ["fix-789.1.1", "fix-789.1.2"],
   }
   ```

**Agent Execution - Step 2: Implement Solution**

4. **Agent Works on fix-789.2 (Implementation Coordinator)**
   ```rust
   task.event {
     task_id: "fix-789.2",
     event: Started,
     agent: "claude-code",
   }
   ```
   
   Agent now implements each plan subtask:

5. **Implement fix-789.1.1**
   ```rust
   task.event {
     task_id: "fix-789.1.1",
     event: Started,
     agent: "claude-code",
   }
   ```
   
   **JJ Change Created**
   ```
   JJ: jj new -m "Add null check before user.name access"
   
   New change created:
     change_id: jkl012
     description: |
       Add null check before user.name access
       
       [aiki]
       agent=claude-code
       session=xyz-new
       tool=fix
       review_id=review-456
       task_id=fix-789.1.1
       parent_task=fix-789
       iteration=1
       quick=true
       [/aiki]
   ```
   
   ```rust
   change.completed {
     change_id: "jkl012",
     file_paths: ["src/auth.ts"],
     tasks: {
       works_on: ["fix-789.1.1"],
       closes: [],
     },
     tool: "fix",
   }
   
   task.event {
     task_id: "fix-789.1.1",
     event: Closed {
       reason: Completed,
     },
   }
   ```

6. **Implement fix-789.1.2**
   ```rust
   task.event {
     task_id: "fix-789.1.2",
     event: Started,
     agent: "claude-code",
   }
   ```
   
   **JJ Change Created**
   ```
   JJ: jj new -m "Add JWT token validation in middleware"
   
   New change created:
     change_id: mno345
     description: |
       Add JWT token validation in middleware
       
       [aiki]
       agent=claude-code
       session=xyz-new
       tool=fix
       review_id=review-456
       task_id=fix-789.1.2
       parent_task=fix-789
       iteration=1
       quick=true
       [/aiki]
   ```
   
   ```rust
   change.completed {
     change_id: "mno345",
     file_paths: ["src/middleware.ts"],
     tasks: {
       works_on: ["fix-789.1.2"],
       closes: [],
     },
     tool: "fix",
   }
   
   task.event {
     task_id: "fix-789.1.2",
     event: Closed {
       reason: Completed,
     },
   }
   ```

7. **Implementation Complete**
    ```rust
    task.event {
      task_id: "fix-789.2",
      event: Closed {
        reason: Completed,
      },
      implementation_tasks_completed: ["fix-789.1.1", "fix-789.1.2"],
    }
    ```

**Agent Execution - Step 3: Verify Issues Resolved**

8. **Agent Works on fix-789.3 (Verification)**
    ```rust
    task.event {
      task_id: "fix-789.3",
      event: Started,
      agent: "claude-code",
    }
    ```
    
    - Agent triggers self-review of all changed code (--self flag)
    - Same agent (claude-code) reviews its own fixes
    - Checks if review-456.1 and review-456.2 are addressed
    - **Result: Both issues resolved!** ✅
    
    ```rust
    // Close review issue tasks
    task.event {
      task_id: "review-456.1",
      event: Closed {
        reason: Approved,
      },
      resolved_by: "fix-789.1.1",
    }
    
    task.event {
      task_id: "review-456.2",
      event: Closed {
        reason: Approved,
      },
      resolved_by: "fix-789.1.2",
    }
    
    // Close verification subtask
    task.event {
      task_id: "fix-789.3",
      event: Closed {
        reason: Completed,
      },
      verification_result: "All review issues resolved",
    }
    ```

9. **Fix Task Complete (Iteration 1 Success)**
    ```rust
    task.event {
      task_id: "fix-789",
      event: Closed {
        reason: Approved,
      },
      iterations: 1,
      issues_resolved: 2,
      all_subtasks_completed: true,
    }
    ```
    - All subtasks (fix-789.1, fix-789.2, fix-789.3) are closed
    - Both review issues (review-456.1, review-456.2) are closed
    - Fix task fix-789: `Open` → `Closed`

**Alternative Scenario: Iteration 1 Fails Verification**

If step 8 verification finds issues still present:

```rust
task.event {
  task_id: "fix-789.3",
  event: Closed {
    reason: Failed,
  },
  verification_result: "review-456.2 still has validation issues",
}
```

Then the `fix:` action **restarts the process for iteration 2**:
- Keeps analysis step (fix-789.1) as completed - no need to re-digest context
- Resets fix-789.2 and fix-789.3 to restart implementation and verification
- Agent re-plans in fix-789.1 with knowledge of what failed (creates new implementation subtasks)
- Mode switches to deep-thinking if iteration 3

**Alternative Scenario: Max Iterations Exceeded**

If iteration 3 verification also fails:

```rust
task.event {
  task_id: "fix-789",
  event: NeedsHuman {
    reason: MaxRetriesExceeded,
    attempts: 3,
  },
  agent: "claude-code",
  unresolved_issues: ["review-456.2"],
  completed_subtasks: ["fix-789.1"],
  failed_subtasks: ["fix-789.2", "fix-789.3"],
}
```
- Fix task status: `Open` → `NeedsHuman`
- Issue review-456.1: `Closed` (was resolved in iteration 1)
- Issue review-456.2: `Open` (still needs fix after 3 attempts)
- Human can pick up fix-789 and continue from the analysis (fix-789.1 already has full context)

**Final JJ State (Success Scenario - Iteration 1):**
```
JJ change graph:
  @ (mno345) - Add JWT token validation in middleware
    [aiki] task_id=fix-789.1.2, parent_task=fix-789, iteration=1
  ○ (jkl012) - Add null check before user.name access
    [aiki] task_id=fix-789.1.1, parent_task=fix-789, iteration=1
  ○ (abc123) - Add JWT token validation [REVIEWED, had issues]
  ○ (def456) - Implement auth middleware [REVIEWED, had issues]
  ○ (ghi789) - Create user model [REVIEWED, no issues]

aiki/tasks branch:
  - review-456 (review task, status: Open)
    - review-456.1 (issue: null check, status: Closed, resolved_by: fix-789.1.1)
    - review-456.2 (issue: validation, status: Closed, resolved_by: fix-789.1.2)
  - fix-789 (fix task, status: Closed, iterations: 1, issues_resolved: 2)
    - fix-789.1 (analyze & plan, status: Closed)
      - fix-789.1.1 (impl: null check, status: Closed)
      - fix-789.1.2 (impl: validation, status: Closed)
    - fix-789.2 (implement, status: Closed)
    - fix-789.3 (verify, status: Closed)
```

**Final JJ State (Failure Scenario - Max Iterations):**
```
JJ change graph:
  @ (stu901-iter3) - Add JWT token validation (iter 3, deep-thinking) [FAILED verification]
  ○ (pqr789-iter3) - Add null check (iter 3, deep-thinking) [PASSED verification]
  ○ (mno567-iter2) - Add JWT token validation (iter 2) [FAILED verification]
  ○ (jkl345-iter2) - Add null check (iter 2) [PASSED verification]
  ○ (ghi123-iter1) - Add JWT token validation (iter 1) [FAILED verification]
  ○ (def012-iter1) - Add null check (iter 1) [PASSED verification]
  ○ (abc123) - Add JWT token validation [REVIEWED, had issues]
  ○ (def456) - Implement auth middleware [REVIEWED, had issues]
  ○ (ghi789) - Create user model [REVIEWED, no issues]

aiki/tasks branch:
  - review-456 (review task, status: Open)
    - review-456.1 (issue: null check, status: Closed, resolved_by: fix-789.1.1 in iteration 1)
    - review-456.2 (issue: validation, status: Open, needs human)
  - fix-789 (fix task, status: NeedsHuman, iterations: 3, unresolved: [review-456.2])
    - fix-789.1 (analyze & plan, status: Closed, kept across iterations)
      - fix-789.1.1 (impl: null check, status: Closed, resolved in iter 1)
      - fix-789.1.2 (impl: validation, status: Open, failed after 3 iterations in iter 1)
      - fix-789.1.3 (impl: validation v2, status: Open, failed in iter 2)
      - fix-789.1.4 (impl: validation v3, status: Open, failed in iter 3)
    - fix-789.2 (implement, last run: iteration 3, status: Closed)
    - fix-789.3 (verify, last run: iteration 3, status: Failed)
```

**Key Observations:**

1. **3-step fix workflow** - The `fix:` action creates a fix task with 3 subtasks: (1) analyze & plan, (2) implement, (3) verify
2. **Agent executes task headlessly** - Agent is given the task structure and works through subtasks sequentially
3. **Analysis runs once** - Step 1 (digest intent, code, review + create plan) completes once and is preserved across iterations
4. **Dynamic implementation subtasks** - Step 1 creates implementation subtasks (fix-789.1.1, fix-789.1.2) based on the agent's plan
5. **Each implementation subtask maps to review issues** - `works_on` field links implementation to specific review issues
6. **Verification gates progress** - Step 3 re-reviews all changes and determines if iteration succeeded
7. **Partial success tracked** - Individual review issues can be resolved while others remain open
8. **Iterations retry failed implementations** - If verification fails, step 1 creates new implementation subtasks for unresolved issues
9. **Every implementation creates a JJ change** - Each fix-789.1.x subtask creates its own change
10. **Full provenance chain** - Changes reference `task_id=fix-789.1.1`, `parent_task=fix-789`, `review_id=review-456`
11. **Quick/deep mode is recorded** - `iteration=1` and `iteration=2` use quick mode, `iteration=3` uses deep-thinking
12. **Failed attempts remain in JJ history** - All implementation attempts are preserved for learning
13. **Human can continue from analysis** - If max iterations exceeded, human picks up with full context already gathered
14. **Clear audit trail** - Can see all implementation attempts (fix-789.1.1, fix-789.1.2, fix-789.1.3, etc.) across iterations

**Querying the History:**

```bash
# Show all fix attempts for a review
jj log -r 'description(glob:"review_id=review-456")'

# Show the fix task
aiki task show fix-789

# Show all review issues
aiki task list --parent review-456

# Show which issues were resolved
aiki task list --parent review-456 --status closed

# Show unresolved issues
aiki task list --parent review-456 --status open

# Show fix tasks that need human attention
aiki task list --type fix_review --status needs-human

# Show full timeline for a fix attempt
aiki task show fix-789 --history
```

**Interactive Agent Mode:**

Agents can also see review issue tasks in their normal work queue:

```
$ aiki task list --ready

Ready Tasks:
  review-abc123.1 - Fix null pointer dereference at src/auth.ts:42
  review-abc123.2 - Add input validation for JWT token at src/auth.ts:67
  feature-456 - Implement user profile page
```

When an interactive agent works on a task:
1. Agent picks up task from ready queue
2. Makes changes to fix the issue
3. Creates JJ change with `tasks.closes: ["review-abc123.1"]`
4. `change.completed` event fires
5. Task system marks subtask as closed

**Benefits:**
- **Headless remediation** - Flows can trigger automatic fixes via `task:` action
- **Interactive workflow** - Agents can pick up tasks naturally from their queue
- **Generic** - `task:` action works for ANY task type, not just review issues
- **Full provenance** - Every fix links back to the task and review
- **Composable** - Review tasks work with full task system features

**User Customization:**
Users can override this behavior by creating their own `review.completed` hook in `.aiki/flows/`. Examples:
- Block immediately on critical issues (no auto-fix)
- Different remediation strategies
- Custom task priorities or assignments
- Notification logic

**Review Tracking in JJ:**
Each review stores metadata in the JJ change description using `[aiki:review]` blocks:
```
[aiki:review:uuid-123]
timestamp=2025-01-05T12:34:56Z
reviewer=codex
mode=default
issues_found=3
[/aiki:review:uuid-123]
```

Modes: `default` (single non-authoring agent), `self` (authoring agent reviews own work)

**Fix Tracking in JJ:**
Each fix attempt creates a new JJ change with metadata linking back to the review:
```
[aiki]
agent=claude
tool=fix
review_id=uuid-123
iteration=1
issues_addressed=3
[/aiki]
```

This creates a causal chain: review → fix → re-review → fix, all linked by review_id.

**Review Action in Flows:**
When using the `review:` action in flows, it returns the review object directly:

```yaml
- review: "@"
  alias: my_review

# Access results (returns the review object from event):
# $my_review.id
# $my_review.task_id                - Task ID for this review (e.g., "review-abc123")
# $my_review.agent
# $my_review.issues_found           - Number of issues (= number of subtasks created)
# $my_review.issues
# $my_review.target.type            - "change" | "git_commit" | "git_staged" | "files"
# $my_review.target.change_id       - JJ change ID (if applicable)
# $my_review.target.commit_sha      - Git commit SHA (if applicable)
# $my_review.target.files           - Files reviewed
# $my_review.target.author          - Agent that authored (if available)

# Query review issue tasks:
# aiki task list --parent $my_review.task_id
```

The review action also emits the `review.completed` event automatically. Review issues are created as tasks on the `aiki/tasks` branch, and agents will see them in their normal work queue.

**Output Format:**
All reviews output JSON in Gerrit RobotCommentInfo format for machine-readable, standardized code review comments.

**Gerrit RobotCommentInfo Schema:**
```json
{
  "robot_comments": {
    "path/to/file.rs": [
      {
        "robot_id": "aiki-review",
        "robot_run_id": "session-id-timestamp",
        "url": "https://aiki.dev/reviews/...",
        "id": "comment-uuid",
        "path": "path/to/file.rs",
        "line": 42,
        "range": {
          "start_line": 42,
          "start_character": 0,
          "end_line": 45,
          "end_character": 10
        },
        "message": "Potential null pointer dereference. Add null check before accessing user.name",
        "updated": "2025-01-05 12:34:56.789000000"
      }
    ]
  }
}
```

**Key Fields:**
- `robot_id`: Agent name that found the issue (e.g., "claude", "codex", "gemini")
- `robot_run_id`: Session ID + timestamp for traceability
- `path`: File path relative to repository root
- `line`: Line number where issue was found
- `range`: Optional character range for precise location
- `message`: Human-readable issue description
- `updated`: ISO timestamp
- `url`: Optional link to more details

**Configuration System:**

**Agent Configuration (hardcoded in v1):**
- **claude**: opus-4 (default) / sonnet-4.5 (--quick)
- **codex**: o3 (default) / o4-mini (--quick)
- **gemini**: gemini-2.0-flash-thinking-exp (default) / gemini-2.0-flash (--quick)
- All use stdin mode "prompt"
- Timeouts: 300s (default), 120s (quick)

**Prompt Templates (configurable in `~/.aiki/config.toml`):**

```toml
[review.prompts]
default = """
Review the following changes for:
- Logic errors and bugs
- Security vulnerabilities
- Performance issues
- Code style and best practices
- Backward compatibility concerns

{{#if target.intent}}
Developer intent: {{target.intent}}
{{/if}}

{{#if target.context}}
Recent changes to these files:
{{#each target.context}}
- {{this.intent}} ({{this.session_id}}, turn {{this.turn}})
{{/each}}
{{/if}}

Provide specific, actionable feedback with file paths and line numbers.
Output in Gerrit RobotCommentInfo JSON format.
"""

security = """
Security-focused code review. Check for:
- SQL injection vulnerabilities
- XSS (Cross-Site Scripting) attacks
- Authentication/authorization bypass
- Sensitive data exposure
- Cryptographic weaknesses
- Input validation failures
- Path traversal vulnerabilities

Provide specific findings with severity assessment.
Output in Gerrit RobotCommentInfo JSON format.
"""

performance = """
Performance-focused code review. Analyze:
- Algorithmic complexity (time and space)
- Database query efficiency (N+1 queries, missing indexes)
- Memory usage and potential leaks
- Caching opportunities
- Unnecessary computations

Provide specific optimization suggestions.
Output in Gerrit RobotCommentInfo JSON format.
"""
```

**Module Organization:**
- New module: `cli/src/commands/review.rs` - Review CLI entry point
- New module: `cli/src/commands/fix.rs` - Fix CLI entry point
- New module: `cli/src/headless/` - Headless agent plugin system (mirrors `editors/` pattern)
  - `mod.rs` - Unified agent registry and common types
  - `claude/` - Claude agent implementation (Opus/Sonnet)
  - `codex/` - Codex agent implementation (o3/o4-mini)
  - `gemini/` - Gemini agent implementation (Thinking/Flash)
  - Each agent module contains:
    - `mod.rs` - Public `review()` and `fix()` functions + CLI executor
    - `review.rs` - Review mode (read-only, handles JSON validation)
    - `fix.rs` - Fix mode (read-write, linked to review)

### Headless Agent Plugin System

**Architecture (inspired by `editors/` pattern):**

The headless agent system uses a modular plugin architecture where each agent has two modes: review (read-only) and code (read-write for auto-remediation).

**Common Types (`headless/mod.rs`):**
- `ReviewRequest` - Input for review mode (diff, prompt, scope, thinking_mode)
- `ReviewResponse` - Output (Gerrit RobotCommentInfo JSON, review_id)
- `FixRequest` - Input for fix mode (review_id, issues, thinking_mode)
- `FixResponse` - Output (modified files, fix summary)
- `pub fn review(agent: &str, request: ReviewRequest) -> Result<ReviewResponse>`
- `pub fn fix(agent: &str, request: FixRequest) -> Result<FixResponse>`

**Per-Agent Modules (`headless/{claude,codex,gemini}/`):**
- `mod.rs` - Public interface (`review()` and `fix()` functions) + CLI executor
- `review.rs` - Review mode implementation (read-only, outputs Gerrit JSON, validates JSON)
- `fix.rs` - Fix mode implementation (read-write, linked to review_id)

**Pattern:**
```
cli/src/headless/
├── mod.rs                    # Unified registry and common types
├── claude/
│   ├── mod.rs               # pub fn review(), fix(), and CLI executor
│   ├── review.rs            # Read-only review mode + JSON validation
│   └── fix.rs               # Read-write fix mode (linked to review)
├── codex/
│   ├── mod.rs
│   ├── review.rs
│   └── fix.rs
└── gemini/
    ├── mod.rs
    ├── review.rs
    └── fix.rs
```

**Native JSON Output Strategy:**
Each CLI has native flags to guarantee JSON output conforming to our Gerrit schema:

- **claude-code**: `--json-schema <schema>` flag enforces schema validation ([CLI Reference](https://code.claude.com/docs/en/cli-reference))
  - Example: `claude -p --json-schema gerrit-schema.json "Review this code"`
  - Returns validated JSON matching the Gerrit RobotCommentInfo schema

- **codex**: `--output-schema <path>` validates output against JSON Schema ([CLI Reference](https://developers.openai.com/codex/cli/reference/))
  - Example: `codex exec "Review code" --output-schema gerrit-schema.json --json`
  - Use `--json` flag for newline-delimited JSON events
  - Codex validates final response against the provided schema

- **gemini**: API-level schema enforcement via SDK ([Structured Output Docs](https://ai.google.dev/gemini-api/docs/structured-output))
  - Set `response_mime_type: "application/json"` and `response_json_schema` in generation config
  - Note: Gemini CLI doesn't have native `--output-schema` flag yet (tracked in [Issue #5021](https://github.com/google-gemini/gemini-cli/issues/5021))
  - Workaround: Use Gemini SDK directly or rely on prompt-based JSON generation

This eliminates custom LLM output parsing - we provide the Gerrit RobotCommentInfo schema to each CLI and receive validated JSON directly.

**Benefits:**
- Unified agent abstraction (review + fix modes)
- Isolates vendor-specific logic (like `editors/` modules)
- Easy to add new agents (create new subdirectory)
- Minimal parsing - CLIs handle JSON generation natively
- Consistent public interface via `headless::review()` and `headless::fix()`
- Explicit link between fixes and reviews via review_id

### Flow Actions - `review:` and `task:`

#### `review:` Action

**Action Types:**
- **Simple form - change/revset**: `review: "@"` or `review: "@-"` (review specific change with defaults)
- **Full form**: Multi-line YAML with scope, prompt, files, quick, self
- **Variable storage**: Results stored via `alias:` for conditional logic
- **Event emission**: Automatically emits `review.completed` event after review completes

**Note on remediation:**
The `review:` action itself only performs the judgment and creates tasks. Auto-remediation happens via the `task:` action in the `aiki/default` flow. Users can customize or disable this behavior by overriding the event handler.

#### `fix:` Action

**Purpose:** Fix review issues headlessly with iteration (blocking operation)

**Action Type:**
- **Simple form**: `fix: "review-id"` - Fix a review and all its issue subtasks
- **With options**: `fix: { review: "review-id", max_iterations: 3, quick: true }`

**How it works:**
1. Takes a review ID (e.g., from `$event.review.id`)
2. Looks up the review's task ID (e.g., `review-abc123`)
3. Iterates through all issue subtasks (already assigned to authoring agent)
4. For each subtask (blocking with retries):
   - Loads task details from `aiki/tasks` branch
   - Uses the task's `assignee` field (authoring agent set when task was created)
   - Marks task as `InProgress`
   - Invokes that agent headlessly to fix the issue
   - Creates JJ change with fix
   - Marks task as `NeedsReview`
   - Runs targeted re-review of the fixed code
   - If passes: marks as `Closed(Approved)`
   - If fails: marks as `NeedsFix`, retries (up to `max_iterations`, default: 3)
   - If max iterations exceeded: marks as `NeedsHuman(MaxRetriesExceeded)`
5. Returns when all issues are fixed, need human attention, or abandoned

**Example:**
```yaml
# Simple fix with defaults (3 iterations max, quick mode for first 2)
- fix: $event.review.id

# Customize iterations
- fix:
    review: $event.review.id
    max_iterations: 5
    quick: false  # Use deep-thinking for all iterations
```

**Implementation Note:**
The `fix:` action is a convenience wrapper around the generic `task:` action:

```yaml
# fix: is equivalent to:
- task:
    start: <review.task_id>  # Looked up from review ID
    headless: true
    # Agent comes from task.assignee (authoring agent set during task creation)
```

**Benefits:**
- Review-specific - smart agent selection (uses authoring agent)
- Blocking - flow waits for completion before continuing
- Headless - no interactive agent needed
- Full provenance - all changes tracked via task system

#### `task:` Action (Generic)

**Purpose:** Work on any task headlessly (blocking operation)

**Action Type:**
- **Simple form**: `task: { start: "task-id", headless: true }` - Work on a task headlessly

**How it works:**
1. Takes a task ID (parent or subtask)
2. If parent task, iterates through all subtasks
3. For each subtask (blocking):
   - Loads task details from `aiki/tasks` branch
   - Invokes specified agent (or default) headlessly
   - Creates JJ changes
   - Marks subtasks as closed
4. Returns when all work is complete

**Example:**
```yaml
# Generic task work with explicit agent
- task:
    start: "feature-123"
    headless: true
    agent: claude-code

# Uses default agent
- task:
    start: "bug-456"
    headless: true
```

**Benefits:**
- Generic - works for ANY task type (features, bugs, chores, etc.)
- Explicit agent control
- Blocking operation
- Full provenance via task system

**Flow Action Examples:**

```yaml
# Simple - review working copy with defaults
# Emits review.completed event, default flow handles auto-remediation
- review: "@"

# Review previous change
- review: "@-"

# Review with quick mode (fast models)
- review:
    scope: working_copy
    quick: true

# Self-review (agent reviews its own work)
- review:
    self: true

# Review specific files only (defaults to working copy)
- review:
    files: ["src/auth.rs", "src/payment.rs"]

# Full configuration
- review:
    scope: working_copy
    quick: false  # Use deep-thinking (default)
    prompt: performance
    files: $event.file_paths
  alias: detailed_review

# The review.completed event will be emitted automatically
# Default flow will handle remediation
```

**Integration Points:**
- Add `Review` variant to `Action` enum in `flows/types.rs`
- Add `Fix` variant to `Action` enum in `flows/types.rs` (wrapper around `Task`)
- Add `Task` variant to `Action` enum in `flows/types.rs`
- Add `review.completed` event type to event system
- Implement `review:`, `fix:`, and `task:` execution in `flows/engine.rs`
- `fix:` action reads authoring agent from JJ `[aiki]` metadata
- `fix:` delegates to `task: { start: ..., headless: true, agent: ... }`
- Support variable resolution from event context
- Enable conditional execution based on review results
- Create tasks on `aiki/tasks` branch for each review issue
- Link review tasks to changes via JJ metadata and task system

---

## Advanced Features

### Review History
- Store results in JJ change descriptions as `[aiki:review]` blocks
- Track: reviewer, timestamp, mode (default/self), issues found/fixed, iteration number
- Always enabled in v1 (not optional)
- Enables provenance tracking of review decisions and remediation attempts
- Each review gets a unique UUID for correlation across remediation iterations

---

## Configuration Architecture

### Agent Configuration (Hardcoded for v1)
All agent configurations are hardcoded in the codebase:
- **claude**: `claude-code --headless --model claude-opus-4` (default) / `claude-sonnet-4.5` (quick)
- **codex**: `codex --model o3` (default) / `o4-mini` (quick)
- **gemini**: `gemini --model gemini-2.0-flash-thinking-exp` (default) / `gemini-2.0-flash` (quick)
- All agents: stdin mode "prompt", 300s timeout (default), 120s timeout (quick)

### Prompt Template System (Configurable)
Built-in templates:
- **default**: General code review (logic, security, performance, style)
- **security**: Security-focused (SQL injection, XSS, auth issues)
- **performance**: Performance analysis (complexity, memory, queries)

Custom templates supported via user configuration.

### Stdin Mode Types
- **prompt**: Send diff + prompt via stdin (standard for LLM CLIs)

---

## User Experience Flows

### Use Case 1: Pre-Commit Review Hook with Auto-Fix

**Trigger**: Git prepare-commit-msg hook (before commit is created)
**Flow**: Review → Create tasks → Auto-fix via fix: action
**Outcome**: Auto-fix issues headlessly, block only if fixes fail

**Note**: This use case assumes integration with Git's `prepare-commit-msg` hook, which runs after staging but before the commit is created. The hook would trigger `aiki review` with a custom event.

```yaml
# .aiki/flows/pre-commit-review.yml
name: "Pre-Commit Review with Auto-Fix"
version: "1.0"

# Triggered by Git prepare-commit-msg hook
commit_message.started:
  - log: "Running pre-commit review..."
  
  # Review staged changes
  - review:
      scope: staged
      quick: true  # Use fast models for speed
      prompt: default
    alias: pre_commit_review
  
  # Auto-fix issues headlessly (uses authoring agent)
  - if: $pre_commit_review.issues_found > 0
    then:
      - log: "Found {{$pre_commit_review.issues_found}} issues, fixing..."
      - fix: $pre_commit_review.id  # Blocks until all fixed
      - log: "✅ All issues resolved!"
```

### Use Case 2: Custom Remediation for Critical Files

**Trigger**: Changes to auth/payment/crypto files
**Flow**: Review with custom remediation strategy
**Outcome**: Block immediately on security issues, don't auto-fix

```yaml
# .aiki/flows/critical-file-review.yml
name: "Critical File Security Review"
version: "1.0"

change.completed:
  - if: |
      $event.file_paths contains "src/auth" or
      $event.file_paths contains "src/payment" or
      $event.file_paths contains "src/crypto"
    then:
      - log: "Critical files detected - running security review"
      
      # Deep security review
      - review:
          scope: working_copy
          prompt: security
          files: $event.file_paths

# Custom review.completed handler (overrides aiki/default)
# Only handle security reviews (ignore other reviews like aiki/review-loop)
review.completed:
  - if: $event.review.prompt == "security" and $event.review.issues_found > 0
    then:
      - block: |
          🚨 SECURITY REVIEW FAILED
          
          Critical files have security issues. Auto-remediation disabled.
          Manual review and fixes required:
          
          {{$event.review.issues | to_json}}
```

### Use Case 3: Autonomous Review with Headless Auto-Fix

**Trigger**: AI agent completes a response
**Flow**: Review entire session → Auto-fix headlessly
**Outcome**: All session changes reviewed and issues automatically fixed

```yaml
# .aiki/flows/auto-review.yml
name: "Autonomous Review with Headless Auto-Fix"
version: "1.0"

response.received:
  - if: $event.session.modified_files_count > 0
    then:
      - log: "Agent completed response - reviewing session changes..."
      
      # Review all changes from this session
      - review:
          scope: session  # Reviews everything the agent did in this turn
          quick: true
        alias: session_review
      
      # Auto-fix issues headlessly if found (uses authoring agent)
      - if: $session_review.issues_found > 0
        then:
          - log: "Found {{$session_review.issues_found}} issues, fixing headlessly..."
          - fix: $session_review.id
          - log: "✅ All issues fixed automatically!"
```

### Use Case 3b: Self-Review with Headless Auto-Fix

**Trigger**: AI agent completes a response
**Flow**: Agent reviews entire session's work → Fixes issues headlessly
**Outcome**: Agent self-corrects all changes from the session

```yaml
# .aiki/flows/self-review-loop.yml
name: "Self-Review with Headless Auto-Fix"
version: "1.0"

response.received:
  - if: $event.session.modified_files_count > 0
    then:
      - log: "Agent completed response - running self-review of session..."
      
      # Self-review: agent reviews all its changes from this session
      - review:
          scope: session  # Reviews everything from this conversation turn
          self: true      # Uses same agent as reviewer
          quick: true
        alias: session_self_review
      
      # Auto-fix issues headlessly (same agent fixes its own issues)
      - if: $session_self_review.issues_found > 0
        then:
          - fix: $session_self_review.id
          - log: "Self-corrections applied to all session changes!"
```

### Use Case 4: Pre-Push Security Review with Custom Handler

**Trigger**: User runs `git push`
**Flow**: Security review with custom review.completed handler
**Outcome**: Block push immediately on security issues, skip auto-remediation

```yaml
# .aiki/flows/pre-push-security.yml
name: "Pre-Push Security Review"
version: "1.0"

shell.permission_asked:
  - if: $event.command contains "git push"
    then:
      - log: "Running security review before push..."
      
      # Review with security prompt
      - review:
          scope: staged
          prompt: security

# Custom review.completed handler (overrides aiki/default)
# Only handle security reviews (ignore other reviews like aiki/review-loop)
# Block immediately on security issues, don't auto-fix
review.completed:
  - if: $event.review.prompt == "security" and $event.review.issues_found > 0
    then:
      - block: |
          🔒 SECURITY REVIEW FAILED
          
          Found {{$event.review.issues_found}} security issues.
          Push blocked. Manual review required.
          
          Details:
          {{$event.review.issues | to_json}}
```

### Use Case 5: Headless Fix with Iterations

**Trigger**: Developer runs `aiki review` or automated flow invokes review
**Flow**: Review → Create tasks → Fix with iterations → Re-review → Retry or needs human
**Outcome**: All issues fixed via iterative `fix:` action, or marked as needing human attention

**How it works:**
1. **Review**: Non-authoring agent reviews code and creates issue tasks
2. **Task Creation**: Each issue becomes a subtask assigned to authoring agent
3. **Headless Fix with Iterations**: `fix:` action attempts each subtask up to 3 times
4. **Per-Iteration**: Fix → Re-review → If fails, retry (up to max)
5. **Result**: Tasks end as `Closed(Approved)` or `NeedsHuman(MaxRetriesExceeded)`

**Example CLI interaction:**
```
$ aiki review

🔍 Reviewing working copy with codex...
   Claude authored this code
✅ Review complete: review-abc123
   Found 3 issues (created 3 subtasks)

$ aiki task show review-abc123

Review Task: review-abc123
Status: Open
Subtasks:
  review-abc123.1 - Fix null pointer dereference at src/auth.ts:42 [Open, assigned: claude-code]
  review-abc123.2 - Add input validation for JWT token at src/auth.ts:67 [Open, assigned: claude-code]
  review-abc123.3 - Extract duplicate code into helper function at src/auth.ts:89 [Open, assigned: claude-code]

# User can trigger headless fix manually (or flows do it automatically)
$ aiki fix review-abc123

🔧 Fixing review issues with iterations...

Issue 1/3: review-abc123.1
  Attempt 1/3 (quick): Fixing... → Re-reviewing... ✅ Approved
  
Issue 2/3: review-abc123.2  
  Attempt 1/3 (quick): Fixing... → Re-reviewing... ❌ Still has issues
  Attempt 2/3 (quick): Fixing... → Re-reviewing... ✅ Approved
  
Issue 3/3: review-abc123.3
  Attempt 1/3 (quick): Fixing... → Re-reviewing... ❌ Still has issues
  Attempt 2/3 (quick): Fixing... → Re-reviewing... ❌ Still has issues
  Attempt 3/3 (deep): Fixing... → Re-reviewing... ✅ Approved

✅ 2 issues resolved, 1 needs human attention after 9 total attempts
   - review-abc123.1: Closed(Approved) after 1 attempt
   - review-abc123.2: Closed(Approved) after 2 attempts
   - review-abc123.3: NeedsHuman(MaxRetriesExceeded) after 3 attempts

$ aiki task list --status needs-human

Tasks Needing Human Attention:
  review-abc123.3 - Extract duplicate code into helper function at src/auth.ts:89
    Status: NeedsHuman (MaxRetriesExceeded after 3 attempts)
    Assigned: claude-code
    Last attempt: Fixed code but review still found duplication in different form
```

### Use Case 6: Manual Review Command (Skip Flow Handlers)

**Trigger**: Developer runs `aiki review --skip-flow`
**Flow**: Review produces judgment and displays results, but doesn't emit `review.completed` event
**Outcome**: Display Gerrit JSON results without triggering flow handlers (no auto-remediation)

```bash
# Basic review (default: emits review.completed event, triggers default flow remediation)
$ aiki review
# Output: Full transaction - judgment, then default flow handles remediation

# Skip flow handlers (judgment only, no event emission)
$ aiki review --skip-flow
# Output: Just the review results, no remediation triggered

# Review specific change with quick models
$ aiki review @- --quick

# Review with security prompt template
$ aiki review --prompt security

# Self-review (agent reviews its own work)
$ aiki review --self

# Quick self-review for iterative improvement
$ aiki review --self --quick
```

**Note:** All review commands emit the `review.completed` event. The `aiki/default` flow handles auto-remediation. Users can customize this by overriding the event handler.

---

## Flow Composition with Review Loop

With composable flows (Milestone 1.3), we can create reusable review flows that compose together.

### Core Review Flow: `aiki/review-loop`

Create a reusable review flow that other flows can include:

```yaml
# ~/.aiki/flows/aiki/review-loop.yml
name: "Review Loop with Auto-Fix"
version: "1"

# This flow reviews ALL changes from the entire session when agent completes a response
# Triggered on response.received (not change.completed to avoid reviewing partial work)

response.received:
  - log: "[Review Loop] Agent completed response, reviewing session changes..."
  
  # Only review if the session made file modifications
  - if: $event.session.modified_files_count > 0
    then:
      - log: "[Review Loop] Reviewing {{$event.session.modified_files_count}} files from session..."
      
      # Review all changes from the entire session (not just working copy)
      # This captures everything the agent did in this conversation turn
      - review:
          scope: session  # Reviews all changes made during this session
      
      # Auto-fix any issues found
      - if: $review.issues_found > 0
        then:
          - log: "[Review Loop] Found {{$review.issues_found}} issues, fixing..."
          - fix:
              review: $review.id
              max_iterations: 3
              quick: true
          - log: "[Review Loop] ✅ Auto-fix complete"
      - else:
          - log: "[Review Loop] ✅ No issues found, session changes look good!"
```

### Using Review Loop in Project default.yml

Add the review loop as a `before:` flow in your project's `.aiki/flows/default.yml`:

```yaml
# .aiki/flows/default.yml
name: "Aiki Repo Default Flow"
version: "1"

# Run review loop before main actions
before:
  - aiki/review-loop  # Reusable review flow from ~/.aiki/flows/aiki/

# Main project-specific actions
change.completed:
  - log: "[Main] Project-specific change.completed actions..."

response.received:
  - log: "[Main] Project-specific response.received actions..."
```

### Execution Order

When an event fires, the execution order is:

```
1. Bundled aiki/core (always first, immutable)
2. default.yml - before flows
   └─> aiki/review-loop (reviews and auto-fixes)
3. default.yml - main actions (project-specific)
4. default.yml - after flows (if any)
```

### Benefits of Composable Review Flows

1. **Reusable** - Write review logic once in `aiki/review-loop`, use across projects
2. **Override-able** - Projects can create `.aiki/flows/aiki/review-loop.yml` to customize
3. **Composable** - Combine with other flows (security checks, linters, formatters)
4. **Namespaced** - Clear organization in `~/.aiki/flows/aiki/` namespace

### Example: Multi-Stage Review

```yaml
# .aiki/flows/default.yml
name: "Multi-Stage Review Pipeline"
version: "1"

before:
  - aiki/quick-lint       # Fast syntax checks
  - aiki/format-check     # Code formatting

after:
  - aiki/review-loop      # Deep code review after main actions
  - aiki/security-scan    # Security analysis
```

---

## Implementation Phases

### Phase 1: Core CLI Command & Review Mode (Weeks 1-2)
- Create `commands/review.rs` module (CLI entry point)
- Create `headless/` plugin system:
  - `mod.rs` with common types (`ReviewRequest`, `ReviewResponse`)
  - Implement one agent (e.g., `claude/`) with `review.rs` submodule
- Add `Review` command to CLI parser
- Add `review.completed` event type to event system
- Hardcoded agent configurations (opus/sonnet, o3/o4-mini, gemini thinking/flash)
- Basic headless review execution with read-only permissions
- Smart agent selection (read authoring agent from JJ `[aiki]` metadata, pick different agent)
- Support for JJ changes (working copy, change ID, revset) and Git commits (SHA, staged)
- Gerrit RobotCommentInfo JSON output format
- Store review metadata in JJ change descriptions (`[aiki:review]` blocks)
- Emit `review.completed` event after review finishes
- **Prompt history integration**:
  - Fetch intent from `aiki/conversations` branch for the change being reviewed
  - Query recent changes to target files (last 5 turns) for context
  - Include intent and context in `review.target` event payload
  - Pass context to review agent in prompt template

### Phase 2: Flow Action (Weeks 2-3)
- Add `Review` action to `flows/types.rs`
- Implement `ReviewAction` serde support
- Add execution to `flows/engine.rs`
- Variable resolution for results
- `on_failure` handling
- `alias` support

### Phase 3: Quick Mode and Prompt Templates (Weeks 3-4)
- Implement `--quick` flag (fast model selection)
- Build prompt template system (default, security, performance)
- Template variable substitution
- Documentation and examples

### Phase 4: Multi-Agent Support (Weeks 4-5)
- Implement remaining agents (`codex/`, `gemini/`)
- Each with their own `review.rs`, `code.rs`, executor, and parser
- Test agent selection logic across all agents
- Ensure all agents work in both review and coding modes

### Phase 5: Auto-Remediation Loop (Weeks 5-6)
- Add `CodeRequest` and `CodeResponse` types to `headless/mod.rs`
- Implement `code.rs` for each agent (coding mode with read-write permissions)
- Build remediation loop in `commands/review.rs`:
  - Iteration 1 & 2: quick mode fixes
  - Iteration 3: deep-thinking mode fix
  - Track iterations in JJ change descriptions
- Create new JJ changes for each remediation attempt with provenance metadata
- User-facing output: "✅ Fixed" or "❌ Remaining issues: ..."

### Phase 6: Advanced Features (Weeks 6-7)
- Review history in JJ descriptions
- Performance optimizations for review execution
- Enhanced error handling and recovery

### Phase 7: Testing & Documentation (Weeks 7-8)
- Unit tests for review and remediation loops
- Integration tests with mock agent responses
- Flow examples (pre-commit, critical files, auto-remediation)
- User documentation
- Prompt template examples

---

## Success Criteria

### Must Have
- CLI command `aiki review` works with at least one model (headless mode)
- Flow action `review:` supports simple and full configuration
- Support for JJ changes (change ID, revset) and Git commits (SHA, staged)
- Gerrit RobotCommentInfo JSON output format
- Configuration file support
- Error handling and on_failure behavior
- Basic documentation and examples

### Should Have
- Multiple agent support (claude, codex, gemini)
- Review result variable storage (alias)
- Prompt templates (default, security, performance)
- File-specific reviews
- Quick mode (--quick flag)

### Nice to Have
- Auto-fix suggestions (Gerrit fix suggestions format)

---

## Open Questions

1. **Should reviews automatically commit to JJ change descriptions?**
   - Pro: Persistent review history
   - Con: Clutters descriptions
   - **Proposal**: Make it opt-in via `store_in_description: true`

2. **How to handle LLM output to Gerrit JSON conversion?**
   - Different CLI tools return different formats
   - **Proposal**: Build LLM output parser that extracts issues and converts to Gerrit RobotCommentInfo JSON

3. **Should we support streaming output for long reviews?**
   - Pro: Better UX for slow models
   - Con: More complex implementation
   - **Proposal**: Phase 2 feature, start with blocking execution

4. **Should review results block the operation by default?**
   - **Proposal**: No - default to `on_failure: continue`, user can override

---

## Future Ideas

### Temporal Review Queries
Leverage JJ's change graph to query review history over time:

```bash
# What Oracle can't do:
aiki review history auth.rs        # Show all reviews of this file
aiki review compare @- @           # How did review findings change?
aiki review stats --by-agent       # Which reviewer catches what?
aiki review regressions            # Issues that were fixed then reappeared
aiki review timeline <change-id>   # Full review/fix/re-review timeline
```

**Why this matters:**
- Oracle is stateless - each review is isolated
- Aiki tracks the full causal chain: change → review → fix → re-review
- Enables learning: which agents catch what types of issues
- Detects patterns: regressions, common mistakes, agent blind spots

### Multi-Agent Review Panel (Future Enhancement)
Add `--panel` flag to run multiple agents in parallel for high-stakes reviews:
- Review with 2-3 different agents simultaneously
- Show per-agent attribution for each issue
- Useful for security reviews, pre-push, critical files
- **Note**: v1 uses single reviewer (one non-authoring agent) by default

**Potential output format with disagreement:**
```json
{
  "review_id": "uuid",
  "positions": [
    {
      "reviewer": "codex",
      "stance": "block",
      "confidence": 0.9,
      "issues": [...]
    },
    {
      "reviewer": "gemini",
      "stance": "approve_with_changes",
      "confidence": 0.7,
      "issues": [...]
    }
  ],
  "consensus": null  // or "block" if unanimous
}
```

**Design questions for future:**
- Should we aggregate and deduplicate similar issues?
- Or preserve reviewer disagreement to enable "which agents agree?" queries?
- How to handle confidence scores and consensus determination?

### Interactive Review Mode
Add an interactive TUI for reviewing results with user actions:
- Display review results in formatted UI
- Action options:
  - Accept and continue
  - Fix automatically (if available)
  - Ignore review
  - Re-run with different model
  - Quit
- File/line navigation to issues
- Real-time preview of suggested fixes

**Implementation considerations:**
- TUI framework selection (e.g., ratatui)
- State management for navigation
- Integration with editor for jumping to issues

### Custom Binary Support
Allow users to specify any CLI tool that accepts stdin and returns stdout for review.

This enables integration with:
- Custom internal review tools
- Experimental LLM wrappers
- Domain-specific analyzers
- Open-source code review models

**Implementation considerations:**
- Custom tools must output Gerrit RobotCommentInfo JSON or provide parser
- Error handling for unknown CLI tools
- Documentation for custom tool integration
- Security considerations for arbitrary binaries
- If tool cannot enforce read-only access, mark as `advisory_only: true`

---

## References

- **Amp Code Oracle**: https://ampcode.com/news/oracle
- **Amp Code Manual**: https://ampcode.com/manual
- **Model Evaluation**: https://ampcode.com/news/model-evaluation
- **Mitchell Hashimoto endorsement**: "Amp Code freaking cooks"
- **Headless CLI agents**: Claude Code, OpenHands, CLI Engineer, Aider, Goose
- **ACP Protocol**: https://agentclientprotocol.com/protocol/schema

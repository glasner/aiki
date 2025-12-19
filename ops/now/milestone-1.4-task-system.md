# Milestone 1.4: Task System

**Status**: 🔴 Not Started
**Priority**: Medium (enables structured agent workflows)
**Complexity**: High

## Overview

The Task System provides structured, event-sourced task management for AI agent workflows. Instead of text-based autoreplies, flows create queryable tasks that agents can work through systematically. Tasks support dependencies, hierarchical organization, and assignment. Reviews are handled separately via the [Review System](#review-system).

**Key Architecture:** Event-sourced task log stored on JJ `aiki/tasks` branch. Tasks reconstructed from immutable event stream. Dependencies stored as data within events, not JJ DAG structure.

**Inspiration:** [Beads](https://github.com/steveyegge/beads) - Steve Yegge's distributed, git-backed issue tracker for AI agents. Key insights adopted:
- Dependencies make "ready" meaningful (only unblocked tasks)
- Content-addressed IDs prevent collisions
- Hierarchical IDs for epics/subtasks
- `discovered-from` links for work found during other work
- Compaction for long-running sessions

**Key Difference from Beads:** Aiki's task system integrates through the existing flow system via two mechanisms:
- **ACP Proxy** (for Zed): Transparent proxy intercepts all protocol messages
- **Editor Hooks** (for Claude Code, Cursor): Registers with each editor's native hook system

Both fire the same Aiki events, so flows work identically regardless of editor. Task context injection, auto-creation, and auto-sync happen automatically through flows.

---

## Table of Contents

1. [Phase 1: Core Task System](#phase-1-core-task-system) ← **START HERE**
2. [Phase 2: Performance & Extended Features](#phase-2-performance--extended-features)
3. [Phase 3: Code Provenance](#phase-3-code-provenance)
4. [Phase 4: Multi-Agent Coordination](#phase-4-multi-agent-coordination)
5. [Review System](#review-system) ← **SEPARATE FROM TASKS**
6. [Agent Adoption: Native Integration](#agent-adoption-native-integration)

---

## Phase 1: Core Task System

**Goal**: Full-featured task system with dependencies, assignments, and hierarchical organization.

**Depends on:** Milestone 1.2 (PostResponse event)

### What We're Building

```yaml
# PostResponse flow creates tasks from errors
PostResponse:
  - let: ts_errors = self.typescript_errors
  - for: error in $ts_errors
    then:
      task.create:
        objective: "Fix: $error.message"
        type: error
        file: $error.file
        line: $error.line
        evidence:
          - source: typescript
            message: $error.message
            code: $error.code

  # Point agent to task queue
  - if: self.ready_tasks | length > 0
    then:
      autoreply: "Run `aiki task ready --json` to see what needs fixing"

```

```bash
# Agent workflow
$ aiki task ready --json
{
  "tasks": [
    {
      "id": "err-a1b2c3d4",
      "objective": "Fix: Type 'null' is not assignable to type 'User'",
      "type": "error",
      "status": "open",
      "blocked_by": [],
      "assignee": null,
      "scope": {"files": [{"path": "src/auth.ts", "lines": [42]}]},
      "evidence": [{"source": "typescript", "message": "...", "code": "TS2322"}]
    }
  ]
}

$ aiki task start err-a1b2c3d4
Started: err-a1b2c3d4

# Agent fixes the error...

$ aiki task close err-a1b2c3d4 --fixed
Closed: err-a1b2c3d4

# Request review of the changes (separate from task)
$ aiki review request @ --from human --context "Fixed null check in auth"
Review requested: rev-xyz123
```

### Core Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Agent CLI                                 │
│  aiki task ready | create | start | close | assign | approve    │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                       TaskManager                                │
│  - Manages aiki/tasks branch (orphan, append-only)              │
│  - Appends events as JJ changes                                  │
│  - Reconstructs task state from event replay                     │
│  - NO SQLite cache (scan JJ directly)                           │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     JJ Repository                                │
│                                                                  │
│  Branch: aiki/tasks (orphan, linear event log)                  │
│  ├── change-001: [created err-a1b2]                             │
│  ├── change-002: [created err-c3d4, blocked_by: err-a1b2]       │
│  ├── change-003: [started err-a1b2]                             │
│  ├── change-004: [closed err-a1b2, fixed: true]                 │
│  └── change-005: [closed err-c3d4, fixed: true]                 │
│                                                                  │
│  Dependencies stored IN events, not as JJ DAG structure         │
└─────────────────────────────────────────────────────────────────┘
```

### Data Model

#### Agent Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    ClaudeCode,
    Cursor,
    Human,
    // Future: Aider, Copilot, etc.
}
```

#### Task Definition

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    // Core (required)
    pub objective: String,
    pub r#type: TaskType,
    pub priority: u8,  // 0-4 (0 = critical, 4 = backlog)

    // Scope (required for errors)
    pub scope: TaskScope,

    // Evidence (for errors)
    pub evidence: Vec<Evidence>,

    // Context (optional, richer descriptions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,      // Why this matters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approach: Option<String>,         // How to fix it
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_when: Option<String>,        // Acceptance criteria

    // Dependencies
    #[serde(default)]
    pub blocked_by: Vec<String>,          // Task IDs that block this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_from: Option<String>,  // Parent task this was found during
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,        // For hierarchical IDs (err-a1b2.1)

    // Assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<AgentType>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Error,
    Warning,
    Suggestion,
    Feature,
    Chore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskScope {
    pub files: Vec<FileScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileScope {
    pub path: PathBuf,
    #[serde(default)]
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
```

#### Event Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub event: EventType,
    pub timestamp: DateTime<Utc>,
    pub agent_type: AgentType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventType {
    // Lifecycle
    Created {
        task: TaskDefinition,
    },
    Started,
    Failed {
        attempt: u32,
    },
    Closed {
        fixed: bool,
    },

    // Assignment
    Assigned {
        to: AgentType,
        by: AgentType,
    },
    Unassigned,

    // Dependencies
    DependencyAdded {
        blocked_by: String,
        dep_type: DependencyType,
    },
    DependencyRemoved {
        blocked_by: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Blocks,          // Hard dependency - affects ready
    ParentChild,     // Hierarchy - affects ready
    DiscoveredFrom,  // Soft - informational only
    Related,         // Soft - informational only
}
```

#### Reconstructed Task State

```rust
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub definition: TaskDefinition,
    pub status: TaskStatus,
    pub assignee: Option<AgentType>,
    pub attempts: Vec<Attempt>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub fixed: Option<bool>,  // Set when closed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Open,
    InProgress,
    Closed,
}
```

### Hierarchical Task IDs

Tasks support hierarchical IDs for organizing epics and subtasks:

```
err-a1b2       (epic or standalone task)
err-a1b2.1     (subtask of err-a1b2)
err-a1b2.1.1   (sub-subtask)
err-a1b2.2     (another subtask)
```

**Implementation:**

```rust
impl TaskManager {
    pub fn create_subtask(
        &self,
        parent_id: &str,
        definition: TaskDefinition,
        agent_type: AgentType,
    ) -> Result<String> {
        // Find next available subtask number
        let existing = self.get_child_tasks(parent_id)?;
        let next_num = existing.len() + 1;

        let task_id = format!("{}.{}", parent_id, next_num);

        let mut def = definition;
        def.parent_id = Some(parent_id.to_string());

        // Subtasks implicitly depend on parent (parent-child relationship)
        // But parent doesn't block subtask - subtask blocks parent closing

        self.append_event(TaskEvent {
            task_id: task_id.clone(),
            event: EventType::Created { task: def },
            timestamp: Utc::now(),
            agent_type,
        })?;

        Ok(task_id)
    }
}
```

### Dependencies and Ready Queue

**Key insight from Beads:** "Ready" means tasks with NO open blockers.

```rust
impl TaskManager {
    /// Returns tasks that are:
    /// - Status: Open (not InProgress or Closed)
    /// - Not blocked by any open task
    /// - Not a parent with open children (for hierarchical tasks)
    pub fn query_ready(&self) -> Result<Vec<Task>> {
        let all_tasks = self.get_all_tasks()?;

        // Build set of open task IDs for quick lookup
        let open_ids: HashSet<_> = all_tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Closed)
            .map(|t| t.id.as_str())
            .collect();

        all_tasks
            .into_iter()
            .filter(|task| {
                // Must be open
                if task.status != TaskStatus::Open {
                    return false;
                }

                // All blockers must be closed
                let all_blockers_closed = task.definition.blocked_by
                    .iter()
                    .all(|blocker_id| !open_ids.contains(blocker_id.as_str()));

                if !all_blockers_closed {
                    return false;
                }

                // For parent tasks: all children must be closed
                // (Can't close epic until subtasks done)
                let children = self.get_child_tasks(&task.id)?;
                let all_children_closed = children
                    .iter()
                    .all(|c| c.status == TaskStatus::Closed);

                all_children_closed
            })
            .collect()
    }
}
```

### CLI Commands

```bash
# ═══════════════════════════════════════════════════════════════════════════════
# QUERYING TASKS
# ═══════════════════════════════════════════════════════════════════════════════

# Ready work (unblocked tasks)
aiki task ready [--json]
aiki task ready --assignee human [--json]
aiki task ready --type error [--json]

# List all tasks with filters
aiki task list [--json]
aiki task list --status open [--json]
aiki task list --assignee claude-code [--json]
aiki task list --blocked [--json]

# Show task details
aiki task show <task-id> [--json]

# ═══════════════════════════════════════════════════════════════════════════════
# CREATING TASKS
# ═══════════════════════════════════════════════════════════════════════════════

# Create standalone task
aiki task create "Fix null check in auth.ts" \
    --type error \
    --file src/auth.ts \
    --line 42 \
    --evidence "typescript:Object is possibly null:TS2531"

# Create with rich context
aiki task create "Add dark mode toggle" \
    --type feature \
    --description "Users requested dark mode for accessibility" \
    --approach "Use CSS variables, store preference in localStorage" \
    --done-when "Toggle works, persists, respects OS preference"

# Create subtask (hierarchical)
aiki task create "Fix null check" \
    --parent err-a1b2 \
    --type error

# Create with dependencies
aiki task create "Add error handling" \
    --type feature \
    --blocked-by err-a1b2 \
    --blocked-by err-c3d4

# Create discovered-from task
aiki task create "Found: missing validation" \
    --type error \
    --discovered-from err-a1b2

# ═══════════════════════════════════════════════════════════════════════════════
# TASK LIFECYCLE
# ═══════════════════════════════════════════════════════════════════════════════

# Start working on a task
aiki task start <task-id>

# Record failed attempt
aiki task fail <task-id>

# Close task
aiki task close <task-id> --fixed
aiki task close <task-id> --abandoned --reason "Not reproducible"

# ═══════════════════════════════════════════════════════════════════════════════
# ASSIGNMENT
# ═══════════════════════════════════════════════════════════════════════════════

# Assign task
aiki task assign <task-id> --to human
aiki task assign <task-id> --to cursor
aiki task assign <task-id> --to claude-code

# Unassign
aiki task unassign <task-id>

# ═══════════════════════════════════════════════════════════════════════════════
# DEPENDENCIES
# ═══════════════════════════════════════════════════════════════════════════════

# Add dependency
aiki task dep add <task-id> --blocked-by <blocker-id>
aiki task dep add <task-id> --blocked-by <blocker-id> --type discovered-from

# Remove dependency
aiki task dep remove <task-id> --blocked-by <blocker-id>

# Show dependency tree
aiki task dep tree <task-id>

# ═══════════════════════════════════════════════════════════════════════════════
# SYNC
# ═══════════════════════════════════════════════════════════════════════════════

# Sync tasks (push to remote, detect orphans)
aiki task sync
```

### Example CLI Output

```bash
$ aiki task ready --json
{
  "tasks": [
    {
      "id": "err-a1b2",
      "objective": "Fix null check in auth.ts:42",
      "type": "error",
      "status": "open",
      "priority": 1,
      "assignee": null,
      "blocked_by": [],
      "scope": {"files": [{"path": "src/auth.ts", "lines": [42]}]},
      "evidence": [{"source": "typescript", "message": "Object is possibly 'null'", "code": "TS2531"}]
    }
  ]
}

$ aiki task show err-a1b2
Task: err-a1b2
Objective: Fix null check in auth.ts:42
Type: error
Status: in_progress
Priority: 1
Assignee: human

Blocked by: (none)
Discovered from: (none)
Attempts: 1
```

### Flow Integration

```yaml
# ═══════════════════════════════════════════════════════════════════════════════
# PostResponse: Create tasks from errors
# ═══════════════════════════════════════════════════════════════════════════════
PostResponse:
  - let: ts_errors = self.typescript_errors

  - for: error in $ts_errors
    then:
      task.create:
        objective: "Fix: $error.message"
        type: error
        file: $error.file
        line: $error.line
        evidence:
          - source: typescript
            message: $error.message
            code: $error.code

  - let: ready_count = self.ready_tasks | length
  - if: $ready_count > 0
    then:
      autoreply: |
        There are $ready_count tasks ready. Run `aiki task ready --json` to see details.

# ═══════════════════════════════════════════════════════════════════════════════
# SessionStart: Notify of ready tasks
# ═══════════════════════════════════════════════════════════════════════════════
SessionStart:
  - let: ready_count = self.ready_tasks | length
  - if: $ready_count > 0
    then:
      autoreply: |
        You have $ready_count task(s) ready to work on.
        Run `aiki task ready --json` for details.
```

**Note:** Review-related flows are in the [Review System](#review-system) section.

### aiki task sync

Ensures task events are committed and pushed:

```rust
pub fn run_sync(repo_path: &Path) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    // 1. Verify aiki/tasks branch integrity
    let events = get_all_events(repo_path)?;
    report.total_events = events.len();

    // 2. Find orphaned in-progress tasks
    let tasks = reconstruct_all_tasks(&events)?;
    for task in &tasks {
        if task.status == TaskStatus::InProgress {
            // Warn about tasks that were started but never closed
            report.orphaned_in_progress.push(task.id.clone());
        }
    }

    // 3. Push to remote if configured
    if has_git_remote(repo_path, "aiki/tasks")? {
        push_branch(repo_path, "aiki/tasks")?;
        report.pushed = true;
    }

    // 4. Report summary
    eprintln!("Task sync complete:");
    eprintln!("  Total events: {}", report.total_events);
    eprintln!("  Orphaned in-progress: {}", report.orphaned_in_progress.len());
    if report.pushed {
        eprintln!("  Pushed to remote: yes");
    }

    Ok(report)
}
```

### Testing Strategy

**Unit tests:**
- Task ID generation (content-addressed, deterministic)
- Hierarchical ID generation (err-a1b2.1, err-a1b2.2)
- Event serialization/deserialization
- Task state reconstruction from events
- Ready queue filtering (respects dependencies)

**Integration tests:**
- Create task → verify event appended
- Create subtask → verify hierarchical ID
- Add dependency → verify blocks ready queue
- Start/close lifecycle
- Sync → verify push to remote

**E2E tests:**
- Flow creates tasks from TypeScript errors
- Agent queries ready tasks (filtered by dependencies)
- Agent completes task and closes it
- Multi-level hierarchy (epic → task → subtask)

### Phase 1 Deliverables

1. **Core library** (`cli/src/tasks/`)
   - `manager.rs` - TaskManager with JJ operations
   - `types.rs` - Task, TaskDefinition, EventType
   - `queries.rs` - Ready queue, dependency filtering

2. **CLI commands** (`aiki task ...`)
   - `ready`, `list`, `show`
   - `create` (with subtasks, dependencies)
   - `start`, `fail`, `close`
   - `assign`, `unassign`
   - `dep add`, `dep remove`, `dep tree`
   - `sync`

3. **Flow actions** (`task:` in YAML)
   - `create`, `close`, `fail`, `assign`

4. **Tests**
   - Unit tests for all components
   - Integration tests with real JJ repo

5. **Documentation**
   - CLI reference
   - Flow DSL reference

---

## Phase 2: Performance & Extended Features

**When to build**: Event reconstruction is slow (>1s) OR need compaction/external refs

### Compaction

For long-running sessions, summarize old closed tasks to reduce context:

```rust
pub enum EventType {
    // ... existing variants

    Compacted {
        summary: String,
        original_size: usize,
        compaction_level: u8,  // 1 = summarized, 2 = ultra-compact
    },
}

impl TaskManager {
    pub fn compact_task(&self, task_id: &str) -> Result<()> {
        let task = self.get_task(task_id)?;

        // Only compact closed tasks older than 30 days
        if task.status != TaskStatus::Closed {
            return Err(AikiError::TaskNotEligibleForCompaction(task_id.into()));
        }

        // Generate summary (could use Claude Haiku or simple template)
        let summary = format!(
            "{}\n\nOutcome: {} after {} attempt(s).",
            task.definition.objective,
            if task.was_fixed() { "Fixed" } else { "Abandoned" },
            task.attempts.len()
        );

        // Append compaction event (original events still in history)
        self.append_event(TaskEvent {
            task_id: task_id.to_string(),
            event: EventType::Compacted {
                summary,
                original_size: self.calculate_task_size(&task),
                compaction_level: 1,
            },
            timestamp: Utc::now(),
            agent_type: AgentType::System,
        })
    }
}
```

During reconstruction, if a `Compacted` event exists, use summary instead of full replay.

### External Refs

Link tasks to external systems (GitHub issues, Jira, etc.):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    // ... existing fields

    #[serde(default)]
    pub external_refs: Vec<ExternalRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalRef {
    pub system: ExternalSystem,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSystem {
    GitHub,
    GitHubPr,
    Jira,
    Linear,
    JjChange,
    Custom(String),
}
```

```bash
# Create task linked to GH issue
aiki task create "Fix auth bug" --ref gh:42

# Add ref to existing task
aiki task ref add err-a1b2 --ref gh:42 --url "https://github.com/org/repo/issues/42"
```

### SQLite Cache (If Needed)

Only add if event reconstruction becomes slow (>1s for typical queries):

```sql
CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY,
    objective TEXT NOT NULL,
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    assignee TEXT,
    blocked_by_json TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE sync_state (
    key TEXT PRIMARY KEY,
    last_event_change_id TEXT NOT NULL
);

CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_assignee ON tasks(assignee);
```

### Phase 2 Deliverables

- Compaction events and CLI (`aiki task compact`)
- External refs support
- SQLite cache (only if performance requires it)
- Task statistics (`aiki task stats`)

---

## Phase 3: Code Provenance

**When to build**: Need to track which code changes attempted/fixed tasks

### Bidirectional Links

```yaml
# Event includes code change reference
---
aiki_task_event: v1
task_id: err-a1b2
event: closed
timestamp: 2025-01-15T10:30:00Z
agent_type: claude-code
fixed: true
code_change: change-xyz123  # JJ change that fixed this
---

# JJ change description references tasks
---
aiki_change: v1
tasks:
  works_on: [err-a1b2]
  closes: [err-c3d4]
---
```

### Phase 3 Deliverables

- `code_change` field on relevant events
- `aiki provenance <change-id> --tasks`
- `aiki task show <task-id> --code-history`

---

## Phase 4: Multi-Agent Coordination

**When to build**: Multiple agents working on same codebase concurrently

Event sourcing already handles this well:
- Append-only events = no conflicts
- Content-addressed IDs = natural deduplication
- Multiple agents can create/update tasks concurrently

### Phase 4 Deliverables

- Multi-agent integration tests
- Event ordering verification
- Documentation: "Multi-Agent Task System Guide"

---

## Review System

**Key Insight:** Reviews are about **code changes**, not task completion. Reviews target JJ revsets, allowing review of single changes, ranges, or any revision set expression.

### Why Revsets?

```
┌─────────────────────────────────────────────────────────────────┐
│  TASK-CENTRIC REVIEWS (Previous design)                         │
│                                                                 │
│  Task ──→ ReviewRequested ──→ ReviewCompleted                   │
│  Problems:                                                       │
│    - What if no task exists?                                    │
│    - What about multi-task changes?                             │
│    - What about ad-hoc "review my code" requests?               │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  REVSET-CENTRIC REVIEWS (New design)                            │
│                                                                 │
│  Revset ──→ ReviewRequested ──→ ReviewCompleted                 │
│  Benefits:                                                       │
│    - Review any changes, task or not                            │
│    - Review ranges: "trunk()..@"                                │
│    - Review branches: "feature-auth::"                          │
│    - Uses JJ's native query language                            │
│    - Decouples concerns: tasks track work, reviews verify code  │
└─────────────────────────────────────────────────────────────────┘
```

### Data Model

```rust
// Stored on aiki/reviews branch (event-sourced like tasks)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewEvent {
    pub review_id: String,           // Content-addressed ID
    pub revset: String,              // JJ revset expression (e.g., "trunk()..@")
    pub resolved_changes: Vec<String>, // Snapshot of change_ids at request time
    pub event: ReviewEventType,
    pub timestamp: DateTime<Utc>,
    pub agent_type: AgentType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReviewEventType {
    Requested {
        from: AgentType,
        by: AgentType,
        context: Option<String>,
    },
    Completed {
        by: AgentType,
        outcome: ReviewOutcome,
    },
    Cancelled {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReviewOutcome {
    Approved {
        feedback: Option<String>,
    },
    Rejected {
        feedback: String,
        blocking_issues: Vec<String>,
    },
    ApprovedWithSuggestions {
        feedback: String,
        suggestions: Vec<String>,
    },
}
```

**Why store both `revset` and `resolved_changes`?**
- `revset`: Human-readable, shows intent ("trunk()..@", "feature-auth::")
- `resolved_changes`: Audit trail of exactly which changes were reviewed

### CLI

```bash
# ═══════════════════════════════════════════════════════════════════════════════
# REQUEST REVIEW
# ═══════════════════════════════════════════════════════════════════════════════

# Review current working copy change
aiki review request @

# Review all changes since trunk
aiki review request 'trunk()..@'

# Review specific change by ID
aiki review request xyz123

# Review with context
aiki review request 'trunk()..@' --from human --context "Ready for merge"

# Review from specific agent
aiki review request @ --from cursor --context "Check error handling"

# ═══════════════════════════════════════════════════════════════════════════════
# LIST & QUERY
# ═══════════════════════════════════════════════════════════════════════════════

# List pending reviews
aiki review list --pending

# List reviews awaiting specific agent
aiki review list --for human
aiki review list --for claude-code

# Show review details
aiki review show <review-id>

# Show reviews for changes in a revset
aiki review history 'trunk()..@'

# ═══════════════════════════════════════════════════════════════════════════════
# COMPLETE REVIEW
# ═══════════════════════════════════════════════════════════════════════════════

# Approve
aiki review approve <review-id>
aiki review approve <review-id> --feedback "Looks good"

# Approve with suggestions
aiki review approve <review-id> --with-suggestions \
    --feedback "Works but could be cleaner" \
    --suggestion "Consider extracting to helper"

# Reject
aiki review reject <review-id> --feedback "Missing error handling"
aiki review reject <review-id> \
    --feedback "Several issues" \
    --issue "Null check missing on line 42" \
    --issue "Error message not user-friendly"

# Cancel pending review
aiki review cancel <review-id> --reason "Changes superseded"
```

### Example Output

```bash
$ aiki review list --pending --json
{
  "reviews": [
    {
      "id": "rev-abc123",
      "revset": "trunk()..@",
      "resolved_changes": ["xyz789", "xyz790", "xyz791"],
      "requested_by": "claude-code",
      "requested_from": "human",
      "context": "Auth refactor complete, ready for review",
      "requested_at": "2025-01-15T10:00:00Z"
    }
  ]
}

$ aiki review history @
Change: xyz791 (current)
Reviews:
  ┌─ Review rev-abc123 ────────────────────────────────────────────
  │ Revset: trunk()..@
  │ Requested: 2025-01-15 10:00 by claude-code
  │ Awaiting: human
  │ Context: "Auth refactor complete, ready for review"
  └────────────────────────────────────────────────────────────────
```

### Flow Integration

```yaml
# ═══════════════════════════════════════════════════════════════════════════════
# change.completed: Auto-request review when significant work done
# ═══════════════════════════════════════════════════════════════════════════════
change.completed:
  - if: self.should_request_review($change)
    then:
      review.request:
        revset: "@"
        from: human
        context: "Completed: $change.description"

# ═══════════════════════════════════════════════════════════════════════════════
# session.ended: Batch review for session's work
# ═══════════════════════════════════════════════════════════════════════════════
session.ended:
  - let: session_range = self.session_revset  # e.g., "xyz123::@"
  - let: change_count = self.resolve_revset($session_range) | length

  - if: $change_count > 0 && !self.has_pending_review($session_range)
    then:
      review.request:
        revset: $session_range
        from: human
        context: "Session complete - $change_count change(s)"

# ═══════════════════════════════════════════════════════════════════════════════
# session.started: Notify of pending reviews
# ═══════════════════════════════════════════════════════════════════════════════
session.started:
  - let: my_reviews = self.pending_reviews_for($agent_type)
  - if: $my_reviews | length > 0
    then:
      autoreply:
        append: |
          # Pending Reviews

          You have $my_reviews.length review(s) awaiting your feedback:
          $for review in $my_reviews:
            • $review.id: $review.context ($review.revset)

          Run `aiki review list --for $agent_type` for details.
```

### Relationship to Tasks

Tasks and reviews are orthogonal:

```
┌─────────────────────────────────────────────────────────────────┐
│  TASKS                          │  REVIEWS                      │
│  Track work to be done          │  Verify code quality          │
│  "Fix the auth bug"             │  "Review changes xyz..@"      │
│  Has status, dependencies       │  Has outcome (approve/reject) │
│  Stored on aiki/tasks branch    │  Stored on aiki/reviews branch│
└─────────────────────────────────────────────────────────────────┘

Connections:
- Task close event CAN reference a change_id (what fixed it)
- Review CAN cover changes that fixed multiple tasks
- Neither requires the other
```

### Deliverables

1. **Core library** (`cli/src/reviews/`)
   - `types.rs` - ReviewEvent, ReviewOutcome
   - `manager.rs` - ReviewManager with JJ operations
   - `queries.rs` - Pending reviews, history lookup

2. **CLI commands** (`aiki review ...`)
   - `request`, `list`, `show`, `history`
   - `approve`, `reject`, `cancel`

3. **Flow actions** (`review:` in YAML)
   - `request`, `approve`, `reject`

4. **Tests**
   - Revset resolution
   - Review lifecycle
   - Multi-change reviews

---

## Agent Adoption: Native Integration

**The Key Insight:** Aiki integrates with agents through two mechanisms, both using the same flow system:

1. **ACP Proxy** (for Zed, future editors): Aiki runs as transparent proxy, intercepting ALL protocol messages
2. **Editor Hooks** (for Claude Code, Cursor): Aiki registers as hook consumer with the editor's native hook system

Both approaches fire the same Aiki events and run the same flows. The difference is architecture:

### Integration Architectures

```
┌─────────────────────────────────────────────────────────────────┐
│  ACP PROXY MODE (Zed, ACP-compatible editors)                   │
│                                                                 │
│  IDE ←→ Aiki ACP Proxy ←→ Agent Process                         │
│              ↑                                                   │
│    - Aiki IS the intermediary                                   │
│    - Intercepts all protocol messages                           │
│    - Fires events from intercepted traffic                      │
│    - Full visibility into agent communication                   │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  EDITOR HOOKS MODE (Claude Code, Cursor)                        │
│                                                                 │
│  Agent ←→ [Editor's Hook System] ←→ aiki hooks handle           │
│                   ↑                                              │
│    - Editor calls Aiki as hook consumer                         │
│    - Installed via `aiki hooks install`                         │
│    - Editor controls when hooks fire                            │
│    - Converts editor events → Aiki events                       │
└─────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
cli/src/editors/
├── acp/           # ACP proxy implementation
│   ├── handlers.rs    # Fires Aiki events from ACP messages
│   ├── protocol.rs    # ACP protocol types
│   └── state.rs       # Session/autoreply state
├── claude_code/   # Claude Code hook integration
│   ├── events.rs      # Claude events → Aiki events
│   └── output.rs      # Aiki results → Claude hook format
├── cursor/        # Cursor hook integration
│   ├── events.rs      # Cursor events → Aiki events
│   └── output.rs      # Aiki results → Cursor hook format
└── mod.rs         # Shared utilities
```

### Why Both Approaches Use the Same Flows

| Aspect | ACP Proxy | Editor Hooks |
|--------|-----------|--------------|
| Setup | `aiki acp claude-code` | `aiki hooks install` |
| Event source | Intercept ACP protocol | Editor calls hooks |
| Aiki events | Same (`session.started`, `response.received`, etc.) | Same |
| Flow execution | Same flow engine | Same flow engine |
| Task context injection | `session.started` flow | `session.started` flow |
| Auto-sync on end | `session.ended` flow | `session.ended` flow |

**Key benefit:** Write flows once, work with all editors.

### Comparison with Beads

| Aspect | Beads | Aiki |
|--------|-------|------|
| Setup | `bd setup claude` + `bd hooks install` | `aiki init` (installs hooks) OR `aiki acp` (starts proxy) |
| Context injection | Manual `bd prime` | Automatic via `session.started` flow |
| Session end sync | User remembers `bd sync` | Automatic via `session.ended` flow |
| Task creation | Manual `bd create` | Auto from `response.received` flow |
| Task auto-close | Manual `bd close` | Auto from `change.completed` flow |
| Compaction survival | PreCompact hook | `prompt.submitted` flow |

### Core Flow Additions

The task system integrates into the existing `cli/src/flows/core/flow.yaml`:

```yaml
# ═══════════════════════════════════════════════════════════════════════════════
# session.started: Inject task context when session begins
# ═══════════════════════════════════════════════════════════════════════════════
session.started:
  # ... existing initialization (jj new, aiki init --quiet) ...

  # Task context injection
  - if: self.has_task_system
    then:
      - let: ready_count = self.task_ready_count
      - if: $ready_count > 0
        then:
          autoreply:
            append: |
              # Tasks
              📋 $ready_count task(s) ready. Run `aiki task ready --json` for details.

  # Review context injection
  - if: self.has_review_system
    then:
      - let: pending_reviews = self.pending_reviews_for($agent_type)
      - if: $pending_reviews | length > 0
        then:
          autoreply:
            append: |
              # Pending Reviews
              ⚠️ $pending_reviews.length review(s) awaiting your feedback.
              Run `aiki review list --for $agent_type` for details.

# ═══════════════════════════════════════════════════════════════════════════════
# session.ended: Auto-sync tasks before session ends
# ═══════════════════════════════════════════════════════════════════════════════
session.ended:
  # Sync tasks to remote (if configured)
  - if: self.has_task_system
    then:
      - shell: aiki task sync --quiet
        on_failure: continue

      # Warn about orphaned in-progress tasks
      - let: orphaned = self.task_orphaned_in_progress
      - if: $orphaned | length > 0
        then:
          - log: "Warning: $orphaned.length task(s) left in progress: $orphaned"

# ═══════════════════════════════════════════════════════════════════════════════
# response.received: Create tasks from errors, remind about task queue
# ═══════════════════════════════════════════════════════════════════════════════
response.received:
  - if: self.has_task_system
    then:
      # Parse response for TypeScript/build errors
      - let: errors = self.parse_response_errors($response)

      # Create tasks for new errors (deduped by content hash)
      - for: error in $errors
        then:
          task.create:
            objective: "Fix: $error.message"
            type: error
            file: $error.file
            line: $error.line
            evidence:
              - source: $error.source
                message: $error.message
                code: $error.code

      # Remind about task queue if errors were created
      - if: $errors | length > 0
        then:
          autoreply:
            append: |
              Created $errors.length task(s) for errors above.
              Run `aiki task ready --json` to see the queue.

# ═══════════════════════════════════════════════════════════════════════════════
# change.completed: Auto-close tasks and request review
# ═══════════════════════════════════════════════════════════════════════════════
change.completed:
  # ... existing provenance tracking ...

  # Auto-close tasks when errors are fixed
  - if: self.has_task_system && $event.write
    then:
      - let: fixed_tasks = self.task_check_fixed($modified_files)
      - for: task in $fixed_tasks
        then:
          task.close:
            id: $task.id
            fixed: true

  # Request review of the change (separate from tasks)
  - if: self.has_review_system && self.should_request_review($change)
    then:
      review.request:
        revset: "@"
        from: human
        context: "Change completed: $change.description"
```

### self.* Functions

Functions available in flows:

```rust
// Task system functions
fn has_task_system(state: &AikiState) -> Result<bool>;
fn task_ready_count(state: &AikiState) -> Result<u32>;
fn task_orphaned_in_progress(state: &AikiState) -> Result<Vec<String>>;
fn parse_response_errors(state: &AikiState, response: &str) -> Result<Vec<ParsedError>>;
fn task_check_fixed(state: &AikiState, files: Vec<PathBuf>) -> Result<Vec<Task>>;

// Review system functions
fn has_review_system(state: &AikiState) -> Result<bool>;
fn pending_reviews_for(state: &AikiState, agent: AgentType) -> Result<Vec<Review>>;
fn should_request_review(state: &AikiState, change: &Change) -> Result<bool>;
fn has_pending_review(state: &AikiState, revset: &str) -> Result<bool>;
fn resolve_revset(state: &AikiState, revset: &str) -> Result<Vec<String>>;
fn session_revset(state: &AikiState) -> Result<String>;  // e.g., "xyz123::@"
```

### User Flow Composition

Users can extend task behavior in `.aiki/flows/tasks.yaml`:

```yaml
name: "Project Tasks"
description: "Custom task workflows for this project"
version: "1"

# Add project-specific error parsing
response.received:
  - let: rust_errors = self.parse_rust_errors($response)
  - for: error in $rust_errors
    then:
      task.create:
        objective: "Fix: $error.message"
        type: error
        file: $error.file
        line: $error.line
        evidence:
          - source: rustc
            message: $error.message
            code: $error.code

# Custom review workflow - require review before any PR
shell.permission_asked:
  - if: $command | starts_with("git push") || $command | starts_with("gh pr create")
    then:
      - let: unreviewed_changes = self.changes_without_review('trunk()..@')
      - if: $unreviewed_changes | length > 0
        then:
          block: "Cannot push: $unreviewed_changes.length change(s) not reviewed"
```

### No Separate Commands Needed

Because everything is integrated via flows:

| Beads Command | Aiki Equivalent |
|---------------|-----------------|
| `bd setup claude` | Not needed - ACP proxy handles it |
| `bd prime` | Not needed - `session.started` flow injects context |
| `bd sync` (manual) | Not needed - `session.ended` flow auto-syncs |
| `bd ready` | `aiki task ready` (CLI still available) |
| `bd create` | Auto via `response.received` flow, or `aiki task create` |
| `bd close` | Auto via `change.completed` flow, or `aiki task close` |

### Agent Experience

From the agent's perspective, tasks and reviews "just work":

**Tasks:**
1. **Session starts** → Agent sees ready task count
2. **Errors appear** → Tasks are auto-created
3. **Errors fixed** → Tasks auto-close
4. **Session ends** → Tasks auto-sync

**Reviews:**
1. **Session starts** → Agent sees pending reviews
2. **Changes made** → Review auto-requested (if configured)
3. **Human reviews** → Agent notified of outcome
4. **Session ends** → Reviews auto-sync

No manual commands needed for the happy path. CLI commands (`aiki task ...`, `aiki review ...`) are available for manual control.

### Implementation Phases

| Component | Phase | Notes |
|-----------|-------|-------|
| Core flow additions (`session.started`, `session.ended`) | Phase 1 | Required for native integration |
| `self.*` task functions | Phase 1 | Enable flow-based task operations |
| `self.*` review functions | Phase 1 | Enable flow-based review operations |
| `response.received` error parsing | Phase 1 | Auto-create tasks from errors |
| `change.completed` fix detection | Phase 1 | Auto-close tasks, auto-request review |
| User flow composition | Phase 1 | `.aiki/flows/*.yaml` support |
| `prompt.submitted` context refresh | Phase 2 | Survive context compaction |

---

## Summary Table

| Component | Delivers | When to Build |
|-----------|----------|---------------|
| **Task System Phase 1** | Core tasks, dependencies, hierarchical IDs, assignments, sync | **Now** |
| **Review System** | Revset-based reviews, approve/reject workflow | **Now** |
| **Task System Phase 2** | Compaction, external refs, SQLite cache (if needed) | When sessions are long or need integrations |
| **Task System Phase 3** | Code provenance (task ↔ change links) | When need to track what fixed what |
| **Task System Phase 4** | Multi-agent coordination | When testing concurrent agents |

---

## Next Steps

1. Review this updated plan
2. Create implementation tickets
3. Start with TaskManager core + dependencies
4. Add ReviewManager with revset support
5. Add sync commands for both
6. Ship Phase 1 + Review System

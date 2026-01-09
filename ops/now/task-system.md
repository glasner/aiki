# Aiki Task System: Unified Implementation Plan

**Date**: 2026-01-09  
**Status**: Design Complete - Ready for Implementation  
**Focus**: AI-first task system optimized for coding agents

---

## Table of Contents

- [Overview](#overview)
- [Design Principles](#design-principles)
- [Architecture](#architecture)
- [Data Model](#data-model)
- [CLI Commands](#cli-commands)
- [Flow Integration](#flow-integration)
- [Implementation Phases](#implementation-phases)
- [Future Ideas](#future-ideas)

---

## Overview

### Vision

An **AI-first task system** designed for coding agents to track work, manage context, and coordinate with humans. Tasks are:

- **Fast to create** - Single command, smart defaults
- **Easy to navigate** - Ready queue always visible via context
- **Naturally integrated** - Flows auto-create tasks from errors/work
- **Event-sourced** - Stored as immutable events on `aiki/tasks` branch

### Relationship to Beads

Aiki's task system is heavily inspired by Beads `bd`. Both track:
- What needs to be done
- What's currently being worked on
- What's blocked
- What's been completed

**Key differences**:
- **Storage**: Aiki uses event-sourced JJ branch, Beads uses its own format
- **Integration**: Aiki tasks are deeply integrated with flows (auto-create, auto-close)
- **Provenance**: Aiki links tasks to JJ change IDs for code tracking
- **AI-first**: Optimized for AI agents with XML output and context elements

---

## Design Principles

### 1. XML-First Output (AI-Friendly)
- All commands return XML by default
- Consistent structure across commands
- Every command includes `context` element showing queue state
- Mixed content (markdown, code) embeds naturally without escaping

### 2. Smart Defaults (Reduce Typing)
- Only `name` required for task creation
- Priority defaults to `p2` (normal)
- Assignee defaults to current agent

### 3. Context Everywhere (Reduce Discovery Overhead)
- `list` defaults to ready queue (sorted by priority)
- `<context>` element on every command shows queue state
- Always know: current task, next task, queue size

### 4. Minimal Commands (Reduce Ceremony)
- Phase 1: Just 5 commands (`add`, `list`, `start`, `stop`, `close`)
- No separate commands for pause/resume/defer/reopen
- `start` handles everything (new, stopped, or closed with `--reopen`)

### 5. Event-Sourced Storage
- Stored as fileless JJ changes on `aiki/tasks` branch
- Each event is a JJ change with metadata in description
- Immutable by nature, views materialized on read
- No file system or database overhead

---

## Architecture

### Core Components

```
┌─────────────────────────────────────────────────────┐
│                   CLI Commands                       │
│  add • list • show • start • stop • close           │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│                  Task Engine                         │
│  • Event writer (append to aiki/tasks branch)       │
│  • Event reader (materialize views)                 │
│  • Ready queue calculator                           │
│  • Context generator                                │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│                 Flow Integration                     │
│  • Auto-create tasks from errors                    │
│  • Auto-close tasks on fix                          │
│  • Inject context into prompts                      │
└─────────────────────────────────────────────────────┘
```

### Storage Model

Tasks are stored as **fileless JJ changes** on the `aiki/tasks` branch. Each task event is a JJ change with no file modifications - just metadata in the change description:

```
# Each event creates a JJ change like:
jj new --no-edit aiki/tasks -m '[aiki-task]
event=created
task_id=a1b2
name=Fix auth bug
priority=p2
timestamp=2026-01-09T10:30:00Z
[/aiki-task]'
```

**Benefits**:
- No file system overhead
- JJ handles deduplication and storage
- **Queryable with JJ revsets**: `jj log -r 'aiki/tasks & description("task_id=a1b2")'`
- Immutable by nature
- Views materialized on read by scanning change descriptions

**Example queries**:
```bash
# All task events
jj log -r 'aiki/tasks'

# Events for specific task
jj log -r 'aiki/tasks & description("task_id=a1b2")'

# All created events
jj log -r 'aiki/tasks & description("event=created")'

# Tasks by priority
jj log -r 'aiki/tasks & description("priority=p0")'
```

---

## Data Model

### Core Enums

```rust
pub enum TaskStatus {
    Open,        // Ready to work on
    InProgress,  // Currently being worked on
    Stopped,     // Was in progress, now stopped (has reason)
    Closed,      // Done or won't do
}

pub enum TaskPriority {
    P0,  // Critical/urgent
    P1,  // High priority
    P2,  // Normal priority (default)
    P3,  // Low priority
}

pub enum TaskOutcome {
    Done,    // Completed successfully
    WontDo,  // Won't implement
}
```

### Task Event Types

**Event Categories**:
- **Lifecycle**: `Created`, `Started`, `Stopped`, `Closed`, `Reopened`
- **Metadata**: `CommentAdded`, `Updated`

```rust
pub enum TaskEvent {
    Created {
        task_id: String,  // Encodes hierarchy: "a1b2.1" is child of "a1b2"
        name: String,
        priority: TaskPriority,
        body: Option<String>,
        scope: Option<TaskScope>,
        blocks: Vec<String>,  // Tasks this task blocks
        assignee: Option<String>,
        timestamp: DateTime<Utc>,
    },
    Started {
        task_id: String,
        agent_type: String,
        timestamp: DateTime<Utc>,
        stopped: Option<String>,  // Task ID that was auto-stopped
    },
    Stopped {
        task_id: String,
        reason: Option<String>,
        blockers: Option<Vec<String>>,  // Created blocker task IDs
        timestamp: DateTime<Utc>,
    },
    Closed {
        task_id: String,
        outcome: TaskOutcome,  // Done, WontDo
        duplicate_of: Option<String>,  // Task ID this is duplicate of
        timestamp: DateTime<Utc>,
    },
    Reopened {
        task_id: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    CommentAdded {
        task_id: String,
        text: String,
        timestamp: DateTime<Utc>,
    },
    Updated {
        task_id: String,
        fields: HashMap<String, Value>,
        timestamp: DateTime<Utc>,
    },
}
```

**Derived Fields** (computed during event reconstruction):
- `blocked_by`: List of tasks blocking this task (derived by scanning all tasks' `blocks` fields)
- `attempts`: Count of `Started` → `Stopped` cycles
- `status`: Current lifecycle state based on latest event

### TaskScope

```rust
pub struct TaskScope {
    pub files: Vec<ScopedFile>,
}

pub struct ScopedFile {
    pub path: PathBuf,
    pub lines: Option<LineRange>,
}

pub struct LineRange {
    pub start: usize,
    pub end: Option<usize>,
}
```

### Hierarchical Task IDs

Task IDs encode parent/child relationships:

```
a1b2      → Parent task (short hash)
a1b2.1    → First child
a1b2.2    → Second child
a1b2.2.1  → Grandchild
```

**Benefits**:
- Clear hierarchy without separate parent field
- Easy to query all children: `starts_with("a1b2.")`
- Natural sorting keeps families together
- Short, memorable IDs (like Beads: `bd-a1b2`)

---

## CLI Commands

### Response Structure (All Commands)

Every command returns an `<aiki_task>` root element with attributes and child elements:

```xml
<aiki_task cmd="start" status="ok">
  <!-- What happened -->
  <started>...</started>
  
  <!-- Current state -->
  <context>
    <in_progress>
      <task id="a1b2" name="Fix null check in auth.ts"/>
    </in_progress>
    <list ready="3">
      <task id="def" name="Fix missing return" priority="p0"/>
      <task id="ghi" name="Consider using const" priority="p1"/>
      <task id="jkl" name="Add dark mode" priority="p2"/>
    </list>
  </context>
</aiki_task>
```

**Root Attributes**:
- `cmd` - Command that produced this output (add, list, start, stop, close, show, comment, update)
- `status` - Result status: "ok" or "error"
- `scope` - (Optional) Parent task ID when working within a parent's children

**Task Element Format**:
- `id` (attribute) - Task identifier
- `name` (attribute) - Task name
- `priority` (attribute, optional) - Priority level (p0-p3)
- Text content (optional) - Task body/description

**Context Notes**:
- `in_progress` can contain multiple tasks (work on multiple tasks simultaneously)
- `list` shows ready queue for batching decisions (typically 3-5 tasks)
- `ready` attribute on `list` is total ready tasks (may be more than shown in `list`)
- When working on parent task, `list` shows only that parent's children and `scope` is set

### Phase 1 Commands (V1.0)

#### `aiki task add <name>`

Create a single task.

```bash
aiki task add "Fix null check in auth.ts"

# XML output:
<aiki_task cmd="add" status="ok">
  <added>
    <task id="a1b2" name="Fix null check in auth.ts" priority="p2" assignee="claude-code"/>
  </added>
  
  <context>
    <in_progress/>
    <list ready="3">
      <task id="abc" name="Fix null check" priority="p0"/>
    </list>
  </context>
</aiki_task>
```

**Options**:
- `--p0`, `--p1`, `--p2`, `--p3` - Set priority (default: p2) [Phase 3]
- `--body <text>` - Add description [Phase 4]
- `--parent <id>` - Create as child of existing task [Phase 2]

#### `aiki task list`

Show ready queue (default behavior).

```bash
aiki task list

# XML output:
<aiki_task cmd="list" status="ok">
  <list total="3">
    <task id="abc" name="Fix null check" priority="p0"/>
    <task id="def" name="Fix missing return" priority="p0"/>
    <task id="ghi" name="Consider using const" priority="p1"/>
  </list>
  
  <context>
    <in_progress/>
    <list ready="3">
      <task id="abc" priority="p0" name="Fix null check"/>
    </list>
  </context>
</aiki_task>
```

**Options**:
- `--all` - Show all tasks regardless of status
- `--open`, `--in-progress`, `--stopped`, `--closed` - Filter by status [Phase 3]

**Default Behavior**:
- Shows Open, unblocked, unclaimed tasks (ready queue)
- Sorted by priority (highest first), then creation time (oldest first)

#### `aiki task start [id...]`

Start working on one or more tasks. If no ID provided, starts first task from `context.list`. Auto-stops current tasks and replaces with new ones.

**No Arguments** (auto-start from list):
```bash
# With context showing multiple ready tasks:
aiki task start

# Starts first task from context.list:
<aiki_task cmd="start" status="ok">
  <started>
    <task id="def" priority="p0" name="Fix missing return">
  Function validateUser in src/auth.ts is missing return statement on line 42. This causes undefined behavior when validation fails.
</task>
  </started>
  
  <context>
    <in_progress>
      <task id="c3d4" name="Fix missing return"/>
    </in_progress>
    <list ready="2">
      <task id="ghi" priority="p1" name="Consider using const"/>
      <task id="jkl" priority="p2" name="Add dark mode"/>
    </list>
  </context>
</aiki_task>
```

**Single Task**:
```bash
aiki task start a1b2

# XML output:
<aiki_task cmd="start" status="ok">
  <stopped reason="Started a1b2: Fix null check in auth.ts">
    <task id="bug-xyz" name="Fix validation bug"/>
  </stopped>
  
  <started>
    <task id="abc" priority="p0" name="Fix null check in auth.ts">
  The authenticateUser function doesn't check if user is null before accessing user.role. Add null check before line 156.
</task>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2" name="Fix null check in auth.ts"/>
    </in_progress>
    <list ready="2">
      <task id="def" priority="p0" name="Fix missing return"/>
    </list>
  </context>
</aiki_task>
```

**Multiple Tasks** (work on multiple tasks with single fix):
```bash
aiki task start rev-234.1 rev-234.2

# XML output:
<aiki_task cmd="start" status="ok">
  <stopped>
    <!-- Previously active tasks -->
  </stopped>
  
  <started>
    <task id="rev-234.1" priority="p2" name="Fix typo in docs">
  README.md line 23 has 'recieve' instead of 'receive'
</task>
    <task id="rev-234.2" priority="p0" name="Add null check">
  Add null check in processPayment before accessing payment.amount
</task>
  </started>
  
  <context>
    <in_progress>
      <task id="rev-234.1" name="Fix typo in docs"/>
      <task id="rev-234.2" name="Add null check"/>
    </in_progress>
    <list ready="3">
      <task id="rev-234.3" priority="p2" name="Update tests"/>
    </list>
  </context>
</aiki_task>
```

**Starting Parent Task** (scopes ready queue to children):

When you start a parent task, it:
1. Auto-creates `parent.0` planning task
2. Starts the planning task
3. Scopes ready queue to show only children

```bash
aiki task start rev-234

# XML output:
<aiki_task cmd="start" status="ok" scope="rev-234">
  <started>
    <task id="rev-234.0" priority="p2" name="Review all subtasks and start first batch">
  Review the following subtasks and start working on related ones that can be addressed together:

**rev-234.1: Fix typo in docs** [chore, p2]
README.md line 23 has 'recieve' instead of 'receive'

**rev-234.2: Add null check** [error, p0]
Add null check in processPayment before accessing payment.amount

**rev-234.3: Update tests** [chore, p2]
Update payment processing tests to cover null payment scenario

Once you've reviewed, start the related tasks with: aiki task start &lt;id&gt; [id...]
</task>
  </started>
  
  <context>
    <in_progress>
      <task id="rev-234.0" name="Review all subtasks and start first batch"/>
    </in_progress>
    <list ready="4">
      <task id="rev-234.1" priority="p2" name="Fix typo in docs"/>
      <task id="rev-234.2" priority="p0" name="Add null check"/>
      <task id="rev-234.3" priority="p2" name="Update tests"/>
    </list>
  </context>
</aiki_task>

# Now list shows ONLY children of rev-234:
aiki task list

<aiki_task cmd="list" status="ok" scope="rev-234">
  <list total="4">
    <task id="rev-234.0" status="in_progress" name="Review all subtasks and start first batch"/>
    <task id="rev-234.1" name="Fix typo in docs"/>
    <task id="rev-234.2" name="Add null check"/>
    <task id="rev-234.3" name="Update tests"/>
  </list>
  
  <context>
    <in_progress>
      <task id="rev-234.0" name="Review all subtasks and start first batch"/>
    </in_progress>
    <list ready="4">
      <task id="rev-234.1" priority="p2" name="Fix typo in docs"/>
      <task id="rev-234.2" priority="p0" name="Add null check"/>
      <task id="rev-234.3" priority="p2" name="Update tests"/>
    </list>
  </context>
</aiki_task>

# After reviewing, start specific children:
aiki task start rev-234.1 rev-234.2
# Planning task (rev-234.0) auto-closes
# current: [rev-234.1, rev-234.2]
```

**Options**:
- `--reopen --reason <text>` - Reopen a closed task [Phase 4]

#### `aiki task stop [id]`

Stop working on a task. Defaults to current task if `id` not provided.

```bash
# Stop current task with reason:
aiki task stop --reason "Need design decision"

# Stop specific task:
aiki task stop a1b2 --reason "Need design decision"

# Stop with blocker (creates blocker task):
aiki task stop --blocked "Need API credentials from ops team"

# XML output:
<aiki_task cmd="stop" status="ok">
  <stopped reason="Need design decision">
    <task id="a1b2" name="Fix null check in auth.ts"/>
  </stopped>
  
  <context>
    <in_progress/>
    <list ready="3">
      <task id="def" priority="p0" name="Fix missing return"/>
    </list>
  </context>
</aiki_task>
```

**Options**:
- `--reason <text>` - Reason for stopping
- `--blocked <reason>` - Create blocker task (assigned to human)
- Multiple `--blocked` flags [Phase 3]

#### `aiki task close [id...]`

Mark one or more tasks as done. Defaults to current tasks if no `id` provided.

**Single Task**:
```bash
# Close current task(s):
aiki task close

# Close specific task:
aiki task close a1b2

# XML output:
<aiki_task cmd="close" status="ok">
  <closed outcome="done" code_change="qpvuntsm">
    <task id="a1b2" name="Fix null check in auth.ts"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="2">
      <task id="def" priority="p0" name="Fix missing return"/>
    </list>
  </context>
</aiki_task>
```

**Multiple Tasks** (fixed by same change):
```bash
aiki task close rev-234.1 rev-234.2

# XML output:
<aiki_task cmd="close" status="ok">
  <closed outcome="done">
    <task id="rev-234.1" name="Fix typo in docs"/>
    <task id="rev-234.2" name="Add null check"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="1">
      <task id="rev-234.3" priority="p2" name="Update tests"/>
    </list>
  </context>
</aiki_task>
```

**Closing Parent Task**:

When you close a parent task:
- Ready queue returns to global scope (exits parent context)
- Parent closes only if all children are closed
- If children remain open, returns error

```bash
# Close parent (after all children closed):
aiki task close rev-234

# XML output:
<aiki_task cmd="close" status="ok">
  <closed outcome="done">
    <task id="rev-234" name="Review feedback"/>
  </closed>
  
  <context>
    <!-- Back to global ready queue -->
    <in_progress/>
    <list ready="5">
      <task id="abc" priority="p0" name="Fix null check"/>
    </list>
  </context>
</aiki_task>

# If children still open:
aiki task close rev-234

<aiki_task cmd="close" status="error">
  <error>Cannot close rev-234, children still open: rev-234.3</error>
</aiki_task>
```

**Options**:
- `--wont-do` - Won't implement (default: done)
- `--duplicate <task-id>` - Duplicate of another task

### Phase 2 Commands (V1.1)

#### Hierarchical Tasks - Create Child

Create a child task using `--parent` flag.

```bash
aiki task add "Set up test fixtures" --parent a1b2

# XML output:
<aiki_task cmd="add" status="ok">
  <added>
    <task id="a1b2.1" name="Set up test fixtures"/>
  </added>
  
  <context>
    <in_progress/>
    <list ready="1">
      <task id="a1b2.1" priority="p2" name="Set up test fixtures"/>
    </list>
  </context>
</aiki_task>
```

### Phase 3 Commands (V1.2)

#### Status and Priority Flags

Filter tasks by status and set priority on creation.

```bash
# Filter by status:
aiki task list --open
aiki task list --in-progress
aiki task list --stopped
aiki task list --closed

# Set priority on creation:
aiki task add "Critical bug" --p0
aiki task add "Nice to have" --p3
```

### Phase 4 Commands (V1.3)

#### `aiki task show [id]`

Show task details. Defaults to current task if `id` not provided. Automatically includes children for parent tasks.

```bash
# Show current task:
aiki task show

# Show specific task:
aiki task show e5f6

# XML output:
<aiki_task cmd="show" status="ok">
  <task id="abc" name="Add authentication" status="open" priority="p2">
    Implement user authentication system with JWT tokens. Includes user model, login/logout endpoints, and middleware for protected routes.
    
    <children>
      <task id="e5f6.1" status="closed" name="Create User model">
  Create User model with email, password hash, and role fields
</task>
      <task id="e5f6.2" status="in_progress" name="Add login endpoint">
  POST /api/login endpoint that validates credentials and returns JWT token
</task>
      <task id="e5f6.3" status="open" name="Add JWT middleware">
  Middleware to verify JWT tokens and attach user to request context
</task>
      <task id="e5f6.4" status="open" name="Update frontend">
  Add login form and store JWT token in localStorage
</task>
    </children>
    
    <progress completed="1" total="4" percentage="25"/>
  </task>
  
  <context>
    <list ready="3">
      <!-- ... -->
    </list>
  </context>
</aiki_task>
```

#### `aiki task update [id]`

Modify task details. Defaults to current task if `id` not provided.

```bash
# Update current task:
aiki task update --body "Found in auth.ts:42, null check needed"
aiki task update --p0

# Update specific task:
aiki task update a1b2 --body "Found in auth.ts:42, null check needed"
aiki task update a1b2 --scope-file src/auth.ts --scope-line 42
```

#### `aiki task comment [id] <text>`

Add a comment to a task. Defaults to current task if `id` not provided.

```bash
# Add comment to current task:
aiki task comment "Tried adding null check, but error persists"

# Add comment to specific task:
aiki task comment a1b2 "Tried adding null check, but error persists"

# XML output:
<aiki_task cmd="comment" status="ok">
  <comment_added task_id="a1b2" timestamp="2026-01-09T10:30:00Z">
    <text>Tried adding null check, but error persists</text>
  </comment_added>
  
  <context>
    <list ready="3">
      <!-- ... -->
    </list>
  </context>
</aiki_task>
```

---

## Hierarchical Task Workflow

### Complete Walkthrough: Parent → Child → Grandchild

This walkthrough demonstrates how scope changes as you work through nested tasks.

#### Initial State: Create Parent with Children

```bash
# Create parent task first
$ aiki task add "Fix auth system"

# Then create children
$ aiki task add "Understand root cause" --parent a1b2
$ aiki task add "Write failing test" --parent a1b2
$ aiki task add "Implement fix" --parent a1b2
$ aiki task add "Verify fix works" --parent a1b2
```

```xml
<aiki_task cmd="add" status="ok">
  <added>
    <task id="a1b2.4" name="Verify fix works"/>
  </added>
  <context>
    <in_progress/>
    <list ready="5">
      <task id="a1b2" priority="p2" name="Fix auth system"/>
      <!-- Other unrelated tasks -->
    </list>
  </context>
</aiki_task>
```

#### Step 1: Start Parent Task

```bash
$ aiki task start a1b2
```

```xml
<aiki_task cmd="start" status="ok" scope="a1b2">
  <started>
    <!-- Auto-created planning task -->
    <task id="a1b2.0" name="Review all subtasks and start first batch">
  Review the following subtasks and start working on related ones that can be addressed together:

**a1b2.1: Understand root cause** [p2]

**a1b2.2: Write failing test** [p2]

**a1b2.3: Implement fix** [p2]

**a1b2.4: Verify fix works** [p2]

Once you've reviewed:
- Run `aiki task close` to close this planning task
- Run `aiki task start` to start the next subtask (or specify IDs for batch work)
</task>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2.0" name="Review all subtasks and start first batch"/>
    </in_progress>
    <list ready="4">
      <task id="a1b2.1" priority="p2" name="Understand root cause"/>
      <task id="a1b2.2" priority="p2" name="Write failing test"/>
      <task id="a1b2.3" priority="p2" name="Implement fix"/>
      <task id="a1b2.4" priority="p2" name="Verify fix works"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2"` - Working within parent task
- Auto-created `.0` planning task started
- Ready list shows only children of `a1b2`

#### Step 2: Close Planning Task and Start Child

```bash
$ aiki task close
```

```xml
<aiki_task cmd="close" status="ok" scope="a1b2">
  <closed>
    <task id="a1b2.0" name="Review all subtasks and start first batch"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="4">
      <task id="a1b2.1" priority="p2" name="Understand root cause"/>
      <task id="a1b2.2" priority="p2" name="Write failing test"/>
      <task id="a1b2.3" priority="p2" name="Implement fix"/>
      <task id="a1b2.4" priority="p2" name="Verify fix works"/>
    </list>
  </context>
</aiki_task>
```

```bash
$ aiki task start
```

```xml
<aiki_task cmd="start" status="ok" scope="a1b2">
  <started>
    <task id="a1b2.1" name="Understand root cause"/>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2.1" name="Understand root cause"/>
    </in_progress>
    <list ready="3">
      <task id="a1b2.2" priority="p2" name="Write failing test"/>
      <task id="a1b2.3" priority="p2" name="Implement fix"/>
      <task id="a1b2.4" priority="p2" name="Verify fix works"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2"` - Still working within parent
- Closed the `.0` planning task first
- Started first child from ready list
- Ready list shows remaining siblings

#### Step 3: Add Grandchildren to a Child

```bash
# While working on a1b2.2, I realize I need to break it down
# Add children to existing task a1b2.2
$ aiki task add "Set up test fixtures" --parent a1b2.2
$ aiki task add "Write test assertions" --parent a1b2.2
```

```xml
<aiki_task cmd="add" status="ok">
  <added>
    <task id="a1b2.2.2" name="Write test assertions"/>
  </added>
  <context>
    <in_progress>
      <task id="a1b2.1" name="Understand root cause"/>
    </in_progress>
    <list ready="3">
      <task id="a1b2.2" priority="p2" name="Write failing test"/>
      <task id="a1b2.3" priority="p2" name="Implement fix"/>
      <task id="a1b2.4" priority="p2" name="Verify fix works"/>
    </list>
  </context>
</aiki_task>
```

#### Step 4: Start Child That Has Children

```bash
$ aiki task start
```

```xml
<aiki_task cmd="start" status="ok" scope="a1b2.2">
  <started>
    <!-- Auto-created planning task for a1b2.2 -->
    <task id="a1b2.2.0" name="Review all subtasks and start first batch">
  Review the following subtasks and start working on related ones that can be addressed together:

**a1b2.2.1: Set up test fixtures** [p2]

**a1b2.2.2: Write test assertions** [p2]

Once you've reviewed, start the related tasks with: aiki task start &lt;id&gt; [id...]
</task>
  </started>
  
  <stopped reason="Started a1b2.2">
    <task id="a1b2.1" name="Understand root cause"/>
  </stopped>
  
  <context>
    <in_progress>
      <task id="a1b2.2.0" name="Review all subtasks and start first batch"/>
    </in_progress>
    <list ready="2">
      <task id="a1b2.2.1" priority="p2" name="Set up test fixtures"/>
      <task id="a1b2.2.2" priority="p2" name="Write test assertions"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2.2"` - **Now scoped to the child!**
- Ready list shows grandchildren (children of a1b2.2)
- Auto-stopped a1b2.1
- Created planning task a1b2.2.0

#### Step 5: Start Grandchild

```bash
$ aiki task start
```

```xml
<aiki_task cmd="start" status="ok" scope="a1b2.2">
  <started>
    <task id="a1b2.2.1" name="Set up test fixtures"/>
  </started>
  
  <stopped reason="Started a1b2.2.1">
    <task id="a1b2.2.0" name="Review all subtasks and start first batch"/>
  </stopped>
  
  <context>
    <in_progress>
      <task id="a1b2.2.1" name="Set up test fixtures"/>
    </in_progress>
    <list ready="1">
      <task id="a1b2.2.2" priority="p2" name="Write test assertions"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2.2"` - Still scoped to immediate parent
- Ready list shows sibling grandchildren
- Working on grandchild a1b2.2.1

#### Step 6: Close First Grandchild

```bash
$ aiki task close
```

```xml
<aiki_task cmd="close" status="ok" scope="a1b2.2">
  <closed>
    <task id="a1b2.2.1" name="Set up test fixtures"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="1">
      <task id="a1b2.2.2" priority="p2" name="Write test assertions"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2.2"` - Still in child context
- No in-progress task
- Ready list shows remaining grandchild

#### Step 7: Start Second Grandchild

```bash
$ aiki task start
```

```xml
<aiki_task cmd="start" status="ok" scope="a1b2.2">
  <started>
    <task id="a1b2.2.2" name="Write test assertions"/>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2.2.2" name="Write test assertions"/>
    </in_progress>
    <list ready="0"/>
  </context>
</aiki_task>
```

**Key points:**
- Starts the last remaining grandchild
- Now in-progress for work
- Empty ready list (last child started)

#### Step 8: Close Last Grandchild (Auto-Starts Parent)

```bash
$ aiki task close
```

```xml
<aiki_task cmd="close" status="ok" scope="a1b2.2">
  <closed>
    <task id="a1b2.2.2" name="Write test assertions"/>
  </closed>
  
  <started>
    <task id="a1b2.2" name="Write failing test"/>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2.2" name="Write failing test"/>
    </in_progress>
    <list ready="0"/>
  </context>
  
  <notice>All subtasks complete. Parent task (id: a1b2.2) auto-started for review/finalization.</notice>
</aiki_task>
```

**Key points:**
- `<notice>` explains parent was auto-started
- Parent task now in-progress for review/finalization work
- Empty child list (all grandchildren done)
- Still in `scope="a1b2.2"` until parent closes

#### Step 9: Close Child (Pops Back to Parent Scope)

```bash
$ aiki task close
```

```xml
<aiki_task cmd="close" status="ok" scope="a1b2">
  <closed>
    <task id="a1b2.2" name="Write failing test"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="2">
      <!-- Back to parent scope, showing siblings -->
      <task id="a1b2.3" priority="p2" name="Implement fix"/>
      <task id="a1b2.4" priority="p2" name="Verify fix works"/>
    </list>
  </context>
</aiki_task>
```

**Key points:**
- `scope="a1b2"` - **Popped back up to parent scope!**
- Ready list shows remaining children of a1b2
- All grandchildren are closed, so parent can close

#### Step 10: Close Remaining Children and Parent

```bash
# Close the stopped tasks by ID (they're not in-progress)
$ aiki task close a1b2.1
$ aiki task close a1b2.3
$ aiki task close a1b2.4
```

```xml
<aiki_task cmd="close" status="ok" scope="a1b2">
  <closed>
    <task id="a1b2.4" name="Verify fix works"/>
  </closed>
  
  <started>
    <task id="a1b2" name="Fix auth system"/>
  </started>
  
  <context>
    <in_progress>
      <task id="a1b2" name="Fix auth system"/>
    </in_progress>
    <list ready="0"/>
  </context>
  
  <notice>All subtasks complete. Parent task (id: a1b2) auto-started for review/finalization.</notice>
</aiki_task>
```

```bash
$ aiki task close
```

```xml
<aiki_task cmd="close" status="ok">
  <closed>
    <task id="a1b2" name="Fix auth system"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="5">
      <!-- Back to global queue, no scope -->
      <!-- Other unrelated tasks -->
    </list>
  </context>
</aiki_task>
```

**Key points:**
- No `scope` attribute - Back to global context
- Parent can only close when all children are closed
- Ready list shows global queue again

#### Scope Rules Summary

| State | Scope | Ready List Shows |
|-------|-------|------------------|
| Start parent `a1b2` | `scope="a1b2"` | Children: a1b2.1, a1b2.2, a1b2.3, a1b2.4 |
| Start child `a1b2.1` | `scope="a1b2"` | Siblings: a1b2.2, a1b2.3, a1b2.4 |
| Start child with children `a1b2.2` | `scope="a1b2.2"` | Children: a1b2.2.1, a1b2.2.2 |
| Start grandchild `a1b2.2.1` | `scope="a1b2.2"` | Siblings: a1b2.2.2 |
| Close grandchild | `scope="a1b2.2"` | Remaining siblings |
| Close child `a1b2.2` | `scope="a1b2"` | Pop up to parent's siblings |
| Close parent `a1b2` | No scope | Global queue |

**The pattern:** `scope` always points to the immediate parent of the tasks shown in the ready list.

---

### Parent Closing Behavior

**Rule:** Parent tasks can only close when all children are closed.

**When closing the last child:**
```xml
<aiki_task cmd="close" status="ok" scope="a1b2.2">
  <closed>
    <task id="a1b2.2.2" name="Write test assertions"/>
  </closed>
  
  <context>
    <in_progress/>
    <list ready="0"/>
  </context>
  
  <notice>All children of a1b2.2 are complete. Close parent with: aiki task close a1b2.2</notice>
</aiki_task>
```

**Benefits:**
- Explicit close gives a natural checkpoint for review
- Parent might have validation/integration work beyond children
- Notice reminds you to close parent when ready
- Prevents accidental premature parent closure

---

## Task-to-Change Linking

### How It Works

When a task is in-progress, all JJ changes created during that time automatically link to the task via the `[aiki]` metadata block.

### Implementation

Update `cli/src/flows/core/flow.yaml` to include `works_on` when embedding provenance:

```yaml
change.completed:
  - if: $event.write
    then:
      # Get all in-progress tasks (can be multiple)
      - let: in_progress_tasks = self.task_in_progress()
      
      # Build [aiki] block with task references
      - let: metadata = |
          [aiki]
          author=$event.agent
          author_type=agent
          session=$event.session_id
          {{- for task in in_progress_tasks }}
          task=$task.id
          {{- endfor }}
          [/aiki]
      
      # Embed in change description
      - jj: describe --message "$metadata"
```

### Required Function

Add to flow engine (`self.*` functions):

```rust
// Get all currently in-progress tasks
fn task_in_progress() -> Vec<Task> {
    // Returns all tasks with status=InProgress
    // Returns empty vec if no tasks are in progress
}
```

### Example Metadata

**Single task:**
```
[aiki]
author=claude-code
author_type=agent
session=claude-session-abc123
task=a1b2
[/aiki]
```

**Multiple tasks:**
```
[aiki]
author=claude-code
author_type=agent
session=claude-session-abc123
task=a1b2
task=c3d4
[/aiki]
```

### Querying Task History

```bash
# Find all changes that worked on a task
jj log -r 'description("task=a1b2")'

# See what tasks a change was working on
jj show qpvuntsm | grep "task="
```

**Benefits:**
- Automatic linking - no manual tracking needed
- Bidirectional - query tasks → changes or changes → tasks
- Uses existing provenance infrastructure
- No additional storage needed in task events

---

## Agent Adoption: Native Integration

### Why Agents Won't Use This Without Integration

**The Problem**: Even the best task system is useless if agents forget it exists.

- Agents (like me) have short attention spans across compaction
- If tasks aren't visible by default, we'll fall back to internal todos
- Manual "remember to check tasks" doesn't scale

**The Solution**: Make tasks impossible to ignore through flow integration.

### Context Injection Strategy

**Problem**: Claude compacts context when conversations get long, losing task awareness.

**Solution**: Multi-layered context injection using Aiki's flow system.

```
┌─────────────────────────────────────────────────────────────────┐
│  CONTEXT COMPACTION SURVIVAL STRATEGY                           │
└─────────────────────────────────────────────────────────────────┘

1. Session Start
   └─> session.started flow fires
       └─> Use `context` action to inject task overview
       └─> Prepended to first prompt

2. Every Prompt
   └─> prompt.submitted flow fires
       └─> Use `context` action to re-inject current task list
       └─> Survives compaction (tasks stored in JJ, re-read each time)

3. Every Response
   └─> response.received flow fires
       └─> Use `autoreply` action to show task reminders
```

**Key Insight from Beads:**
- Tasks stored on persistent branch (aiki/tasks) survive compaction
- Re-read from storage on every prompt = always current
- Context injection works in `prompt.submitted` AND `response.received`

**Available Context Injection Mechanisms:**

| Mechanism | Events | Use Case |
|-----------|--------|----------|
| `context` action | `prompt.submitted`, `response.received` | Inject/modify prompt context |
| `autoreply` action | `response.received` only | Add follow-up messages after agent response |

Both support `prepend` and `append` modes for flexible content placement.

### Core Flow Integration

Add to `cli/src/flows/bundled.yaml`:

```yaml
# ═══════════════════════════════════════════════════════════════
# session.started: Initial task context injection
# ═══════════════════════════════════════════════════════════════
session.started:
  # ... existing session.started logic ...
  
  # Inject task overview at session start
  - let: size = self.task_list_size
  - if: $size
    then:
      context:
         append: |
            ---

            📋 **Tasks** ($size ready)
            Run `aiki task` to view - OR - `aiki task start` to begin work.
    else:
      context:
         append: |
            ---

            📋 **Tasks** (None ready)
            Remember to use `aiki task add` to plan new work.
          

# ═══════════════════════════════════════════════════════════════
# prompt.submitted: Re-inject task context on EVERY prompt
# ═══════════════════════════════════════════════════════════════
prompt.submitted:
  # Re-inject current task list (survives context compaction)
# 
  - let: size = self.task_list_size
  - if: $size
    then:
      - context:
          append: |
            ---

            📋 **Tasks** ($size ready)
            Run `aiki task` to view - OR - `aiki task start` to begin work.
    else:
      context:
         append: |
            ---

            📋 **Tasks** (None ready)
            Remember to use `aiki task add` to plan new work.


# ═══════════════════════════════════════════════════════════════
# response.received: Auto-create tasks from errors + show reminders
# ═══════════════════════════════════════════════════════════════
response.received:
  # Show task reminder (autoreply works in response.received)
  - let: size = self.task_list_size
  - if: $size
    then:
      autoreply:
        append: |
            ---

            📋 **Tasks** ($size ready)
            Run `aiki task` to view - OR - `aiki task start` to begin work.
```

**How Context Injection Works:**

1. **`context` action** - Modifies the prompt before it reaches the agent
   - Works in: `session.started`, `prompt.submitted`, `response.received`
   - Can `prepend` (add before prompt) or `append` (add after prompt)
   - Perfect for injecting task awareness into every prompt

2. **`autoreply` action** - Adds follow-up messages after agent response
   - Works in: `response.received` only
   - Can `prepend` or `append` to the agent's response
   - Perfect for task reminders after agent completes work

**Multi-layered Defense Against Context Loss:**
- Session start: Initial awareness via `context` action
- Every prompt: Re-injection via `context` action (survives compaction)
- Every response: Reminder via `autoreply` action
- Result: Tasks are ALWAYS visible, impossible to forget

### Required Functions

Implement in flow engine (`self.*` functions):

```rust
// Get size of ready task list (open, unblocked, unclaimed)
fn task_list_size() -> usize

// Get all currently in-progress tasks
fn task_in_progress() -> Vec<Task>
```

### Why This Works

1. **Visibility** - Tasks shown on every prompt, impossible to ignore
2. **Automatic** - Errors become tasks without agent effort
3. **Low friction** - Agent sees tasks, runs `aiki task start`, gets work
4. **Persistent** - Stored on JJ branch, survives compaction
5. **Current** - Agent closes tasks when done, queue stays accurate

### Agent Experience

```
# Session starts
---
📋 **Tasks** (3 ready)
Run `aiki task` to view - OR - `aiki task start` to begin work.

# Agent submits prompt
---
📋 **Tasks** (3 ready)
Run `aiki task` to view - OR - `aiki task start` to begin work.

> Let me check what's next
> $ aiki task

# Agent gets XML with context
<aiki_task cmd="list" status="ok">
  <list ready="3">
    <task id="abc" priority="p0" name="Fix null check"/>
    ...
  </list>
  <context>...</context>
</aiki_task>

# Agent starts task
> $ aiki task start a1b2

# Gets full task details with body
<aiki_task cmd="start" status="ok">
  <started>
    <task id="abc" priority="p0" name="Fix null check">
  The authenticateUser function doesn't check if user...
</task>
  </started>
  <context>...</context>
</aiki_task>

# Agent fixes the issue and closes task
> $ aiki task close a1b2

# After agent's next response
---
📋 **Tasks** (2 ready)
Use `aiki task` to view - OR - `aiki task start` to begin work.
```

**This is how agents actually adopt the system** - not through documentation, but through constant, unavoidable visibility.

### AGENTS.md Integration

**Problem**: Flow-based context injection works during sessions, but agents need instructions in their system prompt about the task system itself.

**Solution**: Inject `<aiki>` block into `AGENTS.md` (or create it) during setup.

#### During `aiki init`

1. Check if `AGENTS.md` exists in repo root
2. If exists: Check for `<aiki>` block
   - If missing: Prepend `<aiki>` block to top of file
   - If present: Verify it's current (check version)
3. If doesn't exist: Create `AGENTS.md` with `<aiki>` block

#### During `aiki doctor`

1. Check if `AGENTS.md` exists
   - If missing: Warn and offer to create
2. Check if `<aiki>` block exists
   - If missing: Warn and offer to add
3. Check if `<aiki>` block is current version
   - If outdated: Warn and offer to update

#### The `<aiki>` Block Template

```markdown
<aiki version="1.0">
## Aiki Task System

You have access to an AI-first task management system. Tasks are:
- **Automatically created** from errors you encounter (type errors, test failures, etc.)
- **Automatically closed** when you fix the underlying issue
- **Always visible** via context injection (survives Claude's context compaction)
- **Stored persistently** on the `aiki/tasks` JJ branch

### Quick Reference

```bash
# See what's ready to work on (3-5 tasks with context)
aiki task

# Start working on a task (shows full details with body)
aiki task start <task-id>

# Start multiple related tasks for batch work
aiki task start <id1> <id2> <id3>

# Stop current task (with optional reason)
aiki task stop --reason "Blocked by API credentials"

# Close completed task
aiki task close <task-id>

# Add new task manually
aiki task add "Task name" --p0
```

### Task Output Format

All task commands return XML with this structure:

```xml
<aiki_task cmd="list" status="ok">
  <!-- What just happened -->
  <started>...</started>
  
  <!-- Current state -->
  <context>
    <in_progress>
      <task id="a1b2" name="Fix null check"/>
    </in_progress>
    <list ready="3">
      <task id="def" priority="p0" name="Fix missing return"/>
      <task id="ghi" priority="p1" name="Consider using const"/>
    </list>
  </context>
</aiki_task>
```

The `<context>` element shows:
- What you're currently working on (`<in_progress>`)
- What's ready to work on next (`<list>`)
- Enough context to make batching decisions

### Workflow Tips

1. **Check tasks regularly** - Run `aiki task` to see what's ready
2. **Batch related work** - Start multiple tasks together when they're related
3. **Use task bodies** - When you start a task, read its `<body>` for full context
4. **Stop when blocked** - Use `aiki task stop --reason` to explain blockers
5. **Close when done** - Use `aiki task close` when you complete a task

### Task Priorities

Priorities: `p0` (urgent) → `p1` (high) → `p2` (normal, default) → `p3` (low)

Tasks are automatically sorted by priority in the ready queue.
</aiki>
```

#### Implementation Details

**File operations**:
```rust
// cli/src/commands/init.rs
fn ensure_agents_md() -> Result<()> {
    let agents_path = PathBuf::from("AGENTS.md");
    
    if agents_path.exists() {
        // Read existing file
        let content = fs::read_to_string(&agents_path)?;
        
        // Check for <aiki> block
        if !content.contains("<aiki version=") {
            // Prepend block
            let updated = format!("{}\n\n{}", AIKI_BLOCK_TEMPLATE, content);
            fs::write(&agents_path, updated)?;
            println!("✓ Added <aiki> block to AGENTS.md");
        } else {
            // Verify version
            if !content.contains("<aiki version=\"1.0\">") {
                println!("⚠️  AGENTS.md has outdated <aiki> block");
                println!("   Run `aiki doctor --fix` to update");
            }
        }
    } else {
        // Create new AGENTS.md with just the block
        fs::write(&agents_path, AIKI_BLOCK_TEMPLATE)?;
        println!("✓ Created AGENTS.md with task system instructions");
    }
    
    Ok(())
}

// cli/src/commands/doctor.rs
fn check_agents_md() -> Vec<DoctorIssue> {
    let mut issues = Vec::new();
    let agents_path = PathBuf::from("AGENTS.md");
    
    if !agents_path.exists() {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "AGENTS.md not found".into(),
            fix: Some("Create with: aiki init --agents-md".into()),
        });
        return issues;
    }
    
    let content = fs::read_to_string(&agents_path).ok()?;
    
    if !content.contains("<aiki version=") {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "<aiki> block missing from AGENTS.md".into(),
            fix: Some("Add with: aiki doctor --fix".into()),
        });
    } else if !content.contains("<aiki version=\"1.0\">") {
        issues.push(DoctorIssue {
            severity: Severity::Info,
            message: "AGENTS.md <aiki> block is outdated".into(),
            fix: Some("Update with: aiki doctor --fix".into()),
        });
    }
    
    issues
}
```

**Why This Works:**

1. **System prompt awareness** - Agents read AGENTS.md and know the task system exists
2. **Permanent reference** - Unlike injected context, this survives all compaction
3. **Standardized location** - All agents look for AGENTS.md (Cursor, Claude Code, etc.)
4. **Version control** - `<aiki version="1.0">` allows graceful upgrades
5. **Non-invasive** - Prepends to existing content, doesn't overwrite
6. **Doctor integration** - Ensures the block stays current

**Combined Strategy:**
- **AGENTS.md** = Static reference (what the system is, how to use it)
- **Flow injection** = Dynamic state (what tasks exist right now, what's ready)
- **XML output** = Structured data (easy for agents to parse and act on)

Together, these ensure agents can't ignore the task system.

---

## Implementation Phases

### Phase 1 (Core Workflow - V1.0)

**Goal**: Minimal viable workflow - create, navigate, and complete tasks

**Core Commands**:
- `aiki task` - shortcut for `list` (default entry point)
- `aiki task list` - show ready queue (defaults to ready)
- `aiki task add <name>` - create single task
- `aiki task start` - start task (with auto-stop of current)
- `aiki task stop` - stop task (with optional `--reason` or `--blocked`)
- `aiki task close` - mark task done

**Core Features**:
- `aiki task` with no subcommand defaults to `list`
- XML output with `context` element on every command
- Smart defaults (only `name` required)
- Auto-stop on `start` for context switching
- Event storage on `aiki/tasks` branch

**Remove**:
- `aiki task create` → replaced by `add`
- `aiki task next` → replaced by `list` (defaults to ready)

**Deliverables**:
- [ ] Event storage on `aiki/tasks` branch
- [ ] Task engine (write/read events, materialize views)
- [ ] 5 core CLI commands with XML output
- [ ] Ready queue calculation
- [ ] Context element generation
- [ ] Auto-stop on context switch
- [ ] Context injection via flows (session start, prompt submitted, response received)
- [ ] AGENTS.md management in `aiki init` and `aiki doctor`
- [ ] Tests for core workflow

**Success Criteria**:
- Agent can create, start, stop, and close tasks
- Ready queue always shows what's next
- Context switching is seamless

---

### Phase 2 (Hierarchical Tasks - V1.1)

**Goal**: Parent-child task relationships

**Features**:
- `add --parent <id>` - create child tasks
- Hierarchical task ID generation (parent.child)
- Scope-based context (filter children when working on parent)

**Deliverables**:
- [ ] Hierarchical task ID generation
- [ ] `--parent` flag support
- [ ] Scope-based ready queue filtering
- [ ] Tests for hierarchical operations

---

### Phase 3 (Enhanced Features - V1.2)

**Goal**: Filtering and workflow enhancements

**Features**:
- Status flags: `--open`, `--in-progress`, `--stopped`, `--closed`
- Priority flags: `--p0`, `--p1`, `--p2`, `--p3`
- Multiple `--blocked` flags on `stop`

**Deliverables**:
- [ ] Status filtering on `list`
- [ ] Priority flags on `add`
- [ ] Multiple blocker task creation
- [ ] Tests for filtering

---

### Phase 4 (New Commands - V1.3)

**Goal**: Task inspection and modification

**Commands**:
- `aiki task show` - show task details (includes children for parents)
- `aiki task update` - modify task details
- `aiki task start --reopen --reason` - reopen closed tasks
- `aiki task comment` - add comments to tasks

**Remove**:
- `aiki task link`/`unlink` → replaced by `update`

**Deliverables**:
- [ ] `show` command with child aggregation
- [ ] `update` command for field modifications
- [ ] `--reopen` flag on `start`
- [ ] `comment` command for timestamped comments
- [ ] Tests for new commands

---

### Phase 5 (Optimizations - V1.4)

**Goal**: Performance and scalability

**Features**:
- Performance optimizations
- Event compaction (if needed)

**Deliverables**:
- [ ] Query optimization for ready queue
- [ ] Event compaction (if needed)
- [ ] Benchmarks

---

---

## Future Ideas

Additional enhancements are documented in separate files:

- [Task Types](../../future/tasks/task-types.md) - Semantic categorization (bug, feature, chore, spike)
- [Bulk Operations](../../future/tasks/bulk-operations.md) - Create multiple tasks via heredoc
- [Code Provenance](../../future/tasks/code-provenance.md) - Bidirectional linking between tasks and JJ changes
- [Smart Type Detection](../../future/tasks/smart-type-detection.md) - Auto-detect type from task name patterns
- [Humanized Tasks](../../future/tasks/humanized-tasks.md) - Human-friendly output, prefix matching, colors
- [Flow Actions](../../future/tasks/flow-actions.md) - Programmatic task creation/closing from flows
- [Show Related Tasks](../../future/tasks/show-related-tasks.md) - Discover tasks by shared context
- [Task Search](../../future/tasks/task-search.md) - Search tasks by name, body, or scope
- [Session Progress Summary](../../future/tasks/session-progress-summary.md) - Show session accomplishments

---

## Success Criteria

An AI agent should be able to:

1. **Capture thoughts quickly** - `add` tasks in seconds with smart defaults
2. **Navigate work easily** - see ready queue, current task, progress via context
3. **Switch context smoothly** - auto-stop on `start`, resume anytime
4. **Track provenance** - link tasks to code changes automatically
5. **Learn in one session** - 5 commands for full Phase 1 workflow

If these are met, tasks will become a natural part of the coding process rather than overhead.

---

## Summary

This unified plan combines the architectural design from `milestone-1.4` with the CLI optimizations from `tasks-cli-improvements`. The result is:

- **Minimal Phase 1** - Just 5 commands, XML-only, core workflow
- **Clear phases** - Each phase has focused goals and deliverables
- **AI-first** - Optimized for coding agents
- **Event-sourced** - Simple storage model, no database needed initially
- **Flow-integrated** - Context injection for task awareness

Next step: Implement Phase 1 (Core Workflow - V1.0)

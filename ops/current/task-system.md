# Aiki Task System: JJ-Native Design (Reconciled)

## The Two Approaches

### Approach A: YAML Files (Previous Design)
```
.aiki/tasks/
├── active/
│   ├── ts-auth-42.yaml
│   └── ts-session-15.yaml
└── closed/
    └── ts-config-88.yaml
```

**Pros**: Human-readable, editable, familiar
**Cons**: File conflicts, need merge driver, mixes task changes with code changes

### Approach B: JJ Changes (Beads/JJ Conversation)
```
Branch: aiki/tasks (orphan branch)
├── change qrst: Task { "Fix null in auth.ts:42" }
├── change uvwx: Task { "Missing import" }
└── change yzab: Task { "Add error handling" }
```

**Pros**: No file conflicts, JJ handles merging, task history separate from code
**Cons**: Less visible, requires JJ understanding

## Decision: JJ-Native (Approach B)

The JJ-native approach is superior because:

1. **No merge conflicts** - Each task is a separate change. Two agents creating tasks = two changes. JJ merges automatically.

2. **Provenance for free** - JJ's operation log tracks every task creation, update, closure. No custom audit trail needed.

3. **Stable references** - JJ change IDs survive rebases. Task ID = Change ID. References never break.

4. **Natural multi-agent** - JJ's DAG is designed for concurrent work. Task branch inherits this.

5. **Clean separation** - Code history (main) and task history (aiki/tasks) are independent.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Agent CLI Interface                          │
│                                                                  │
│   aiki task ready     aiki task create    aiki task close       │
│                                                                  │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                     TaskManager                                  │
│                                                                  │
│   - Abstracts JJ operations                                      │
│   - Manages aiki/tasks branch                                    │
│   - Maintains SQLite cache                                       │
│                                                                  │
└──────────────────────────┬───────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
┌─────────────────────────┐   ┌─────────────────────────────────┐
│   SQLite Cache          │   │   JJ Repository                 │
│   (.aiki/tasks.db)      │   │                                 │
│                         │   │   Branch: aiki/tasks            │
│   - Fast queries        │   │   ├── change xyz1: Task A       │
│   - Ready work          │   │   ├── change xyz2: Task B       │
│   - Indexed search      │   │   └── change xyz3: Task C       │
│                         │   │                                 │
│   Hydrates from ◄───────────┤   Source of truth               │
│   JJ on startup         │   │   Synced via operation log      │
│                         │   │                                 │
└─────────────────────────┘   └─────────────────────────────────┘
```

### Key Insight: SQLite is Cache, JJ is Truth

Like Beads' JSONL + SQLite architecture, but simpler:

- **JJ changes** = source of truth (replaces JSONL)
- **SQLite** = fast query cache (same role)
- **No daemon needed** = JJ operation log provides sync

---

## Task Storage: Changes, Not Files

### Task as JJ Change Description

Each task is a JJ change on the `aiki/tasks` branch. The change has:
- **No file changes** (empty tree or minimal marker)
- **Structured description** (YAML frontmatter + markdown body)

```
┌────────────────────────────────────────────────────────────────┐
│  JJ Change: qrstuvwx                                           │
│  Branch: aiki/tasks                                            │
│  Parent: previous-task-change (or root for first)              │
│                                                                │
│  Description:                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ ---                                                      │  │
│  │ aiki_task: v1                                            │  │
│  │ id: ts-a1b2c3d4                                          │  │
│  │ objective: "Fix: null not assignable to User"            │  │
│  │ type: error                                              │  │
│  │ priority: 0                                              │  │
│  │ status: open                                             │  │
│  │ scope:                                                   │  │
│  │   files:                                                 │  │
│  │     - path: src/auth.ts                                  │  │
│  │       lines: [42]                                        │  │
│  │ evidence:                                                │  │
│  │   - source: typescript                                   │  │
│  │     message: "Type 'null' is not assignable..."          │  │
│  │     code: TS2322                                         │  │
│  │ relations:                                               │  │
│  │   code_refs: [change-abc123]                             │  │
│  │ attempts: []                                             │  │
│  │ ---                                                      │  │
│  │                                                          │  │
│  │ ## Details                                               │  │
│  │                                                          │  │
│  │ The `getUser()` function returns `null` when user        │  │
│  │ not found, but return type is `User`.                    │  │
│  │                                                          │  │
│  │ ## Suggested Fix                                         │  │
│  │                                                          │  │
│  │ Either:                                                  │  │
│  │ 1. Change return type to `User | null`                   │  │
│  │ 2. Throw error when user not found                       │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                │
│  Files: (none - empty tree)                                    │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### Why No Files?

1. **No file conflicts** - Two agents create tasks simultaneously? Two different changes. JJ handles it.

2. **Atomic operations** - Task creation = single JJ operation. No partial states.

3. **History via operation log** - JJ tracks every change to the description. No custom versioning.

4. **Branch isolation** - Task branch has no overlap with code branch. Clean separation.

### The Marker File Option

If we want tasks visible in working copy (for debugging), use a single marker file:

```
.aiki/
└── tasks.marker    # Contains: "Tasks stored in aiki/tasks branch"
```

Or, for visibility, create one empty file per task:

```
# In aiki/tasks branch working tree:
.tasks/
├── ts-a1b2c3d4     # Empty file, metadata in change description
├── ts-e5f6g7h8     # Empty file
└── warn-i9j0k1l2   # Empty file
```

This makes tasks `ls`-able while keeping data in descriptions.

---

## Task Operations via JJ

### Creating a Task

```rust
impl TaskBranch {
    pub fn create_task(&self, task: Task) -> Result<TaskId> {
        // 1. Create new change on task branch (without switching working copy)
        let description = task.to_jj_description();
        
        // JJ can create changes on other branches without checkout
        let change_id = self.repo.run(&[
            "new",
            &format!("{}@", self.branch_name),  // Parent = tip of task branch
            "-m", &description,
            "--no-edit",  // Don't switch working copy
        ])?;
        
        // 2. Move branch pointer to include new change
        self.repo.run(&[
            "branch", "set", &self.branch_name,
            "-r", &change_id,
        ])?;
        
        // 3. Generate task ID from change ID
        let task_id = TaskId::from_change_id(&change_id);
        
        // 4. Update SQLite cache
        self.cache.insert(&task, &task_id)?;
        
        Ok(task_id)
    }
}
```

### Updating a Task

Task updates are **evolutions** of the change - same change ID, new description:

```rust
impl TaskBranch {
    pub fn update_task(&self, task_id: &TaskId, updates: TaskUpdate) -> Result<()> {
        let change_id = task_id.to_change_id();
        
        // 1. Get current description
        let current_desc = self.repo.run(&[
            "log", "-r", &change_id,
            "--no-graph", "-T", "description"
        ])?;
        
        // 2. Parse, update, serialize
        let current_task = Task::from_description(&current_desc)?;
        let updated_task = current_task.apply(updates);
        let new_desc = updated_task.to_jj_description();
        
        // 3. Amend the change (JJ's "describe" command)
        self.repo.run(&[
            "describe", "-r", &change_id,
            "-m", &new_desc,
        ])?;
        
        // 4. Update cache
        self.cache.update(&task_id, &updated_task)?;
        
        Ok(())
    }
}
```

### Querying Tasks

```rust
impl TaskBranch {
    pub fn query(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        // Fast path: use SQLite cache
        if self.cache.is_fresh()? {
            return self.cache.query(&filter);
        }
        
        // Slow path: rebuild from JJ
        let output = self.repo.run(&[
            "log",
            "-r", &format!("{}::{}@", "root()", self.branch_name),
            "--no-graph",
            "-T", r#"change_id ++ "\n---\n" ++ description ++ "\n===\n""#,
        ])?;
        
        let tasks: Vec<Task> = output
            .split("\n===\n")
            .filter_map(|chunk| {
                let parts: Vec<&str> = chunk.splitn(2, "\n---\n").collect();
                if parts.len() == 2 {
                    let change_id = parts[0].trim();
                    let description = parts[1];
                    Task::from_description(description)
                        .ok()
                        .map(|t| t.with_id(TaskId::from_change_id(change_id)))
                } else {
                    None
                }
            })
            .filter(|t| filter.matches(t))
            .collect();
        
        // Update cache
        self.cache.rebuild(&tasks)?;
        
        Ok(tasks)
    }
}
```

### Task History (Free from JJ)

```rust
impl TaskBranch {
    pub fn history(&self, task_id: &TaskId) -> Result<Vec<TaskEvent>> {
        let change_id = task_id.to_change_id();
        
        // JJ operation log tracks ALL changes to descriptions
        let output = self.repo.run(&[
            "op", "log",
            "--no-graph",
            "-T", r#"id ++ " " ++ time ++ " " ++ user ++ "\n""#,
        ])?;
        
        // Filter ops that affected this change
        // (In practice, would need JJ API to filter by change)
        let events = parse_operations(&output)
            .filter(|op| op.affects_change(&change_id))
            .map(|op| TaskEvent::from_op(&op, task_id))
            .collect();
        
        Ok(events)
    }
}
```

---

## Working Copy Isolation

### The Problem

Agent is working on code (main branch). Task operations must not disturb their working copy.

### Solution 1: JJ Workspace (Recommended)

Maintain a separate JJ workspace for task operations:

```
project/
├── .jj/
│   └── repo/                    # Shared repo storage
├── src/                         # Main working copy (code)
└── .aiki/
    └── task-workspace/          # Separate workspace for tasks
        └── .jj/
            └── working-copy/    # Points to aiki/tasks branch
```

```rust
impl TaskBranch {
    pub fn init(repo_path: &Path) -> Result<Self> {
        let workspace_path = repo_path.join(".aiki/task-workspace");
        
        if !workspace_path.exists() {
            // Create workspace pointing to task branch
            Command::new("jj")
                .args(["workspace", "add", workspace_path.to_str().unwrap()])
                .args(["--revision", "aiki/tasks"])
                .current_dir(repo_path)
                .output()?;
        }
        
        Ok(Self {
            repo_path: repo_path.to_path_buf(),
            workspace_path,
            cache: TaskCache::open(repo_path.join(".aiki/tasks.db"))?,
        })
    }
    
    fn run_in_workspace(&self, args: &[&str]) -> Result<String> {
        // Run JJ command in task workspace context
        let output = Command::new("jj")
            .args(args)
            .current_dir(&self.workspace_path)
            .output()?;
        
        Ok(String::from_utf8(output.stdout)?)
    }
}
```

### Solution 2: JJ's `--at-operation` / `--repository`

JJ can operate on repos without being in the working directory:

```rust
impl TaskBranch {
    fn run_detached(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("jj")
            .args(["--repository", self.repo_path.to_str().unwrap()])
            .args(args)
            .output()?;
        
        Ok(String::from_utf8(output.stdout)?)
    }
}
```

### Solution 3: Branch-Only Operations

Some JJ operations don't need working copy at all:

```bash
# Creating change on branch (no working copy needed)
jj new aiki/tasks@ -m "task description" --no-edit

# Updating description (no working copy needed)
jj describe -r <change_id> -m "new description"

# Querying (definitely no working copy needed)
jj log -r "aiki/tasks::" --no-graph -T description
```

**Recommendation**: Use Solution 3 where possible (most operations), fall back to Solution 1 (workspace) for complex operations.

---

## Linking Tasks to Code Changes

### Code Change Metadata

When agent makes code changes, link to tasks via change metadata:

```yaml
# JJ change description for code change (main branch)
---
aiki_change: v1
provenance:
  agent: claude-code
  session: session-12345
tasks:
  works_on: [ts-a1b2c3d4]       # Currently working on
  closes: []                     # Will close when committed
  discovered: [ts-e5f6g7h8]     # Found while working
---

feat(auth): add null check to getUser

Fixes the null return type issue by adding explicit check.
```

### Task Back-References

Tasks reference code changes that touched them:

```yaml
# Task change description (aiki/tasks branch)
---
aiki_task: v1
id: ts-a1b2c3d4
# ... other fields ...
code_refs:
  discovered_in: change-aaa111    # Code change that revealed this
  attempted_in:                   # Code changes that tried to fix
    - change-bbb222
    - change-ccc333
  fixed_in: change-ddd444         # Code change that actually fixed
---
```

### Bidirectional Link Manager

```rust
pub struct TaskCodeLinker {
    task_branch: TaskBranch,
    repo: JjRepo,
}

impl TaskCodeLinker {
    /// When code change attempts to fix a task
    pub fn record_attempt(
        &self,
        code_change: &ChangeId,
        task_id: &TaskId,
    ) -> Result<()> {
        // Update task with code reference
        self.task_branch.update_task(task_id, TaskUpdate {
            add_code_ref: Some(CodeRef::AttemptedIn(code_change.clone())),
            ..Default::default()
        })?;
        
        // Update code change with task reference
        self.update_code_metadata(code_change, |meta| {
            meta.tasks.works_on.push(task_id.clone());
        })?;
        
        Ok(())
    }
    
    /// When code change successfully fixes a task
    pub fn record_fix(
        &self,
        code_change: &ChangeId,
        task_id: &TaskId,
    ) -> Result<()> {
        // Close task
        self.task_branch.update_task(task_id, TaskUpdate {
            status: Some(TaskStatus::Closed),
            resolution: Some(Resolution {
                outcome: ResolutionOutcome::Fixed,
                commit: Some(code_change.clone()),
                closed_at: Utc::now(),
                closed_by: AgentId::current(),
                notes: None,
            }),
            add_code_ref: Some(CodeRef::FixedIn(code_change.clone())),
            ..Default::default()
        })?;
        
        // Update code change
        self.update_code_metadata(code_change, |meta| {
            meta.tasks.closes.push(task_id.clone());
        })?;
        
        Ok(())
    }
}
```

---

## Multi-Agent Coordination

### Why JJ Handles This Naturally

```
Agent A (Claude Code)              Agent B (Cursor)
        │                                  │
        ▼                                  ▼
   Create Task 1                     Create Task 2
        │                                  │
        ▼                                  ▼
   jj new aiki/tasks@              jj new aiki/tasks@
   -m "Task 1 desc"                -m "Task 2 desc"
        │                                  │
        ▼                                  ▼
   change-aaa111                     change-bbb222
        │                                  │
        └──────────┬───────────────────────┘
                   │
                   ▼
            JJ merges automatically
                   │
                   ▼
         aiki/tasks branch now has:
         ├── change-aaa111: Task 1
         └── change-bbb222: Task 2
         
         (No conflicts - different changes!)
```

### Concurrent Task Updates

If both agents update the **same** task:

```
Agent A                              Agent B
   │                                    │
   ▼                                    ▼
Update task-xyz                    Update task-xyz
status: in_progress               add attempt: failed
   │                                    │
   ▼                                    ▼
jj describe -r task-xyz           jj describe -r task-xyz
   │                                    │
   └─────────────┬──────────────────────┘
                 │
                 ▼
          JJ conflict!
          (Same change, different descriptions)
                 │
                 ▼
          Resolution options:
          1. Last-write-wins (configurable)
          2. Merge descriptions (structured merge)
          3. Create divergent changes (manual resolution)
```

### Conflict Resolution Strategy

```rust
impl TaskBranch {
    pub fn update_task_with_retry(&self, task_id: &TaskId, updates: TaskUpdate) -> Result<()> {
        loop {
            match self.update_task(task_id, updates.clone()) {
                Ok(()) => return Ok(()),
                Err(e) if e.is_conflict() => {
                    // Refresh and retry
                    let current = self.get_task_fresh(task_id)?;
                    
                    // Merge updates with current state
                    let merged_updates = updates.merge_with_current(&current);
                    
                    // Retry with merged updates
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

---

## SQLite Cache Schema

The cache mirrors JJ state for fast queries:

```sql
-- Task table (cached from JJ change descriptions)
CREATE TABLE tasks (
    change_id TEXT PRIMARY KEY,        -- JJ change ID (source of truth)
    task_id TEXT UNIQUE NOT NULL,      -- Human-readable ID (ts-a1b2c3d4)
    objective TEXT NOT NULL,
    type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    priority INTEGER NOT NULL DEFAULT 0,
    assignee TEXT,
    scope_json TEXT,                   -- JSON blob for files/lines
    evidence_json TEXT,                -- JSON blob for evidence list
    relations_json TEXT,               -- JSON blob for relations
    attempts_json TEXT,                -- JSON blob for attempt history
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    jj_op_id TEXT NOT NULL             -- Last JJ operation that synced this
);

CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_priority ON tasks(priority);
CREATE INDEX idx_tasks_assignee ON tasks(assignee);

-- File scope index for "tasks affecting file X"
CREATE TABLE task_files (
    task_id TEXT REFERENCES tasks(task_id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    PRIMARY KEY (task_id, file_path)
);
CREATE INDEX idx_task_files_path ON task_files(file_path);

-- Ready work view
CREATE VIEW ready_tasks AS
SELECT t.*
FROM tasks t
WHERE t.status = 'open'
  AND NOT EXISTS (
    SELECT 1 FROM json_each(t.relations_json, '$.blocked_by') AS blocker
    WHERE blocker.value IN (SELECT task_id FROM tasks WHERE status != 'closed')
  )
ORDER BY t.priority ASC, t.created_at ASC;

-- Cache freshness tracking
CREATE TABLE cache_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: last_jj_op_id, last_sync_time
```

### Cache Sync

```rust
impl TaskCache {
    /// Check if cache is fresh relative to JJ state
    pub fn is_fresh(&self) -> Result<bool> {
        let last_sync_op = self.get_meta("last_jj_op_id")?;
        let current_op = self.repo.current_operation_id()?;
        
        Ok(last_sync_op == Some(current_op))
    }
    
    /// Sync cache from JJ (incremental)
    pub fn sync(&self) -> Result<SyncReport> {
        let last_op = self.get_meta("last_jj_op_id")?;
        let current_op = self.repo.current_operation_id()?;
        
        if last_op == Some(current_op.clone()) {
            return Ok(SyncReport::unchanged());
        }
        
        // Get changes since last sync
        let changes = if let Some(ref last) = last_op {
            self.repo.changes_between(last, &current_op)?
        } else {
            self.repo.all_changes_on_branch("aiki/tasks")?
        };
        
        // Update cache
        let mut report = SyncReport::default();
        for change in changes {
            if let Ok(task) = Task::from_change(&change) {
                self.upsert(&task)?;
                report.updated += 1;
            }
        }
        
        // Update sync marker
        self.set_meta("last_jj_op_id", &current_op)?;
        
        Ok(report)
    }
}
```

---

## CLI Commands (Same Interface)

The CLI interface remains identical - agents don't know about JJ:

```bash
# Query ready work
$ aiki task ready --json
{
  "ready": [
    {
      "id": "ts-a1b2c3d4",
      "objective": "Fix: null not assignable to User",
      "scope": {"files": [{"path": "src/auth.ts", "lines": [42]}]},
      "priority": 0
    }
  ]
}

# Create task
$ aiki task create "Missing error handling" \
    --type error \
    --file src/payment.ts \
    --line 55 \
    --evidence "typescript:Property 'error' does not exist"
Created: ts-e5f6g7h8

# Start working
$ aiki task start ts-a1b2c3d4
Started: ts-a1b2c3d4 (assigned to claude-code)

# Close task
$ aiki task close ts-a1b2c3d4 --commit abc123
Closed: ts-a1b2c3d4 (fixed)
```

Internally, these translate to JJ operations:

```rust
// aiki task create "..." --file X --line Y
fn cmd_create(args: CreateArgs) -> Result<()> {
    let task = Task {
        id: generate_task_id(&args),
        objective: args.message,
        scope: TaskScope { 
            files: vec![FileScope { path: args.file, lines: vec![args.line] }]
        },
        // ... other fields
    };
    
    // Creates JJ change on aiki/tasks branch
    let task_id = task_branch.create_task(task)?;
    
    println!("Created: {}", task_id);
    Ok(())
}
```

---

## Flow Integration (Same Interface)

Flows use the same `task:` action - implementation changes, not syntax:

```yaml
PostToolUse:
  - let: errors = self.typescript_errors
  
  - for: error in $errors
    do:
      - task:
          create:
            objective: "Fix: $error.message"
            type: error
            file: $error.file
            line: $error.line
            evidence:
              - source: typescript
                message: $error.message
                code: $error.code
```

The `task:` action implementation now calls `TaskBranch::create_task()` which creates a JJ change instead of writing a YAML file.

---

## Comparison: Files vs JJ Changes

| Aspect | YAML Files | JJ Changes |
|--------|------------|------------|
| Storage | `.aiki/tasks/*.yaml` | `aiki/tasks` branch descriptions |
| Conflicts | File merge conflicts | JJ auto-merge (different changes) |
| History | Git/JJ tracks file changes | JJ operation log (native) |
| Multi-agent | Needs merge driver | Native concurrent creation |
| Visibility | `ls .aiki/tasks/` | `jj log -r aiki/tasks::` |
| Human editing | Edit YAML directly | `jj describe -r <change>` |
| Sync | File watcher | JJ operation log polling |
| Cache | SQLite from files | SQLite from JJ queries |
| Provenance | JJ tracks file commits | JJ tracks change evolution |

**Winner: JJ Changes** - More complex implementation, but eliminates entire categories of problems (conflicts, sync, merge drivers).

---

## Implementation Phases (Revised)

### Phase 1: Foundation (2 weeks)

- [ ] Task data model (Rust types)
- [ ] JJ change description serialization (YAML frontmatter)
- [ ] `TaskBranch` manager (create, query, update via JJ)
- [ ] Basic workspace isolation
- [ ] CLI: `create`, `list`, `show`

### Phase 2: Cache & Query (1 week)

- [ ] SQLite cache schema
- [ ] Cache sync from JJ operation log
- [ ] Ready work query
- [ ] CLI: `ready`, `start`, `close`

### Phase 3: Code Linking (1 week)

- [ ] Bidirectional task ↔ code references
- [ ] Code change metadata format
- [ ] `TaskCodeLinker` implementation
- [ ] Automatic link on task operations

### Phase 4: Flow Integration (1-2 weeks)

- [ ] `task:` action in flow DSL
- [ ] Auto-create from review findings
- [ ] Auto-close on fix detection
- [ ] Stuck detection via attempts

### Phase 5: Multi-Agent (1 week)

- [ ] Conflict resolution strategy
- [ ] Concurrent update handling
- [ ] Task claiming/assignment

### Phase 6: Polish (1 week)

- [ ] Error handling
- [ ] Performance optimization
- [ ] Documentation
- [ ] Example flows

**Total: 7-9 weeks**

---

## Summary

The JJ-native approach stores tasks as **changes on a separate branch**, not as files. This:

1. **Eliminates file conflicts** - Each task is a separate JJ change
2. **Provides history for free** - JJ operation log tracks everything
3. **Enables multi-agent** - JJ's DAG handles concurrent work
4. **Maintains clean separation** - Code branch and task branch are independent
5. **Preserves the CLI interface** - Agents see the same `aiki task` commands

The SQLite cache provides fast queries while JJ remains the source of truth. The flow integration and CLI remain identical - only the storage layer changes.

This is architecturally cleaner and more aligned with Aiki's JJ-first philosophy.

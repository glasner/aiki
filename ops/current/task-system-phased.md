# Aiki Task System: Phased Implementation for PostResponse

This document breaks down the task system implementation into phases, with **Phase 1 focused exclusively on what's needed for PostResponse event validation**.

---

## Table of Contents

1. [Phase 1: PostResponse Foundation (2-3 weeks)](#phase-1-postresponse-foundation-2-3-weeks) ← **START HERE**
2. [Phase 2: Performance & Scale (1 week)](#phase-2-performance--scale-1-week)
3. [Phase 3: Code Provenance (1 week)](#phase-3-code-provenance-1-week)
4. [Phase 4: Multi-Agent Coordination (1 week)](#phase-4-multi-agent-coordination-1-week)
5. [Phase 5: Enterprise Features (1-2 weeks)](#phase-5-enterprise-features-1-2-weeks)

---

## Phase 1: PostResponse Foundation (2-3 weeks)

**Goal**: Enable PostResponse flows to create, query, and close tasks. Agent gets structured work queue instead of text autoreplies.

### What We're Building

```yaml
# PostResponse flow creates tasks from errors
PostResponse:
  - let: ts_errors = self.count_typescript_errors
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
  "ready": [
    {
      "id": "ts-a1b2c3d4",
      "objective": "Fix: Type 'null' is not assignable to type 'User'",
      "scope": {"files": [{"path": "src/auth.ts", "lines": [42]}]},
      "evidence": [{"source": "typescript", "message": "...", "code": "TS2322"}],
      "attempts": 0
    }
  ]
}

$ aiki task start ts-a1b2c3d4
Started: ts-a1b2c3d4

# Agent makes changes...

# PostToolUse detects fix and auto-closes
```

### Core Architecture (Minimal)

```
┌─────────────────────────────────────────────────────────┐
│                  Agent CLI                              │
│  aiki task ready  |  aiki task create  |  aiki task close│
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│                  TaskManager                            │
│  - Manages aiki/tasks branch                            │
│  - Creates/updates JJ changes                           │
│  - NO SQLite cache yet (scan JJ directly)               │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│              JJ Repository                              │
│                                                         │
│  Branch: aiki/tasks (orphan branch)                     │
│  ├── change xyz1: Task { "Fix null in auth.ts" }       │
│  ├── change xyz2: Task { "Missing import" }            │
│  └── change xyz3: Task { "Add error handling" }        │
│                                                         │
│  Each task = JJ change with metadata in description     │
└─────────────────────────────────────────────────────────┘
```

### Task Data Model (Event-Sourced)

**Core principle**: Tasks are reconstructed from an immutable event log stored as JJ changes on the `aiki/tasks` branch.

**Event types:**

```yaml
# Event 1: Task Created
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: created
timestamp: 2025-01-15T09:00:00Z
agent: claude-code
task:
  objective: "Fix: null not assignable to User"
  type: error
  priority: 0
  scope:
    files:
      - path: src/auth.ts
        lines: [42]
  evidence:
    - source: typescript
      message: "Type 'null' is not assignable to type 'User'"
      code: TS2322
---

# Event 2: Task Started
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: started
timestamp: 2025-01-15T09:05:00Z
agent: claude-code
---

# Event 3: Task Failed
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: failed
timestamp: 2025-01-15T09:15:00Z
agent: claude-code
attempt: 1
---

# Event 4: Task Started (retry)
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: started
timestamp: 2025-01-15T09:20:00Z
agent: claude-code
---

# Event 5: Task Closed
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: closed
timestamp: 2025-01-15T09:30:00Z
agent: claude-code
fixed: true
---
```

**Event log on aiki/tasks branch:**
```
aiki/tasks branch (append-only):
  change-001  [created ts-001]
  change-002  [created ts-002]
  change-003  [started ts-001 agent=claude-code]
  change-004  [failed ts-001 attempt=1]
  change-005  [started ts-001 agent=claude-code]
  change-006  [closed ts-001 fixed=true]
  change-007  [created ts-003]
```

**Rust types:**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Event stored in JJ change description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub event: EventType,
    pub timestamp: DateTime<Utc>,
    pub agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventType {
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
}

// Task definition (embedded in Created event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub objective: String,
    pub r#type: TaskType,
    pub priority: u8,
    pub scope: TaskScope,
    pub evidence: Vec<Evidence>,
}

// Reconstructed task state (derived from events)
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub definition: TaskDefinition,
    pub status: TaskStatus,
    pub attempts: Vec<Attempt>,
    pub created_at: DateTime<Utc>,
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
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Attempt {
    pub agent: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: Option<AttemptOutcome>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Error,
    Warning,
    Suggestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Open,
    InProgress,
    Closed,
}

#[derive(Debug, Clone, Copy)]
pub enum AttemptOutcome {
    Fixed,
    Failed,
    Abandoned,
}
```

### JJ Operations (Event-Sourced)

**Core operations append events to the aiki/tasks branch:**

```rust
impl TaskManager {
    pub fn new(repo_path: impl AsRef<Path>) -> Result<Self> {
        let repo_path = repo_path.as_ref().to_path_buf();
        let manager = Self { repo_path };
        manager.ensure_task_branch_exists()?;
        Ok(manager)
    }
    
    fn ensure_task_branch_exists(&self) -> Result<()> {
        // Check if aiki/tasks branch exists
        let output = Command::new("jj")
            .args(["log", "-r", "aiki/tasks", "-T", "change_id"])
            .current_dir(&self.repo_path)
            .output()?;
        
        if !output.status.success() {
            // Branch doesn't exist - create orphan root
            Command::new("jj")
                .args(["new", "root()", "-m", "aiki/tasks: event log root"])
                .current_dir(&self.repo_path)
                .output()?;
            
            Command::new("jj")
                .args(["branch", "create", "aiki/tasks", "-r", "@"])
                .current_dir(&self.repo_path)
                .output()?;
            
            // Return to working copy
            Command::new("jj")
                .args(["edit", "@-"])  // Go back to where we were
                .current_dir(&self.repo_path)
                .output()?;
        }
        
        Ok(())
    }

    // Append event to the log
    fn append_event(&self, event: TaskEvent) -> Result<()> {
        let description = format!(
            "---\n{}\n---",
            serde_yaml::to_string(&event)?
        );
        
        // Create new change on aiki/tasks branch
        let output = Command::new("jj")
            .args([
                "new",
                "aiki/tasks@",           // Parent = tip of event log
                "-m", &description,
                "--no-edit",             // Don't switch working copy
            ])
            .current_dir(&self.repo_path)
            .output()?;
        
        if !output.status.success() {
            return Err(AikiError::JjCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }
        
        // Move branch pointer to new change
        Command::new("jj")
            .args(["branch", "set", "aiki/tasks", "-r", "@"])
            .current_dir(&self.repo_path)
            .output()?;
        
        Ok(())
    }
    
    pub fn create_task(&self, definition: TaskDefinition, agent: &str) -> Result<String> {
        let task_id = self.generate_task_id(&definition);
        
        let event = TaskEvent {
            task_id: task_id.clone(),
            event: EventType::Created { task: definition },
            timestamp: Utc::now(),
            agent: agent.to_string(),
        };
        
        self.append_event(event)?;
        Ok(task_id)
    }
    
    pub fn start_task(&self, task_id: &str, agent: &str) -> Result<()> {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event: EventType::Started,
            timestamp: Utc::now(),
            agent: agent.to_string(),
        };
        
        self.append_event(event)
    }
    
    pub fn fail_task(&self, task_id: &str, agent: &str, attempt: u32) -> Result<()> {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event: EventType::Failed { attempt },
            timestamp: Utc::now(),
            agent: agent.to_string(),
        };
        
        self.append_event(event)
    }
    
    pub fn close_task(&self, task_id: &str, agent: &str, fixed: bool) -> Result<()> {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event: EventType::Closed { fixed },
            timestamp: Utc::now(),
            agent: agent.to_string(),
        };
        
        self.append_event(event)
    }
    
    fn generate_task_id(&self, definition: &TaskDefinition) -> String {
        // Content-addressed ID for deduplication
        let message_hash = definition.evidence.first()
            .map(|e| &blake3::hash(e.message.as_bytes()).to_hex()[..8])
            .unwrap_or("");
        
        let content = format!(
            "{}:{}:{}:{}:{}",
            definition.r#type.short_prefix(),
            definition.scope.files.first().map(|f| f.path.display()).unwrap_or(""),
            definition.scope.files.first().and_then(|f| f.lines.first()).unwrap_or(&0),
            definition.evidence.first().and_then(|e| e.code.as_ref()).unwrap_or(""),
            message_hash
        );
        
        let hash = blake3::hash(content.as_bytes());
        format!("{}-{}", definition.r#type.short_prefix(), &hash.to_hex()[..8])
    }
}

impl TaskType {
    fn short_prefix(&self) -> &str {
        match self {
            TaskType::Error => "err",
            TaskType::Warning => "warn",
            TaskType::Suggestion => "sugg",
        }
    }
}
```

**Query Tasks (Reconstruct from Events):**

```rust
impl TaskManager {
    pub fn query_ready(&self) -> Result<Vec<Task>> {
        // Get all events from aiki/tasks branch
        let events = self.get_all_events()?;
        
        // Group events by task_id
        let mut tasks_by_id: HashMap<String, Vec<TaskEvent>> = HashMap::new();
        for event in events {
            tasks_by_id.entry(event.task_id.clone())
                .or_insert_with(Vec::new)
                .push(event);
        }
        
        // Reconstruct each task and filter for open tasks
        let tasks: Vec<Task> = tasks_by_id
            .into_iter()
            .filter_map(|(_, events)| self.reconstruct_task(&events).ok())
            .filter(|t| t.status == TaskStatus::Open)
            .collect();
        
        Ok(tasks)
    }
    
    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        let events = self.get_task_events(task_id)?;
        self.reconstruct_task(&events)
    }
    
    fn get_all_events(&self) -> Result<Vec<TaskEvent>> {
        let output = Command::new("jj")
            .args([
                "log",
                "-r", "aiki/tasks::",
                "--no-graph",
                "--reversed",  // Chronological order
                "-T", r#"description ++ "\n===EVENT_SEPARATOR===\n""#,
            ])
            .current_dir(&self.repo_path)
            .output()?;
        
        let stdout = String::from_utf8(output.stdout)?;
        
        stdout
            .split("\n===EVENT_SEPARATOR===\n")
            .filter_map(|desc| self.parse_event(desc).ok())
            .collect()
    }
    
    fn get_task_events(&self, task_id: &str) -> Result<Vec<TaskEvent>> {
        let output = Command::new("jj")
            .args([
                "log",
                "-r", &format!("aiki/tasks:: & description('task_id: {}')", task_id),
                "--no-graph",
                "--reversed",
                "-T", r#"description ++ "\n===EVENT_SEPARATOR===\n""#,
            ])
            .current_dir(&self.repo_path)
            .output()?;
        
        let stdout = String::from_utf8(output.stdout)?;
        
        stdout
            .split("\n===EVENT_SEPARATOR===\n")
            .filter_map(|desc| self.parse_event(desc).ok())
            .collect()
    }
    
    fn parse_event(&self, description: &str) -> Result<TaskEvent> {
        let content = description.strip_prefix("---\n")
            .ok_or(AikiError::InvalidTaskFormat("Missing opening ---"))?;
        
        let (yaml, _) = content.split_once("\n---")
            .ok_or(AikiError::InvalidTaskFormat("Missing closing ---"))?;
        
        serde_yaml::from_str(yaml)
            .map_err(|e| AikiError::InvalidTaskFormat(format!("YAML parse error: {}", e)))
    }
    
    fn reconstruct_task(&self, events: &[TaskEvent]) -> Result<Task> {
        let mut definition: Option<TaskDefinition> = None;
        let mut status = TaskStatus::Open;
        let mut attempts = Vec::new();
        let mut created_at = None;
        
        for event in events {
            if created_at.is_none() {
                created_at = Some(event.timestamp);
            }
            
            match &event.event {
                EventType::Created { task } => {
                    // If multiple agents create the same task concurrently,
                    // multiple Created events will exist. Take the first one.
                    if definition.is_none() {
                        definition = Some(task.clone());
                    }
                }
                EventType::Started => {
                    status = TaskStatus::InProgress;
                    attempts.push(Attempt {
                        agent: event.agent.clone(),
                        started_at: event.timestamp,
                        ended_at: None,
                        outcome: None,
                    });
                }
                EventType::Failed { .. } => {
                    if let Some(attempt) = attempts.last_mut() {
                        attempt.ended_at = Some(event.timestamp);
                        attempt.outcome = Some(AttemptOutcome::Failed);
                    }
                    status = TaskStatus::Open;
                }
                EventType::Closed { fixed } => {
                    if let Some(attempt) = attempts.last_mut() {
                        attempt.ended_at = Some(event.timestamp);
                        attempt.outcome = Some(if *fixed {
                            AttemptOutcome::Fixed
                        } else {
                            AttemptOutcome::Abandoned
                        });
                    }
                    status = TaskStatus::Closed;
                }
            }
        }
        
        let definition = definition.ok_or(AikiError::TaskNotFound)?;
        let created_at = created_at.ok_or(AikiError::TaskNotFound)?;
        
        Ok(Task {
            id: events.first().unwrap().task_id.clone(),
            definition,
            status,
            attempts,
            created_at,
        })
    }
}
```

**Query Task History:**

```rust
impl TaskManager {
    // Get full event history for debugging/auditing
    pub fn get_task_history(&self, task_id: &str) -> Result<Vec<TaskEvent>> {
        self.get_task_events(task_id)
    }
    
    // Check if task is stuck (3+ failed attempts)
    pub fn is_task_stuck(&self, task_id: &str) -> Result<bool> {
        let task = self.get_task(task_id)?;
        let failed_count = task.attempts.iter()
            .filter(|a| matches!(a.outcome, Some(AttemptOutcome::Failed)))
            .count();
        Ok(failed_count >= 3)
    }
}
```

**Event Log Example:**

```bash
# View all events for a task
$ jj log -r "aiki/tasks:: & description('task_id: ts-abc123')" --reversed

# Event 1: created
○  Change: abcd1234
│  Timestamp: 2025-01-15 10:00:00
│  Event: created

# Event 2: started
○  Change: efgh5678
│  Timestamp: 2025-01-15 10:05:00
│  Event: started (agent: claude-code)

# Event 3: failed
○  Change: ijkl9012
│  Timestamp: 2025-01-15 10:10:00
│  Event: failed (attempt: 1)

# Event 4: closed
○  Change: mnop3456
│  Timestamp: 2025-01-15 10:15:00
│  Event: closed (fixed: true)
```

### CLI Commands (Phase 1)

```bash
# Query ready work
aiki task ready [--json]

# Create task
aiki task create <objective> \
    --type <error|warning|suggestion> \
    --file <path> \
    --line <number> \
    --evidence <source:message:code>

# Start task (claim it, mark in-progress)
aiki task start <task-id> [--agent <name>]

# Fail task (record failed attempt)
aiki task fail <task-id> [--agent <name>]

# Close task
aiki task close <task-id> [--fixed | --abandoned] [--agent <name>]

# Show task details
aiki task show <task-id>

# Show task event history
aiki task history <task-id>
```

**Implementation:**

```rust
// cli/src/commands/task.rs

use crate::error::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct TaskCommand {
    #[command(subcommand)]
    command: TaskSubcommand,
}

#[derive(Subcommand)]
enum TaskSubcommand {
    /// List ready tasks
    Ready {
        #[arg(long)]
        json: bool,
    },
    
    /// Create a new task
    Create {
        objective: String,
        #[arg(long)]
        r#type: String,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long)]
        evidence: Option<String>,
    },
    
    /// Start working on a task
    Start {
        task_id: String,
    },
    
    /// Close a task
    Close {
        task_id: String,
        #[arg(long)]
        fixed: bool,
        #[arg(long)]
        abandoned: bool,
    },
    
    /// Show task details
    Show {
        task_id: String,
    },
}

pub fn run(cmd: TaskCommand) -> Result<()> {
    let manager = TaskManager::new(std::env::current_dir()?)?;
    
    match cmd.command {
        TaskSubcommand::Ready { json } => {
            let tasks = manager.query_ready()?;
            
            if json {
                println!("{}", serde_json::to_string_pretty(&tasks)?);
            } else {
                for task in tasks {
                    println!("{}: {}", task.id, task.objective);
                }
            }
        }
        
        TaskSubcommand::Create { objective, r#type, file, line, evidence } => {
            let task = Task {
                id: String::new(),  // Generated by manager
                objective,
                task_type: parse_task_type(&r#type)?,
                status: TaskStatus::Open,
                priority: 0,
                scope: TaskScope {
                    files: file.map(|f| vec![FileScope { path: f, lines: line.into_iter().collect() }])
                        .unwrap_or_default(),
                },
                evidence: parse_evidence(&evidence)?,
                attempts: vec![],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            
            let task_id = manager.create_task(task)?;
            println!("Created: {}", task_id);
        }
        
        TaskSubcommand::Start { task_id } => {
            let agent = std::env::var("AIKI_AGENT").unwrap_or_else(|_| "unknown".to_string());
            manager.start_task(&task_id, &agent)?;
            println!("Started: {}", task_id);
        }
        
        TaskSubcommand::Close { task_id, fixed, abandoned } => {
            let outcome = if fixed {
                AttemptOutcome::Fixed
            } else if abandoned {
                AttemptOutcome::Abandoned
            } else {
                AttemptOutcome::Failed
            };
            
            manager.close_task(&task_id, outcome)?;
            println!("Closed: {}", task_id);
        }
        
        TaskSubcommand::Show { task_id } => {
            let task = manager.get_task(&task_id)?;
            println!("{}", serde_yaml::to_string(&task)?);
        }
    }
    
    Ok(())
}
```

### Flow Integration (Phase 1)

**New flow context variables:**
- `self.ready_tasks` → `Vec<Task>` via `TaskManager::query_ready()`
- `self.current_task` → `Option<Task>` via current in_progress task lookup
- `self.errors_for_scope(scope)` → helper to check if task scope is clean

```yaml
# PostResponse: Create tasks from errors
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
        There are $ready_count tasks. Run `aiki task ready --json` to see details.

# PostToolUse: Auto-close fixed tasks
PostToolUse:
  - let: current_task = self.current_task
  - if: $current_task != null
    then:
      - let: errors = self.errors_for_scope($current_task.scope)
      - if: $errors | length == 0
        then:
          task.close:
            id: $current_task.id
            fixed: true
        else:
          # Record failed attempt (for stuck detection)
          task.fail:
            id: $current_task.id
```

**Flow action implementation:**

```rust
// cli/src/flows/actions/task.rs

impl FlowAction for TaskAction {
    fn execute(&self, context: &ExecutionContext) -> Result<ActionResult> {
        let manager = TaskManager::new(&context.cwd)?;
        let agent = &context.agent;  // From ACP notification or hook payload
        
        match &self.operation {
            TaskOperation::Create { objective, r#type, file, line, evidence } => {
                let definition = TaskDefinition {
                    objective: objective.clone(),
                    r#type: parse_task_type(r#type)?,
                    priority: 0,
                    scope: TaskScope {
                        files: vec![FileScope {
                            path: file.clone(),
                            lines: vec![*line],
                        }],
                    },
                    evidence: vec![Evidence {
                        source: evidence.source.clone(),
                        message: evidence.message.clone(),
                        code: evidence.code.clone(),
                    }],
                };
                
                let task_id = manager.create_task(definition, agent)?;
                Ok(ActionResult::success().with_output(format!("Created: {}", task_id)))
            }
            
            TaskOperation::Start { id } => {
                manager.start_task(id, agent)?;
                Ok(ActionResult::success())
            }
            
            TaskOperation::Fail { id } => {
                // Get current attempt number
                let task = manager.get_task(id)?;
                let attempt_num = task.attempts.len() as u32;
                manager.fail_task(id, agent, attempt_num)?;
                Ok(ActionResult::success())
            }
            
            TaskOperation::Close { id, fixed } => {
                manager.close_task(id, agent, *fixed)?;
                Ok(ActionResult::success())
            }
        }
    }
}
```

### Stuck Detection (Phase 1)

Simple attempt-based stuck detection:

```rust
impl TaskManager {
    pub fn is_stuck(&self, task_id: &str) -> Result<bool> {
        let task = self.get_task(task_id)?;
        
        // Stuck = 3+ failed attempts
        let failed_count = task.attempts.iter()
            .filter(|a| matches!(a.outcome, Some(AttemptOutcome::Failed)))
            .count();
        
        Ok(failed_count >= 3)
    }
}
```

```yaml
# Flow checks stuck state
PostToolUse:
  - let: current_task = self.current_task
  - if: $current_task.is_stuck
    then:
      autoreply.prepend: |
        ⚠️ This task has failed 3 times. Consider:
        - Reverting changes: jj undo
        - Asking for help
        - Breaking into smaller tasks
```

### Testing Strategy (Phase 1)

**Unit tests:**
- Task ID generation (content-addressed, deterministic)
- Event serialization/deserialization (YAML frontmatter)
- Task state reconstruction from events
- JJ command construction

**Integration tests:**
- Create task → verify event appended to aiki/tasks
- Start task → verify started event created
- Fail task → verify failed event created
- Close task → verify closed event created
- Query tasks → verify state reconstruction and filtering
- Task history → verify all events returned in order

**E2E tests:**
- Flow creates tasks from TypeScript errors → events appear in JJ
- Agent queries tasks → reconstructed state is correct
- Agent starts task → in_progress status reconstructed
- PostToolUse auto-closes fixed task → closed event created
- Multiple attempts → attempt count accurate in reconstructed state

### What We're NOT Building (Phase 1)

❌ **SQLite cache** - Scan JJ directly (fast enough for <100 tasks)  
❌ **JJ workspace isolation** - Use `--repository` flag for now  
❌ **Task relationships** - No blocking, parent/child, epics  
❌ **Code provenance links** - Tasks don't reference code changes yet  
❌ **Multi-agent conflict resolution** - Single agent only  
❌ **Priority queues** - Simple FIFO ordering  
❌ **Task history queries** - Basic JJ log is enough

### Phase 1 Implementation Notes

**Agent Identity (Multiple Integration Paths)**

Agent identity is determined based on how Aiki is invoked:

**1. ACP Server Mode (Claude Code, Cursor with ACP)**

The flow engine receives agent identity from ACP notification payloads:

```json
{
  "method": "session/update",
  "params": {
    "agent": "claude-code",
    "session_id": "abc123",
    ...
  }
}
```

```rust
impl FlowEngine {
    fn execute_post_response(&self, notification: &AcpNotification) -> Result<()> {
        let agent = &notification.params.agent;  // From ACP payload
        
        // Available to flow actions via context
        context.agent = agent.clone();
        
        // Passed automatically to task operations
        task_manager.create_task(task, agent)?;
    }
}
```

**2. Hook-Based Integrations (JJ hooks, Git hooks)**

Hook payloads also include agent identity (similar to ACP):

```rust
impl FlowEngine {
    fn execute_hook_flow(&self, hook_payload: &HookPayload) -> Result<()> {
        let agent = &hook_payload.agent;  // From hook payload
        
        // Available to flow actions via context
        context.agent = agent.clone();
        
        // Passed automatically to task operations
        task_manager.create_task(task, agent)?;
    }
}
```

**How this works:**
- Agent embeds metadata in the change: `[aiki] agent=claude-code`
- JJ hook fires on `jj describe` or `jj new`
- Hook invokes `aiki` with agent passed in the payload/environment
- Flow engine uses the agent from the hook invocation

**3. CLI Operations (Manual)**

- Manual task operations use `--agent` flag: `aiki task start ts-abc123 --agent myname`
- Defaults to `"cli-user"` if not specified

**Summary:**
- **ACP mode**: Agent from `notification.params.agent`
- **Hook mode**: Agent from `hook_payload.agent`
- **CLI mode**: Agent from `--agent` flag or default

In both ACP and hook modes, the agent is **passed to the flow engine**, not read from disk. This ensures correctness with concurrent agents.

**Concurrent Task Creation (Known Limitation)**

If two PostResponse flows run simultaneously (e.g., rapid successive agent changes), both might create tasks for the same error before the content-addressed ID check completes. This results in acceptable duplicate tasks.

**Why this is acceptable in Phase 1:**
- JJ handles concurrent writes gracefully (creates separate changes)
- Duplicates are obvious (same file/line in task list)
- Manual cleanup is trivial: `jj abandon <duplicate-task-change-id>`
- This happens rarely in single-agent workflows (the Phase 1 target)

**Phase 4 will address this** with proper locking or transaction-based deduplication.  

### Phase 1 Success Criteria

✅ PostResponse creates tasks from validation errors  
✅ Agent queries tasks via `aiki task ready --json`  
✅ Agent starts/closes tasks  
✅ PostToolUse auto-closes fixed tasks  
✅ Stuck detection works (3+ failed attempts)  
✅ Content-addressed IDs prevent duplicates  
✅ Tasks persist across sessions (JJ changes)  
✅ All operations work without disturbing working copy  

### Phase 1 Deliverables

1. **Core library** (`cli/src/tasks/`)
   - `manager.rs` - TaskManager with JJ operations
   - `types.rs` - Task, TaskScope, Evidence, etc.
   - `cli.rs` - CLI command handlers

2. **CLI commands** (`aiki task ...`)
   - `ready`, `create`, `start`, `close`, `show`

3. **Flow actions** (`task:` in YAML)
   - `create`, `close`, `fail`

4. **Tests**
   - Unit tests for TaskManager
   - Integration tests with real JJ repo
   - E2E test with TypeScript error flow

5. **Documentation**
   - Tutorial: "Using Tasks in PostResponse Flows"
   - CLI reference
   - Flow DSL reference for `task:` action

---

## Phase 2: Performance & Scale (1 week)

**When to build**: Event reconstruction is slow (>1s query time) or you have >1000 events

### SQLite Cache (Materialized Task State)

**Core principle**: Event log in JJ is source of truth. SQLite materializes current task state for fast queries.

```sql
-- Materialized current state of each task
CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY,
    objective TEXT NOT NULL,
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL,
    scope_json TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Cached attempt summary
    attempt_count INTEGER DEFAULT 0,
    failed_attempt_count INTEGER DEFAULT 0,
    last_agent TEXT
);

-- Track sync position in event log
CREATE TABLE sync_state (
    key TEXT PRIMARY KEY,
    last_event_change_id TEXT NOT NULL
);

CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_priority ON tasks(priority, created_at);

CREATE VIEW ready_tasks AS
SELECT * FROM tasks
WHERE status = 'open'
ORDER BY priority ASC, created_at ASC;
```

### Cache Sync (Incremental Event Replay)

```rust
impl TaskCache {
    pub fn sync(&self) -> Result<()> {
        // Get last synced event
        let last_event_id = self.get_last_synced_event()?;
        
        // Fetch new events since last sync
        let new_events = self.manager.get_events_since(last_event_id)?;
        
        if new_events.is_empty() {
            return Ok(());  // Cache is fresh
        }
        
        // Apply events to cache
        for event in &new_events {
            self.apply_event(event)?;
        }
        
        // Update sync position
        self.set_last_synced_event(new_events.last().unwrap().change_id)?;
        
        Ok(())
    }
    
    fn apply_event(&self, event: &TaskEvent) -> Result<()> {
        match &event.event {
            EventType::Created { task } => {
                self.insert_task(&event.task_id, task, event.timestamp)?;
            }
            EventType::Started => {
                self.update_status(&event.task_id, "in_progress")?;
                self.increment_attempts(&event.task_id)?;
                self.set_last_agent(&event.task_id, &event.agent)?;
            }
            EventType::Failed { .. } => {
                self.update_status(&event.task_id, "open")?;
                self.increment_failed_attempts(&event.task_id)?;
            }
            EventType::Closed { .. } => {
                self.update_status(&event.task_id, "closed")?;
            }
        }
        Ok(())
    }
}
```

**Performance characteristics:**

- **Phase 1 (no cache)**: O(events) to reconstruct all tasks
  - 100 tasks × 3 events avg = 300 events → ~50ms query time
- **Phase 2 (SQLite cache)**: O(new events) to sync, O(1) to query
  - Incremental sync: only replay events since last check
  - Query: SQL index lookup → <5ms for 10,000 tasks

**Cache invalidation:**

Cache is automatically fresh because sync happens on every query. Alternative: sync in background every 1s.

### Phase 2 Deliverables

- SQLite cache schema with event replay
- Incremental sync on query
- Fast queries via SQLite (<10ms)
- Benchmark: <100ms for 10,000 tasks
- Cache rebuild command: `aiki task cache rebuild`

---

## Phase 3: Code Provenance (1 week)

**When to build**: Need to track which code changes attempted/fixed tasks

### Bidirectional Links (Event-Based)

**Task events already contain agent info**, now add code change references:

```yaml
# Event: Task Started (with code change reference)
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: started
timestamp: 2025-01-15T10:05:00Z
agent: claude-code
code_change: change-abc123  # NEW: Link to code change that started work
---

# Event: Task Closed (with code change that fixed it)
---
aiki_task_event: v1
task_id: ts-a1b2c3d4
event: closed
timestamp: 2025-01-15T10:30:00Z
agent: claude-code
fixed: true
code_change: change-def456  # NEW: Link to code change that fixed task
---
```

**Code change references tasks (same as before):**

```yaml
# JJ change description on main branch
---
aiki_change: v1
tasks:
  works_on: [ts-a1b2]  # Started work on these
  closes: [ts-c3d4]    # Fixed these
---

feat(auth): add null check
```

### Provenance Queries

```rust
impl TaskManager {
    // Get all code changes that attempted this task
    pub fn get_task_code_history(&self, task_id: &str) -> Result<Vec<CodeRef>> {
        let events = self.get_task_events(task_id)?;
        
        events.iter()
            .filter_map(|e| match &e.event {
                EventType::Started | EventType::Closed { .. } => {
                    e.code_change.as_ref().map(|c| CodeRef {
                        change_id: c.clone(),
                        event_type: e.event.clone(),
                        timestamp: e.timestamp,
                    })
                }
                _ => None
            })
            .collect()
    }
}

impl ProvenanceManager {
    // Get all tasks related to a code change
    pub fn get_change_tasks(&self, change_id: &str) -> Result<Vec<String>> {
        // 1. Read tasks from [aiki_change] block
        let provenance = ProvenanceRecord::from_change(change_id)?;
        let mut task_ids = provenance.tasks_worked_on;
        task_ids.extend(provenance.tasks_closed);
        
        // 2. Also check event log for any events referencing this change
        let events = self.task_manager.get_events_for_code_change(change_id)?;
        task_ids.extend(events.iter().map(|e| e.task_id.clone()));
        
        Ok(task_ids)
    }
}
```

### Phase 3 Deliverables

- Add `code_change` field to Started and Closed events
- Flow integration: auto-populate code_change from current working copy
- `aiki provenance <change-id> --tasks` shows related tasks
- `aiki task show <task-id> --history` shows all code changes that touched it
- Bidirectional navigation: code → tasks, task → code

---

## Phase 4: Multi-Agent Coordination (1 week)

**When to build**: Multiple agents working on same codebase concurrently

### Why Event Sourcing Makes This Simple

**The problem with mutable tasks:**
- Agent A reads task, updates status → writes description
- Agent B reads task, adds attempt → writes description
- One update overwins the other (data loss)

**Event sourcing eliminates this:**
- Agent A appends "started" event → new JJ change
- Agent B appends "failed" event → new JJ change  
- Both events are preserved in the log
- Task reconstruction sees both events in order

### Concurrent Task Operations

**No conflicts possible** - each operation is an append-only event:

```rust
impl TaskManager {
    pub fn start_task(&self, task_id: &str, agent: &str) -> Result<()> {
        // Just append an event - never conflicts
        self.append_event(TaskEvent {
            task_id: task_id.to_string(),
            event: EventType::Started,
            timestamp: Utc::now(),
            agent: agent.to_string(),
        })
    }
}
```

**JJ handles branch updates atomically:**
```bash
# Agent A appends event
jj new aiki/tasks@ -m "started event"
jj branch set aiki/tasks -r @

# Agent B appends event (concurrent)
jj new aiki/tasks@ -m "failed event"  
jj branch set aiki/tasks -r @

# Both succeed - JJ resolves branch pointer automatically
# Event log contains both events in temporal order
```

### Deduplication Across Agents

**Content-addressed task IDs provide logical deduplication:**

```rust
// Agent A creates task for "TS2322 at auth.ts:42"
let task_id_a = generate_task_id(&definition);  // → "err-a1b2c3d4"

// Agent B creates task for same error (concurrent)
let task_id_b = generate_task_id(&definition);  // → "err-a1b2c3d4" (same!)

// Both agents append "created" events to the log
// Both events persist (useful audit trail: shows both agents detected the error)
```

**During reconstruction, duplicates are handled gracefully:**

```rust
fn reconstruct_task(&self, events: &[TaskEvent]) -> Result<Task> {
    let mut definition: Option<TaskDefinition> = None;
    // ...
    for event in events {
        match &event.event {
            EventType::Created { task } => {
                // If multiple agents create the same task concurrently,
                // multiple Created events will exist. Take the first one.
                if definition.is_none() {
                    definition = Some(task.clone());
                }
            }
            // ...
        }
    }
}
```

**Result:** Idempotent task creation with full audit trail. Multiple "created" events for the same task_id don't cause errors—they just show that multiple agents independently detected the same issue.

### Phase 4 Deliverables

- Multi-agent integration tests (2+ agents creating/updating tasks)
- Verify event ordering is consistent
- Verify no event loss under concurrent load
- Document: "Multi-Agent Task System Guide"

---

## Phase 5: Enterprise Features (1-2 weeks)

**When to build**: Enterprise customers need compliance

### Features

- Task relationships (blocking, parent/child)
- Priority queues
- Assignee tracking
- SLA/deadline tracking
- Task history queries
- Audit trail exports
- Custom task types

### Phase 5 Deliverables

- Relationship support in schema
- Priority-based ready queue
- `aiki task history <id>` command
- Compliance documentation

---

## Summary Table

| Phase | Time | Delivers | When to Build |
|-------|------|----------|---------------|
| **Phase 1** | 2-3 weeks | Event-sourced tasks: create, start, fail, close | **Now** (required for PostResponse) |
| **Phase 2** | 1 week | SQLite cache with event replay | When >1000 events or >1s query time |
| **Phase 3** | 1 week | Task ↔ Code provenance via events | When need to track what fixed what |
| **Phase 4** | 1 week | Multi-agent (already works!) | When testing concurrent agents |
| **Phase 5** | 1-2 weeks | Enterprise compliance features | When enterprise customers require it |

**Key architectural decision:** Event sourcing in Phase 1 makes Phase 4 trivial (append-only events = no conflicts).

---

## Decision: Start with Phase 1 Only

**Rationale:**

1. **Solves PostResponse immediately** - Structured tasks instead of text autoreplies
2. **Validates the approach** - Learn if tasks are better than autoreplies
3. **Low risk** - 2-3 weeks, can iterate based on feedback
4. **Easy to enhance** - SQLite cache is drop-in optimization later
5. **No premature optimization** - Don't build multi-agent until we need it

**After Phase 1 ships**, evaluate:
- Is query performance acceptable? (If no → Phase 2)
- Do we need provenance tracking? (If yes → Phase 3)
- Are multiple agents working? (If yes → Phase 4)
- Do we have enterprise users? (If yes → Phase 5)

---

## Next Steps

1. **Review this phased plan**
2. **Approve Phase 1 scope**
3. **Create GitHub issue for Phase 1**
4. **Start implementation** with TaskManager core
5. **Ship Phase 1** in 2-3 weeks
6. **Gather feedback** before building Phase 2+

# Quick Start: Atomic Task Creation and Start

**Status**: ✅ Complete
**Priority**: Medium
**Depends On**: Milestone 1.4 Task System
**Related**: `task-system.md`, `run-task.md`

---

## Overview

Make `aiki task start` smart enough to accept either a task ID or a task description, enabling atomic task creation and start in a single command.

**Key UX Improvement:**
```bash
# Before: Two commands
aiki task add "Implement user authentication"
# Created task: xqrmnpst
aiki task start xqrmnpst

# After: Single atomic command
aiki task start "Implement user authentication"
# Created and started task: xqrmnpst
```

---

## Table of Contents

1. [Motivation](#motivation)
2. [Design](#design)
3. [CLI Behavior](#cli-behavior)
4. [Implementation](#implementation)
5. [Examples](#examples)

---

## Motivation

### Current Workflow

Today, starting work on a new task requires two steps:

```bash
# Step 1: Create task
aiki task add "Fix null pointer in auth.ts"
# Output: Created task: xqrmnpst

# Step 2: Start task
aiki task start xqrmnpst
# Output: Started task xqrmnpst
```

### Problems

1. **Extra friction** - Two commands for a common workflow
2. **Mental overhead** - Remember the task ID between commands
3. **Copy-paste required** - Need to copy task ID from add to start
4. **Not atomic** - Task visible before work begins

### Solution

Make `aiki task start` accept either a task ID or description:

```bash
# Start existing task
aiki task start xqrmnpst

# Create and start new task (atomic)
aiki task start "Fix null pointer in auth.ts"
```

---

## Design

### Command Signature

```bash
aiki task start <task_id_or_description> [options]
```

**Behavior:**
- If input matches task ID format → start existing task
- If input is a string → create new task and start it atomically

### Task ID Detection

```rust
fn is_task_id(input: &str) -> bool {
    // Task IDs are alphanumeric, typically 8-12 chars
    // Examples: xqrmnpst, lpqrstwo, mxsl
    input.len() >= 4 
        && input.len() <= 16 
        && input.chars().all(|c| c.is_ascii_lowercase())
        && !input.contains(' ')
}
```

**Examples:**
- `xqrmnpst` → Task ID (start existing)
- `Fix auth bug` → Description (create and start)
- `implement-login` → Description (has hyphen, not a task ID)

### Options

All existing `aiki task start` options still work:

```bash
# Start with options
aiki task start "New task" --assignee codex --priority p0

# Start existing with options
aiki task start xqrmnpst --context "Focus on security"
```

---

## CLI Behavior

### Starting Existing Task

```bash
aiki task start xqrmnpst
```

**Output:**
```
Started task xqrmnpst: "Fix null pointer in auth.ts"
Agent: claude-code
```

**Behavior:**
1. Validate task exists
2. Check task is not already started
3. Emit `started` event
4. Begin work (if agent session)

### Creating and Starting New Task

```bash
aiki task start "Implement rate limiting"
```

**Output:**
```
Created and started task xqrmnpst: "Implement rate limiting"
Agent: claude-code
```

**Behavior:**
1. Create task with `created` event
2. Immediately emit `started` event
3. Both events in same transaction (atomic)
4. Begin work

### Ambiguous Input Handling

If input could be either:

```bash
# Edge case: short description that looks like task ID
aiki task start "test"
```

**Resolution strategy:**
1. Check if task with ID "test" exists
2. If exists → start that task
3. If not exists → create new task with name "test"

User can force interpretation:

```bash
# Force create new task
aiki task add "test"  # Then start separately

# Force start existing (will error if not found)
aiki task start --id test
```

---

## Implementation

### Command Handler

```rust
pub struct StartCommand {
    /// Task ID or task description
    pub input: String,
    
    /// Force interpretation as task ID
    #[arg(long)]
    pub id: bool,
    
    /// Options for new task creation
    #[arg(long)]
    pub assignee: Option<String>,
    
    #[arg(long)]
    pub priority: Option<Priority>,
    
    /// Additional context for starting
    #[arg(long)]
    pub context: Option<String>,
}

impl StartCommand {
    pub fn run(&self) -> Result<()> {
        if self.id {
            // Force treat as task ID
            start_existing_task(&self.input, self.context.as_deref())
        } else if is_task_id(&self.input) {
            // Looks like task ID, try to start existing
            match start_existing_task(&self.input, self.context.as_deref()) {
                Ok(()) => Ok(()),
                Err(AikiError::TaskNotFound(_)) => {
                    // Task not found, ask user if they meant to create
                    eprintln!("Task '{}' not found.", self.input);
                    eprintln!("Did you mean to create a new task?");
                    eprintln!("  aiki task start \"{}\"", self.input);
                    Err(AikiError::TaskNotFound(self.input.clone()))
                }
                Err(e) => Err(e),
            }
        } else {
            // Treat as description, create and start
            create_and_start_task(CreateStartOptions {
                name: self.input.clone(),
                assignee: self.assignee.clone()
                    .or_else(|| get_current_agent().ok()),
                priority: self.priority,
                context: self.context.clone(),
            })
        }
    }
}

fn is_task_id(input: &str) -> bool {
    input.len() >= 4 
        && input.len() <= 16 
        && input.chars().all(|c| c.is_ascii_lowercase())
        && !input.contains(' ')
}

fn start_existing_task(task_id: &str, context: Option<&str>) -> Result<()> {
    let task = load_task(task_id)?;
    
    if task.is_started() {
        return Err(AikiError::TaskAlreadyStarted(task_id.to_string()));
    }
    
    emit_task_event(TaskEvent::Started {
        task_id: task_id.to_string(),
        timestamp: Utc::now(),
    })?;
    
    eprintln!("✅ Started task {}: \"{}\"", task_id, task.name);
    eprintln!("   Agent: {}", task.assignee);
    
    Ok(())
}

struct CreateStartOptions {
    name: String,
    assignee: Option<String>,
    priority: Option<Priority>,
    context: Option<String>,
}

fn create_and_start_task(opts: CreateStartOptions) -> Result<()> {
    let task_id = generate_task_id()?;
    let assignee = opts.assignee.unwrap_or_else(|| "claude-code".to_string());
    let timestamp = Utc::now();
    
    // Create both events atomically
    let events = vec![
        TaskEvent::Created {
            task_id: task_id.clone(),
            name: opts.name.clone(),
            assignee: assignee.clone(),
            priority: opts.priority,
            instructions: None,
            metadata: None,
            timestamp,
        },
        TaskEvent::Started {
            task_id: task_id.clone(),
            timestamp: timestamp + Duration::milliseconds(1),
        },
    ];
    
    // Write both events in single transaction
    emit_task_events(&events)?;
    
    eprintln!("✅ Created and started task {}: \"{}\"", task_id, opts.name);
    eprintln!("   Agent: {}", assignee);
    
    Ok(())
}
```

### Event Atomicity

Both `created` and `started` events written in single transaction:

```rust
pub fn emit_task_events(events: &[TaskEvent]) -> Result<()> {
    let repo = load_repo()?;
    let mut tx = repo.start_transaction();
    
    for event in events {
        write_event_to_branch(&mut tx, event)?;
    }
    
    tx.commit("aiki: task events")?;
    Ok(())
}
```

---

## Examples

### Example 1: Quick Start on New Work

```bash
# Developer starts working on something new
aiki task start "Add password reset flow"

# Output:
# ✅ Created and started task xqrmnpst: "Add password reset flow"
#    Agent: claude-code

# Task is immediately in started state
aiki task list
# Tasks (1 in progress):
#   xqrmnpst  Add password reset flow  (started, p1, claude-code)
```

### Example 2: Start Existing Task

```bash
# List tasks
aiki task list
# Tasks (3 ready):
#   lpqrstwo  Fix null pointer in auth  (p0, claude-code)
#   mxsl      Update documentation       (p2, claude-code)

# Start existing task
aiki task start lpqrstwo

# Output:
# ✅ Started task lpqrstwo: "Fix null pointer in auth"
#    Agent: claude-code
```

### Example 3: With Options

```bash
# Create and start with specific assignee and priority
aiki task start "Security audit of API endpoints" --assignee codex --priority p0

# Output:
# ✅ Created and started task npts: "Security audit of API endpoints"
#    Agent: codex
#    Priority: p0
```

### Example 4: Ambiguous Input (Edge Case)

```bash
# Try to start task that might not exist
aiki task start xyzabc

# If task exists:
# ✅ Started task xyzabc: "Some existing task"

# If task doesn't exist:
# ❌ Task 'xyzabc' not found.
# Did you mean to create a new task?
#   aiki task start "xyzabc"
```

### Example 5: Force ID Interpretation

```bash
# Force treat as task ID (will error if not found)
aiki task start --id test

# Output if not found:
# ❌ Task 'test' not found
```

---

## AGENTS.md Update

Update the agent guide to show the new workflow:

```markdown
## Working with Tasks

### Quick Start Workflow

Start working on something new in one command:

```bash
# Create and start task atomically
aiki task start "Description of what you're working on"
```

This is equivalent to but faster than:

```bash
# Old two-step process
aiki task add "Description of what you're working on"
aiki task start <task-id>
```

### Task Lifecycle

```bash
# Create and start immediately
aiki task start "Implement feature X"

# Do your work...

# Close with comment
aiki task close <task-id> --comment "Completed: summary of what was done"
```

### Starting Existing Tasks

If you created a task earlier and want to start it now:

```bash
# List tasks
aiki task list

# Start existing task by ID
aiki task start <task-id>
```
```

---

## Implementation Plan

### Phase 1: Core Functionality (2 days)

**Deliverables:**
- Implement `is_task_id()` detection
- Add logic to `StartCommand` for dual behavior
- Implement `create_and_start_task()` with atomic events
- Basic error handling

**Files:**
- `cli/src/commands/task.rs` - Update `StartCommand`
- `cli/src/tasks/manager.rs` - Add `create_and_start_task()`

### Phase 2: Edge Cases and UX (1 day)

**Deliverables:**
- Handle ambiguous inputs
- Add `--id` flag for forcing interpretation
- Improve error messages
- Add helpful suggestions

**Files:**
- `cli/src/commands/task.rs` - Error handling improvements
- Help text updates

### Phase 3: Documentation (1 day)

**Deliverables:**
- Update AGENTS.md with new workflow
- Add examples to help text
- Update task system documentation
- Add to CLAUDE.md guidelines

**Files:**
- `AGENTS.md` - Update quick start section
- `cli/src/commands/task.rs` - Update help text
- `CLAUDE.md` - Add to best practices

**Total Timeline:** 4 days

---

## Success Criteria

### Must Have
- ✅ `aiki task start "description"` creates and starts task atomically
- ✅ `aiki task start <task-id>` starts existing task
- ✅ Task ID detection works correctly
- ✅ Both events written in single transaction

### Should Have
- ✅ Helpful error messages for ambiguous inputs
- ✅ `--id` flag for forcing interpretation
- ✅ Updated AGENTS.md documentation
- ✅ All existing `start` options still work

### Nice to Have
- ✅ Interactive prompt for ambiguous cases
- ✅ Shell completion for both IDs and descriptions
- ✅ Statistics on usage of each mode

---

## Benefits

1. **Faster workflow** - One command instead of two
2. **Less friction** - No need to remember/copy task IDs
3. **Atomic operation** - Task created and started together
4. **Backward compatible** - Existing usage still works
5. **Natural UX** - "start" implies beginning work
6. **Agent-friendly** - Simpler for agents to use

---

## References

- Milestone 1.4: Task System
- `ops/now/task-system.md` - Task system architecture
- `ops/now/run-task.md` - Task execution model
- AGENTS.md - Agent workflow guidelines

# Task Assignment to Agents

**Date**: 2026-01-15
**Status**: Proposed Design
**Purpose**: Enable routing tasks to specific agents with automatic context injection

---

## Executive Summary

This design adds the ability to assign tasks to specific agents (claude-code, cursor, codex, gemini, human) so that:

1. **Task routing** - Tasks are automatically routed to the correct agent
2. **Context injection** - Assigned agents receive their tasks in context on session start
3. **Agent validation** - Assignees are validated against supported agent types
4. **Task execution** - `aiki task run` spawns the correct agent for the task

---

## Table of Contents

1. [Current State](#current-state)
2. [Proposed Design](#proposed-design)
3. [Agent Types](#agent-types)
4. [CLI Changes](#cli-changes)
5. [Context Injection](#context-injection)
6. [Task Execution](#task-execution)
7. [Implementation Plan](#implementation-plan) (Phases 0-4)
8. [Open Questions](#open-questions)
9. [Summary](#summary)

---

## Current State

### What Exists

The task system already has an `assignee` field in the data model:

```rust
// cli/src/tasks/types.rs
pub enum TaskEvent {
    Created {
        task_id: String,
        name: String,
        priority: TaskPriority,
        assignee: Option<String>,  // <-- exists in schema
        timestamp: DateTime<Utc>,
    },
    // ...
}

pub struct Task {
    pub id: String,
    pub name: String,
    pub assignee: Option<String>,  // <-- exists in schema
    // ...
}
```

**Current CLI behavior** (from `cli/src/commands/task.rs`):

| Command | Assignee Behavior |
|---------|-------------------|
| `aiki task add` | Always `None` (line 336: `assignee: None, // Phase 3: auto-detect`) |
| `aiki task stop --blocked` | Hardcoded `Some("human")` for blocker tasks (line 704) |
| `aiki task start` | Has TODO: `// TODO: get from context` (line 543) |

**No CLI flag exists** - Users cannot specify `--for` when creating tasks.

### What's Missing

1. **CLI flag** - No `--for <agent>` option on `aiki task add`
2. **Agent type validation** - No validation that assignee is a valid agent
3. **Routing logic** - No logic to filter tasks by agent
4. **Context injection** - Agents don't receive their assigned tasks
5. **Task execution** - No `aiki task run` command to spawn agents

---

## Proposed Design

### Core Principles

1. **Assignee is optional** - Unassigned tasks are visible to all agents
2. **Agent-aware context** - Each agent sees tasks assigned to it (plus unassigned)
3. **Human tasks** - Tasks assigned to "human" are excluded from agent context
4. **Current session agent** - Context injection uses the current session's agent type

### Task Visibility Rules

| Task Assignee | Visible to Claude Code | Visible to Cursor | Visible to Codex | Visible to Gemini | Visible to Human |
|---------------|------------------------|-------------------|------------------|-------------------|------------------|
| `None` (unassigned) | Yes | Yes | Yes | Yes | Yes |
| `claude-code` | Yes | No | No | No | No |
| `cursor` | No | Yes | No | No | No |
| `codex` | No | No | Yes | No | No |
| `gemini` | No | No | No | Yes | No |
| `human` | No | No | No | No | Yes |

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│  User creates task: aiki task add "Fix auth" --for claude-code
│                                                             │
│  Task stored with: assignee="claude-code"                   │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Claude Code session starts (session.started event)         │
│                                                             │
│  Context injection queries: tasks where                     │
│    assignee IS NULL OR assignee = "claude-code"            │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Agent receives task list in XML context:                   │
│                                                             │
│  <aiki_task cmd="context">                                  │
│    <list ready="3">                                         │
│      <task id="ab12" name="Fix auth" assignee="claude-code"/>
│      <task id="cd34" name="Add tests"/>  <!-- unassigned -->│
│    </list>                                                  │
│  </aiki_task>                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Agent Types

### Supported Assignees

| Assignee | Description | Can Run Tasks | Receives Context |
|----------|-------------|---------------|------------------|
| `claude-code` | Claude Code extension | Yes (via hooks) | Yes |
| `cursor` | Cursor IDE | Yes (via hooks) | Yes |
| `codex` | OpenAI Codex (headless) | Yes (via aiki task run) | Yes |
| `gemini` | Google Gemini | Yes (via hooks) | Yes |
| `human` | Human developer | No (manual) | No |
| `None` | Unassigned | N/A | All agents |

### Validation

**Note**: The codebase currently has duplicate `AgentType` enums in `provenance.rs` and `history/types.rs`. Phase 0 consolidates these before adding `Assignee`.

```rust
// Uses existing AgentType from cli/src/agents/types.rs (consolidated in Phase 0)
pub enum Assignee {
    Agent(AgentType),   // claude-code, cursor, codex, gemini
    Human,              // human
    Unassigned,         // None/absent
}

impl Assignee {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "claude-code" | "claude" => Ok(Assignee::Agent(AgentType::ClaudeCode)),
            "cursor" => Ok(Assignee::Agent(AgentType::Cursor)),
            "codex" => Ok(Assignee::Agent(AgentType::Codex)),
            "gemini" => Ok(Assignee::Agent(AgentType::Gemini)),
            "human" | "me" => Ok(Assignee::Human),
            "" => Ok(Assignee::Unassigned),
            _ => Err(AikiError::UnknownAssignee(s.to_string())),
        }
    }

    pub fn is_visible_to(&self, agent: &AgentType) -> bool {
        match self {
            Assignee::Unassigned => true,  // visible to all
            Assignee::Human => false,       // not visible to agents
            Assignee::Agent(a) => a == agent,
        }
    }
}
```

---

## CLI Changes

### Task Creation

```bash
# Assign to specific agent
aiki task add "Fix auth bug" --for claude-code
aiki task add "Review security" --for codex
aiki task add "Manual testing" --for human

# Aliases for convenience
aiki task add "Fix auth bug" --for claude  # expands to claude-code
aiki task add "Fix auth bug" --for me      # expands to human
aiki task add "Fix auth bug" --for self    # auto-detect current agent session

# Unassigned (default)
aiki task add "General cleanup"
```

### Task Listing

```bash
# Default behavior depends on caller:
# - From agent session: tasks for that agent + unassigned
# - From human (terminal): human-assigned + unassigned (agent tasks hidden)
aiki task list

# Filter by assignee
aiki task list --for claude-code
aiki task list --for human
aiki task list --unassigned

# Show all regardless of assignee (overrides default filtering)
aiki task list --all
```

**Note**: When invoked from a terminal (not in an agent session), `aiki task list` defaults to showing human-assigned and unassigned tasks. This prevents humans from seeing agent-specific tasks in their queue. Use `--all` to override this behavior.

### Task Update

```bash
# Reassign task
aiki task update ab12 --for cursor
aiki task update ab12 --for human

# Unassign task
aiki task update ab12 --unassign
```

### XML Output

```xml
<aiki_task cmd="list" status="ok">
  <context>
    <in_progress/>
    <list ready="3">
      <task id="ab12" name="Fix auth bug" priority="p1" assignee="claude-code"/>
      <task id="cd34" name="Add tests" priority="p2"/>  <!-- unassigned -->
      <task id="ef56" name="Review code" priority="p2" assignee="codex"/>
    </list>
  </context>
</aiki_task>
```

---

## Context Injection

### On Session Start

When an agent session starts, inject context with tasks assigned to that agent:

```rust
// In session_started handler
fn handle_session_started(payload: &AikiSessionStartPayload) -> Result<HookResult> {
    let agent_type = &payload.session.agent_type;

    // Get tasks visible to this agent
    let tasks = get_tasks_for_agent(agent_type)?;

    // Format as XML context
    let context = format_task_context(&tasks)?;

    Ok(HookResult {
        context: Some(context),
        decision: Decision::Allow,
        failures: vec![],
    })
}

fn get_tasks_for_agent(agent: &AgentType) -> Result<Vec<Task>> {
    let all_tasks = materialize_tasks()?;

    Ok(all_tasks
        .into_iter()
        .filter(|t| {
            // Parse assignee, defaulting to Unassigned on parse error
            let assignee = Assignee::from_str(t.assignee.as_deref().unwrap_or(""))
                .unwrap_or(Assignee::Unassigned);
            assignee.is_visible_to(agent)
        })
        .collect())
}
```

### Context Format

```xml
<aiki_task cmd="context" agent="claude-code">
  <in_progress>
    <task id="ab12" name="Fix auth bug" priority="p1" started_at="2026-01-15T10:00:00Z"/>
  </in_progress>
  <list ready="2">
    <task id="cd34" name="Add tests" priority="p2"/>
    <task id="ef56" name="Update docs" priority="p3"/>
  </list>
</aiki_task>
```

### Flow Integration (Single Source)

Context injection happens **only in the flow** (`cli/src/flows/core/flow.yaml`), not in the session_started handler. The handler simply invokes the flow:

```rust
// session_started.rs - just invokes flow, no context injection here
let flow_result = execute_flow(EventType::SessionStarted, &mut state, &core_flow.session_started)?;
```

The flow handles task context injection:

```yaml
# cli/src/flows/core/flow.yaml - single source of context injection
session.started:
  # ... init actions ...
  - let: task_count = self.task_list_size
  - if: $task_count
    then:
      - context:
          append: |
            Tasks ($task_count ready)
            Run `aiki task` to view - OR - `aiki task start` to begin work.
```

User-defined flows can customize this in `.aiki/flows/`:

```yaml
# .aiki/flows/custom.yml - overrides core flow
session.started:
  - context: |
      You have tasks assigned to you. Run `aiki task list` to see them.
      Start a task with `aiki task start <id>`.
```

---

## Task Execution

### `aiki task run` Command

Execute a task by spawning the appropriate agent:

```bash
# Run a task (spawns agent based on assignee)
aiki task run ab12

# Run with explicit agent (overrides assignee)
aiki task run ab12 --agent codex
```

### Execution Flow

```
┌─────────────────────────────────────────────────────────────┐
│  aiki task run ab12                                         │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  1. Load task ab12 from aiki/tasks                          │
│  2. Check assignee (e.g., "codex")                          │
│  3. Mark task as started                                    │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  4. Spawn agent session (headless)                          │
│     - For codex: spawn via API                              │
│     - For claude-code: use current session                  │
│     - For human: error (cannot run)                         │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  5. Agent receives task context                             │
│  6. Agent executes task                                     │
│  7. Agent session ends                                      │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  8. Task marked as completed or stopped                     │
│  9. Return control to caller                                │
└─────────────────────────────────────────────────────────────┘
```

### Agent Spawning

```rust
fn spawn_agent_for_task(task: &Task, agent: &AgentType) -> Result<()> {
    match agent {
        AgentType::Codex => {
            // Spawn headless codex via API
            let codex = CodexClient::new()?;
            codex.run_task(task)?;
        }
        AgentType::ClaudeCode => {
            // If we're in a Claude Code session, execute inline
            // Otherwise, error (need claude-code running)
            if is_in_claude_session()? {
                // Task is already in context, just return
                Ok(())
            } else {
                Err(AikiError::AgentNotRunning("claude-code".to_string()))
            }
        }
        AgentType::Cursor => {
            // Similar to Claude - needs Cursor running
            if is_in_cursor_session()? {
                Ok(())
            } else {
                Err(AikiError::AgentNotRunning("cursor".to_string()))
            }
        }
        _ => Err(AikiError::UnsupportedAgentType(agent.to_string())),
    }
}
```

---

## Implementation Plan

### Phase 0: Consolidate AgentType

**Goal**: Unify duplicate `AgentType` enums into a single canonical type

**Problem**: The codebase currently has two different `AgentType` enums:
- `cli/src/provenance.rs:18` - `Claude`, `Codex`, `Cursor`, `Gemini`, `Unknown`
- `cli/src/history/types.rs:17` - `ClaudeCode`, `Cursor`, `Gemini`, `Other(String)`

**Changes**:
- Create canonical `AgentType` in `cli/src/agents/types.rs`
- Use `ClaudeCode` (not `Claude`) for consistency with CLI flag `--for claude-code`
- Update `provenance.rs` and `history/types.rs` to use the canonical type
- Add `from_str()` with alias support (`"claude"` → `ClaudeCode`)

**Files**:
- `cli/src/agents/mod.rs` - New module for agent types
- `cli/src/agents/types.rs` - Canonical `AgentType` enum
- `cli/src/provenance.rs` - Remove local `AgentType`, import from agents
- `cli/src/history/types.rs` - Remove local `AgentType`, import from agents

### Phase 1: Assignee Validation

**Goal**: Add assignee type with validation

**Changes**:
- Add `Assignee` enum to `cli/src/tasks/types.rs`
- Add `from_str()` and `is_visible_to()` methods
- Add `UnknownAssignee` error variant
- Update `TaskEvent::Created` to validate assignee

**Files**:
- `cli/src/tasks/types.rs` - Add Assignee enum
- `cli/src/error.rs` - Add UnknownAssignee error

### Phase 2: CLI Flags

**Goal**: Add `--for` flag to task commands

**Changes**:
- Add `--for <agent>` to `aiki task add`
- Add `--for`, `--unassigned`, `--all` to `aiki task list`
- Add `--for`, `--unassign` to `aiki task update`
- Add `assignee` field to `TaskEvent::Updated` variant
- Update XML output to include assignee attribute

**Files**:
- `cli/src/commands/task.rs` - Add CLI flags
- `cli/src/tasks/types.rs` - Add assignee to Updated event
- `cli/src/tasks/xml.rs` - Update XML output

### Phase 3: Context Filtering

**Goal**: Filter tasks by agent in context injection

**Changes**:
- Add `get_tasks_for_agent()` helper
- Update `handle_session_started` to filter tasks
- Update flow context to respect assignee

**Files**:
- `cli/src/tasks/manager.rs` - Add filtering helpers
- `cli/src/events/session_started.rs` - Filter context
- `cli/src/flows/core/flow.yaml` - Update context injection

### Phase 4: Task Execution (Future)

**Goal**: Implement `aiki task run` command

**Changes**:
- Add `run` subcommand to task CLI
- Implement agent spawning logic
- Add headless codex support

**Files**:
- `cli/src/commands/task.rs` - Add run subcommand
- `cli/src/agents/codex.rs` - Headless codex client (new)
- `cli/src/agents/mod.rs` - Agent spawning logic (new)

---

## Open Questions

1. **Default assignee**: Should tasks default to the current agent, or stay unassigned?
   - **Recommendation**: Unassigned by default (explicit assignment)

2. **Agent aliases**: Support shorthand like `claude` → `claude-code`?
   - **Recommendation**: Yes, for convenience

3. **Reassignment notifications**: Should agents be notified when tasks are reassigned?
   - **Recommendation**: Future enhancement (via events)

4. **Human task visibility**: Should human tasks appear in any agent context?
   - **Recommendation**: No, human tasks are agent-invisible

5. **Task inheritance**: Should child tasks inherit parent's assignee?
   - **Recommendation**: Yes, unless explicitly overridden
   - **Implementation**: Add to Phase 2 - when creating subtask, default assignee to parent's assignee

---

## Summary

This design enables task assignment to specific agents:

- **`--for` flag** - Assign tasks to claude-code, cursor, codex, gemini, or human
- **Context filtering** - Agents only see their assigned tasks (plus unassigned)
- **Validation** - Assignees validated against known agent types
- **Future: task run** - Execute tasks by spawning the appropriate agent

The implementation is incremental: Phase 0 consolidates existing code, Phases 1-3 enable assignment and filtering, Phase 4 adds execution.

### Migration Note

Existing tasks with `assignee: None` remain **visible to all agents** - no migration required. The system treats unassigned tasks as implicitly available to any agent.

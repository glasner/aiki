# Task Execution: aiki task run

**Status**: 🟢 Implemented  
**Priority**: High  
**Depends On**: Milestone 1.4 Task System  
**Enables**: Review Loop, Flow Actions (task.run)

---

## Overview

The `aiki task run` command starts an agent session to work on a task. This is the foundation for:
- Automated code reviews (review tasks executed by codex)
- Review loops (auto-fix → re-review cycles)
- Flow-triggered task execution (task.run action)
- Delegating work to specialized agents

**Key Design Principle**: The CLI command is the primitive; flow actions build on top of it.

---

## Critical Blockers & Design Decisions

> **Feedback from codex review:**
> - No agent spawning mechanism exists
> - Task context fields (instructions, metadata, scope) don't exist in current Task struct
> - No session-to-task linking defined

### Blocker 1: Agent Runtime Trait

**Problem:** No mechanism to spawn/manage agent processes.

**Solution:** Define an `AgentRuntime` abstraction that handles agent spawning:

**Core Types:**

```
AgentSessionResult = one of:
  - Completed(summary: string)
      Agent finished successfully
  
  - Stopped(reason: string)
      Agent explicitly stopped (needs user input, blocked, etc.)
  
  - Failed(error: string)
      Agent failed (crash, timeout, error)

AgentSpawnOptions:
  - cwd: path
      Working directory for the agent
  
  - task_id: string
      Task ID to work on
  
  - agent_override: optional AgentType
      Override the task's assignee

AgentRuntime interface:
  - agent_type() → AgentType
      Returns the agent type this runtime handles
  
  - spawn_blocking(options) → Result<AgentSessionResult>
      Spawns an agent session and waits for completion

AgentSessionHandle:
  - session_id: string
      Unique session identifier
  
  - child_process: process handle
      Internal process handle
  
  Methods:
  - wait() → Result<AgentSessionResult>
      Wait for the session to complete
  
  - is_running() → boolean
      Check if session is still running
  
  - kill() → Result<void>
      Kill the session
```

### Blocker 2: Extended Task Struct

**Problem:** Current `Task` struct lacks execution context fields.

**Solution:** Add optional execution context that's populated for runnable tasks:

**Core Types:**

```
TaskType = one of:
  - Generic (default)
  - Review (code review task)
  - Fix (fix/implement task)
  - Research (investigation task)

TaskContext:
  - task_type: optional TaskType
      Classification of the task
  
  - instructions: optional string
      Detailed instructions for the agent
  
  - metadata: map of string to string
      Arbitrary metadata (type-specific, e.g., revset, prompt)
  
  - scope_files: list of strings
      Files in scope for this task

Task object:
  Methods:
  - load_context(cwd) → Result<TaskContext>
      Load execution context for this task from aiki/tasks branch
```

**Storage:** Task context is stored alongside events on `aiki/tasks` branch:
- `tasks/{task_id}/context.yaml` - execution context
- Events stream continues to use existing format

### Session-to-Task Linking (Already Solved)

**No new mechanism needed!** The existing task claim system already provides session-to-task linking:

**How it works:**
- When `aiki task run` executes, it calls `aiki task start <task_id>`
- This emits `TaskEvent::Started` which claims the task for the current session
- The task's `claimed_by_session` field is set to the session ID
- To find which task a session is working on: query tasks where `claimed_by_session == session_id`

**Bidirectional linking:**
- Task → Session: `claimed_by_session` field (already exists)
- Session → Task: Query tasks by `claimed_by_session` (already possible)

No session file changes needed.

### V1 Scope Decisions

Based on the blockers, here's the phased approach:

| Feature | V1 (MVP) |
|---------|----------|
| `aiki task run <id>` | ✅ |
| `--agent` override | ✅ |
| Flow integration (`task.run`) | ✅ |

**V1 focuses on:** Synchronous, single-task execution with Claude Code or Codex.

---

## Table of Contents

1. [CLI Command Design](#cli-command-design)
2. [Task Execution Model](#task-execution-model)
3. [Agent Session Integration](#agent-session-integration)
4. [Task Hierarchy Handling](#task-hierarchy-handling)
5. [Flow Action Integration](#flow-action-integration)
6. [Implementation](#implementation)
7. [Use Cases](#use-cases)

---

## CLI Command Design

### Basic Command

```bash
aiki task run <task_id>
```

**Behavior:**
1. Loads task from `aiki/tasks` branch
2. Determines assignee agent from task metadata
3. Spawns agent session with task context (prompt includes instruction to run `aiki task start <task_id>`)
4. Agent claims task via `aiki task start <task_id>`
5. Agent works on task (may complete subtasks, add comments, make changes)
6. Agent marks task as closed when done (using `aiki task close`)
7. Returns with summary

**Example:**
```bash
# Run a review task assigned to codex
aiki task run xqrmnpst

# Output:
Spawning codex agent session for task xqrmnpst...
Agent started task xqrmnpst
Agent completed subtask xqrmnpst.1 (Digest code changes)
Agent working on subtask xqrmnpst.2 (Review code)...
Agent added 2 comments
Agent completed task xqrmnpst
Task run complete (duration: 9.2s)
```

### Options

```bash
aiki task run <task_id> [options]
```

**Options:**
- `--agent <name>` - Override assignee (run with different agent)

**Examples:**
```bash
# Run task with default assignee
aiki task run xqrmnpst

# Override agent assignment
aiki task run xqrmnpst --agent claude-code
```

---

## Task Execution Model

### Execution States

These match the existing `TaskStatus` enum in `cli/src/tasks/types.rs`:

```
┌─────────────────────────────────────────────────────────────┐
│  TASK STATES (TaskStatus enum)                              │
├─────────────────────────────────────────────────────────────┤
│  Open        → Task exists, ready to work on                │
│  InProgress  → Agent session spawned, working on task       │
│  Stopped     → Agent returned control, task incomplete      │
│  Closed      → Task done (outcome: Done or WontDo)          │
└─────────────────────────────────────────────────────────────┘
```

### Task Run Lifecycle

```
┌──────────────────────────────────────────────────────────────┐
│  1. Load Task                                                │
│     • Read from aiki/tasks branch                            │
│     • Validate task exists and not closed                    │
│     • Extract assignee, subtasks, metadata                   │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  2. Spawn Agent Session                                      │
│     • Start agent with task context                          │
│     • Prompt includes: "Run `aiki task start <task_id>`"     │
│     • Agent sees task instructions, subtasks, scope          │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  3. Agent Claims Task                                        │
│     • Agent runs: aiki task start <task_id>                  │
│     • Emits Started event, task now InProgress               │
│     • Task claimed_by_session set to agent's session         │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  4. Agent Works on Task                                      │
│     • Agent executes subtasks sequentially (if present)      │
│     • Agent can mark subtasks complete                       │
│     • Agent adds comments as needed                          │
│     • Agent makes code changes                               │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  5. Agent Closes Task                                        │
│     • Agent runs: aiki task close <task_id>                  │
│     • Emits Closed event (outcome: Done)                     │
│     • Task status now Closed                                 │
└──────────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────────┐
│  6. Session Ends                                             │
│     • Agent session completes                                │
│     • Control returns to caller                              │
└──────────────────────────────────────────────────────────────┘
```

### Task Context Passed to Agent

The agent's initial prompt is minimal - just instructions to start the task:

```
Run `aiki task start xqrmnpst` to begin working on this task.
```

The agent can then use `aiki task show xqrmnpst` to view full task details, instructions, subtasks, and scope.

### Context Passing Mechanism

**Question:** How is task context passed to agents (env vars, file, stdin)?

**Answer:** Use **stdin** for passing the simple prompt:

```bash
# Claude Code invocation (non-interactive)
claude --print --dangerously-skip-permissions "Run \`aiki task start xqrmnpst\` to begin working on this task."

# Codex invocation (non-interactive)
codex exec "Run \`aiki task start xqrmnpst\` to begin working on this task."
```

**Implementation Approach:**

```
ClaudeCodeRuntime:
  
  agent_type() → AgentType:
    return ClaudeCode
  
  spawn_blocking(options) → Result<AgentSessionResult>:
    # Build the simple task prompt
    prompt = "Run `aiki task start " + options.task_id + "` to begin working on this task."
    
    # Spawn claude process with prompt via command args
    child_process = spawn_command(
      command: "claude",
      args: ["--print", "--dangerously-skip-permissions", prompt],
      cwd: options.cwd
    )
    
    # Wait for completion
    output = wait_for_completion(child_process)
    
    # Parse exit status and return result
    if process exited successfully:
      summary = extract_summary(output.stdout)
      return Completed(summary)
    else:
      return Failed(output.stderr)
```

**Why this approach:**

The prompt is minimal (just the task ID), so we simply pass it as a command argument. The agent uses `aiki task show` to get full task details.

---

## Agent Session Integration

### Session Spawning

**Main execution flow:**

```
function task_run(task_id, options):
  # Load task from aiki/tasks branch
  task = load_task(task_id)
  
  # Validate task can be run
  if task.status is Closed:
    return error("Task already closed")
  
  # Determine which agent to use
  agent = options.agent OR task.assignee
  
  # Build spawn options
  spawn_options = {
    cwd: get_repo_root(),
    task_id: task_id,
    agent_override: options.agent
  }
  
  # Spawn agent session (agent will claim and close task)
  runtime = get_runtime(agent)
  result = runtime.spawn_blocking(spawn_options)
  
  # Handle result (agent already closed task if successful)
  match result:
    case Completed:
      # Agent already closed the task via `aiki task close`
      print("✅ Task " + task_id + " completed")
    
    case Stopped(reason):
      # Agent stopped - emit Stopped event
      emit_event(TaskEvent.Stopped(
        task_ids: [task_id],
        reason: reason,
        timestamp: now()
      ))
      print("⏸️  Task " + task_id + " stopped: " + reason)
    
    case Failed(error):
      # Agent failed - emit Stopped event
      emit_event(TaskEvent.Stopped(
        task_ids: [task_id],
        reason: "Session failed: " + error,
        timestamp: now()
      ))
      print("❌ Task " + task_id + " failed: " + error)
      return error(error)
  
  return success
```

### Failure Handling

When an agent session fails (crashes, times out, or returns an error), the task must always be updated:

**Failure Scenarios:**

| Scenario | Task Event | Outcome |
|----------|------------|---------|
| Agent completes successfully | `Closed` | `Done` |
| Agent explicitly stops/pauses | `Stopped` | reason: "Agent needs user input" |
| Agent crashes/panics | `Stopped` | reason: "Session failed: <error>" |
| Spawn failure | `Stopped` | reason: "Failed to spawn agent: <error>" |

**Key Principle:** A task must never be left in `InProgress` without an active session. If the session ends for any reason, the task must transition to either `Stopped` or `Closed`.

**Error handling pseudocode:**

```
# Spawn failures must also emit Stopped
try:
  session_id = spawn_agent_session(agent, spawn_options)
catch spawn_error:
  emit_event(TaskEvent.Stopped(
    task_ids: [task_id],
    reason: "Failed to spawn agent: " + spawn_error,
    timestamp: now()
  ))
  return error(spawn_error)
```



---

## Task Hierarchy Handling

### Parent Tasks with Subtasks

When running a parent task with subtasks:

**Behavior:**
- Agent receives all subtasks in context
- Agent works through subtasks sequentially
- Each subtask completion emits a `closed` event
- Parent task marked closed (Done) when all subtasks done

**Example:**
```bash
aiki task run xqrmnpst  # Parent with 2 subtasks

# Agent flow:
# 1. Sees subtask xqrmnpst.1, works on it
# 2. Completes xqrmnpst.1 → emits closed event
# 3. Sees subtask xqrmnpst.2, works on it
# 4. Completes xqrmnpst.2 → emits closed event
# 5. Parent xqrmnpst marked closed (Done)
```

### Running Individual Subtasks

You can also run a single subtask:

```bash
aiki task run xqrmnpst.2  # Just the review subtask

# Agent only works on this subtask
# Parent task NOT marked closed
```

---

## Flow Action Integration

### task.run Flow Action

Once the CLI command is implemented, flows can use it:

```yaml
# Flow action builds on CLI primitive
task.run:
  task_id: $event.review.followup_task_id
  agent: claude-code  # Optional override
```

**Implementation approach:**

```
function handle_task_run_action(action, event) → Result:
  # Build task run options from flow action
  options = {
    agent: action.agent
  }
  
  # Execute the task
  result = task_run(action.task_id, options)
  
  return result
```

### Example: Auto-Fix Review Issues

```yaml
review.completed:
  - if: $event.review.issues_found > 0
    then:
      - log: "Review found ${event.review.issues_found} issues, starting fixes..."
      - task.run:
          task_id: $event.review.followup_task_id
      - log: "✅ Fixes completed"
```

### Example: Review Loop

```yaml
review.completed:
  - if: $event.review.loop_enabled && $event.review.issues_found > 0
    then:
      - task.run:
          task_id: $event.review.followup_task_id
      - log: "Fixes complete, re-reviewing..."
      - review:
          scope: $event.review.scope
          files: $event.review.files
          prompt: $event.review.prompt
          loop: true
```

---

## Implementation

### V1 Implementation Plan

Based on the [V1 Scope Decisions](#v1-scope-decisions), here's the implementation plan:

### Phase 1: Agent Runtime Infrastructure

**Deliverables:**
- `AgentRuntime` trait definition
- `ClaudeCodeRuntime` implementation
- `CodexRuntime` implementation
- Basic spawn/wait functionality

**Files:**
```
cli/src/agents/
├── mod.rs              # Export runtime types
├── types.rs            # Existing: AgentType, Assignee
├── detect.rs           # Existing: process tree detection
└── runtime/
    ├── mod.rs          # AgentRuntime trait, AgentSessionResult
    ├── claude_code.rs  # ClaudeCodeRuntime
    └── codex.rs        # CodexRuntime
```

**Core abstraction:**

```
AgentRuntime interface (defined above in Blocker 1)

function get_runtime(agent_type) → AgentRuntime:
  match agent_type:
    case ClaudeCode:
      return ClaudeCodeRuntime
    case Codex:
      return CodexRuntime
    case other:
      return error("Unsupported agent type: " + agent_type)
```

### Phase 2: Task Context Infrastructure

**Deliverables:**
- `TaskContext` struct with execution metadata
- Context storage/loading on `aiki/tasks` branch

**Files:**
```
cli/src/tasks/
├── types.rs            # Add TaskType, TaskContext
├── context.rs          # NEW: TaskContext storage/loading
└── ...
```

**Storage format:** `tasks/{task_id}/context.yaml`
```yaml
type: review
instructions: |
  Code review orchestration task
metadata:
  revset: "@"
  prompt: security
scope_files:
  - src/auth.ts
  - src/crypto.ts
```

### Phase 3: CLI Command (`aiki task run`)

**Deliverables:**
- `Run` subcommand in `TaskCommands`
- Task validation and loading
- Agent spawning with task context
- Task state updates (InProgress → Stopped/Closed)
- `--agent` flag

**Files:**
```
cli/src/commands/task.rs   # Add TaskCommands::Run variant
cli/src/tasks/runner.rs    # NEW: task_run() main logic
```

**CLI command structure:**

```
TaskCommands.Run:
  Fields:
    - id: string (required)
        Task ID to run
    
    - agent: optional string (--agent flag)
        Override assignee agent
```

### Phase 4: Integration Testing

**Deliverables:**
- Integration test for task run with mock agent
- Test task state transitions
- Test error handling (agent failure)

**Files:**
```
cli/tests/test_task_run.rs   # Integration tests
```



---

## Use Cases

### Use Case 1: Manual Task Execution

```bash
# User has a task, delegates to agent
aiki task add "Fix null pointer bug in auth.ts" --assignee claude-code
# Created task: xqrmnpst

aiki task run xqrmnpst
# Agent works on the task...
# ✅ Task xqrmnpst completed
```

### Use Case 2: Review Task Execution

```bash
# Review command uses task run internally
aiki review @
# Creates review task xqrmnpst
# Calls: aiki task run xqrmnpst (internally)
# Agent executes digest + review subtasks
# Creates followup tasks if issues found
```

### Use Case 3: Flow-Triggered Auto-Fix

```yaml
# .aiki/flows/auto-fix.yml
review.completed:
  - if: $event.review.issues_found > 0 && $event.review.issues_found <= 3
    then:
      - log: "Auto-fixing ${event.review.issues_found} issues..."
      - task.run:
          task_id: $event.review.followup_task_id
      - log: "✅ Auto-fix complete"
  - else:
      - log: "Too many issues (${event.review.issues_found}), manual review required"
```

### Use Case 4: Iterative Review Loop

```yaml
# .aiki/flows/review-loop.yml
review.completed:
  - if: $event.review.loop_enabled
    then:
      - if: $event.review.issues_found > 0
        then:
          - log: "🔄 Review iteration ${event.review.loop_iteration}: ${event.review.issues_found} issues"
          - task.run:
              task_id: $event.review.followup_task_id
          - review:
              scope: $event.review.scope
              files: $event.review.files
              prompt: $event.review.prompt
              loop: true
        else:
          - log: "✅ Review loop approved after ${event.review.loop_iteration} iteration(s)"
```

---

## Open Questions (with Decisions)

| Question | Decision |
|----------|----------|
| **Task resumption** - Can a stopped task be resumed with `aiki task run` again? | ✅ Yes. Running a stopped task resumes it. |
| **Concurrent execution** - Can multiple tasks run simultaneously? | ⏸️ V1: No. V2 may support parallel execution. |
| **Session nesting** - Can a task spawn another task? | ✅ Yes, agent can call `aiki task run` recursively. No max depth enforced in V1. |
| **Agent failures** - How to handle crashes? | ✅ Task transitions to Stopped with error reason. See [Failure Handling](#failure-handling). |
| **Task dependencies** - Check blocking tasks? | ⏸️ V1: No. User/agent responsible for ordering. |
| **Context passing** - How is the prompt passed to agents? | ✅ Command line argument. See [Context Passing Mechanism](#context-passing-mechanism). |

---

## References

- Milestone 1.4: Task System
- `ops/now/code-review-task-native.md` - Review system using task run
- `ops/future/tasks/flow-actions.md` - Flow action integration
- Agent Client Protocol: Session spawning

---
status: draft
---

# Autorun: Automatic Task Start for Workflows and Loops

**Date**: 2026-02-16
**Status**: Draft
**Priority**: P2
**Depends on**: 
- `ops/now/semantic-blockers.md` (semantic blocking link types)
- `ops/now/spawn-tasks.md` (conditional task spawning)
- `ops/now/rhai-for-conditionals.md` (expression evaluation)

**Related Documents**:
- [Semantic Blocking Links](semantic-blockers.md) - Defines `validates`, `follows-up`, `depends-on` link types
- [Review-Fix Workflow](loop-flags.md) - Uses autorun with `validates` and `follows-up`
- [Conditional Task Spawning](spawn-tasks.md) - Uses autorun for spawned tasks

---

## Problem

Task workflows require automatic execution in several contexts:

### 1. Dependency Workflows
Task dependencies create workflows where tasks naturally follow one another:
- Reviews start after implementation completes
- Fixes start after reviews complete (if issues found)
- Tests start after features are implemented
- Deployments start after tests pass

### 2. Spawned Task Workflows
When tasks spawn followup tasks (via `spawns:` frontmatter), those spawned tasks often need to run immediately:
- Review spawns fix → fix should start automatically
- Build failure spawns notification → notification should run immediately

### 3. Loop Iterations
When tasks loop (re-spawn themselves until a condition is met), each iteration must start automatically:
- Fix → Review → Fix (loop until approved)
- Build → Test → Fix → Build (loop until tests pass)

Currently, these transitions require manual intervention — the user must remember to start the next task. This creates friction and breaks flow, especially in automated workflows.

**Example of the problem:**

```bash
# Manual workflow (current)
aiki task close <impl-id>
# ... user must remember ...
aiki task start <review-id>

# ... later ...
aiki task close <review-id>
# ... user must remember again ...
aiki task start <fix-id>
```

---

## Summary

Add **autorun** capability across all workflow contexts: blocking links, spawned tasks, and loops. When enabled, tasks automatically start when their preconditions are met.

**Key features:**
- **Opt-in per link/spawn** — `--autorun` flag or `autorun:` field in frontmatter
- **Works in all contexts** — blocking links, spawned tasks, loops
- **Safe defaults** — No autorun unless explicitly requested
- **Idempotent** — Safe to trigger multiple times (no-op if task already started/closed)

**Benefits:**
- **Reduced friction** — No need to manually start follow-up tasks
- **Better automation** — Enable hands-free workflows (e.g., build → review → fix)
- **Explicit intent** — Autorun flag documents expected workflow
- **Loop support** — Task chains can iterate until completion criteria met

---

## Design

Autorun is implemented in three distinct contexts, each with its own trigger mechanism but sharing the same core behavior: automatically starting tasks when preconditions are met.

### Link Direction Convention

Blocking links are stored on the **dependent task** (the one that is blocked) and point to the **blocker task** (the one that must complete first):

```
Task B  --depends-on-->  Task A    (link stored on B, B.link.target = A)
Task R  --validates-->   Task I    (link stored on R, R.link.target = I)
Task F  --follows-up-->  Task R    (link stored on F, F.link.target = R)
```

When Task A closes, we need a **reverse lookup**: find all tasks whose links point to Task A (i.e., tasks blocked by A). In the pseudocode below:
- `find_blocked_by(task_id)` — returns tasks that have a blocking link targeting `task_id`
- `task.blocking_links()` — returns the blocking links owned by a specific task (forward lookup)

### Context 1: Blocking Link Autorun

Add an `autorun` boolean field to all blocking link types:

```rust
pub enum LinkType {
    Validates {
        target: TaskId,   // The task being validated (blocker)
        autorun: bool,    // Start the owner of this link when target completes
    },

    FollowsUp {
        target: TaskId,   // The task being followed up on (blocker)
        autorun: bool,    // Start the owner of this link when target completes
    },

    DependsOn {
        target: TaskId,   // The task depended upon (blocker)
        autorun: bool,    // Start the owner of this link when target completes
    },
}
```

**Trigger:** When a task closes, find tasks blocked by it that have autorun enabled.

**Implementation:**

```rust
fn trigger_blocking_autorun(closed_task_id: &str, graph: &TaskGraph) {
    // Reverse lookup: find tasks that have blocking links targeting the closed task
    // (i.e., tasks that depend on the closed task)
    for candidate in find_blocked_by(closed_task_id) {
        if should_autorun(&candidate, graph) {
            start_task(candidate.id);
        }
    }
}
```

Note: `should_autorun` (defined in the [Multiple Blockers](#multiple-blockers) section) checks whether ANY of the candidate's blocking links has `autorun: true`, then verifies ALL blockers are closed via `is_blocked`. This ensures autorun triggers correctly regardless of which blocker closes last.

Both `trigger_blocking_autorun` and `trigger_spawn_autorun` (Context 2) are called from the unified `handle_task_close` entry point — see [Unified Entry Point](#unified-entry-point) below.

### Context 2: Spawned Task Autorun

Add an `autorun` boolean field to spawn configurations in frontmatter:

```yaml
---
spawns:
  - when: not approved
    task:
      template: aiki/fix
      autorun: true      # Start spawned task immediately
      data:
        max_iterations: 3
---
```

**Trigger:** When a task closes and spawn conditions evaluate true, create spawned tasks and auto-start them if `autorun: true`.

**Implementation:**

```rust
fn trigger_spawn_autorun(closed_task: &Task) {
    // Forward lookup: evaluate the closed task's own spawn conditions
    for spawn_config in closed_task.spawns {
        if evaluate_condition(&spawn_config.when, closed_task) {
            // Create spawned task
            let spawned_task_id = create_spawned_task(&spawn_config.task, closed_task);

            // Auto-start if configured
            if spawn_config.task.autorun {
                start_task(&spawned_task_id);
            }
        }
    }
}
```

### Unified Entry Point

Both autorun contexts are triggered from a single close handler:

```rust
fn handle_task_close(closed_task: &Task, graph: &TaskGraph) {
    // 1. Reverse lookup: auto-start tasks that were blocked by the closed task
    trigger_blocking_autorun(closed_task.id, graph);

    // 2. Forward lookup: evaluate the closed task's own spawn configs
    trigger_spawn_autorun(closed_task);
}
```

### Context 3: Loop Autorun

Loops use the spawn mechanism with `template: self` and automatically enable autorun:

```yaml
---
loop:
  until: subtasks.review.approved or data.loop.index1 >= 10
  data:
    custom_field: value
---
```

**Desugars to:**

```yaml
---
spawns:
  - when: not (subtasks.review.approved or data.loop.index1 >= 10)
    task:
      template: self
      autorun: true    # Always true for loops (implicit)
      data:
        custom_field: value
---
```

**Trigger:** When loop task closes, evaluate `loop.until`. If false (loop should continue), spawn next iteration with autorun enabled.

**Implementation:** Handled by spawn mechanism (Context 2) with special handling for `template: self`.

### Idempotency

Autorun must be safe to trigger multiple times:

- **Task already in progress** — No-op, don't restart
- **Task already closed** — No-op, don't reopen
- **Task stopped** — Restart it (user stopped it, but blocker completing means it can proceed)
- **Task open** — Start it (normal case)

```rust
fn start_task(task_id: &str) {
    let task = get_task(task_id)?;
    
    match task.status {
        TaskStatus::Open | TaskStatus::Stopped => {
            // Start the task (normal case or restart after stop)
            emit_started_event(task_id);
        }
        TaskStatus::InProgress | TaskStatus::Closed => {
            // No-op — already running or done
        }
    }
}
```

### CLI Syntax

**Blocking links:**

```bash
# Autorun when blocker completes
aiki task add "Review implementation" --validates <impl-id> --autorun
aiki task add "Fix issues" --follows-up <review-id> --autorun
aiki task add "Run tests" --depends-on <impl-id> --autorun

# No autorun (default) — manual start required
aiki task add "Deploy to prod" --depends-on <tests-id>  # No --autorun
```

**Spawned tasks (frontmatter):**

```yaml
---
spawns:
  - when: not approved
    task:
      template: aiki/fix
      autorun: true    # CLI equivalent: N/A (frontmatter only)
---
```

**Loops (frontmatter):**

```yaml
---
loop:
  until: data.loop.index1 >= 10
  # autorun: always enabled implicitly
---
```

### Default Behavior

| Context | Default | Rationale |
|---------|---------|-----------|
| **Blocking links** | `autorun: false` | Explicit opt-in for manual control |
| **Spawned tasks** | `autorun: false` | Explicit opt-in per spawn config |
| **Loops** | `autorun: true` (implicit) | Required for loop iteration |
| **`aiki review`** | `autorun: true` | High-volume command, reviews always follow impl |
| **`aiki review --fix`** | `autorun: true` | High-volume command, fixes always follow reviews |

**Examples:**

```bash
# Blocking links: No autorun by default
aiki task add "Task B" --depends-on <task-a>  # Requires explicit --autorun

# Review commands: Autorun by default
aiki review <task-id>              # Creates review with autorun: true
aiki review <task-id> --fix        # Creates review AND fix with autorun: true

# Opt-out for manual workflows
aiki review <task-id> --no-autorun
```

**Spawned tasks and loops:**

```yaml
---
# Spawned tasks: explicit autorun per config
spawns:
  - when: not approved
    task:
      template: aiki/fix
      autorun: true    # Explicit opt-in

# Loops: autorun always enabled (implicit)
loop:
  until: approved
  # autorun is implicit (always true)
---
```

---

## Interaction with Spawn Conditions

Conditional task creation (e.g., "only spawn fix if not approved") is handled by the `spawns` frontmatter mechanism (see `spawn-tasks.md`), not by autorun. Autorun only handles starting tasks that already exist.

**Example using spawns:**

```yaml
---
template: aiki/review
spawns:
  - when: not approved
    task:
      template: aiki/fix
      autorun: true
---
```

**Behavior:**
- Review closes with `approved=true` → No fix task created
- Review closes with `approved=false` → Fix task spawned and auto-started

---

## Examples

### Example 1: Review-Fix Workflow (Blocking Links)

```bash
# 1. Create implementation task
aiki task add "Implement auth feature"
aiki task start <impl-id>

# 2. Create review (autorun by default)
aiki review <impl-id>  # Creates review with validates link, autorun: true

# 3. Create fix (conditional spawn via review template)
# The review template uses spawns: {when: not approved, task: {template: aiki/fix, autorun: true}}

# Work on implementation
aiki task close <impl-id> --summary "Implemented auth"

# Review auto-starts immediately (no manual intervention)
# Codex agent performs review...

# If review approves:
aiki task close <review-id>  # No fix spawned (condition not met)

# If review finds issues:
aiki task close <review-id>  # Fix spawned and auto-started
```

### Example 2: Build Pipeline (Blocking Links)

```bash
# Create pipeline tasks
aiki task add "Run unit tests" --depends-on <impl-id> --autorun
aiki task add "Run integration tests" --depends-on <unit-tests-id> --autorun
aiki task add "Deploy to staging" --depends-on <integration-tests-id> --autorun
aiki task add "Deploy to production" --depends-on <staging-id>  # No autorun (manual gate)

# Start implementation
aiki task start <impl-id>
# ... work ...
aiki task close <impl-id>

# Chain reaction:
# 1. Unit tests auto-start
# 2. When unit tests pass → integration tests auto-start
# 3. When integration tests pass → staging deploy auto-starts
# 4. When staging succeeds → prod deploy becomes ready (waits for manual start)
```

### Example 3: Review Template with Spawn (Spawned Task Autorun)

```yaml
---
template: aiki/review
spawns:
  - when: not approved
    task:
      template: aiki/fix
      autorun: true      # Fix starts immediately after review closes
      data:
        max_iterations: 3
---
# Review {{source.task_id}}

Review the changes and close with approved=true or approved=false.
```

**Workflow:**

```bash
# Create review from template
aiki task add --template aiki/review

# Start and complete review
aiki task start <review-id>
aiki task close <review-id>  # Sets approved=false

# Fix task auto-spawns and auto-starts immediately
# (no manual intervention needed)
```

### Example 4: Fix-Review Loop (Loop Autorun)

```yaml
---
template: aiki/fix
loop:
  until: subtasks.review.approved or data.loop.index1 >= 10
  data: {}

subtasks:
  - slug: implement
    description: Make the fix
  - slug: review
    description: Review the fix
    template: aiki/review
---
# Fix {{source.task_id}} (Iteration {{data.loop.index1}})

Make fixes, then review. Loop until approved or max iterations reached.
```

**Workflow:**

```bash
# Create fix task from template
aiki task add --template aiki/fix

# Start fix (iteration 1)
aiki task start <fix-id>

# Complete iteration 1
aiki task close <fix-id>  # subtasks.review.approved = false

# Iteration 2 auto-spawns and auto-starts (loop continues)
# Complete iteration 2
aiki task close <fix-id-2>  # subtasks.review.approved = true

# Loop terminates (approved = true)
```

---

## Loop Metadata

The system automatically provides and increments `data.loop` metadata for loop iterations:

| Field | Type | Description |
|-------|------|-------------|
| `index` | int | Current iteration (0-indexed) |
| `index1` | int | Current iteration (1-indexed) |
| `first` | bool | Is this the first iteration? |
| `last` | bool | Is this the last iteration? (false until termination) |
| `length` | int? | Total iterations (null for dynamic loops) |

**Example usage in templates:**

```markdown
---
loop:
  until: data.loop.index1 >= 5
---
# Iteration {{data.loop.index1}} of {{data.loop.length}}

{{#if data.loop.first}}
This is the first iteration!
{{/if}}

{{#if data.loop.last}}
This is the final iteration!
{{/if}}

Progress: {{data.loop.index1}} / 5
```

**System behavior:**
- First iteration: `data.loop = {index: 0, index1: 1, first: true, last: false, length: null}`
- Subsequent iterations: System increments counters, sets `first: false`
- Final iteration: System sets `last: true` (when `loop.until` evaluates true)

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| **Blocking Links** | |
| Blocker closed as won't-do | Autorun still triggers (blocker "completed", even if skipped) |
| Blocker stopped (not closed) | No autorun (blocker not complete) |
| Autorun task already in progress | No-op (idempotent) |
| Autorun task already closed | No-op (idempotent) |
| Autorun task stopped | Restart it (blocker completing unblocks it) |
| Multiple autorun tasks on same blocker | All auto-start in parallel |
| Circular autorun dependencies | Prevented by cycle detection at link creation |
| Task has multiple blockers | Only autoruns when ALL blockers close |
| **Spawned Tasks** | |
| Multiple spawn conditions true | All spawned tasks created; those with `autorun: true` start immediately |
| Spawn condition evaluation error | Log warning, skip that spawn entry, continue with others |
| Spawned task template not found | Error logged, skip that spawn entry |
| Spawned task with `autorun: false` | Created but not started (manual start required) |
| **Loops** | |
| Loop condition always false | Runs until max spawn depth (e.g., 10 iterations) |
| Loop condition syntax error | Error logged, loop terminates (no spawn) |
| Nested loops | Each loop task tracks its own `data.loop` metadata |
| Loop with spawn conditions | Both loop spawn (self) and explicit spawns can trigger |

### Multiple Blockers

A task with multiple blocking links only autoruns when **all blockers** are closed:

```bash
aiki task add "Deploy" --depends-on <tests-id> --depends-on <review-id> --autorun
```

**Behavior:**
- Tests close → Deploy still blocked (review not done)
- Review closes → Deploy auto-starts (all blockers done)

**Implementation:**

```rust
fn should_autorun(candidate: &Task, graph: &TaskGraph) -> bool {
    // Forward lookup: check if this task has autorun enabled on ANY of its blocking links
    let has_autorun = candidate.blocking_links()
        .iter()
        .any(|link| link.autorun);

    if !has_autorun {
        return false;
    }

    // Only autorun if ALL blockers are closed
    !graph.is_blocked(candidate.id)
}
```

Note: `should_autorun` is a helper used by `handle_task_close` (see Context 1). The close handler performs the reverse lookup to find candidates, then calls this function on each one.

---

## Implementation Plan

### Phase 1: Core Autorun Infrastructure

1. **Add `autorun: bool` field** to blocking link types (`Validates`, `FollowsUp`, `DependsOn`)
2. **Add `autorun` field** to spawn configuration struct
3. **Implement idempotent `start_task()` function** — only start if task is Open or Stopped
4. **Update link storage** to persist autorun flag
5. **Update spawn config storage** to persist autorun flag

### Phase 2: Blocking Link Autorun

1. **Implement `trigger_blocking_autorun`** — reverse lookup for autorun candidates (see [Context 1](#context-1-blocking-link-autorun))
2. **Implement `should_autorun` helper** — check ANY link for autorun, verify ALL blockers closed (see [Multiple Blockers](#multiple-blockers))
3. **Add `--autorun` flag** to `task add` command:
   ```rust
   #[arg(long)]
   pub autorun: bool,
   ```

4. **Default autorun for review commands**:
   - `aiki review <task-id>` → `autorun: true`
   - `aiki review <task-id> --fix` → `autorun: true`
   - Add `--no-autorun` flag to disable

### Phase 3: Spawned Task Autorun

1. **Parse `spawns:` frontmatter** with `autorun` field (depends on spawn-tasks.md implementation)
2. **Implement `trigger_spawn_autorun`** — evaluate closed task's spawn conditions and auto-start (see [Context 2](#context-2-spawned-task-autorun))
3. **Wire up unified `handle_task_close`** entry point that calls both `trigger_blocking_autorun` and `trigger_spawn_autorun` (see [Unified Entry Point](#unified-entry-point))

### Phase 4: Loop Autorun

1. **Parse `loop:` frontmatter** with `until` condition
2. **Desugar loop to spawn config**:
   ```rust
   fn desugar_loop(loop_config: &LoopConfig) -> SpawnConfig {
       SpawnConfig {
           when: format!("not ({})", loop_config.until),
           task: SpawnTaskConfig {
               template: "self".to_string(),
               autorun: true,  // Always true for loops
               data: loop_config.data.clone(),
           }
       }
   }
   ```

3. **Implement `data.loop` metadata tracking**:
   - Initialize on first iteration: `{index: 0, index1: 1, first: true, last: false}`
   - Increment on each spawn: `index += 1`, `index1 += 1`, `first = false`
   - Detect termination: Set `last: true` when `until` condition becomes true

4. **Max spawn depth guard** — prevent infinite loops (e.g., max 10 iterations)

### Phase 5: Tests

1. **Unit tests**:
   - Autorun flag persistence and link creation
   - Loop desugaring logic
   - `data.loop` metadata generation

2. **Integration tests**:
   - Blocking link autorun triggering on task close
   - Spawned task autorun
   - Loop iteration with autorun
   - Multiple blockers (autorun only when all close)
   - Conditional skip interaction with autorun

3. **Edge case tests**:
   - Stopped tasks restarting
   - Idempotency (multiple triggers)
   - Max spawn depth prevention
   - Loop condition errors

---

## Files Changed

| File | Change |
|------|--------|
| `cli/src/tasks/graph.rs` | Add `autorun` field to blocking link types; add spawn config structs |
| `cli/src/tasks/types.rs` | Add `LoopConfig` and `SpawnConfig` structs |
| `cli/src/commands/task.rs` | Add `--autorun` and `--no-autorun` flags |
| `cli/src/commands/review.rs` | Default `autorun: true` for review/fix |
| `cli/src/tasks/lifecycle.rs` | Add autorun triggers in task close handler (blocking links + spawns) |
| `cli/src/tasks/templates/parser.rs` | Parse `loop:` and `spawns:` frontmatter |
| `cli/src/tasks/templates/resolver.rs` | Desugar `loop:` to spawn config; generate `data.loop` metadata |
| Tests | Add autorun behavior tests (all contexts) |

---

## Success Criteria

### Blocking Link Autorun
- [ ] Autorun field added to all blocking link types
- [ ] `--autorun` flag works with `task add` commands
- [ ] Task close handler triggers autorun for linked tasks
- [ ] Autorun is idempotent (safe to trigger multiple times)
- [ ] Multiple blockers handled correctly (autorun only when all close)
- [ ] `aiki review` and `aiki review --fix` default to autorun
- [ ] Conditional skip takes precedence over autorun for `follows-up`

### Spawned Task Autorun
- [ ] `spawns:` frontmatter parsed with `autorun` field
- [ ] Spawned tasks with `autorun: true` start immediately
- [ ] Spawned tasks with `autorun: false` remain in ready state
- [ ] Multiple spawn conditions can trigger simultaneously

### Loop Autorun
- [ ] `loop:` frontmatter parsed with `until` condition
- [ ] Loop desugars to self-spawn with `autorun: true`
- [ ] `data.loop` metadata generated and incremented correctly
- [ ] Loop termination works (when `until` evaluates true)
- [ ] Max spawn depth guard prevents infinite loops
- [ ] Templates can combine `loop:` + explicit `spawns:`

### General
- [ ] All tests passing
- [ ] No regressions in task lifecycle
- [ ] Documentation updated

---

## Open Questions

1. **Should autorun be per-link or per-task?** — Current design is per-link, which allows different downstream tasks to have different autorun behavior. Alternative: per-task flag that applies to all blocking links.

2. **Should autorun respect assignees?** — If a task is assigned to a different agent, should autorun still trigger? Might create conflicts if wrong agent starts the task.

3. **Autorun output?** — Should closing a task show which tasks were auto-started? E.g., "Closed task X. Auto-started: Y, Z"

4. **Batch autorun?** — If closing multiple tasks at once (batch close), should autorun happen for each, or defer until all are closed?

5. **Autorun hooks?** — Should there be a hook that fires before autorun, allowing users to intercept/customize behavior?

6. **Loop nesting depth:** Should `data.loop` be a stack for nested loops? Or does each loop task track its own `data.loop`?

7. **Max spawn depth configuration:** Should max loop iterations be configurable per-loop or globally?

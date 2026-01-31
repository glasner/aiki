# Aiki Plan and Build Commands

**Date**: 2026-01-29
**Status**: Draft
**Purpose**: Plan and build implementation from spec files via `aiki plan` and `aiki build`

**Related Documents**:
- [Review and Fix Commands](review-and-fix.md) - Similar pipeable pattern with `aiki review | aiki fix`
- [Task Templates](../done/task-templates.md) - Template system for task creation
- [Task Execution: aiki task run](../done/run-task.md) - Agent runtime and task execution

---

## Executive Summary

Two new commands:

- **`aiki plan <spec-path>`** - Agent reads spec, creates implementation task with subtasks, outputs impl task ID
- **`aiki build <spec-path>`** - Plan + run in one command (equivalent to `aiki plan | aiki task run`)

The implementation task's template handles subtask orchestration — the agent running the impl task calls `aiki task run` for each subtask. This keeps `aiki task run` simple and allows different orchestration strategies (sequential, parallel, conditional).

This mirrors the `aiki review | aiki fix` pattern:

| Phase | Review Flow | Plan/Build Flow |
|-------|-------------|-----------------|
| Analysis | `aiki review` → agent adds comments | `aiki plan` → agent creates impl task |
| Execution | `aiki fix` → runs followup tasks | `aiki build` → agent runs subtasks |

**Pipeable:**
```bash
aiki build ops/now/feature.md | aiki review | aiki fix
```

---

## Commands

### `aiki plan`

Creates a planning task that reads a spec and produces an implementation task with subtasks.

```bash
aiki plan <spec-path> [options]
```

**Arguments:**
- `<spec-path>` - Path to spec file (e.g., `ops/now/my-feature.md`)

**Options:**
- `--async` - Run planning asynchronously, return planning task ID immediately
- `--start` - After planning, start implementation task (calling agent takes over)
- `--template <name>` - Planning task template (default: `aiki/plan`)
- `--agent <name>` - Agent for planning (default: `claude-code`)

**Behavior (default):**
1. Create planning task from `aiki/plan` template with `source: file:<spec-path>`
2. Run planning task via `aiki task run`
   - Agent reads spec file
   - Agent creates implementation task with subtasks
   - Agent closes planning task
3. Find implementation task via `source: task:<planning_id>`
4. Output **implementation task ID** to stdout

**Behavior (`--start`):**
1-3. Same as default
4. Start implementation task via `aiki task start` — calling agent takes over
5. Output implementation task ID

**Output (stdout when piped):**
```
impl5678
```

**Output (stderr):**
```xml
<aiki_plan cmd="plan" status="ok">
  <completed planning_task="plan1234" impl_task="impl5678" subtasks="5">
    Planning completed. Implementation task created with 5 subtasks.

    1. Add JWT library dependency
    2. Create auth middleware
    3. Add login endpoint
    4. Add token validation
    5. Write tests

    Run: aiki task run impl5678
  </completed>
</aiki_plan>
```

---

### `aiki build`

Convenience command that runs `aiki plan` then `aiki task run` on the result.

```bash
aiki build <spec-path> [options]
```

**Arguments:**
- `<spec-path>` - Path to spec file (e.g., `ops/now/my-feature.md`)

**Options:**
- `--async` - Run asynchronously, return implementation task ID immediately
- `--template <name>` - Planning task template (default: `aiki/plan`)
- `--agent <name>` - Agent for planning/building (default: `claude-code`)

Note: No `--start` flag — use `aiki plan --start` if you want the calling agent to take over.

**Behavior:**
1. Run `aiki plan <spec-path>` — creates planning task, runs it, outputs impl task ID
2. Run `aiki task run <impl-task-id>` — runs each subtask sequentially
3. Output implementation task ID to stdout

Equivalent to: `aiki plan spec.md | aiki task run`

**Output (stdout when piped):**
```
impl5678
```

**Output (stderr):**
```xml
<aiki_build cmd="build" status="ok">
  <completed impl_task="impl5678" subtasks="5" duration_ms="120000">
    Build completed successfully.

    1. Add JWT library dependency (done)
    2. Create auth middleware (done)
    3. Add login endpoint (done)
    4. Add token validation (done)
    5. Write tests (done)
  </completed>
</aiki_build>
```

---

## How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│  aiki build <spec-path>  (or: aiki plan | aiki task run)        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: aiki plan                                             │
│  ─────────────────                                              │
│  1. Creates planning task from aiki/plan template               │
│     • source: file:<spec-path>                                  │
│                                                                 │
│  2. Runs planning task via aiki task run                        │
│     • Agent reads spec file                                     │
│     • Agent creates implementation task with subtasks           │
│     • Agent closes planning task                                │
│                                                                 │
│  3. Finds implementation task via source: task:<planning_id>    │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 2: aiki task run <impl-task>                             │
│  ──────────────────────────────────                             │
│  4. Spawns agent session for implementation task                │
│                                                                 │
│  5. Agent follows template instructions to run subtasks:        │
│     • aiki task run subtask.1                                   │
│     • aiki task run subtask.2                                   │
│     • aiki task run subtask.3                                   │
│                                                                 │
│  6. Agent closes implementation task when done                  │
│                                                                 │
│  7. Returns implementation task ID                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key**: The implementation task's template tells the agent how to orchestrate subtasks. This allows different strategies (sequential, parallel, conditional) without changing `aiki task run`.

---

## Use Cases

### 1. Full Pipeline

```bash
aiki build ops/now/feature.md | aiki review | aiki fix
```

### 2. Plan and Inspect Before Running

```bash
# Plan only
aiki plan ops/now/risky-refactor.md
# Returns: impl5678

# Inspect the plan
aiki task show impl5678

# Approve and run
aiki task run impl5678
```

### 3. Agent Plans Mid-Chat

```bash
# Agent decides to plan before implementing
aiki plan ops/now/complex-feature.md
# Returns: impl5678

# Agent or human runs later
aiki task run impl5678
```

### 4. Agent Takes Over After Planning

```bash
# Plan, then calling agent takes over implementation
aiki plan ops/now/feature.md --start
```

---

## Task Templates

### Planning Template (`aiki/plan`)

**Location:** `.aiki/templates/aiki/plan.md`

```markdown
---
version: 1.0.0
type: plan
assignee: claude-code
---

# Plan: {data.spec_path}

Read the spec file and create an implementation task with subtasks.

## Instructions

1. Read the spec file at `{data.spec_path}` using the Read tool
2. Understand the requirements and identify implementation steps
3. Create an implementation task with orchestration instructions:
   ```
   aiki task add "Implement: <spec title>" \
     --source task:{task.id} \
     --source file:{data.spec_path}
   ```

   The implementation task instructions should tell the executing agent to:
   - Run each subtask sequentially via `aiki task run <subtask_id>`
   - Close the implementation task when all subtasks complete

4. For each implementation step, add a subtask:
   ```
   aiki task add "<step description>" --parent <impl_task_id>
   ```
5. Close this planning task:
   ```
   aiki task close
   ```

## Guidelines

- The implementation task MUST have `--source task:{task.id}` so plan can find it
- Include orchestration instructions in the implementation task (how to run subtasks)
- Each subtask should be a discrete, actionable step
- Include enough context in subtask names for the executing agent
- Order subtasks logically (dependencies first)
```

**Note**: The planning agent writes orchestration instructions into the implementation task. This allows different orchestration strategies — sequential, parallel, or conditional execution of subtasks.

---

## Task Relationships

```
Planning Task (type: plan)
  source: file:ops/now/feature.md
  ↓
  Agent creates:
  ↓
Implementation Task
  source: task:<planning_id>
  source: file:ops/now/feature.md
  ├── Subtask 1
  ├── Subtask 2
  └── Subtask 3
```

---

## Comparison with Review/Fix

| Aspect | `aiki review` | `aiki plan` |
|--------|---------------|-------------|
| Input | Task ID (what to review) | Spec file path |
| Agent produces | Comments on task | Implementation task with subtasks |
| Output | Review task ID | **Implementation task ID** |

| Aspect | `aiki fix` | `aiki task run` (enhanced) |
|--------|------------|----------------------------|
| Input | Review task ID | Task ID |
| Finds | Comments on review task | Subtasks of task |
| Executes | Creates followup tasks, runs them | Runs existing subtasks |
| Output | Followup task ID | Task ID |

Note: `aiki plan` outputs the **implementation task ID** (not planning task ID), so it pipes directly to `aiki task run`.

---

## Implementation Plan

### Phase 1: Plan Command

**Deliverables:**
- `aiki plan <spec-path>` command
- `--async`, `--start`, `--template`, `--agent` flags
- Create planning task from template
- Run planning task
- Find and output implementation task ID
- `--start`: start impl task (agent takes over)

**Files:**
- `cli/src/commands/plan.rs` - New module
- `cli/src/commands/mod.rs` - Export plan module

### Phase 2: Build Command

**Deliverables:**
- `aiki build <spec-path>` command
- `--async`, `--template`, `--agent` flags (no `--start`)
- Internally runs `aiki plan` then `aiki task run`

**Files:**
- `cli/src/commands/build.rs` - New module
- `cli/src/commands/mod.rs` - Export build module

### Phase 3: Template

**Deliverables:**
- `.aiki/templates/aiki/plan.md` - Default planning template

**Files:**
- `.aiki/templates/aiki/plan.md`

### Phase 4: Flow Integration

**Deliverables:**
- `plan:` flow action
- `build:` flow action
- Sugar triggers: `plan.completed`, `build.completed`

**Files:**
- `cli/src/flows/types.rs` - Add action variants
- `cli/src/flows/engine.rs` - Add handlers
- `cli/src/flows/sugar.rs` - Add sugar triggers

---

## Open Questions

1. **Plan without spec file** - Should `aiki plan` work with inline descriptions?
   - Recommendation: Future enhancement. For now, require a spec file.

2. **Orchestration strategy** - Should the planning template prescribe a default orchestration (sequential), or leave it flexible?
   - Recommendation: Default template uses sequential execution, custom templates can vary.

---

## Summary

**Two new commands:**

| Command | Input | Does | Output |
|---------|-------|------|--------|
| `aiki plan` | Spec file | Agent creates impl task with subtasks | Impl task ID |
| `aiki build` | Spec file | Plan + run to completion | Impl task ID |

**Orchestration**: The implementation task's instructions tell the executing agent how to run subtasks. This keeps `aiki task run` simple and allows different orchestration strategies per template.

**Pipelines:**
```bash
# Build from spec (most common)
aiki build spec.md

# Full workflow
aiki build spec.md | aiki review | aiki fix

# Plan only (for inspection)
aiki plan spec.md

# Plan and agent takes over
aiki plan spec.md --start

# Plan, inspect, then run
aiki plan spec.md
aiki task show impl5678
aiki task run impl5678

# Async build
aiki build spec.md --async | aiki wait | aiki review | aiki fix
```

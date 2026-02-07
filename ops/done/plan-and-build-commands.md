# Implementation: Plan and Build Commands

**Date**: 2026-02-05
**Status**: Ready for implementation
**Phase**: 2 of 3
**Related**: [Workflow Commands Overview](workflow-commands.md), [Template Conditionals](template-conditionals.md)

---

## Overview

Implement `aiki plan` and `aiki build` - workflow commands that transform specs into implementation plans and execute them.

**Key insight:** These commands provide two distinct workflows:

1. **`aiki plan`** - Planning workflow (inspect before executing)
   - Creates a plan task with subtasks
   - Returns immediately so you can review/modify the plan
   - Start subtasks manually when ready

2. **`aiki build`** - Automated workflow (one-shot execution)
   - Creates plan and executes all subtasks automatically
   - Useful for fully-automated builds or CI/CD

## Command Comparison

| Feature | `aiki plan` | `aiki build` |
|---------|-------------|--------------|
| Creates plan task | ✓ | ✓ |
| Auto-executes subtasks | ✗ | ✓ |
| Returns immediately | ✓ | ✗ (waits for completion) |
| Review before execution | ✓ | ✗ (unless `--plan-only`) |
| Use case | Interactive planning | Automated execution |

---

## `aiki plan` Command

Creates an implementation plan (task with subtasks) from a spec file. Returns immediately so you can inspect and modify the plan before execution.

### Command Syntax

```bash
aiki plan <spec-path> [options]       # Create plan from spec
aiki plan show <spec-path-or-plan-id>      # Show existing plan
```

**Arguments:**
- `<spec-path>` - Path to spec file (e.g., `ops/now/my-feature.md`)
- `<spec-path-or-plan-id>` - Spec file path or plan task ID (32 lowercase letters)

**Options:**
- `--restart` - Ignore existing plan, create new one from scratch
- `--template <name>` - Planning template (default: `aiki/plan`)
- `--agent <name>` - Agent for planning (default: `claude-code`)

### Behavior

1. Check for existing plan with `data.spec=<spec-path>`
2. If incomplete plan exists: show interactive prompt (resume vs start fresh)
3. If no plan or `--force`: create planning task from `aiki/plan` template
4. Agent reads spec and creates plan task with subtasks
5. Return plan task ID to stdout
6. User can then:
   - Review with `aiki task show <plan-id>`
   - Modify subtasks manually
   - Start work with `aiki task start <plan-id>.1`

### Examples

```bash
# Create a plan from spec
aiki plan ops/now/add-auth.md
# Output: plan_id="xyxkluynlnonltwtprxupswknoquuvnz"

# Review the plan
aiki plan show xyxkluynlnonltwtprxupswknoquuvnz

# Start working on first subtask
aiki task start xyxkluynlnonltwtprxupswknoquuvnz

# Check plan status (by spec path)
aiki plan show ops/now/add-auth.md

# Check plan status (by plan ID)
aiki plan show xyxkluynlnonltwtprxupswknoquuvnz

# Create fresh plan (ignore existing)
aiki plan ops/now/add-auth.md --restart
```

### Workflow Example

```bash
# Step 1: Create the plan
$ aiki plan ops/now/feature.md

Plan created: nzwtoqqrluppzupttosl

  1. Add database schema
  2. Create API endpoints  
  3. Add middleware
  4. Write tests
  5. Update documentation

Review: aiki task show nzwtoqqrluppzupttosl
Execute: aiki build nzwtoqqrluppzupttosl

# Step 2: Review and modify if needed
$ aiki task show nzwtoqqrluppzupttosl

# Step 3: Execute the plan
$ aiki build nzwtoqqrluppzupttosl
```

**This is much cleaner than:**
```bash
# Old complex workflow
OUTPUT=$(aiki build ops/now/feature.md --plan-only)
PLAN_ID=$(echo "$OUTPUT" | grep -oP 'plan_id="\K[^"]+')
aiki task show $PLAN_ID
```

### Output Format

**Stdout (machine-readable):**
```xml
<aiki_plan plan_id="nzwtoqqrluppzupttosl"/>
```

**Stderr (human-readable):**
```
Plan created: nzwtoqqrluppzupttosl

  1. Add database schema
  2. Create API endpoints
  3. Add middleware
  4. Write tests
  5. Update documentation

Review:  aiki task show nzwtoqqrluppzupttosl
Execute: aiki build nzwtoqqrluppzupttosl
```

### Existing Plan Handling

When running `aiki plan <spec-path>`, check for existing plans with `data.spec=<spec-path>`:

| Plan Status | `--restart`? | Behavior |
|-------------|------------|----------|
| None found | - | Create new plan |
| `pending` or `in_progress` | No | **Interactive prompt** (see below) |
| `pending` or `in_progress` | Yes | Close as `wont_do`, **revert file changes**, create new plan |
| `completed` | - | Create new plan (start fresh implementation) |

**Interactive prompt for incomplete plans:**

```
Incomplete plan exists for this spec.

Plan: nzwtoqqrluppzupttosl (3/5 subtasks done)
  ├── [✓] Add database schema
  ├── [✓] Create API endpoints
  ├── [✓] Add middleware
  ├── [ ] Write tests
  └── [ ] Update documentation

  1. Resume this plan
  2. Start fresh (closes existing plan)

Choice [1-2]:
```

- Press `1` → return existing plan ID (no-op)
- Press `2` → equivalent to `aiki plan <spec-path> --restart`

**Non-interactive mode:** If stdin is not a TTY, error with message showing command options.

### File Revert Behavior (--restart)

When `--restart` is used with an incomplete plan, the command reverts all file changes made by completed subtasks before creating a new plan. This ensures a clean slate for the new implementation attempt.

**Implementation:** Uses `aiki task undo <plan-id> --completed` internally. See [task-undo-command.md](task-undo-command.md) for complete design including:
- Conflict detection and error handling
- Backup branch creation for safety
- Baseline computation via JJ revsets
- File restoration logic

**Example:**

```bash
# Plan has 3 completed subtasks, 2 pending
aiki plan ops/now/feature.md --restart

Output:
  Creating backup: aiki/undo-backup-nzwtoqqr

  Undoing 3 completed subtasks from plan nzwtoqqr:
    ✓ Subtask 1: Add database schema
    ✓ Subtask 2: Create API endpoints
    ✓ Subtask 3: Add middleware

  Files reverted (6):
    src/db/schema.rs
    src/api/endpoints.rs
    src/middleware.rs
    (+ 3 more)

  Creating new plan...
  Plan created: pqrstuxy...
```

**Error Handling:** If conflicts are detected (e.g., files manually edited after subtask completion), the command aborts with a helpful error message. User must resolve conflicts manually before retrying.

### Planning Template

**Location:** `.aiki/templates/aiki/plan.md`

```markdown
---
version: 1.0.0
type: plan
assignee: claude-code
---

# Plan: {{data.spec}}

**Goal**: Read the spec file and create an implementation plan with subtasks.

## Create Implementation Plan

Read the spec file at `{{data.spec}}` and understand:
- What is the goal/vision?
- What are the requirements?
- What are the constraints?
- Are there open questions that need resolving?

Identify the implementation steps needed. Each step should be a discrete, testable change.

## Create Plan Task

Create the plan task (a container for subtasks):

```bash
aiki task add "Plan: <spec title from the spec file>" \
  --source task:{{task.id}} \
  --source file:{{data.spec}} \
  --data spec={{data.spec}}
```

The command outputs the task ID on stdout (single line, e.g., `xyxkluynlnonltwtprxupswknoquuvnz`). Capture this as the plan task ID.

**Spec title extraction:** Use the first H1 heading (`# Title`) from the spec file. If no H1 found, use the filename without extension.

## Add Subtasks

For each implementation step identified, create a subtask:

```bash
aiki task add "<step description>" --parent <plan_task_id>
```

**Guidelines for subtasks:**
- Each subtask should be discrete and actionable
- Include enough context for the executing agent
- Order subtasks logically (dependencies first)
- Keep subtask names concise but descriptive

## Close Planning Task

When all subtasks are added, close this planning task and report the plan:

```bash
aiki task close --comment "Plan created with N subtasks. Plan ID: <plan_task_id>"
```

Output the plan task ID and subtask summary for the user.
```

### Deliverables

- `cli/src/commands/plan.rs` - New module with plan and show subcommands
- `cli/src/commands/mod.rs` - Export plan module
- `.aiki/templates/aiki/plan.md` - Planning template

---

## `aiki build` Command

Creates a plan from a spec file and automatically executes all subtasks. Useful for fully-automated builds.

### Command Syntax

```bash
aiki build <spec-path> [options]    # Create plan from spec, then execute
aiki build <plan-id> [options]      # Execute existing plan
aiki build show <spec-path>         # Show build/plan status
```

**Arguments:**
- `<spec-path>` - Path to spec file (e.g., `ops/now/my-feature.md`)
- `<plan-id>` - Task ID of existing plan (32 lowercase letters)

**Argument detection:** If argument is 32 lowercase letters → plan ID; otherwise → spec path.

**Options:**
- `--async` - Run asynchronously, return build task ID immediately
- `--restart` - Ignore existing plan, create new one from scratch
- `--template <name>` - Build task template (default: `aiki/build`)
- `--agent <name>` - Agent for build orchestration (default: `claude-code`)

**Note:** Build tasks are ephemeral — they exist only for the lifetime of the command. Plans are persistent.

### Behavior

**From spec file (`aiki build <spec-path>`):**
1. Clean up stale builds for this spec (see "Stale Build Cleanup")
2. Check for existing plan (see "Existing Plan Handling")
3. Create build task from `aiki/build` template
4. If no existing plan: agent reads spec, creates plan with subtasks
5. Agent executes incomplete plan subtasks in order
6. On exit: close build task, mark unfinished build subtasks as `wont_do`
7. Output build task ID to stdout

**From plan ID (`aiki build <plan-id>`):**
1. Verify plan task exists and has subtasks
2. Create build task with `data.plan=<plan-id>` (skip plan creation phase)
3. Agent executes incomplete plan subtasks in order
4. On exit: close build task
5. Output build task ID to stdout

### Examples

```bash
# Build from spec (creates plan, executes all subtasks)
aiki build ops/now/add-dark-mode.md

# Resume interrupted build (continues from incomplete subtasks)
aiki build ops/now/add-dark-mode.md

# Execute an existing plan directly
aiki build nzwtoqqrluppzupttosl

# Start fresh, ignore existing plan
aiki build ops/now/feature.md --restart

# Build asynchronously (returns immediately)
aiki build ops/now/feature.md --async

# Show build/plan status
aiki build show ops/now/feature.md
```

### Two-Phase Workflow: Plan then Build

If you want to review before executing:

```bash
# Phase 1: Create the plan
aiki plan ops/now/feature.md
# Output: plan_id="nzwtoqqrluppzupttosl"

# Phase 2: Review the plan
aiki plan show nzwtoqqrluppzupttosl

# Phase 3: Execute the plan
aiki build nzwtoqqrluppzupttosl
```

This is cleaner than `--plan-only` flag and shell parsing.

### Task Lifecycle

**Build tasks are ephemeral:**
- Created when command starts
- Closed when command exits (success, failure, or interrupt)
- Unfinished build subtasks marked `wont_do` on exit
- No orphaned in_progress builds

**Plan tasks are persistent:**
- Created once per spec (unless `--force`)
- Subtasks track actual implementation progress
- Survive across multiple build invocations
- Re-running `aiki build` resumes from incomplete subtasks

### Stale Build Cleanup

On startup, find and close any stale builds for this spec:
- Query builds with `data.spec=<spec-path>` and status `in_progress` or `pending`
- Close each as `wont_do` with comment "Stale build cleaned up"
- This handles builds orphaned by crashes or signals

### Existing Plan Handling

When running `aiki build <spec-path>`, check for existing plans with `data.spec=<spec-path>`:

| Plan Status | `--restart`? | Behavior |
|-------------|------------|----------|
| None found | - | Create new plan |
| `pending` or `in_progress` | No | **Interactive prompt** (see below) |
| `pending` or `in_progress` | Yes | Close as `wont_do`, **revert file changes**, create new plan |
| `completed` | - | Create new plan (start fresh implementation) |

**When running `aiki build <plan-id>` (explicit plan ID):**
- No prompt - automatically continues executing the specified plan
- You explicitly chose the plan, so no need to ask

**Interactive prompt for `aiki build <spec-path>` when incomplete plan exists:**

```
Incomplete plan exists for this spec.

Plan: nzwtoqqrluppzupttosl (3/5 subtasks done)
  ├── [✓] Add database schema
  ├── [✓] Create API endpoints
  ├── [✓] Add middleware
  ├── [ ] Write tests
  └── [ ] Update documentation

  1. Resume this plan
  2. Start fresh (closes existing plan)

Choice [1-2]:
```

- Press `1` → equivalent to `aiki build nzwtoqqrluppzupttosl`
- Press `2` → equivalent to `aiki build <spec-path> --restart`

**Non-interactive mode:** If stdin is not a TTY (piped input), error with message showing both command options instead of prompting.

**File Revert Behavior:** When `--restart` is used, file changes from completed subtasks are reverted before creating a new plan. See the "File Revert Behavior (--restart)" section under `aiki plan` for detailed algorithm, conflict handling, and implementation notes.

### Output Format

**Stdout (machine-readable):**
```xml
<aiki_build build_id="build9012" plan_id="plan5678"/>
```

**Stderr (human-readable):**
```xml
<aiki_build cmd="build" status="ok">
  <completed build_task="build9012" plan_task="plan5678" subtasks="5" duration_ms="120000">
    Build completed successfully.

    Plan: plan5678
    1. Add JWT library dependency (done)
    2. Create auth middleware (done)
    3. Add login endpoint (done)
    4. Add token validation (done)
    5. Write tests (done)
  </completed>
</aiki_build>
```

### Build Template

**Location:** `.aiki/templates/aiki/build.md`

```markdown
---
version: 1.0.0
type: build
---

# Build: {{data.spec}}

**Overall Goal**: Execute plan to implement the spec.

## Subtasks

{% if not data.plan %}
### Create Plan
No existing plan. Create one using the plan command:

```bash
aiki plan {{data.spec}}
```

This command will:
- Read and analyze the spec file
- Create a plan task with subtasks
- Output the plan task ID

Capture the plan task ID and store it in the task metadata:

```bash
# Capture plan ID (from XML output: <aiki_plan plan_id="..."/>)
PLAN_ID=<extracted-plan-id>

# Store it in build task data for future reference
aiki task update {{task.id}} --data plan=$PLAN_ID
```
{% endif %}

### Execute Subtasks

Execute each subtask of the plan task sequentially:

```bash
aiki task run <plan_task_id>.<subtask number>)
...
```

Run them in order. If a subtask fails do not continue, stop and report the failure using the following command:

```bash
aiki task stop {{task.id}} --comment "Failed subtask <subtask_number>: <reason>"
```

...

### Close Build Task

When all plan subtasks are complete, close this build task:

```bash
aiki task close {{task.id}} --comment "Build completed: all N subtasks done. Plan task: <plan_task_id>"
```
```

### Build Show Subcommand

Shows the build task and plan task for a spec file.

```bash
aiki build show <spec-path>
```

**Behavior:**
1. Find build task with `data.spec=<spec-path>` (most recent if multiple)
2. Find associated plan task via `source: task:<build_id>`
3. Display both tasks with subtask status

**Output:**
```
Build Task: build-xyz (orchestrator)
  Status: in_progress
  Started: 2 min ago

Plan Task: plan-abc
  Source: ops/now/add-auth.md
  Progress: 3/5 subtasks

  ├── [✓] Add database schema
  ├── [✓] Create API endpoints
  ├── [✓] Add middleware
  ├── [ ] Write tests
  └── [ ] Update documentation
```

### Error Handling

| Scenario | Behavior |
|----------|----------|
| Spec file doesn't exist | Error: "Spec file not found: <path>" |
| Invalid spec file (not .md) | Error: "Spec file must be markdown (.md)" |
| Incomplete plan exists (TTY) | Interactive prompt: resume or start fresh |
| Incomplete plan exists (non-TTY) | Error with command suggestions |
| Plan task creation fails | Close build, return error with agent output |
| Subtask execution fails | Close build (mark remaining build subtasks `wont_do`), return failed subtask ID |
| Command interrupted (Ctrl+C) | Close build (mark remaining build subtasks `wont_do`), plan preserved |
| Agent crashes | Build left stale; cleaned up on next `aiki build` invocation |
| Async build timeout | Return task ID, let it run in background |
| Plan ID not found | Error: "Plan not found: <id>" |

### Deliverables

- `cli/src/commands/build.rs` - New module with build and show subcommands
- `cli/src/commands/mod.rs` - Export build module
- `.aiki/templates/aiki/build.md` - Build orchestration template

---

## Task Relationships

```
Spec Task (type: spec)
  source: prompt:<user-request>
  artifact: ops/now/feature.md
  ↓
  ├─→ Planning Task (ephemeral, used by aiki plan)
  │     Creates Plan Task below
  │
  └─→ Build Task (ephemeral, used by aiki build)
        data.spec: ops/now/feature.md
        Creates/uses Plan Task below
  ↓
Plan Task (persistent)
  source: file:ops/now/feature.md
  data.spec: ops/now/feature.md
  ├── Subtask 1: Add database schema
  ├── Subtask 2: Create API endpoints
  └── Subtask 3: Add tests
```

---

## Implementation Notes

### `aiki plan` Implementation

1. **Planning task lifecycle:**
   - Created when command starts
   - Runs planning agent to create plan task with subtasks
   - Closes immediately after plan is created
   - Returns plan task ID to stdout

2. **Finding and handling existing plans:**
   - Query tasks with `data.spec=<spec-path>`
   - If incomplete plan found: show interactive prompt (resume vs start fresh)
   - If `--restart`: close existing plan as `wont_do`, revert file changes, create new one
   - If plan completed: create new plan (new implementation cycle)
   - Non-TTY: error with command suggestions instead of prompting

3. **File revert implementation (`--restart`):**
   - Use `aiki task undo <plan-id> --completed` to revert all completed subtask changes
   - See [task-undo-command.md](task-undo-command.md) for detailed design
   - `task undo` handles: conflict detection, backup creation, baseline computation, file restoration
   - If conflicts detected: abort with helpful error (user resolves manually)
   - After successful undo: close old plan as `wont_do`, create new plan

4. **Plan show implementation:**
   - If argument is 32 lowercase letters → plan task ID (direct lookup)
   - Otherwise → spec path (query for plan task with `data.spec=<spec-path>`, most recent)
   - Display plan with subtask status

### `aiki build` Implementation

1. **Build task lifecycle:**
   - Build task is created when command starts
   - Build task is ALWAYS closed when command exits
   - On exit: mark unfinished build subtasks as `wont_do`, close build as `completed`
   - Stale builds (orphaned by crashes) are cleaned up on next invocation

2. **Finding and handling existing plans:**
   - Query tasks with `data.spec=<spec-path>`
   - If incomplete plan found: show interactive prompt (resume vs start fresh)
   - If `--restart`: close existing plan as `wont_do`, revert file changes, create new one
   - If plan completed: create new plan (new implementation cycle)
   - Non-TTY: error with command suggestions instead of prompting

3. **File revert implementation (`--restart`):**
   - Use `aiki task undo <plan-id> --completed` to revert completed subtask changes
   - See [task-undo-command.md](task-undo-command.md) for detailed design
   - Revert behavior applies when restarting from spec path, not when building explicit plan ID

4. **Plan task creation (done by build agent):**
   - Build agent reads spec, creates plan task with subtasks
   - Sets `source: file:<spec-path>` for traceability
   - Sets `data.spec=<spec-path>` for querying
   - No need to store start commit - JJ revsets compute baseline via `parents(roots(task=<id>))`

5. **`--async` behavior:**
   - Create build task with `--background` flag (spawns detached agent process)
   - Return build task ID immediately to stdout
   - Build agent runs in background, updating task status as it progresses
   - User can check status with `aiki build show <spec-path>`
   - Implementation: Uses existing `aiki task run --background` infrastructure

6. **Build show implementation:**
   - Query for plan task with `data.spec=<spec-path>` (most recent)
   - Query for build tasks associated with that plan
   - Display plan with subtask status, recent build info

---

## Prerequisites

- Task query by `data.<key>=<value>` OR `source: file:<path>` must exist to find tasks by spec path
- [Template conditionals](template-conditionals.md) must be implemented (for `{% if %}` syntax in build template)
- [`aiki task undo`](task-undo-command.md) must be implemented for file revert functionality
- `aiki task update --data <key>=<value>` must be implemented to store plan ID in build task
- `aiki task update --blocked-by <task-id>` for subtask dependencies (optional for v1)

**Note:** Task data can be set at creation time via `aiki task add --data key=value` (already supported) or updated later via `aiki task update --data key=value` (needs implementation).

---

## Files to Create/Modify

### New Files
- `cli/src/commands/plan.rs` - Plan command implementation
- `cli/src/commands/build.rs` - Build command implementation
- `.aiki/templates/aiki/plan.md` - Planning template
- `.aiki/templates/aiki/build.md` - Build orchestration template

### Modified Files
- `cli/src/commands/mod.rs` - Export plan and build modules

---

## Future Enhancements (v2)

**Parallel Execution:**
- Execute independent subtasks in parallel based on blocking relationships
- `aiki build --parallel` flag

**Build Caching:**
- Skip subtasks if inputs haven't changed
- `aiki build --cached`

**Plan Editing:**
- `aiki plan edit <plan-id>` to interactively modify subtasks
- Add, remove, reorder subtasks before execution

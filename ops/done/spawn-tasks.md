# Conditional Task Spawning: `spawns` frontmatter + `spawned-by` link

**Date**: 2026-02-16
**Status**: Draft
**Priority**: P1
**Depends on**: 
- Template system (variable substitution, conditionals)
- Rhai expression evaluation (for spawn conditions)

**Related Documents**:
- [Review-Fix Workflow](loop-flags.md) - Uses spawn mechanism for fix workflows
- [Semantic Blocking Links](semantic-blockers.md) - Replaces `spawned-by` with semantic link types
- [Rhai for Conditionals](rhai-for-conditionals.md) - Expression evaluation infrastructure
- [Autorun](autorun-unblocked-tasks.md) - Adds autorun and loop behavior on top of spawning (depends on this spec)

**Scope**:
This spec covers **conditional task creation** via `spawns:` frontmatter. When a task closes, spawn conditions are evaluated, and new tasks are **created** on-demand if conditions are met. This spec focuses purely on the **spawning mechanism** (creating tasks with links and data). Starting spawned tasks automatically (autorun) and loop iterations are covered in [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md).

---

## Problem

Currently, followup tasks (like fixes after review) must be created manually after determining they're needed. There's no declarative way to express "create this task if condition X is true when the parent task completes."

**Issues with manual creation:**
1. **Requires human intervention** — Someone must remember to create the followup task
2. **Inconsistent workflows** — Different agents/humans may handle followups differently
3. **Lost context** — Time delay between parent completion and followup creation
4. **No automation** — Can't express "always create fix task when review fails"

---

## Solution: `spawns` Frontmatter

Instead of creating tasks upfront with skip conditions, declare **spawn intent** in the parent task's frontmatter. Tasks are created on-demand when conditions evaluate true.

### Design

```yaml
---
template: aiki/review
spawns:
  - when: not approved
    task:
      template: aiki/fix
  - when: data.issues_found > 3
    task:
      template: aiki/follow-up
      data:
        issue_count: data.issues_found
---
# Review {{source.task_id}}

Review the changes and determine if fixes are needed.
```

**When the review task closes:**
1. Evaluate each spawn condition against the task's final state
2. Separate spawn entries into `subtask:` and `task:` groups
3. **If any `subtask:` conditions are true:**
   - Create only the subtasks (this automatically reopens the spawner)
   - Skip all `task:` spawns (even if their conditions are true)
   - Rationale: Spawner is now managing subtasks, can't also spawn standalone followups
4. **If no `subtask:` conditions are true:**
   - Create all `task:` spawns where conditions are true
   - Spawner remains closed

**For each created spawn:**
- Instantiate the specified template
- Create the new task in "open" state
- Add a `spawned-by` link on the new task pointing back to the spawner
- Pass configured data to the spawned task

**Result:**
- Clean backlog (no tasks until needed)
- Positive semantics ("create if" vs "won't do if")
- Just-in-time creation (tasks materialize exactly when needed)
- Clear provenance (`spawned-by` link tracks origin)
- Subtasks take precedence over standalone spawns (avoid conflicting workflows)

**Note**: To automatically start spawned tasks, see the `autorun` field in [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md).

---

## Spawn Configuration

### Frontmatter Schema

```yaml
spawns:
  - when: String           # Rhai expression evaluated against task state
    task:                  # Spawned task configuration (creates standalone task)
      template: String     # Template identifier (e.g., "aiki/fix")
      priority: String     # Task priority (p0/p1/p2/p3, default: inherit from spawner)
      assignee: String     # Task assignee (default: from template)
      data:                # Custom data fields
        key: value
  - when: String           # Another spawn condition
    subtask:               # Subtask configuration (creates task with spawner as parent)
      template: String     # Template identifier
      priority: String     # Optional priority override
      assignee: String     # Optional assignee override
      data:                # Custom data fields
        key: value
```

**Note**: The `autorun` field for automatically starting spawned tasks is defined in [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md).

### Task vs Subtask

Each spawn entry can use **either** `task:` **or** `subtask:`:

| Field | Parent Relationship | Use Case |
|-------|-------------------|----------|
| `task:` | **No parent** (standalone task) | Followup tasks, review workflows, independent tasks spawned based on conditions |
| `subtask:` | **Spawner becomes parent** | Breaking spawner into child tasks, decomposition workflows |

**Examples:**

```yaml
# Spawn a standalone followup task (no parent)
spawns:
  - when: not approved
    task:
      template: aiki/fix

# Spawn a subtask (spawner becomes parent)
spawns:
  - when: data.needs_decomposition
    subtask:
      template: aiki/implementation
      data:
        focus_area: data.area_name
```

### Precedence: Subtasks Block Standalone Tasks

**Important**: If any `subtask:` spawns are created, all `task:` spawns are skipped (even if their conditions are true).

**Example with mixed spawns:**

```yaml
spawns:
  - when: not approved
    task:
      template: aiki/fix         # Standalone followup
  
  - when: data.needs_breakdown
    subtask:
      template: aiki/analysis    # Decompose into subtask
```

**Scenario 1**: `approved=false`, `needs_breakdown=false`
- Result: Creates `aiki/fix` task (standalone)
- Spawner stays closed

**Scenario 2**: `approved=false`, `needs_breakdown=true`
- Result: Creates `aiki/analysis` subtask only
- Spawner **reopens** (now managing subtask)
- `aiki/fix` task is **skipped** (even though `not approved` is true)

**Rationale**: A task can't be both "done and spawning followups" and "reopened to manage subtasks". Subtasks take precedence because they represent decomposition of the spawner itself.

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `when` | String | Yes | Rhai expression evaluated on task close; spawn if true |
| `task` / `subtask` | Map | Yes (one required) | Configuration for the spawned task; use `task:` for standalone or `subtask:` for parent-child |
| `*.template` | String | Yes | Template to instantiate (e.g., `aiki/fix`, `aiki/follow-up`) |
| `*.priority` | String | No (default: inherit) | Priority (p0/p1/p2/p3); inherits from spawner if not set |
| `*.assignee` | String | No (default: template) | Assignee; uses template's assignee if not set |
| `*.data` | Map<String, Value> | No | Key-value data passed to spawned task's context (see [Data Value Evaluation](#data-value-evaluation)) |

### Condition Evaluation Context

**Evaluation timing**: Spawn conditions are evaluated against the **post-transition state** of the task. By the time `when` expressions run, the task's status has already been set to `closed` and the `outcome` has been recorded. This means:
- `status` is always `closed` (since spawns only trigger on close)
- `outcome` is always populated (`done` or `wont_do`)
- `approved`, `data.*`, and other fields reflect their final values at close time

This applies identically to both `when` conditions and `data` value expressions — they share the same post-transition evaluation context.

The `when` expression has access to:

| Variable | Type | Description |
|----------|------|-------------|
| `approved` | bool | Whether task was approved |
| `status` | String | Task status — always `closed` in spawn evaluation (post-transition) |
| `outcome` | String | Close outcome (`done` or `wont_do`) |
| `data.*` | Various | Any data fields set on the task (e.g., `data.issues_found`) |
| `comments` | Array | Task comments |
| `priority` | String | Task priority (p0/p1/p2/p3) |
| `subtasks.{slug}.*` | Task | Reference to subtask by slug |

**Example conditions:**
```yaml
when: not approved                         # Spawn if not approved
when: data.issues_found > 0                # Spawn if any issues
when: data.issues_found > 5                # Spawn only if many issues
when: outcome == 'done' && data.needs_follow_up # Complex logic
when: priority == 'p0'                     # Spawn for critical tasks
when: not subtasks.review.approved         # Spawn if review subtask not approved
when: subtasks.test.data.issues_found > 3  # Spawn if test found many issues
```

### Data Value Evaluation

Values in `task.data` are **Rhai expressions** evaluated against the spawner's state at close time — the same evaluation context as `when` conditions. This means:

| Value syntax | Type | Example | Evaluates to |
|-------------|------|---------|-------------|
| Integer/float literal | Number | `max_iterations: 3` | `3` |
| String literal | String | `label: "urgent"` | `"urgent"` |
| Boolean literal | Bool | `needs_review: true` | `true` |
| Variable reference | Various | `issue_count: data.issues_found` | Value of spawner's `data.issues_found` |
| Expression | Various | `is_critical: priority == "p0"` | `true` or `false` |

**Evaluation rules:**
1. Each value is parsed and evaluated as a Rhai expression
2. The evaluation context is identical to `when` conditions (access to `approved`, `status`, `outcome`, `data.*`, `subtasks.*`, etc.)
3. YAML scalars that are valid Rhai expressions are evaluated; quoted YAML strings are parsed as Rhai string literals
4. Allowed result types: `String`, `i64`, `f64`, `bool`, `Array`, `Map` — other types are rejected

**Error behavior:** If a `task.data` value fails to evaluate (undefined variable, type error, disallowed result type):
- The spawn entry is **skipped** (task is not created)
- A warning is logged with the spawner ID, spawn index, key name, and error
- Other spawn entries in the same `spawns:` array are unaffected

**Rationale:** Skipping on error (rather than substituting a default) prevents spawning tasks with silently wrong data. The template author should fix the expression.

### Data Flow Through Spawning

**1. Evaluating `when` conditions (at spawn time):**
- Access spawner's current state: `approved`, `status`, `outcome`, `priority`, etc.
- Access spawner's data: `data.issues_found`, `data.custom_field`, etc.
- Access spawner's subtasks: `subtasks.review.approved`, etc.

**2. Setting spawned task attributes:**
```yaml
spawns:
  # Spawn a standalone fix task (no parent)
  - when: not approved
    task:
      template: aiki/fix
      priority: p0                # Override: make fix urgent
      assignee: claude-code       # Override template assignee
      data:
        max_iterations: 3         # Static value
        issue_count: data.issues_found  # Copy spawner's data
  
  # Spawn a followup task (no parent)
  - when: data.issues_found > 3
    task:
      template: aiki/follow-up
      # priority: inherits from spawner (not specified)
      # assignee: uses template default (not specified)
      data:
        issue_count: data.issues_found
  
  # Spawn a subtask (spawner becomes parent)
  - when: data.needs_breakdown
    subtask:
      template: aiki/detailed-analysis
      data:
        area: data.complex_area
```

**3. In spawned task template:**
```markdown
---
template: aiki/fix
---
# Fix {{spawner.id}}

Max iterations: {{data.max_iterations}}
Issue count: {{data.issue_count}}

Context from spawner:
- Spawner ID: {{spawner.id}}
- Spawner status: {{spawner.status}}
- Spawner priority: {{spawner.priority}}
- Spawner approved: {{spawner.approved}}
- Original issues: {{spawner.data.issues_found}}
```

**Key points:**
- `when` evaluates against spawner's state at close time
- `task.priority` inherits from spawner if not specified; can override
- `task.assignee` uses template default if not specified; can override
- `task.data` explicitly passes values to spawned task's `data.*` namespace
- Spawned task can access spawner via `{{spawner.*}}` in templates
- No implicit data inheritance - must explicitly pass via `task.data`

---

## Link Type: `spawned-by`

### Schema

```rust
enum LinkType {
    SpawnedBy {
        task_id: TaskId,     // The task that spawned this one
    },
    // ... other link types
}
```

**Note**: The `autorun` field is defined in [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md) as part of the autorun mechanism.

### Semantics

- **Direction**: Child → Parent (spawned task points to spawner)
- **Purpose**: Provenance tracking - records that this task was automatically created by another task
- **Created**: Automatically when spawn condition evaluates true
- **Stored**: On the spawned task's links
- **Queryable**: Bidirectional
  - "What spawned task X?" → traverse X's `spawned-by` link
  - "What did task X spawn?" → find all tasks with `spawned-by` pointing to X

### Parent Relationship

- **`task:` spawns**: Get only a `spawned-by` link (no parent relationship)
- **`subtask:` spawns**: Get both a `spawned-by` link **and** a parent relationship
  - The spawner becomes the parent task
  - Subtask IDs follow parent-child naming: `<parent-id>.<index>`
  - Index allocation is deterministic (see [Subtask Index Allocation](#subtask-index-allocation))
  - Subtasks appear under parent in task listings

### Example

```yaml
# Review task (after closing with approved=false)
id: abc123
links:
  - type: validates
    task_id: original-task-id
# (No forward "spawns" link — derive via query)

# Fix task (automatically spawned from review using `task:`)
id: def456
parent: null                  # No parent (standalone task)
links:
  - type: spawned-by
    task_id: abc123           # Provenance: created by review abc123

# Analysis subtask (spawned from review using `subtask:`)
id: abc123.1                  # Child ID follows parent naming
parent: abc123                # Parent relationship
links:
  - type: spawned-by
    task_id: abc123           # Provenance: created by review abc123
```

**Note:** In [semantic-blockers.md](semantic-blockers.md), spawned tasks will have BOTH `spawned-by` (for provenance) AND a semantic link type like `remediates`, `validates`, or `depends-on` (for relationship semantics).

---

## Reference Model for Subtask Access

When referencing subtasks in conditions and templates:

| Context | Syntax | Example | Description |
|---------|--------|---------|-------------|
| **Parent template frontmatter** | `subtasks.{slug}.*` | `subtasks.review.approved` | Direct reference from parent to its subtasks |
| **Subtask frontmatter** | `parent.subtasks.{slug}.*` | `parent.subtasks.build.status` | Reference to sibling subtasks via parent |

### Examples

**Parent template with spawn conditions:**
```yaml
---
template: aiki/review
spawns:
  - when: not subtasks.review.approved     # Parent context: subtasks.{slug}
    task:
      template: aiki/fix
  - when: subtasks.test.data.issues_found > 5   # Parent context: subtasks.{slug}
    task:
      template: aiki/alert
---
```

**Subtask referencing siblings:**
```yaml
---
slug: deploy
depends-on: parent.subtasks.build
when: parent.subtasks.test.status == 'closed'
---
```

**Subtask with complex sibling conditions:**
```yaml
---
slug: cleanup
when: parent.subtasks.deploy.status == 'closed' && parent.data.cleanup_needed
---
```

---

## Workflow Example: Review → Fix

### Before (manual creation)

```bash
# Create review
aiki review <task-id>

# Work on review, close it
aiki task close <review-id> --summary "Found 3 issues"

# Manually create fix task after review
aiki task add "Fix issues from review" --parent <task-id>
aiki task start <fix-id>
```

**Problems:**
- Requires manual followup after review
- Easy to forget to create fix task
- Inconsistent handling across different reviewers

### After (with `spawns`)

```bash
# Create review (template has spawns: defined)
aiki review <task-id>

# Work on review, close it
aiki task close <review-id> --summary "Found 3 issues"
# (approved=false implicitly set)

# Fix task automatically created (in "open" state)
# Clean backlog until review completes
# To auto-start: see autorun-unblocked-tasks.md
```

**Benefits:**
- No manual fix creation
- Backlog stays clean until needed
- Clear spawn provenance via `spawned-by` link

### Example: Task vs Subtask Spawning

**Scenario**: A planning task that spawns both a standalone implementation task and subtasks for validation.

```yaml
---
template: aiki/plan
spawns:
  # Standalone implementation task (no parent)
  - when: status == 'closed' && outcome == 'done'
    task:
      template: aiki/implement
      data:
        plan_ref: this.id
  
  # Subtasks for validation (parent = planner)
  - when: data.needs_security_review
    subtask:
      template: aiki/security-check
      priority: p0
  
  - when: data.needs_performance_review
    subtask:
      template: aiki/perf-check
---
```

**Result when plan closes successfully with both flags set:**

```bash
aiki task list

# Standalone tasks:
- impl-task-id — Implement plan-task-id (spawned-by: plan-task-id)

# plan-task-id (parent):
  - plan-task-id.1 — Security check (spawned-by: plan-task-id)
  - plan-task-id.2 — Performance check (spawned-by: plan-task-id)
```

**Use `task:` when:**
- Creating followup work that's independent
- Spawning review/fix workflows
- Creating tasks that should appear at top-level

**Use `subtask:` when:**
- Breaking down the spawner into smaller pieces
- Creating validation/verification steps for the spawner
- Work that's conceptually "part of" the spawner

---

## Template Integration

### Review Template with Spawning

```markdown
---
template: aiki/review
version: "1.0.0"
assignee: claude-code
spawns:
  - when: not approved
    task:
      template: aiki/fix
      data:
        max_iterations: 3
---
# Review {{source.task_id}}

Review the changes for task `{{source.task_id}}`.

## Instructions

1. Run `aiki task diff {{source.task_id}}`
2. Review all changes
3. Add comments for any issues found
4. Close this task:
   - If approved: `aiki task close {{this.id}} --summary "LGTM"`
   - If issues: `aiki task close {{this.id}} --summary "Found N issues"`

## Evaluation

- **Approved**: Set `approved=true` if clean, else `false`
- **Issues Found**: Count issues in comments

---

# Fix Spawning

If `approved=false`, a fix task will be automatically created.
(To automatically start it, configure `autorun` in autorun-unblocked-tasks.md)
```

### Fix Template (Spawned)

```markdown
---
template: aiki/fix
version: "1.0.0"
assignee: claude-code
---
# Fix {{spawner.id}}

Address the issues found in review `{{spawner.id}}`.

## Instructions

1. Run `aiki task show {{spawner.id}}` to see review comments
2. Fix each issue
3. Close when all issues addressed

## Context

- **Spawned by**: {{spawner.id}}
- **Original task**: {{spawner.links.validates.task_id}}
- **Max iterations**: {{data.max_iterations | default: 1}}
```

**Variable context for spawned tasks:**
- `{{spawner.*}}` — Spawning task fields
- `{{data.*}}` — Data passed from spawn config
- `{{this.*}}` — Current task fields

---

## Implementation Plan

### Phase 1: Core Spawn Mechanism

1. **Add `spawns` field to task frontmatter parser**
   - Parse `spawns:` array from YAML
   - Validate: `when` (required), `task` OR `subtask` (one required, mutually exclusive)
   - Validate `task`/`subtask` fields: `template` (required), `priority`, `assignee`, `data` (optional)

2. **Implement condition evaluation on task close**
   - Build Rhai evaluation context from post-transition task state (status=closed, outcome set)
   - Evaluate each spawn condition
   - Separate results into `subtask:` spawns and `task:` spawns
   - If any `subtask:` conditions are true, skip all `task:` spawns
   - Collect templates to instantiate

3. **Implement template instantiation with spawn context**
   - Instantiate specified template
   - Pass `spawner` variable to template
   - Apply `task.data` to spawned task
   - Set `task.priority` (inherit from spawner if not specified)
   - Set `task.assignee` (use template default if not specified)
   - For `subtask:` spawns: set parent relationship to spawner

4. **Create `spawned-by` link**
   - Add `SpawnedBy` variant to `LinkType` enum
   - Store on spawned task
   - **Note:** [semantic-blockers.md](semantic-blockers.md) will later add semantic links (`validates`, `remediates`, `depends-on`) in addition to `spawned-by`

5. **Implement idempotent spawn execution**
   - Generate deterministic `spawn_key` from `hash(spawner_id, spawn_index)`
   - Store `spawn_key` on spawned task metadata
   - Check for existing `spawn_key` before creating (dedupe)
   - For `subtask:` spawns: assign indices deterministically (see [Subtask Index Allocation](#subtask-index-allocation))
   - Perform close + spawn as a single atomic commit

6. **Leave spawned tasks in "open" state**
   - Spawned tasks are created but not started
   - Starting behavior is handled by the autorun system (see [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md))

### Phase 2: Query Support

1. **Add `spawned-by` link queries**
   - `get_spawned_by(task_id)` → spawner
   - `get_spawned_tasks(task_id)` → children

2. **Add CLI support**
   - `aiki task show <id>` displays spawned tasks
   - `aiki task graph` visualizes spawn relationships

**Note**: Template integration (review/fix templates with `spawns:`) is covered in [loop-flags.md](loop-flags.md) as part of the review-fix workflow implementation.

---

## Edge Cases

### Multiple Spawn Conditions True

If multiple spawn configs evaluate true, all are instantiated:

```yaml
spawns:
  - when: data.issues_found > 0
    task:
      template: aiki/fix
  - when: priority == 'p0' && not approved
    task:
      template: aiki/urgent-fix
```

Both tasks are created if conditions are met.

### Spawn Recursion

A spawned task can itself have `spawns:` config. This enables chains:
- Review spawns Fix
- Fix spawns Re-review
- Re-review spawns Another-fix

**Guard**: Set max spawn depth (e.g., 10 levels) to prevent infinite recursion.

**Note**: For loop-like behavior (task spawning itself repeatedly with auto-start), see loop functionality in [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md).

### Idempotency and Exactly-Once Spawning

Close operations may be retried or replayed (e.g., failed write retry, crash recovery). Spawning must be idempotent — re-executing a close must not create duplicate tasks.

**Deterministic spawn key**: Each spawn entry gets a deterministic key derived from the spawner ID and the spawn's index in the `spawns:` array:

```
spawn_key = hash(spawner_id, spawn_index)
```

**Dedupe check**: Before creating a spawned task, check if a task with the same `spawn_key` already exists (stored on the spawned task's metadata). If it does, skip creation.

**Close + spawn consistency**: The close operation uses a two-phase write model:

1. **Phase 1 — Spawn creation**: Each spawned task is created via `create_from_template` + `write_link_event`, writing individual JJ commits. These happen first so that failures don't leave the spawner in an inconsistent closed state.
2. **Phase 2 — Atomic batch write**: The close event, reopen events (for subtask spawns), and `_spawns_failed` metadata are written in a single JJ commit. This ensures close and reopen are always consistent.

**Failure modes and recovery**:
- If Phase 1 fails (spawn creation): No spawned tasks exist, Phase 2 hasn't run, spawner stays open. Clean state.
- If Phase 1 succeeds but Phase 2 fails: Spawned tasks exist but spawner isn't closed. On retry, `spawn_key` dedup skips existing spawns, and Phase 2 writes the close. Final state is correct.
- If both phases succeed: Nominal path, everything consistent.

**Why not a single commit?** `create_from_template` handles template loading, ID generation, subtask creation, and link writes — refactoring it to return events instead of writing them would require changes across all template-based task creation paths. The current model provides eventual consistency via `spawn_key` dedup with a narrow failure window between phases.

**Schema addition** (on spawned task metadata):
```yaml
_spawn_key: String  # Deterministic key for idempotent spawn creation
```

### Subtask Index Allocation

When `subtask:` spawns create child tasks, their IDs follow the `<parent-id>.<index>` convention. Index allocation must be deterministic across retries.

**Rules:**

1. **Index assignment is based on spawn entry position**: Each `subtask:` spawn entry in the `spawns:` array has a fixed position (0-indexed among all entries, not just subtask entries). The subtask index is derived from this position plus any pre-existing subtasks.

2. **Counting excludes spawner-created children**: The offset N counts only non-spawn children (those without a `_spawn_key` matching this spawner). This ensures N is stable across retries — if a previous attempt partially succeeded, those spawner-created children don't shift the base index.

3. **Ordering among subtask spawns**: Subtask entries are assigned indices in the order they appear in the `spawns:` array. If entries at positions 1 and 3 are `subtask:` spawns (and both conditions are true), the first gets index N+1, the second gets N+2.

4. **Skipped entries don't consume indices**: If a `subtask:` spawn's condition evaluates to false, it does not consume an index. Only spawns that are actually created get indices assigned.

5. **Retries produce identical indices**: The base offset N is stable because spawner-created children are excluded from the count. Combined with `spawn_key` dedup (which skips already-created spawns), retries produce the same index assignments even after partial success.

**Example:**

```yaml
spawns:
  - when: not approved          # index 0 in spawns array — task: spawn (skipped if subtasks win)
    task:
      template: aiki/fix
  - when: data.needs_analysis   # index 1 — subtask spawn
    subtask:
      template: aiki/analysis
  - when: data.needs_perf       # index 2 — subtask spawn
    subtask:
      template: aiki/perf-check
```

If spawner has 0 existing subtasks and both subtask conditions are true:
- `aiki/analysis` → `<parent-id>.1` (spawn_key = hash(spawner_id, 1))
- `aiki/perf-check` → `<parent-id>.2` (spawn_key = hash(spawner_id, 2))

If only `data.needs_perf` is true:
- `aiki/perf-check` → `<parent-id>.1` (spawn_key = hash(spawner_id, 2))

Note: the `spawn_key` always uses the entry's position in the `spawns:` array (not the subtask index), so it remains stable regardless of which other conditions are true.

### Condition Evaluation Errors

If a condition fails to evaluate (syntax error, undefined variable):
- Log warning with task ID and condition
- Skip that spawn entry
- Continue with other spawn entries

### Spawning with No Template Match

If `template` doesn't exist:
- Error: "Template 'aiki/fix' not found"
- Skip spawn entry
- Log warning

---

## Comparison to Beads

Beads doesn't have a `spawns` link type in its 19 dependency types. Instead, it uses:

| Beads | Aiki (proposed) |
|-------|-----------------|
| **Molecules** — Proto/Mol/Wisp phases for template instantiation | `spawns:` frontmatter for conditional instantiation |
| **Dynamic bonding** — Runtime child creation with `bd mol bond` | Spawn happens on task close, evaluated via condition |
| **`waits-for`** — Gate coordination for dynamic children | Deferred to [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md) |
| **`delegated-from`** — Delegation chain | `spawned-by` — Spawn provenance |

Aiki's spawn system is more declarative (frontmatter-based) vs beads' imperative molecule commands.

---

## Benefits

1. **Clean backlog** — No tasks until needed
2. **Positive semantics** — "create if X" vs "won't do if !X"
3. **Just-in-time creation** — Tasks materialize when conditions met
4. **Clear provenance** — `spawned-by` link tracks origin
5. **Flexible conditions** — Full Rhai expressions for complex logic
6. **Template-driven** — Spawn logic lives in templates, not CLI
7. **Queryable** — Spawn relationships are first-class graph links

---

## Open Questions

1. **Should spawn depth be configurable?** (e.g., via `max_spawn_depth` in config)
2. **Should we support spawn on events other than close?** (e.g., on start, on comment)
3. ~~**Should spawned tasks inherit priority from spawner?**~~ **Resolved**: Yes, inherit from spawner when `task.priority` is not specified. This is consistent with the schema, field table, data flow section, and success criteria. Templates can still override by setting `task.priority` explicitly.
4. **Should we validate conditions at template parse time?** (or only at evaluation time)
5. ~~**Loop nesting depth**~~ **Moved**: This question is about loop behavior, not spawning. Deferred to [autorun-unblocked-tasks.md](autorun-unblocked-tasks.md).

---

## Alternatives Considered

### 1. `followup_if` on links instead of frontmatter

**Rejected**: Conditions are about the task's state, not about the relationship. Frontmatter is the right place.

### 2. `on_close` hooks instead of `spawns`

**Rejected**: Hooks are imperative (side effects). `spawns` is declarative (intent). Declarative is easier to reason about and test.

### 3. Both `spawns` (forward) and `spawned-by` (backward) links

**Rejected**: Redundant. Store only `spawned-by` on child, derive forward via query.

---

## Success Criteria

- [ ] Review templates can spawn fix tasks conditionally
- [ ] Fix tasks have `spawned-by` link pointing to review
- [ ] Spawned tasks are created in "open" state (not started)
- [ ] Conditions support full Rhai expressions
- [ ] `aiki task show` displays spawned tasks
- [ ] Backlog stays clean (no tasks until spawn condition met)
- [ ] Multiple spawn conditions can trigger simultaneously
- [ ] Spawned tasks inherit priority from spawner (unless overridden)
- [ ] Spawned tasks can access spawner data via `{{spawner.*}}` variables
- [ ] Max spawn depth guard prevents infinite recursion
- [ ] Condition evaluation errors are logged and don't block other spawns
- [ ] Template not found errors are handled gracefully
- [ ] Spawning works independently of autorun (autorun is a separate feature)
- [ ] Spawn execution is idempotent (retrying close does not create duplicate tasks)
- [ ] Close + reopen is atomic; spawn creation is idempotent with automatic recovery on retry
- [ ] `task:` spawns create standalone tasks (no parent relationship)
- [ ] `subtask:` spawns create tasks with spawner as parent
- [ ] Subtask spawns get parent-child ID naming (`<parent-id>.<index>`)
- [ ] Both `task:` and `subtask:` spawns get `spawned-by` link for provenance
- [ ] If any `subtask:` spawns are created, all `task:` spawns are skipped
- [ ] Creating subtasks automatically reopens the spawner (existing aiki behavior)
- [ ] Precedence logic ensures spawner can't be both closed and managing subtasks

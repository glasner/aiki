# `needs-context` Link & `--next-session`

**Date**: 2026-02-25
**Status**: Draft
**Purpose**: Add `needs-context` link type for session-shared subtask chains, and replace `--next-subtask` with the needs-context-aware `--next-session`.

**Related Documents**:
- [graph.rs](../../cli/src/tasks/graph.rs) - Task graph and link types
- [runner.rs](../../cli/src/tasks/runner.rs) - Task execution runner
- [types.rs](../../cli/src/tasks/templates/types.rs) - Template subtask frontmatter types
- [implement-command.md](implement-command.md) - Lanes & orchestration (depends on this plan)

---

## Executive Summary

Two related changes that form the foundation for session-aware task orchestration:

1. **`needs-context` link type** — a new link between sibling subtasks meaning "must run in the same agent session, after the predecessor completes." Implies `depends-on`. Transitive chains form multi-task sessions.

2. **`--next-session` replaces `--next-subtask`** — the new flag is needs-context-aware. When the next ready task heads a `needs-context` chain, `--next-session` runs the whole chain in one agent session (scoped). For standalone tasks, behavior is identical to the old `--next-subtask`.

These are independent of lanes and orchestration — they work with any template that loops `--next-session`, including existing fix/build workflows.

---

## `needs-context` Link Type

### Problem

When subtasks run in separate sessions (e.g., via `--next-subtask` loops), context built up in one subtask is lost for the next:

```
Fix Task
  ├── Explore issues        ← reads review, understands codebase
  ├── Plan remediation      ← needs explore's mental model
  ├── Implement fixes       ← needs plan's decisions
  └── Review fix quality    ← independent, can be new session
```

The explore → plan → implement chain should share a session. The agent that explored has context that would be lost if a new agent starts fresh for planning.

### Solution

A new link type: `needs-context`.

**Via CLI:**
```bash
aiki task link <plan> --needs-context <explore>
aiki task link <implement> --needs-context <plan>
```

**Via template subtask frontmatter** (references sibling via `subtasks.<slug>` namespace):
```markdown
## Explore Issues
---
slug: explore
---
Review the code and identify problems...

## Plan Remediation
---
slug: plan
needs-context: subtasks.explore
---
Based on your exploration, create a remediation plan...

## Implement Fixes
---
slug: implement
needs-context: subtasks.plan
---
Implement the fixes according to the plan...

## Review Fix Quality
---
slug: review
---
Review the changes made (independent — no context needed)...
```

### Semantics

- `A needs-context B` means: A must run in the same agent session as B, after B completes
- **Implies `depends-on`** — A can't start before B finishes
- **Transitive** — if A needs-context B and B needs-context C, then C → B → A form one session
- **Sibling constraint** — must be between subtasks of the same parent
- **No cycles** — validated at link creation time

### Template frontmatter

Uses the same `subtasks.<slug>` namespace used elsewhere in templates (e.g., `subtasks.review.data.approved`):

- `needs_context: Option<String>` added to `SubtaskFrontmatter` in `types.rs`
- Value is a `subtasks.<slug>` reference (e.g., `subtasks.explore`)
- Links are materialized after all subtasks are created during template instantiation
- Validation: referenced slug must exist in the same template, no cycles

### Graph helpers

- `graph.get_needs_context_chain(task_id)` — given any task in a chain, returns the ordered list of all tasks in that chain (head to tail)
- `graph.is_needs_context_head(task_id)` — true if this task is the first in a `needs-context` chain (nothing needs-context it)

---

## `--next-session` (Replaces `--next-subtask`)

### The problem with `--next-subtask`

`--next-subtask` picks one subtask, spawns one session, runs it, returns. It's unaware of `needs-context` — it breaks chains across separate sessions, losing the context that `needs-context` was meant to preserve.

### `--next-session`

```bash
# Pick next ready session, run it, return
aiki task run <parent> --next-session

# Scoped to a specific lane (used by orchestrators)
aiki task run <parent> --next-session --lane <lane-id>

# Async
aiki task run <parent> --next-session --async
```

**How it works:**
1. Finds the next ready subtask of `<parent>` (optionally within `--lane`)
2. If that task is the head of a `needs-context` chain → runs the whole chain in one agent session. Agent's `aiki task` is scoped to the chain's tasks.
3. If that task is standalone → runs just that task in one session.
4. Returns when the session ends.

### Session scoping for multi-task sessions

When `--next-session` creates a multi-task session (needs-context chain), the spawned agent is scoped to the chain's tasks:

```
# Agent is spawned for chain: explore → plan → implement

aiki task
# Ready (1):
# - <explore-id> Explore auth issues

aiki task start <explore-id>
# ... explore the codebase ...
aiki task close <explore-id> --summary "Found 3 issues"

# plan was blocked by explore (needs-context implies depends-on).
# Now unblocked:
aiki task
# Ready (1):
# - <plan-id> Plan remediation

aiki task start <plan-id>
# ... plan (with full explore context still in session) ...
aiki task close <plan-id> --summary "Plan ready"

aiki task
# Ready (1):
# - <implement-id> Implement fixes

aiki task start <implement-id>
# ... implement (with explore + plan context) ...
aiki task close <implement-id> --summary "All fixed"

aiki task
# (no more tasks — session complete)
```

The agent uses the normal `aiki task` workflow. It doesn't know it's in a "needs-context chain" — it just sees a scoped backlog that reveals tasks as dependencies are satisfied.

### Migration from `--next-subtask`

`--next-subtask` is removed. `--next-session` replaces it.

| Old | New | Behavior change |
|-----|-----|-----------------|
| `aiki task run <parent> --next-subtask` | `aiki task run <parent> --next-session` | Now groups `needs-context` chains into single sessions. Standalone tasks behave identically. |

**For tasks without `needs-context` links:** No behavior change. `--next-session` picks one task, runs it, returns — same as `--next-subtask`.

**Templates to update:**
- `aiki/build.md` — replace `--next-subtask` with `--next-session`
- `aiki/fix.md` and variants — replace `--next-subtask` with `--next-session`
- Any custom templates using `--next-subtask`

Using `--next-subtask` after migration returns an error: `"--next-subtask has been replaced by --next-session"`

---

## Use Cases

### 1. Fix workflow with context preservation
```bash
aiki task link <plan> --needs-context <explore>
aiki task link <implement> --needs-context <plan>

# Template loops --next-session:
aiki task run <fix> --next-session
# → runs explore → plan → implement in ONE session (chain)
aiki task run <fix> --next-session
# → runs review in its own session (standalone)
aiki task run <fix> --next-session
# → nothing left, done
```

### 2. Backward-compatible with no needs-context links
```bash
# No needs-context links — behaves exactly like old --next-subtask:
aiki task run <parent> --next-session
# → runs one task, returns
aiki task run <parent> --next-session
# → runs next task, returns
```

### 3. Template subtask frontmatter
```markdown
## Explore
---
slug: explore
---
...

## Plan
---
slug: plan
needs-context: subtasks.explore
---
...

## Implement
---
slug: implement
needs-context: subtasks.plan
---
...
```

Template instantiation creates the tasks, then materializes `needs-context` links from the frontmatter references.

### 4. Used by orchestrators with `--lane`
```bash
# The implement-command plan's orchestrator uses --lane to scope:
aiki task run <parent> --next-session --lane xtuttn
# → picks next ready task within lane xtuttn.., respects needs-context
```

---

## Implementation Plan

### Phase 1: `needs-context` link type

1. Add `NeedsContext` variant to link type enum in `cli/src/tasks/graph.rs`
2. Semantics: implies `depends-on` (blocking), plus same-session constraint
3. Add `aiki task link <A> --needs-context <B>` — new flag on the `link` subcommand
4. Validation:
   a. Must be between siblings (same parent task)
   b. No cycles in `needs-context` chains
   c. A task can have at most one `needs-context` predecessor (chains are linear)
5. Add `graph.get_needs_context_chain(task_id)` — returns ordered chain
6. Add `graph.is_needs_context_head(task_id)` — true if head of chain

### Phase 2: Template frontmatter support

1. Add `needs_context: Option<String>` to `SubtaskFrontmatter` in `types.rs`
2. Value format: `subtasks.<slug>` (same namespace as existing references)
3. During template instantiation, after all subtasks are created:
   a. Resolve `subtasks.<slug>` to task IDs
   b. Create `needs-context` links
   c. Validate: referenced slug exists, no cycles
4. Add validation error messages for bad references

### Phase 3: `--next-session`

1. Add `--next-session` flag to `task run` command
2. Implementation:
   a. Find next ready subtask (same logic as `--next-subtask`)
   b. Check if task is head of a `needs-context` chain (`is_needs_context_head`)
   c. If chain: get full chain via `get_needs_context_chain`, scope session to chain tasks
   d. If standalone: run single task (same as old behavior)
3. Add `--lane <lane-id>` filter (for orchestrator use — lane ID is the head task ID, prefix matching supported)
4. Session scoping: when running a chain, `aiki task` within the session shows only chain tasks, revealed as deps are satisfied

### Phase 4: Remove `--next-subtask`

1. Remove `--next-subtask` flag from `task run`
2. Add error message: `"--next-subtask has been replaced by --next-session"`
3. Update all built-in templates:
   a. `aiki/build.md` → `--next-session`
   b. `aiki/fix.md` → `--next-session`
   c. `aiki/fix/quality.md` → `--next-session`
   d. Any other templates referencing `--next-subtask`

### Phase 5: Tests

1. Unit tests: `needs-context` link creation, validation (siblings, no cycles, linear chains)
2. Unit tests: chain resolution (`get_needs_context_chain`, `is_needs_context_head`)
3. Unit tests: template frontmatter parsing and link materialization
4. Unit tests: `--next-session` — standalone tasks, chain grouping, `--lane` filtering
5. Integration tests: session scoping (agent sees only chain tasks)
6. Integration tests: `--next-subtask` removed, error on use
7. Integration tests: end-to-end fix workflow with needs-context chain

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `needs-context` between non-siblings | Error: "needs-context links must be between subtasks of the same parent" |
| `needs-context` cycle | Error: "Cycle detected in needs-context chain" |
| Task with two `needs-context` predecessors | Error: "Task can only have one needs-context predecessor (chains must be linear)" |
| `needs-context` frontmatter references unknown slug | Error: "Unknown subtask slug '<slug>' in needs-context" |
| `--next-subtask` used | Error: "--next-subtask has been replaced by --next-session" |
| `--next-session` with no ready subtasks | Returns: "No ready subtasks for <parent>" |
| `--lane` with unknown ID | Error: "No lane with head task matching '<id>' for task <parent>" |
| `--lane` with ambiguous prefix | Error: "Multiple lanes match prefix '<prefix>', be more specific" |
| Task fails within multi-task session | Session ends. Remaining chain tasks stay open. |
| Session crash mid-chain | Current task marked as stopped. Remaining chain tasks stay open. |

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| New link type | `needs-context` | Explicit modeling. Not overloading `depends-on`. Clear semantic: same session + ordering. |
| Implies `depends-on` | Yes | Can't share context with something that hasn't run yet. |
| Sibling constraint | Required | Session sharing only makes sense within the same parent's subtask graph. |
| Linear chains only | One predecessor per task | Simplifies chain resolution. A task can't be in two chains. Fork/join is modeled with separate chains + `depends-on`. |
| `--next-session` name | Replaces `--next-subtask` | Reflects that the unit of execution is now a session (one or more tasks), not a single subtask. |
| Clean break | Remove `--next-subtask` | New semantics warrant new name. Error message guides migration. |
| Session scoping | `aiki task` scoped to chain | Agent uses normal workflow. No new commands. Consistent with other scoping (parent tasks, etc.). |
| Template frontmatter | `needs-context: subtasks.<slug>` | Reuses existing `subtasks.*` namespace. Consistent with other frontmatter references. |
| `--lane` filter | On `--next-session` | Forward-compatible with lanes plan. No-op if lanes aren't used. |

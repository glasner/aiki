# Semantic Blocking Links: Replace `blocked-by` with Domain-Specific Links

**Date**: 2026-02-15
**Status**: Draft
**Priority**: P2
**Depends on**: 
- `ops/now/loop-flags.md` (semantic links design)
- [Conditional Task Spawning](spawn-tasks.md) - Uses semantic links for spawned tasks

**Related Documents**:
- [Review-Fix Workflow](loop-flags.md) - Introduces `validates` and `remediates` links
- [Conditional Task Spawning](spawn-tasks.md) - Introduces `spawned-by` link for provenance

---

Note: Some work was completed as part of following task:
owxyylx - Update ready task fetching to handle semantic blocking links

---

## Problem

The current task system uses a generic `blocked-by` link to express dependencies between tasks. This creates several issues:

1. **Lost semantics** — "Task B is blocked by Task A" doesn't explain *why* the dependency exists
2. **No smart automation** — Generic blocking can't enable domain-specific behavior
3. **Poor queryability** — "What's blocked by X?" is less useful than "What validates X?" or "What follows up X?"
4. **Encourages lazy modeling** — Easy to add `blocked-by` without thinking about the relationship

**Example of the problem:**

```bash
# What does this mean?
task B blocked-by task A

# Is B validating A? Fixing A? Requiring A's output? Unclear.
```

---

## Summary

Replace generic `blocked-by` with **three semantic blocking link types** that express domain-specific relationships:

| Link Type | Meaning | Blocking Behavior | Special Features |
|-----------|---------|-------------------|------------------|
| `validates` | B reviews/validates A | B waits for A to complete | Autorun when A closes |
| `remediates` | B fixes/addresses issues from A (typically A is a validation) | B waits for A to complete | Autorun when A closes |
| `depends-on` | B requires A's output | B waits for A to complete | Simple prerequisite, autorun |

**Benefits:**
- **Self-documenting** — Task graph explains relationships, not just ordering
- **Queryable** — "Show all reviews validating this task" vs "show all blockers"
- **Automatable** — Domain-specific behavior (conditional skip for follows-up)
- **Forces clarity** — Must think about *why* dependency exists

**Migration:** Existing `blocked-by` links convert to `depends-on` (closest semantic match for generic prerequisites).

---

## Design

### Link Type Definitions

```rust
pub enum LinkType {
    /// B validates A (review relationship)
    Validates {
        task_id: TaskId,
        autorun: bool,  // Start B when A completes
    },
    
    /// B remediates issues found by A (A is typically a validation/review)
    Remediates {
        task_id: TaskId,
        autorun: bool,
    },
    
    /// B depends on A (prerequisite sequencing)
    DependsOn {
        task_id: TaskId,
        autorun: bool,  // Start B when A completes
    },
    
    // Existing non-blocking links
    SubtaskOf { parent_id: TaskId },
    Source { source: SourceRef },
}
```

**Note:** `blocked-by` is removed entirely. All blocking relationships must use one of the three semantic types.

### Integration with Spawned Tasks

When tasks are spawned via the `spawns:` frontmatter ([spawn-tasks.md](spawn-tasks.md)), they receive TWO link types:

1. **`spawned-by`** — For provenance tracking (records that the task was auto-created)
2. **Semantic link** — For relationship semantics (one of `validates`, `remediates`, or `depends-on`)

**Scope note:** `spawned-by` is **not** defined in this spec. It is a non-blocking provenance link defined and implemented in [spawn-tasks.md](spawn-tasks.md). It is not part of the `LinkType` enum changes in this spec. This spec only adds the three blocking semantic link types (`validates`, `remediates`, `depends-on`). The two specs are designed to compose: `spawn-tasks.md` creates the task with a `spawned-by` link, and this spec's semantic links are added alongside it via the `links` field in the spawn config.

**Example:**
```yaml
# Fix task spawned by review
id: fix-123
links:
  - type: spawned-by      # Provenance (from spawn-tasks.md): auto-created by review abc123
    task_id: abc123
  - type: remediates      # Semantics (from this spec): fixes issues found by abc123
    task_id: abc123
    autorun: true
```

**Benefits:**
- `spawned-by` answers: "Was this task automatically created? By what?" (non-blocking, provenance only)
- Semantic links answer: "What is the relationship? (validation, remediation, dependency)" (blocking + autorun)

### Blocking Semantics

All three link types share a common rule:
- Task B cannot start while Task A is in progress or pending

**Unblocking behavior differs by link type:**

| Terminal State | `validates` / `remediates` | `depends-on` |
|----------------|---------------------------|--------------|
| Task A closed (done) | Unblocks B | Unblocks B |
| Task A stopped | Unblocks B | **Keeps B blocked** |
| Task A closed as won't-do | Unblocks B | **Keeps B blocked** |

**Rationale:** `depends-on` expresses a prerequisite — B needs A's output. If A was stopped or abandoned, that output doesn't exist, so B should not proceed. In contrast, `validates` and `remediates` express follow-up relationships where the upstream task becoming irrelevant doesn't invalidate downstream work.

**Handling blocked `depends-on` chains:** When a prerequisite is stopped or won't-do, the dependent task remains blocked. To unblock it, either:
1. **Re-do the prerequisite** — create and complete a replacement task, then update the link
2. **Remove the dependency** — `aiki task unlink` to drop the `depends-on` link if the prerequisite is no longer needed
3. **Close the dependent as won't-do** — if the whole chain is no longer relevant

### UX for Blocked and Permanently Blocked Tasks

**`aiki task show` output for semantic links:**

The show command replaces the generic "Blocked by:" section with semantic link sections. Each link type gets its own label, and the upstream task's terminal state is shown:

```
Task: Write integration tests
ID: abcdefghijklmnopqrstuvwxyzabcdef
Status: pending
Priority: p2

Depends on:
- klmnopq — Implement API endpoint (closed)     ✓ satisfied
- rstuvwx — Design database schema (stopped)     ✗ blocked (prerequisite stopped)

Validates:
- yzabcde — Feature implementation (closed)      ✓ satisfied
```

**When a `depends-on` link is permanently blocked** (prerequisite stopped or won't-do), the show output includes actionable remediation guidance:

```
Task: Deploy to production
ID: abcdefghijklmnopqrstuvwxyzabcdef
Status: pending (blocked)
Priority: p2

Depends on:
- klmnopq — Run staging tests (won't-do)         ✗ blocked (prerequisite won't-do)

⚠ This task is blocked by a stopped/abandoned prerequisite.
  To unblock, choose one of:
  1. Re-do the prerequisite:  aiki task add "Replacement task" && aiki task link <this-id> --depends-on <new-id>
  2. Remove the dependency:   aiki task unlink <this-id> --depends-on klmnopq
  3. Close this task:         aiki task close <this-id> --wont-do --summary "Prerequisite abandoned"
```

**`aiki task` (list) output for permanently blocked tasks:**

Permanently blocked tasks do NOT appear in the "Ready" queue (they are blocked). They appear in a separate "Blocked" section when present:

```
In Progress:
- abcdefg — Current work item

Ready (2):
- hijklmn [p0] Urgent fix
- opqrstu [p2] Add tests

Blocked (1):
- vwxyzab [p2] Deploy to production (depends-on: klmnopq stopped)
```

The "Blocked" section only appears when tasks are permanently blocked (i.e., a `depends-on` prerequisite reached a terminal state that doesn't unblock). Temporarily blocked tasks (upstream still pending or in-progress) are simply omitted from the output — they'll appear in Ready once their prerequisites complete.

**`aiki task start` for a blocked task:**

Attempting to start a permanently blocked task produces a clear error with the same remediation guidance:

```
$ aiki task start vwxyzab
Error: Task 'vwxyzab' is blocked by prerequisite 'klmnopq' (stopped).

To unblock, choose one of:
  1. Re-do the prerequisite:  aiki task add "Replacement" && aiki task link vwxyzab --depends-on <new-id>
  2. Remove the dependency:   aiki task unlink vwxyzab --depends-on klmnopq
  3. Close this task:         aiki task close vwxyzab --wont-do --summary "Prerequisite abandoned"
```

**Note:** There is no separate "failed" state. A task that encounters errors is either stopped (by the agent or user) or closed. `validates`/`remediates` dependents unblock in either case. `depends-on` dependents only unblock on successful close.

### Multi-Link Blocking Evaluation

A task may have multiple blocking links of different types (e.g., `validates` task A **and** `depends-on` task B). Blocking uses **AND semantics**: the task remains blocked until **every** blocking link is individually satisfied.

**Evaluation rule:** For each link, apply the per-type unblocking rules from the table above. The task is ready only when all links evaluate to "unblocked."

**Example — mixed link types with mixed upstream states:**

```
Task C:
  - validates: Task A (stopped)       → unblocked (validates unblocks on any terminal state)
  - depends-on: Task B (closed, done) → unblocked (depends-on unblocks on close)
  Result: Task C is READY (both links satisfied)

Task D:
  - validates: Task A (stopped)       → unblocked
  - depends-on: Task B (stopped)      → BLOCKED (depends-on requires close)
  Result: Task D is BLOCKED (one unsatisfied link is enough)
```

**Autorun with multiple links:** Autorun triggers only when the task transitions from blocked to ready — i.e., when the *last* unsatisfied link becomes satisfied. If a task has autorun and multiple links, the autorun fires on the state change that satisfies the final blocking link, not on each individual link resolution.

**Differences** are in domain semantics, automation, and unblocking:

| Feature | `validates` | `remediates` | `depends-on` |
|---------|-------------|--------------|--------------|
| **Meaning** | Review/validation | Fix/remediation | Prerequisite |
| **Unblocks on** | Any terminal state | Any terminal state | Close (done) only |
| **Autorun** | Yes (optional) | Yes (optional) | Yes (optional) |
| **Conditional creation** | Via spawns | Via spawns | N/A |
| **Typical usage** | Review after task | Fix issues from validation | Sequence work |

### Use Case Mapping

**When to use each link type:**

#### `validates`
```bash
# Review validates a task
aiki review <task-id>  # Creates review with validates link

# Custom validation task
aiki task add "Security audit of auth module" --validates <task-id>
```

**Use when:** One task checks the quality/correctness of another task.

#### `remediates`
```bash
# Fix remediates issues from review
aiki review <task-id> --fix  # Creates fix with remediates link

# Remediate validation findings
aiki task add "Fix security issues" --remediates <audit-task>
```

**Use when:** One task fixes or addresses issues found by a validation/review task.

**Conditional creation (via spawns, not link flags):**
```yaml
# Fix task is only created if the review found issues (see spawn-tasks.md)
spawns:
  - when: not approved
    task:
      template: aiki/fix
      links: [{ type: remediates, task: "{{ parent.id }}" }]
```

#### `depends-on`
```bash
# Implementation depends on design
aiki task add "Implement API endpoint" --depends-on <design-task>

# Tests depend on feature
aiki task add "Write integration tests" --depends-on <feature-task>

# Deployment sequence
aiki task add "Deploy to production" --depends-on <staging-deploy>
```

**Use when:** One task requires another's output, but it's not validation or follow-up work.

### Choosing the Right Link

**Decision tree:**

1. **Is B checking the quality of A?** → `validates`
2. **Is B fixing issues found by A (where A is a validation)?** → `remediates`
3. **Does B just need A to finish first?** → `depends-on`

**Examples:**

| Relationship | Link Type | Why |
|--------------|-----------|-----|
| Code review validates implementation | `validates` | Checking quality |
| Fix addresses review findings | `remediates` | Fixing issues found by validation |
| Tests depend on feature implementation | `depends-on` | Need output, not validating |
| Security audit validates release | `validates` | Quality check |
| Fix addresses security audit findings | `remediates` | Fixing issues found by validation |
| Deploy prod depends on deploy staging | `depends-on` | Sequencing |

---

## Migration Strategy

### Existing `blocked-by` Links

**Option 1: Convert all to `depends-on`**
- Simple mechanical conversion
- Loses semantic information if relationship was actually validation or follow-up
- Safe default for unknown relationships

**Option 2: Smart conversion based on task names/templates**
- Review tasks with `blocked-by` → convert to `validates`
- Fix tasks with `blocked-by` → convert to `remediates`
- Everything else → `depends-on`
- More accurate, but requires heuristics

**Recommendation:** Option 1 (convert all to `depends-on`) with manual cleanup.

### Storage Compatibility

Links are stored as `LinkAdded`/`LinkRemoved` events with a string `kind` field (e.g., `kind: "blocked-by"`). The `LINK_KINDS` registry in `graph.rs` defines recognized kinds. This string-based design means no schema version bump is needed — the new semantic link types (`validates`, `remediates`, `depends-on`) are just new `kind` strings.

**Rollout phases:**

1. **Phase A — Read both, write new only (current state)**
   - `LINK_KINDS` already contains both `"blocked-by"` (legacy) and the three semantic types
   - Reader: materializes `blocked-by` events as blocking links (existing behavior preserved)
   - Writer: new CLI flags emit semantic link types; `--blocked-by` flag is removed from CLI
   - **Compatibility:** Old task branches with `blocked-by` events continue to work. No data rewrite needed.

2. **Phase B — Migrate existing links**
   - Run a one-time migration that emits `LinkRemoved(kind: "blocked-by")` + `LinkAdded(kind: "depends-on")` event pairs for each existing `blocked-by` link
   - This is append-only — original events remain in history for auditability
   - Migration is idempotent: re-running skips links already converted (the `LinkRemoved` for the old `blocked-by` makes the old link inactive, and the idempotency check in `write_link_event` prevents duplicate `depends-on` links)

3. **Phase C — Remove legacy support**
   - Remove `"blocked-by"` entry from `LINK_KINDS`
   - Any unconverted `blocked-by` events in the log become inert (unknown kind strings are ignored by the materializer)
   - Update cycle detection in `write_link_event` to check all semantic blocking kinds instead of hardcoded `"blocked-by"`

**What does NOT change:**
- Event format (`LinkAdded`/`LinkRemoved` structs) — no new fields needed
- Storage backend (JJ commits on `aiki/tasks` branch) — same append-only event log
- Graph materialization logic (`EdgeStore`) — already kind-agnostic, uses string keys

**Migration steps:**
1. Add new link types to `LINK_KINDS` (already done)
2. Update CLI to support new link flags, remove `--blocked-by`
3. Run migration: emit `LinkRemoved`/`LinkAdded` pairs converting `blocked-by` → `depends-on`
4. Remove `"blocked-by"` from `LINK_KINDS` registry
5. Update cycle detection to cover all semantic blocking kinds
6. Update documentation

### CLI Changes

**Before:**
```bash
aiki task add "Task B" --blocked-by <task-a>
```

**After:**
```bash
# Choose semantic link type
aiki task add "Task B" --depends-on <task-a>
aiki task add "Review B" --validates <task-a>
aiki task add "Fix B" --remediates <review-task>
```

**Multiple prerequisites:** All link flags are repeatable. Pass the flag multiple times to create multiple links of the same type:
```bash
# Task depends on two prerequisites
aiki task add "Deploy" --depends-on <staging-task> --depends-on <tests-task>

# Task validates multiple implementations
aiki task add "Integration review" --validates <feature-a> --validates <feature-b>
```

**Mixed link types** are also supported — a single task can have links of different types:
```bash
# Task depends on design AND remediates a review
aiki task add "Fix and extend auth" --depends-on <design-task> --remediates <review-task>
```

**Validation:** The CLI rejects circular dependencies at link creation time. Self-links (linking a task to itself) are also rejected.

**Note:** Conditional creation (e.g., "only create fix if review finds issues") is handled by the `spawns` mechanism, not by CLI flags. See `spawn-tasks.md`.

**Autorun:**
```bash
# Start B automatically when A completes
aiki task add "Task B" --depends-on <task-a> --autorun
aiki task add "Review B" --validates <task-a> --autorun
aiki task add "Fix B" --remediates <review-task> --autorun
```

**Note:** `--autorun` applies to all link flags on the same command. If you need different autorun settings per link, create the task first and add links separately.

---

## Implementation Plan

### Phase 1: Core link types

1. **Add new link type variants** (`cli/src/task/links.rs` or similar):
   ```rust
   pub enum LinkType {
       Validates { task_id: TaskId, autorun: bool },
       Remediates { task_id: TaskId, autorun: bool },
       DependsOn { task_id: TaskId, autorun: bool },
       SubtaskOf { parent_id: TaskId },
       Source { source: SourceRef },
   }
   ```

2. **Update blocking logic** — all three new types block task start
3. **Tests:** Unit tests for link creation and blocking behavior

### Phase 2: Autorun behavior

1. **Task close handler** — trigger autorun for linked tasks, respecting unblocking rules:
   ```rust
   fn handle_task_terminal(task: &Task, terminal_state: TerminalState) {
       for link in find_links_to(task.id) {
           let should_unblock = match (&link.link_type, &terminal_state) {
               // validates/remediates unblock on any terminal state
               (LinkType::Validates { .. }, _) => true,
               (LinkType::Remediates { .. }, _) => true,
               // depends-on only unblocks on successful close
               (LinkType::DependsOn { .. }, TerminalState::Closed) => true,
               (LinkType::DependsOn { .. }, _) => false,
               _ => false,
           };
           if should_unblock {
               if let Some(true) = link.link_type.autorun() {
                   start_task(link.from_task);
               }
           }
       }
   }
   ```

2. **Tests:** Integration tests for autorun triggering

### Phase 3: Conditional Task Creation (via spawns)

**Note:** Conditional task creation (e.g., "only create fix if not approved") is handled by the `spawns` frontmatter mechanism (see `spawn-tasks.md`), not by link fields.

**Example:**
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

This eliminates the need for conditional fields on links — the task simply isn't created if the condition isn't met. The remediation link only exists when a fix task is actually spawned.

### Phase 4: CLI flags

1. **Add flags to `task add`** (repeatable for multi-prerequisite):
   ```rust
   /// Task IDs that this task validates (repeatable)
   #[arg(long)]
   pub validates: Vec<TaskId>,

   /// Task IDs that this task remediates (repeatable)
   #[arg(long)]
   pub remediates: Vec<TaskId>,

   /// Task IDs that this task depends on (repeatable)
   #[arg(long)]
   pub depends_on: Vec<TaskId>,

   /// Enable autorun for all links created by this command
   #[arg(long)]
   pub autorun: bool,
   ```

2. **Update `aiki review`** — create validates link with autorun
3. **Tests:** CLI integration tests

### Phase 5: Migration

1. **Migration script** — convert `blocked-by` → `depends-on` in task storage
2. **Remove `BlockedBy` variant** from `LinkType` enum
3. **Update documentation** — remove `--blocked-by` references
4. **Tests:** Migration tests on sample data

---

## Examples

### Code Review Workflow

```bash
# 1. Implement feature
aiki task add "Implement user authentication"
aiki task start <task-id>
# ... do work ...
aiki task close <task-id>

# 2. Review validates implementation (autorun when feature closes)
aiki review <task-id>  # Creates review with validates link, autorun: true

# 3. Fix remediates review (created conditionally via spawns)
# The review template uses spawns: {when: not approved, task: {template: aiki/fix}}
```

**Task graph:**
```
Feature Task (closed)
  ├── Review Task (validates feature, autorun)
  └── Fix Task (remediates review, conditional skip)
```

### Sequential Prerequisites

```bash
# Design → Implementation → Tests → Deployment
aiki task add "Design API schema"
aiki task add "Implement API endpoint" --depends-on <design-id> --autorun
aiki task add "Write integration tests" --depends-on <impl-id> --autorun
aiki task add "Deploy to production" --depends-on <tests-id>  # No autorun (manual gate)
```

**Task graph:**
```
Design (closed)
  └─→ Implementation (depends-on design, autorun)
        └─→ Tests (depends-on impl, autorun)
              └─→ Production Deploy (depends-on tests, manual)
```

### Multi-Phase Work

```bash
# Phase 1 completes, Phase 2 depends on it
aiki task add "Phase 1: Basic caching"
aiki task start <phase1-id>
# ... do work ...
aiki task close <phase1-id>

# Phase 2 depends on Phase 1 (continuation work, not remediation)
aiki task add "Phase 2: Distributed cache" --depends-on <phase1-id> --autorun
```

**Note:** Multi-phase continuation work should use `depends-on`, not `remediates`. `Remediates` is specifically for fixing issues found by validations.

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Task A closed (done) | All linked tasks unblock (autorun if enabled) |
| Task A stopped | `validates`/`remediates` dependents unblock; `depends-on` dependents stay blocked |
| Task A closed as won't-do | `validates`/`remediates` dependents unblock; `depends-on` dependents stay blocked |
| Multiple tasks validate same target | All reviews can run in parallel after target closes |
| Multiple tasks depend on same prerequisite | All dependent tasks unblock simultaneously when prerequisite closes (done) |
| Task with mixed link types (validates A + depends-on B) | AND semantics: blocked until all links individually satisfied |
| Autorun with multiple links | Fires only when last blocking link is satisfied (blocked→ready transition) |
| Circular dependencies | Detected and rejected at link creation time |
| Remediation task spawned conditionally | Use spawns mechanism (see spawn-tasks.md) |
| Autorun task already started manually | No-op (task already running) |
| Autorun task already closed | No-op (task already done) |
| `depends-on` prerequisite stopped/won't-do | Dependent stays blocked; user must re-do, unlink, or close as won't-do |

---

## Open Questions

1. ~~**Should remediation links be auto-created by spawns?**~~ **Resolved:** Yes. The `spawns` block in the template defines both the semantic link type and provenance link. When a review template spawns a fix, the `spawns` config explicitly declares the `remediates` link (see "Integration with Spawned Tasks" section). This is not implicit — the template author specifies which links to create.

2. **Subtask remediation semantics?** — Should subtasks of a parent task automatically get `remediates` links if spawned conditionally? E.g., if a review subtask spawns a fix subtask.

3. **Link removal?** — Should users be able to remove links? `aiki task unlink <task-id> <link-id>`

5. **Link visibility in task output?** — How to show links in `aiki task show`?
   ```
   Task: Fix auth bug
   Status: Ready
   Remediates: Review abc123 (waiting for review to close)
   ```

---

## What This Does NOT Change

- **Subtask relationships** — `subtask-of` is unchanged (not a blocking link)
- **Source tracking** — `source:` links unchanged (lineage, not blocking)
- **Task lifecycle** — start/stop/close behavior unchanged
- **Existing templates** — templates using `blocked-by` need updating to use semantic links

---

## Files Changed

| File | Change | Status |
|------|--------|--------|
| `cli/src/task/links.rs` (or similar) | Replace `BlockedBy` with `Validates`, `Remediates`, `DependsOn` | **New work** |
| `cli/src/task/lifecycle.rs` | Add autorun logic in task close handler (with `depends-on` success-gating) | **New work** |
| `cli/src/commands/task.rs` | Add `--validates`, `--remediates`, `--depends-on` (repeatable), `--autorun` flags | **New work** |
| `cli/src/commands/review.rs` | Create review with `validates` link instead of `blocked-by` | **Update** |
| Task storage schema | Add new link type variants, remove `blocked-by` | **Migration** |
| Documentation | Update all `--blocked-by` references to semantic links | **Update** |

**Out of scope:** `spawned-by` link type is defined and implemented in [spawn-tasks.md](spawn-tasks.md), not in this change set. The two specs compose at runtime — spawned tasks receive both a `spawned-by` link (provenance) and a semantic link from this spec (relationship semantics).

---

## Success Criteria

- [ ] All three semantic link types implemented and tested
- [ ] Autorun behavior works for all link types
- [ ] Conditional task creation via `spawns` works (e.g., fix task only spawned when review is not approved)
- [ ] CLI supports creating tasks with semantic links
- [ ] `aiki review --fix` uses `validates` and `remediates` links
- [ ] Existing `blocked-by` links migrated to `depends-on`
- [ ] `BlockedBy` variant removed from codebase
- [ ] Documentation updated
- [ ] No regressions in task lifecycle or blocking behavior

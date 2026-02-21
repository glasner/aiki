# Complete Link Flags Implementation

**Date**: 2026-02-20
**Status**: Draft

## Problem

The current `link-flags.md` plan only covers 4 link types (`blocked-by`, `sourced-from`, `supersedes`, `subtask-of`) but the codebase actually supports 9 link types total. The missing link types are:

- `implements` - Link a plan to its spec file
- `orchestrates` - Link an orchestrator to the plan it drives
- `scoped-to` - Link a task to the target it operates on
- `depends-on` - Dependency link (blocks ready state, task-only)
- `spawned-by` - Provenance link tracking automatic task creation

Currently:
- `task link` and `task unlink` support all 9 link types via flags
- `task add` and `task start` only support 4 link types via flags

This creates an inconsistency where some relationships can only be established post-creation via `task link`, defeating the purpose of atomic task creation with links.

## Summary

Extend `task add` and `task start` to support all applicable link types as flags, matching what `task link` and `task unlink` already support.

**Link types to add:**
1. `--implements` - Plan implements this spec file
2. `--orchestrates` - Orchestrator drives this plan
3. `--scoped-to` - Task operates on this target
4. `--depends-on` - Task depends on this (blocks ready state)

**Not adding:** `--spawned-by` (internal provenance link, not user-facing)

## All Link Types in Codebase

Based on scan of `cli/src/tasks/graph.rs`:

| Link Kind | Cardinality (forward) | Cardinality (reverse) | Blocks Ready | Task-only | Current Support |
|-----------|----------------------|----------------------|--------------|-----------|-----------------|
| `blocked-by` | None (many) | None (many) | Yes | Yes | ✅ Add/Start/Link/Unlink |
| `depends-on` | None (many) | None (many) | Yes | Yes | ❌ Link/Unlink only |
| `sourced-from` | None (many) | None (many) | No | No | ✅ Add/Start/Link/Unlink |
| `subtask-of` | 1 | None (many) | No | Yes | ✅ Add/Start/Link/Unlink |
| `implements` | 1 | 1 | No | No | ❌ Link/Unlink only |
| `orchestrates` | 1 | 1 | No | Yes | ❌ Link/Unlink only |
| `scoped-to` | None (many) | None (many) | No | No | ❌ Link/Unlink only |
| `supersedes` | 1 | None (many) | No | Yes | ✅ Add/Start/Link/Unlink |
| `spawned-by` | 1 | None (many) | No | Yes | ⚠️ Internal only |

**Key:**
- `blocked-by`, `depends-on` - Blocking links (prevent task from being "ready")
- `sourced-from` - Provenance (where task came from)
- `subtask-of` - Parent-child hierarchy
- `implements` - Plan → spec file
- `orchestrates` - Orchestrator → plan
- `scoped-to` - Task → target (file, directory, etc.)
- `supersedes` - Replacement/obsolescence
- `spawned-by` - Automatic provenance (emitted by system, not user)

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Link types to add | `implements`, `orchestrates`, `scoped-to`, `depends-on` | Match what Link/Unlink already support (except internal `spawned-by`) |
| Cardinality | Match existing constraints | `implements`/`orchestrates` are single, others are multi |
| Auto-replace behavior | `implements` and `orchestrates` replace existing | Already implemented in `write_link_event` |
| Flag names | Match link kind names exactly | Consistency with existing patterns |

## Changes

### 1. Add missing link flags to `Add` variant

**File:** `cli/src/commands/task.rs`

Add after existing link flags:

```rust
Add {
    // ... existing fields ...

    /// Spec file this task implements (plan → spec)
    #[arg(long)]
    implements: Option<String>,

    /// Plan task this orchestrator drives (orchestrator → plan)
    #[arg(long)]
    orchestrates: Option<String>,

    /// Target this task operates on (e.g., file:src/main.rs)
    #[arg(long, action = clap::ArgAction::Append)]
    scoped_to: Vec<String>,

    /// Task(s) this depends on (blocks ready state)
    #[arg(long, action = clap::ArgAction::Append)]
    depends_on: Vec<String>,
},
```

### 2. Add missing link flags to `Start` variant

**File:** `cli/src/commands/task.rs`

Add the same flags to `Start` (works for both quick-start and existing task start):

```rust
Start {
    // ... existing fields ...

    /// Spec file this task implements (plan → spec)
    #[arg(long)]
    implements: Option<String>,

    /// Plan task this orchestrator drives (orchestrator → plan)
    #[arg(long)]
    orchestrates: Option<String>,

    /// Target this task operates on (e.g., file:src/main.rs)
    #[arg(long, action = clap::ArgAction::Append)]
    scoped_to: Vec<String>,

    /// Task(s) this depends on (blocks ready state)
    #[arg(long, action = clap::ArgAction::Append)]
    depends_on: Vec<String>,
},
```

### 3. Update `emit_link_flags` helper

**File:** `cli/src/commands/task.rs`

The `emit_link_flags` helper (from link-flags.md plan) needs to handle the new link types:

```rust
fn emit_link_flags(
    cwd: &Path,
    graph: &mut TaskGraph,
    task_id: &str,
    blocked_by: &[String],
    depends_on: &[String],      // NEW
    supersedes: &Option<String>,
    sourced_from: &[String],
    implements: &Option<String>,   // NEW
    orchestrates: &Option<String>, // NEW
    scoped_to: &[String],          // NEW
) -> Result<()> {
    // Handle blocked-by (multiple allowed)
    for target in blocked_by {
        if write_link_event(cwd, graph, "blocked-by", task_id, target)? {
            graph.edges.insert(task_id, target, "blocked-by");
        }
    }

    // Handle depends-on (multiple allowed, blocks ready)
    for target in depends_on {
        if write_link_event(cwd, graph, "depends-on", task_id, target)? {
            graph.edges.insert(task_id, target, "depends-on");
        }
    }

    // Handle sourced-from (multiple allowed)
    for target in sourced_from {
        if write_link_event(cwd, graph, "sourced-from", task_id, target)? {
            graph.edges.insert(task_id, target, "sourced-from");
        }
    }

    // Handle supersedes (single)
    if let Some(target) = supersedes {
        if write_link_event(cwd, graph, "supersedes", task_id, target)? {
            graph.edges.insert(task_id, target, "supersedes");
        }
    }

    // Handle implements (single, auto-replaces)
    if let Some(target) = implements {
        if write_link_event(cwd, graph, "implements", task_id, target)? {
            graph.edges.insert(task_id, target, "implements");
        }
    }

    // Handle orchestrates (single, auto-replaces)
    if let Some(target) = orchestrates {
        if write_link_event(cwd, graph, "orchestrates", task_id, target)? {
            graph.edges.insert(task_id, target, "orchestrates");
        }
    }

    // Handle scoped-to (multiple allowed)
    for target in scoped_to {
        if write_link_event(cwd, graph, "scoped-to", task_id, target)? {
            graph.edges.insert(task_id, target, "scoped-to");
        }
    }

    Ok(())
}
```

### 4. Wire new flags in `run_add`

**File:** `cli/src/commands/task.rs`

Update call to `emit_link_flags` to pass new parameters:

```rust
emit_link_flags(
    cwd,
    &mut graph,
    &task_id,
    &blocked_by,
    &depends_on,      // NEW
    &supersedes,
    &all_sources,
    &implements,      // NEW
    &orchestrates,    // NEW
    &scoped_to,       // NEW
)?;
```

### 5. Wire new flags in `run_start`

**File:** `cli/src/commands/task.rs`

Same as `run_add` - update both quick-start and existing task paths to pass new parameters to `emit_link_flags`.

### 6. Tests

Add tests for:
- `task add` with `--implements` (single value, auto-replace)
- `task add` with `--orchestrates` (single value, auto-replace)
- `task add` with `--scoped-to` (single and multiple)
- `task add` with `--depends-on` (single and multiple, verify blocks ready)
- `task start` quick-start with all new link types
- `task start` existing task with all new link types
- Multiple link types of different kinds in one command
- Verify `depends-on` correctly blocks ready state
- Verify `implements` and `orchestrates` auto-replace existing links

## Examples

```bash
# Create plan and link to spec
aiki task add "Plan: Authentication system" --implements file:ops/now/auth-spec.md

# Create orchestrator and link to plan
aiki task add "Build: Authentication" --orchestrates <plan-id>

# Create task scoped to specific files
aiki task add "Refactor auth handler" --scoped-to file:src/auth.rs --scoped-to file:src/session.rs

# Create task with dependency (blocks ready until dep is done)
aiki task add "Integration tests" --depends-on <unit-test-task-id>

# Quick-start with multiple link types
aiki task start "Fix auth bug" \
  --blocked-by <blocker-id> \
  --sourced-from file:ops/now/bugs.md \
  --scoped-to file:src/auth.rs \
  --depends-on <prerequisite-id>

# Start existing task and add scope
aiki task start <existing-id> --scoped-to file:src/main.rs
```

## Files Changed

| File | Change |
|------|--------|
| `cli/src/commands/task.rs` | Add `implements`, `orchestrates`, `scoped-to`, `depends-on` flags to `Add` and `Start`; update `emit_link_flags` signature and implementation; wire in `run_add` and `run_start` |
| `cli/tests/task_tests.rs` | Add tests for new link flags |

## Relationship to link-flags.md

This plan **extends** the `link-flags.md` plan, which covers the first 4 link types. Implementation order:

1. **First**: Implement `link-flags.md` (blocked-by, sourced-from, supersedes, subtask-of)
2. **Then**: Implement this plan (implements, orchestrates, scoped-to, depends-on)

Or implement both together by extending `emit_link_flags` with all 8 link types at once.

## Notes

- `spawned-by` is intentionally excluded - it's an internal provenance link emitted by the system (e.g., when templates spawn tasks), not a user-facing flag
- `depends-on` is functionally similar to `blocked-by` (both block ready state) but represents a different semantic relationship (dependency vs blocker)
- All new flags follow existing patterns for cardinality and auto-replace behavior

# Link flags on `task add`, `task start`, `link`, and `unlink`

**Date**: 2026-02-20
**Status**: Draft

## Problem

Creating a task and linking it requires two separate commands:

```bash
aiki task add "Review auth module"
aiki task link <id> --blocked-by <target-id>
```

This is verbose, error-prone (you need the ID from step 1), and means the link isn't part of the atomic task-creation event. Templates and internal code can emit links at creation time, but CLI users and agents cannot.

We also have two inconsistencies with our flag naming:
- `--parent` is a special-cased alias for the `subtask-of` link kind but isn't the canonical name
- `--source` is an alias for `sourced-from` but is the primary exposed flag

We want to expose `--subtask-of` and `--sourced-from` as first-class flags and keep `--parent` and `--source` as ergonomic aliases (hidden for backward compatibility).

## Summary

1. Add link-kind flags for the 4 existing link types to `task add` and `task start` (`blocked-by`, `sourced-from`, `supersedes`, `subtask-of`)
2. Add `--sourced-from` and `--subtask-of` as canonical flags; keep `--source` and `--parent` as hidden ergonomic aliases
3. Link flags on `task start` work for both quick-start (new task) and starting existing tasks

**After:**

```bash
# Create task with link in one command
aiki task add "Implement login" --blocked-by <design-id>
aiki task add "Build step" --subtask-of <parent-id>
aiki task add "New approach" --supersedes <old-task-id>
aiki task add "Fix bug" --sourced-from file:ops/now/design.md

# Multiple links at once
aiki task add "Fix bug" --blocked-by <task-id> --sourced-from file:ops/now/design.md

# Quick-start with links
aiki task start "Fix issue" --blocked-by <blocker-id> --source prompt  # --source still works

# Start existing task and add links
aiki task start <existing-id> --blocked-by <other-id>

# Ergonomic aliases still work (hidden from help)
aiki task add "Build step" --parent <parent-id>  # same as --subtask-of
aiki task add "Fix bug" --source file:design.md  # same as --sourced-from
```

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Link types to support | Only 4 existing types: `blocked-by`, `sourced-from`, `supersedes`, `subtask-of` | Match what's actually implemented in the codebase today |
| Flag names | Match link kind names (`--blocked-by`, `--sourced-from`, `--supersedes`, `--subtask-of`) | Consistent with link type naming |
| Ergonomic aliases | Keep `--source` and `--parent` as hidden aliases | Backward compatible, ergonomic for common cases |
| Multiple links per command | Yes, allow multiple flags | Links are independent |
| Multiple values per flag | `--blocked-by` and `--sourced-from` allow multiple; others are single | Matches existing cardinality constraints |
| Cardinality enforcement | At write time via `write_link_event` | Already implemented — no new validation needed |
| Link flags on `task start` | Work for both quick-start AND existing task IDs | Emit links for whatever task is being started |
| Graph refresh | In-memory insert after each `write_link_event` | Faster than re-materializing; insert edge into EdgeStore after write |

## Changes

### 1. Add link flags to `Add` variant

**File:** `cli/src/commands/task.rs`

Add to the `Add` enum variant:

```rust
Add {
    // ... existing fields ...

    /// Task that blocks this one
    #[arg(long, action = clap::ArgAction::Append)]
    blocked_by: Vec<String>,

    /// Task this supersedes
    #[arg(long)]
    supersedes: Option<String>,

    /// Sources that spawned this task (canonical form of --source)
    #[arg(long, action = clap::ArgAction::Append)]
    sourced_from: Vec<String>,

    /// Parent task this is a subtask of (canonical form of --parent)
    #[arg(long)]
    subtask_of: Option<String>,
},
```

**Alias handling:**
- Keep `--parent` as a hidden field: `#[arg(long, hide = true)]`
- Keep `--source` as a hidden field: `#[arg(long, hide = true, action = clap::ArgAction::Append)]`
- At runtime, resolve: 
  - `let subtask_of = subtask_of.or(parent);`
  - `let mut all_sources = sourced_from.clone(); all_sources.extend(source);`
- Error if both canonical and alias are provided (`--subtask-of` + `--parent`, or `--sourced-from` + `--source`)

**Cardinality:**
- `blocked-by`: `Vec<String>` with `Append` (multiple blockers allowed)
- `sourced-from`: `Vec<String>` with `Append` (multiple sources allowed)
- `supersedes`: `Option<String>` (single value)
- `subtask-of`: `Option<String>` (single parent)

### 2. Add same link flags to `Start` variant

**File:** `cli/src/commands/task.rs`

Add the same link fields to the `Start` variant. These work in both paths:
- **Quick-start** (creating new task): links emitted after task creation
- **Existing task** (starting by ID): links emitted for the task being started

### 3. Create `emit_link_flags` helper

**File:** `cli/src/commands/task.rs`

Factor out a shared helper used by both `run_add` and `run_start`:

```rust
/// Emit link events for all link flags provided on task add/start.
/// Inserts edges into the in-memory graph after each write for cardinality validation.
fn emit_link_flags(
    cwd: &Path,
    graph: &mut TaskGraph,
    task_id: &str,
    blocked_by: &[String],
    supersedes: &Option<String>,
    sourced_from: &[String],
) -> Result<()> {
    // Handle blocked-by (multiple allowed)
    for target in blocked_by {
        if write_link_event(cwd, graph, "blocked-by", task_id, target)? {
            graph.edges.insert(task_id, target, "blocked-by");
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

    Ok(())
}
```

**Note:** `subtask-of` links are handled separately via existing `--parent` codepath.

### 4. Wire link flags in `run_add`

**File:** `cli/src/commands/task.rs`

After existing `subtask-of` link emission:
1. Merge `--source` and `--sourced-from` values
2. Call `emit_link_flags` with merged sources and other link flags

The `subtask-of` link from `--subtask-of` flag is handled via the existing `--parent` codepath (since `subtask_of.or(parent)` resolves first).

### 5. Wire link flags in `run_start`

**File:** `cli/src/commands/task.rs`

**Quick-start path:** After task creation, merge aliases and call `emit_link_flags`.

**Existing task path:** After starting the task, merge aliases and call `emit_link_flags` with the started task's ID.

### 6. Add missing flags to `Link` and `Unlink` variants (if needed)

**File:** `cli/src/commands/task.rs`

Check if `Link` and `Unlink` already have the needed flags. Add canonical forms:
- `--sourced-from` (with `--source` as hidden alias)
- `--subtask-of` (with `--parent` as hidden alias)
- `--blocked-by`
- `--supersedes`

### 7. Add `EdgeStore::insert` method

**File:** `cli/src/tasks/graph.rs`

If not already public, add a method to insert an edge into the in-memory store:

```rust
impl EdgeStore {
    /// Insert an edge into the store (for in-memory graph updates after writes).
    pub fn insert(&mut self, from: &str, to: &str, kind: &str) {
        // Insert into forward and reverse indices
    }
}
```

### 8. Update documentation to use canonical forms

**Files:**
<<<<<<< conflict 1 of 2
%%%%%%% diff from: rvuxxwpq 61ad60bf (parents of rebased revision)
\\\\\\\        to: wtuvmvyx 1d201746 (rebase destination)
 - `cli/src/commands/task.rs` — Hide `--parent` with `#[arg(long, hide = true)]`
+- `cli/src/commands/agents_template.rs` — Replace `--parent` with `--subtask-of` in workflow examples (6 occurrences)
 - `CLAUDE.md` — Replace `--parent` with `--subtask-of` in all examples and guidance
 - `.aiki/templates/` — Update any templates that reference `--parent`
 - Agent guidance docs — Update references
+++++++ rsyqotwv 48653322 (rebased revision)
- `cli/src/commands/task.rs` — Hide `--parent` and `--source` with `#[arg(long, hide = true)]`
- `CLAUDE.md` — Update to use `--subtask-of` and `--sourced-from` as primary examples, mention `--source`/`--parent` as ergonomic shortcuts
- `.aiki/templates/` — Update any templates that reference `--parent` (keep `--source` for brevity)
- Agent guidance docs — Use canonical forms in explanations, mention aliases exist
>>>>>>> conflict 1 of 2 ends

### 9. Tests

Add tests for:
- `task add` with `--blocked-by` (single and multiple)
- `task add` with `--supersedes`
- `task add` with `--subtask-of`
- `task add` with `--sourced-from` (single and multiple)
- `task add` with multiple link flags of different kinds
- `task start` quick-start with link flags
- `task start` existing task with link flags
- `--subtask-of` works as replacement for `--parent`
- `--sourced-from` works as replacement for `--source`
- `--parent` still works (hidden backward compat)
- `--source` still works (hidden backward compat)
- Error when both `--parent` and `--subtask-of` provided
- Error when both `--source` and `--sourced-from` provided
- Alias merging works correctly when mixing flags

---

## Files Changed

| File | Change |
|------|--------|
<<<<<<< conflict 2 of 2
%%%%%%% diff from: rvuxxwpq 61ad60bf (parents of rebased revision)
\\\\\\\        to: wtuvmvyx 1d201746 (rebase destination)
 | `cli/src/commands/task.rs` | Add link flags to `Add`, `Start`, `Link`, `Unlink`; add `emit_link_flags` helper; update `extract_link_flag`; hide `--parent`; wire flags in `run_add` and `run_start` |
+| `cli/src/commands/agents_template.rs` | Replace `--parent` with `--subtask-of` in workflow examples |
+++++++ rsyqotwv 48653322 (rebased revision)
| `cli/src/commands/task.rs` | Add link flags to `Add`, `Start`, possibly `Link`, `Unlink`; add `emit_link_flags` helper; hide `--parent` and `--source`; wire flags in `run_add` and `run_start` |
>>>>>>> conflict 2 of 2 ends
| `cli/src/tasks/graph.rs` | Add `EdgeStore::insert` if needed |
| `CLAUDE.md` | Update to use `--subtask-of` and `--sourced-from` as primary; mention aliases |

---

## Resolved Questions

1. **Which link types to support** — Only the 4 existing types found in the codebase: `blocked-by`, `sourced-from`, `supersedes`, `subtask-of`

2. **Canonical vs ergonomic flags** — Add `--sourced-from` and `--subtask-of` as canonical (shown in help). Keep `--source` and `--parent` as hidden ergonomic aliases for common use.

3. **Graph refresh strategy** — Use in-memory insert. After each `write_link_event` call, insert the edge into the in-memory `EdgeStore` so subsequent writes in the same command see correct cardinality.

4. **Link flags on `task start` with existing IDs** — Apply links to the existing task being started. This makes link flags universally useful, not just for quick-start.

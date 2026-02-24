# Workspace Absorption Conflict Resolution UX

**Date**: 2026-02-23
**Status**: Design Complete

## Problem

When workspace absorption detects conflicts, agents must manually parse JJ conflict markers and reconstruct files. This is fragile and error-prone.

## Solution: Auto-resolve append-only conflicts, manual resolution for others

Use JJ's built-in conflict resolution where possible:
1. **First pass**: Try `jj resolve --all` to auto-resolve append-only conflicts
2. **If conflicts remain**: Use `aiki fix <conflict-id>` for manual resolution

## Design

### Conflict ID

The **conflict ID** is the JJ change ID of the conflicted workspace change:
- Unique per conflict
- Points to the conflicted JJ change (created by rebase)
- All conflict info queryable from JJ using this ID
- In code: `conflict_id = ws_change_id` (the workspace's working copy change ID)

### Workflow

```
Turn N:
  Agent closes task xqrm...ssly
  → Turn ends
  → workspace.absorb_all runs
  → Rebase creates conflicted change (conflict_id = ws_change_id)
  → jj resolve --list detects conflicts
  → Run: jj resolve --all (auto-resolves append-only conflicts)
  → Check again: jj resolve --list
  → If no conflicts remain:
      - Absorption continues normally
  → If conflicts remain:
      - Autoreply: "Conflict {conflict_id}. Run: aiki fix {conflict_id}"

Turn N+1 (only if auto-resolve failed):
  Agent runs: aiki fix {conflict_id}
  → Creates subtask using aiki/fix/merge-conflict template
  → Task gets data.conflict_id = {conflict_id}
  → Template instructs agent to query JJ:
      - jj resolve --list -r {{data.conflict_id}} → conflicted files
      - jj diff -r {{data.conflict_id}} → what changed
      - jj log -r {{data.conflict_id}} → change description
      - jj log -r {{data.conflict_id}} (parents) → both conflict sides with task provenance
  → Agent follows template instructions
  → Agent resolves conflicts in files
  → Agent closes subtask
  → Next turn: workspace absorption retries → SUCCESS
```

### Command Behavior

```bash
# Fix a merge conflict (or review task - auto-detected)
aiki fix <id>

# Detection logic:
# 1. Check if JJ change {id} has conflicts: jj resolve --list -r {id}
#    → If yes: treat as conflict, create merge-conflict task with data.conflict_id = {id}
# 2. Check if task {id} exists in task graph
#    → If yes: treat as review, create fix tasks from review
# 3. Error: "No conflict or review task found for ID: {id}"
```

### Auto-Resolution Strategy

During workspace absorption, after detecting conflicts:

```bash
# Try to auto-resolve simple conflicts
jj resolve --all -r {conflict_id}

# Check if any conflicts remain
jj resolve --list -r {conflict_id}
```

**What `jj resolve --all` handles automatically:**
- **Append-only conflicts**: Both sides added different content at the same location
  - JJ keeps both additions in a sensible order
- **Non-overlapping changes**: Changes in different parts of the file
- **Identical changes**: Both sides made the same modification

**What requires manual resolution:**
- **Competing modifications**: Both changed the same lines differently
- **Modification vs deletion**: One side modified, other deleted
- **Semantic conflicts**: Changes that don't overlap textually but conflict logically

### No Metadata Storage Needed!

Originally planned `/tmp/aiki/conflicts/{repo-id}/{conflict_id}.json`, but **NOT NEEDED** - JJ is the source of truth:

- **Conflicted files**: `jj resolve --list -r {conflict_id}`
- **What changed**: `jj diff -r {conflict_id}`
- **Change description**: `jj log -r {conflict_id} -T description`
- **Both conflict parents**: `jj log -r {conflict_id} -T 'parents.map(|c| c.change_id() ++ " " ++ c.description().first_line()).join("\n")'`
  - Shows both sides of the conflict with their task provenance
  - Parent 1: the rebase destination (main branch's latest)
  - Parent 2: the workspace change being rebased
- **File content**: `jj cat -r {conflict_id} path/to/file`

### Autoreply Message

```
CONFLICT RESOLUTION REQUIRED

Workspace absorption detected conflicts during rebase that could not be auto-resolved.

Conflict ID: {conflict_id}
Conflicted files: {file1, file2, ...}

To resolve:
  aiki fix {conflict_id}

This will create a merge-conflict task with instructions.
```

### Task Data

When `aiki fix <conflict-id>` creates the merge-conflict task, it stores the conflict_id in the task's data field:

```
data.conflict_id = <conflict-id>
```

This allows the template to reference `{{data.conflict_id}}` for JJ queries.

### Template Variables

The `aiki/fix/merge-conflict.md` template receives:

```handlebars
{{id}}                    - This task's ID (for closing)
{{data.conflict_id}}      - The JJ change ID with conflicts
```

The template **instructs the agent** to query JJ for all other info:
- Conflicted files: agent runs `jj resolve --list -r {{data.conflict_id}}`
- What changed: agent runs `jj diff -r {{data.conflict_id}}`
- Change description: agent runs `jj log -r {{data.conflict_id}}`
- Both conflict parents: agent runs `jj log -r {{data.conflict_id}} -T 'parents.map(|c| c.change_id() ++ " " ++ c.description().first_line()).join("\n")'`
  - Shows both sides of the conflict with their task provenance (task=... in descriptions)
  - Parent 1: the rebase destination (main branch's latest change)
  - Parent 2: the workspace change being rebased

## Implementation Tasks

1. **Update workspace absorption in isolation.rs**
   - After rebase creates conflicts, run `jj resolve --all`
   - Check if conflicts remain with `jj resolve --list`
   - Only trigger autoreply if conflicts still exist

2. **Update autoreply in hooks.yaml**
   - Show conflict_id and conflicted files
   - Instruct: "Run: aiki fix {conflict_id}"
   - Clarify these are conflicts that could not be auto-resolved

3. **Extend `aiki fix` command**
   - Add detection: check if ID has conflicts via `jj resolve --list -r {id}`
   - If conflicted: create subtask using `aiki/fix/merge-conflict` template
   - Store conflict_id in task data: `task.data.conflict_id = {id}`
   - If not conflicted: fall back to review task logic

4. **Update merge-conflict template**
   - Path: `.aiki/templates/aiki/fix/merge-conflict.md`
   - Use `{{data.conflict_id}}` for JJ queries
   - Instruct agent to query both parents for full context
   - Remove "append-only" from conflict types since those are auto-resolved

## Benefits

- **Fewer manual interventions** - Most common conflicts (append-only) resolve automatically
- **No metadata storage needed** - JJ is source of truth
- **Simpler implementation** - Just query JJ, no JSON serialization
- **Always up-to-date** - Agent sees current conflict state, not stale snapshot
- **Consistent UX** - Uses existing `aiki fix` pattern
- **Trackable** - Conflict resolution becomes a subtask
- **Full context** - Agent sees both sides of conflict with task provenance
- **Reusable** - Task data persists, can be accessed by other agents/commands

## JJ Query Reference

```bash
# Check if change has conflicts
jj resolve --list -r {change_id}

# Auto-resolve simple conflicts (append-only, non-overlapping)
jj resolve --all -r {change_id}

# Get conflicted files
jj resolve --list -r {change_id} | awk '{print $1}'

# See what changed in the conflicted change
jj diff -r {change_id}

# Get change description
jj log -r {change_id} -T description --no-graph

# Get both parent changes (shows which changes are conflicting)
jj log -r {change_id} -T 'parents.map(|c| c.change_id() ++ " " ++ c.description().first_line()).join("\n")' --no-graph

# Get parent change info (what we rebased onto) - shows only first parent
jj log -r {change_id}~ -T 'change_id ++ " " ++ description' --no-graph

# Get file content at conflict state
jj cat -r {change_id} path/to/file
```

## Related

- `.aiki/templates/aiki/fix/merge-conflict.md` - Template for conflict resolution tasks
- `cli/src/session/isolation.rs:264` - `absorb_workspace()` conflict detection
- `cli/src/commands/fix.rs` - Extend to handle conflict IDs

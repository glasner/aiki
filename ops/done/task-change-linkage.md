# Task-to-Change Linkage

**Date**: 2026-01-17
**Status**: Proposed Design
**Purpose**: Track bidirectional linkage between tasks and JJ changes

**Related**: [Code Review Task-Native Design](code-review-task-native.md) - uses `source` for review-to-task linkage

---

## Executive Summary

This design adds **bidirectional linkage** between tasks and JJ changes:

### Direction 1: Change → Task (Provenance)
Track which tasks were in-progress when a JJ change was made:
- **Query by task** - Find all changes made while working on a specific task
- **Context in history** - See task context when viewing JJ log
- **Audit trail** - Track which work was done for which task
- **Task completion evidence** - Know what changes implemented a task

### Direction 2: Task → Source (source)
Track where a task came from:
- **Plan lineage** - Link tasks to the design doc or agent plan that spawned them
- **Review lineage** - Link followup tasks to the code review that found issues
- **Traceability** - Answer "why does this task exist?"

---

## Table of Contents

1. [Current State](#current-state)
2. [The Gap](#the-gap)
3. [Direction 1: Change → Task (Provenance)](#direction-1-change--task-provenance)
4. [Direction 2: Task → Source (source)](#direction-2-task--source-source)
5. [Implementation Plan](#implementation-plan)
6. [Querying](#querying)
7. [Open Questions](#open-questions)

---

## Current State

### Task Storage

Tasks are stored as event-sourced changes on the `aiki/tasks` branch:

```
aiki/tasks branch
├── TaskEvent::Started { task_ids, session_id, ... }
├── TaskEvent::Stopped { task_ids, ... }
├── TaskEvent::Closed { task_ids, ... }
└── ...
```

Each `TaskEvent::Started` records:
- `task_ids` - Which tasks were started
- `session_id` - Session UUID (deterministic from agent + external_id)
- `agent_type` - Which agent started the task

### Provenance Storage

Working copy changes have provenance in their change descriptions:

```
[aiki]
author=claude
author_type=agent
agent_version=0.10.6
client=zed
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]
```

### The Separation

**Currently, these two systems are completely independent:**

| System | Stores | Identifier |
|--------|--------|------------|
| Tasks (`aiki/tasks` branch) | Task events, session claims | Task IDs, Session UUIDs |
| Provenance (`[aiki]` blocks) | Agent info, tool used | Session ID (external) |

**No linkage exists** between task IDs and the working copy changes that implement the task.

---

## The Gap

When an agent makes code changes, we record:
- ✅ Agent type (claude, cursor, etc.)
- ✅ Session ID
- ✅ Tool used (Edit, Write, etc.)
- ✅ Confidence and detection method

But we do NOT record:
- ❌ Which task(s) were in-progress
- ❌ Task context for the change

This means we cannot:
- Query which changes were made for task X
- See task context when viewing JJ history
- Associate completed work with its logical task

---

## Direction 1: Change → Task (Provenance)

### Core Principle

Add `tasks` field to the `[aiki]` provenance block containing the task ID(s) that were in-progress when the change was made.

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│  Agent starts task: aiki task start abc123                  │
│                                                             │
│  Task abc123 marked in-progress (on aiki/tasks branch)      │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Agent makes changes via Edit/Write tools                   │
│                                                             │
│  change.completed event fires                               │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Provenance record includes task context:                   │
│                                                             │
│  [aiki]                                                     │
│  author=claude                                              │
│  session=abc123                                             │
│  tool=Edit                                                  │
│  task=xtuttnyvykpulsxzqnznsxylrzkkqssy                      │  ← NEW
│  [/aiki]                                                    │
└─────────────────────────────────────────────────────────────┘
```

### Multiple Tasks

An agent may have multiple tasks in-progress simultaneously. Each task gets its own `task` line:

```
[aiki]
author=claude
session=abc123
tool=Edit
task=abc123
task=def456
task=ghi789
[/aiki]
```

Tasks are ordered by start time (most recent first).

### Provenance Changes

Add `tasks: Vec<String>` field to `ProvenanceRecord`:
- Serialize as multiple `task=` lines, one per task
- Parse by collecting all `task=` lines
- Omit field entirely if no tasks in-progress

---

## Direction 2: Task → Source (source)

### Core Principle

Add `source` field to `TaskEvent::Created` to track where tasks originate from. This enables:
- Linking tasks to design documents that spawned them
- Linking tasks to agent plans that created them
- Linking followup tasks to code reviews (see [code-review-task-native.md](code-review-task-native.md))

### source Format

The field uses a URI-like prefix to identify the source type:

| Prefix | Meaning | Example |
|--------|---------|---------|
| `file:` | File path (design doc, agent plan) | `file:ops/now/assign-tasks.md` |
| `task:` | Another task (follow-up, review) | `task:xqrmnpst` |
| `comment:` | Specific comment within a task | `comment:c1a2b3c4` |

### Examples

**Task from a design document:**
```
[aiki-task]
event=created
task_id=abc123
name=Implement session tracking
source=file:ops/now/assign-tasks.md
timestamp=2026-01-17T10:30:00Z
[/aiki-task]
```

**Task from an agent plan:**
```
[aiki-task]
event=created
task_id=def456
name=Add error handling to auth module
source=file:.aiki/plans/session-abc123.md
timestamp=2026-01-17T11:00:00Z
[/aiki-task]
```

**Task from a code review (see [code-review-task-native.md](code-review-task-native.md)):**
```
[aiki-task]
event=created
task_id=ghi789
name=Fix null pointer in auth.ts
source=task:xqrmnpst
source=comment:c1a2b3c4
timestamp=2026-01-17T12:00:00Z
[/aiki-task]
```

### Task Type Changes

Add `sources: Vec<String>` field to `TaskEvent::Created`:
- Stored alongside existing fields (task_id, name, priority, assignee)
- Serialized as multiple `source=` lines (one per source)
- Parsed by collecting all `source=` lines

### CLI Usage

```bash
# Create task from a design doc
aiki task add "Implement session tracking" --source file:ops/now/assign-tasks.md

# Create task with multiple sources
aiki task add "Fix auth bug" --source task:xqrmnpst --source comment:c1a2b3c4

# Quick-start with source reference
aiki task start "Implement X" --source file:ops/now/assign-tasks.md

# Query tasks from a specific source
aiki task list --source file:ops/now/assign-tasks.md
```

---

## Implementation Plan

### Phase 1: Add Tasks Field to ProvenanceRecord (Direction 1)

**Goal**: Extend the provenance data model to include task IDs

**Changes**:
- Add `tasks: Vec<String>` field to `ProvenanceRecord`
- Update `to_description()` to serialize tasks
- Update `from_description()` to parse tasks
- Add unit tests for serialization/deserialization

**Files**:
- `cli/src/provenance.rs`

### Phase 2: Query In-Progress Tasks (Direction 1)

**Goal**: Provide a way to get current in-progress task IDs for a session

**Changes**:
- Add `get_in_progress_tasks_for_session(session_id: &str) -> Vec<String>` to task manager
- This queries the `aiki/tasks` branch to find tasks currently claimed by the session

**Files**:
- `cli/src/tasks/manager.rs`

### Phase 3: Wire Up Event Handlers (Direction 1)

**Goal**: Include task IDs when creating provenance records

**Changes**:
- Update `ProvenanceRecord::from_change_completed_event()` to accept task IDs
- Update `change.completed` event handler to query in-progress tasks
- Pass task IDs to provenance record creation

**Files**:
- `cli/src/provenance.rs`
- `cli/src/events/change_completed.rs` (or equivalent handler)
- `cli/src/editors/*/handlers.rs`

### Phase 4: Add Query Commands (Direction 1)

**Goal**: Enable querying changes by task

**Changes**:
- `aiki task show` lists changes by default
- Add `--diff` flag to show full diffs for task changes
- Optionally add `aiki blame --task <id>` to filter blame by task

**Files**:
- `cli/src/commands/task.rs` - Add `--diff` flag to show subcommand
- `cli/src/tasks/manager.rs` - Add query helper

### Phase 5: Add source to Tasks (Direction 2)

**Goal**: Track where tasks originate from

**Changes**:
- Add `sources: Vec<String>` to `TaskEvent::Created`
- Update storage serialization/deserialization (multiple `source=` lines)
- Add `--source` flag to `aiki task add` and `aiki task start` (can be repeated)
- Add `--source` filter to `aiki task list`

**Files**:
- `cli/src/tasks/types.rs` - Add field to `TaskEvent::Created`
- `cli/src/tasks/storage.rs` - Serialize/deserialize multiple `source=` lines
- `cli/src/commands/task.rs` - Add `--source` flag (supports multiple values)

---

## Querying

### Querying Changes by Task (Direction 1)

#### Using JJ Revsets

Once task IDs are in provenance, users can query with JJ:

```bash
# Find all changes for task abc123
jj log -r 'description("task=abc123")'

# Find all changes for task with grep
jj log -T 'change_id ++ " " ++ description' | grep "task=abc123"
```

#### Using aiki CLI

```bash
# Show task details with list of changes (default)
aiki task show abc123

# Show task with full diffs for all changes
aiki task show abc123 --diff
```

#### Query Approach

Use JJ revsets to find changes with matching `task=` in provenance:
```
jj log -r 'description("task=<task_id>")'
```

### Querying Tasks by Source (Direction 2)

#### Using aiki CLI

```bash
# Find all tasks from a design doc
aiki task list --source file:ops/now/assign-tasks.md

# Find all tasks from a code review
aiki task list --source task:xqrmnpst

# Show source for a specific task
aiki task show abc123
# Output includes: source: file:ops/now/assign-tasks.md
```

#### Query Approach

Filter materialized tasks by `sources` field. Support partial matching so `ops/now/assign-tasks.md` matches `file:ops/now/assign-tasks.md`. When filtering, match if ANY source in the task's `sources` list matches the query.

---

## Open Questions

### 1. Task ID Format in Provenance

**Question**: Should we store full 32-char task IDs or abbreviated versions?

**Options**:
- Full ID: `task=xtuttnyvykpulsxzqnznsxylrzkkqssy` (unambiguous but verbose)
- Short ID: `task=xtuttnyv` (8 chars, like git short hashes)

**Recommendation**: Full ID for unambiguous matching. Storage is cheap, and it enables exact revset queries.

### 2. Subtask Handling

**Question**: If working on subtask `parent.1`, should we also record the parent?

**Options**:
- Just subtask: `task=parent.1`
- Both: `task=parent.1` and `task=parent`
- Just parent: `task=parent`

**Recommendation**: Just the subtask (`parent.1`). Queries can expand to parent if needed. Simpler is better.

### 3. Task Order

**Question**: When multiple tasks are in-progress, what order?

**Options**:
- Start time (oldest first)
- Start time (newest first)
- Alphabetical by ID

**Recommendation**: Newest first (most recently started task is likely the primary context).

### 4. Retroactive Linkage

**Question**: Should we backfill task IDs for existing changes?

**Options**:
- Yes, infer from session_id + timestamp
- No, only apply going forward

**Recommendation**: No backfill. The data doesn't exist reliably for past changes. Clean slate going forward.

### 5. Empty Tasks List (Direction 1)

**Question**: What if no tasks are in-progress when a change is made?

**Behavior**: Omit the `task=` field entirely (same as `coauthor` when absent). This distinguishes "no tasks" from "tasks unknown".

### 6. Multiple source Values (Direction 2)

**Question**: Should tasks support multiple `source` values?

**Context**: Code reviews need this pattern (see [code-review-task-native.md](code-review-task-native.md)):
```yaml
source: task:xqrmnpst
source: comment:c1a2b3c4
```

**Options**:
- Single value: `Option<String>` - simpler, but doesn't support review use case
- Multiple values: `Vec<String>` - more expressive, aligns with review system, matches `task=` field pattern

**Recommendation**: Use `Vec<String>` to support multiple sources from the start. Tasks can originate from multiple places (e.g., mentioned in a design doc AND created from a code review comment).

### 7. File Path Resolution (Direction 2)

**Question**: Should file paths be absolute or relative?

**Options**:
- Relative to repo root: `file:ops/now/assign-tasks.md`
- Absolute: `file:/Users/glasner/code/aiki/ops/now/assign-tasks.md`

**Recommendation**: Relative to repo root. Absolute paths break when repos are cloned elsewhere.

### 8. Prefix Requirement (Direction 2)

**Question**: Should the `file:` prefix be required or optional in CLI?

**Options**:
- Required: `--source file:ops/now/assign-tasks.md`
- Optional: `--source ops/now/assign-tasks.md` (auto-detects file paths)

**Recommendation**: Required in both CLI and storage. This keeps the interface explicit and prevents ambiguity.

---

## Summary

This design adds **bidirectional linkage** between tasks and changes:

### Direction 1: Change → Task (via Provenance)
- Add `tasks` field to `[aiki]` provenance blocks
- Query in-progress tasks at write time
- Find changes by task via JJ revsets or CLI

### Direction 2: Task → Source (via source)
- Add `sources` field to `TaskEvent::Created` (supports multiple sources)
- Link tasks to design docs, agent plans, and code reviews
- Query tasks by source file or review

### Implementation Phases

1. **Phase 1-4**: Direction 1 (provenance `task=` field)
2. **Phase 5**: Direction 2 (`sources` field with multiple `source=` lines)

### Benefits

- **Forward traceability**: "What changes implemented this task?"
- **Backward traceability**: "Where did this task come from?"
- **Accountability**: Task completion is evidenced by actual changes
- **Plan lineage**: Connect tasks to their source documents
- **Review integration**: Consistent with [code-review-task-native.md](code-review-task-native.md)

### Non-Goals

- **Automatic task closure**: Changes don't auto-close tasks
- **Change validation**: We don't verify that changes match task scope
- **Plan content capture**: We store file references, not content (for now)

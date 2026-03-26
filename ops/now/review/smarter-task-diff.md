# Smarter Task Diff: Filter Inherited Workspace State

**Status:** Investigation complete, ready for implementation

## Problem

`aiki task diff` includes files that were not created or modified by the task — they were inherited from the workspace's prior state. This causes reviewers to flag unrelated files as scope leaks.

## Observed Incident

The fix loop on epic `tnnuxwm` ("Fix spurious Released event warnings") ran through 6 review→fix rounds. The final review (`uuvpxpo`) flagged `ops/now/tui/workflow-commands-thin.md` (246 lines) as an unrelated file included in the diff.

### Root Cause

Session `9f5eb275` (workspace `aiki-9f5eb275`) ran planning work for the workflow module split. One of its tasks (`nlwpqrx` — "Create phase 2 plan: move orchestration + helpers into workflow/") created `workflow-commands-thin.md` via change `vukxoqzm`.

Later, the fix epic's subtask agents were spawned into the same workspace (or a workspace whose tip descended from `vukxoqzm`). The file was already present when the fix work began. By subtask 8 (`qmqknxmy`), `vukxoqzm` was a direct ancestor — confirmed via:

```
jj log -r 'vukxoqzm & ::lnwvlqvxnrvv'  # returns vukxoqzm — it IS an ancestor
```

The full ancestry chain from `vukxoqzm` to the final fix subtask spans ~150 changes.

### Impact

- The final review spent effort evaluating a 246-line plan file that had nothing to do with the fix
- The fix loop spawned a "Plan Fix" task (`wnsypqr`) to address the review finding — wasted work
- Reviewers lose trust in diff scope when unrelated files appear

## Proposed Fix

### Option A: Tag-Based Filtering (Recommended)

`aiki task diff` already knows the task ID. Each JJ change carries `task=<id>` in its description metadata. The diff should:

1. Collect the set of task IDs in scope (the target task + its subtasks)
2. Walk the changes, filtering to only those whose `task=` tag matches
3. Diff only those changes, excluding inherited workspace state

This is precise — it uses the provenance metadata that's already being written.

### Option B: Timestamp-Based Filtering

Use the task's `Started` event timestamp as a lower bound. Only include changes committed after the task started. Simpler but less precise — could include unrelated concurrent work in the same workspace.

### Option C: Workspace Snapshot at Task Start

When `aiki task start` runs, record the current workspace tip (JJ change ID) as metadata on the task. `aiki task diff` then diffs from that snapshot forward. Clean but requires a schema addition to task metadata.

## Key Evidence

| Change | Session | Task | File |
|---|---|---|---|
| `vukxoqzm` | `9f5eb275` | `nlwpqrx` (workflow planning) | Created `workflow-commands-thin.md` |
| `lnwvlqvx` | `9258d0d3` | `qmqknxmy` (fix round 6) | Inherited the file via ancestry |

## Secondary Benefit

Fixing this also improves `aiki review` accuracy — reviews that use `task diff` to scope their analysis won't waste time on inherited files, reducing false positives and unnecessary fix rounds.

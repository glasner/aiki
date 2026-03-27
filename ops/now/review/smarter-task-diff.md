# Smarter Task Diff: Pure Task Diffs via Snapshot + File Filtering

**Status:** Ready for implementation

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

## Chosen Approach: Snapshot Baseline + File Filtering

Combines workspace snapshot (Option C) with tag-based file filtering (from Option A) for pure task diffs.

### Why not the original options alone?

- **Option A (tag-based filtering only):** Tried previously — interleaved non-task changes between tagged changes produce broken diffs. JJ diffs tree snapshots, so `parents(roots(...))` → `heads(...)` includes everything in between.
- **Option B (timestamp-based):** Too imprecise — includes concurrent unrelated work.
- **Option C (snapshot only):** Solves inherited state but still includes interleaved non-task changes to unrelated files.

### How it works

**Step 1: Record snapshot at task start**

Add a `working_copy: Option<String>` field to the `Started` event. When `aiki task start` runs:

- Call `get_working_copy_change_id(cwd)` to capture the current workspace tip (`@`)
- Store the result in `Started.working_copy`

This records the JJ change ID at the moment work actually begins — not when the task is created. Tasks can be created long before they're started (batch `task add`), so the snapshot must reflect the workspace state when the agent begins making changes.

Note: `Created` has an existing `working_copy` field (intended for historical template lookup) that is dead code — never populated (always `None`), never read (destructured as `_`). Remove it as part of this work to avoid confusion with the new `Started.working_copy`. Safe to remove: the deserializer already handles absent `Option` fields, so existing events with `working_copy` in their serialized form will just ignore it.

**Step 2: Collect task-touched files**

Use the existing task revset pattern (tag-based matching) to find all changes belonging to the task and its subtasks, then extract only the files those changes touched:

```
jj log -r '<task_pattern>' --no-graph -T '' --summary
```

Parse the output to get the set of file paths.

**Step 3: Diff snapshot → heads, scoped to task files**

```
jj diff --from <snapshot> --to heads(<task_pattern>) -- file1 file2 ...
```

This gives a pure task diff because:
1. **Snapshot baseline** eliminates inherited workspace state (the primary bug)
2. **File filtering** eliminates interleaved non-task changes to unrelated files
3. The only remaining edge case is a non-task change modifying the *same file* a task touched — rare and arguably relevant context

### Implementation checklist

1. **Remove dead `working_copy` from `Created` event**
   - Remove `working_copy: Option<String>` field from `Created` variant in `cli/src/tasks/types.rs`
   - Remove serialization of `working_copy` in `cli/src/tasks/storage.rs`
   - Remove deserialization of `working_copy` in `cli/src/tasks/storage.rs` (keep tolerant parsing so old events don't break)
   - Remove all `working_copy: None` / `working_copy: _` from match arms in `graph.rs`, `manager.rs`, `spawner.rs`, `lanes.rs`, `storage.rs`, etc.

2. **Add `working_copy` to `Started` event**
   - Add `working_copy: Option<String>` field to `Started` variant in `cli/src/tasks/types.rs`
   - Update serialization in `cli/src/tasks/storage.rs` (write `working_copy` metadata)
   - Update deserialization in `cli/src/tasks/storage.rs` (parse `working_copy` field)
   - In the task start path, call `get_working_copy_change_id(cwd)` and pass to `Started { working_copy: Some(...), ... }`

3. **Update `run_diff()` in `cli/src/commands/task.rs`**
   - Read the task's `Started` event to get `working_copy` (snapshot change ID)
   - If present, use it as `--from` baseline instead of `parents(roots(pattern))`
   - Collect file list from task-tagged changes via `jj log --summary`
   - Pass file list as path arguments to `jj diff`
   - Fall back to current behavior if `working_copy` is `None` (backward compat for tasks started before this change)

4. **Handle subtask diffs**
   - When diffing a parent task, use the parent's snapshot as baseline
   - Include subtask changes in both the file list and `--to` revset
   - This works because subtask changes merge back into the workspace lineage

### Files to modify

- `cli/src/tasks/types.rs` — remove `working_copy` from `Created`, add to `Started`
- `cli/src/tasks/storage.rs` — update serialization/deserialization for both events
- `cli/src/tasks/graph.rs` — remove `working_copy` from `Created` match arms
- `cli/src/tasks/manager.rs` — remove from `Created` construction, populate on `Started`
- `cli/src/tasks/spawner.rs` — remove `working_copy` from `Created` construction
- `cli/src/tasks/lanes.rs` — remove `working_copy` from `Created` match arms
- `cli/src/tasks/templates/data_source.rs` — remove `working_copy` from `Created` construction
- `cli/src/commands/task.rs` — update `run_diff()` with new approach
- `cli/src/tasks/templates/resolver.rs` — `get_working_copy_change_id()` already exists, no changes needed

## Key Evidence

| Change | Session | Task | File |
|---|---|---|---|
| `vukxoqzm` | `9f5eb275` | `nlwpqrx` (workflow planning) | Created `workflow-commands-thin.md` |
| `lnwvlqvx` | `9258d0d3` | `qmqknxmy` (fix round 6) | Inherited the file via ancestry |

## Secondary Benefit

Fixing this also improves `aiki review` accuracy — reviews that use `task diff` to scope their analysis won't waste time on inherited files, reducing false positives and unnecessary fix rounds.

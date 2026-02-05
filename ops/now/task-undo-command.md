# Design: `aiki task undo` Command

**Date**: 2026-02-05
**Status**: Design
**Related**: [Plan and Build Commands](plan-and-build-commands.md)

---

## Overview

Implement `aiki task undo` - a command that reverts file changes made by a task or set of tasks. This provides a reusable primitive for:

- Manual task rollback ("oops, that change was wrong")
- `aiki plan --restart` implementation (revert completed subtasks)
- `aiki build --restart` implementation (revert completed subtasks)
- Undoing work before closing a task as `wont_do`

**Key insight:** By leveraging the existing `aiki task diff` infrastructure, we can safely revert changes while detecting conflicts.

---

## Command Syntax

```bash
aiki task undo <task-id> [options]          # Undo a single task
aiki task undo <task-id1> <task-id2> ...    # Undo multiple tasks
aiki task undo <plan-id> --completed        # Undo all completed subtasks of a plan
```

**Arguments:**
- `<task-id>` - Task ID(s) to undo (32 lowercase letters)
- `<plan-id>` - Plan task ID (when using `--completed` flag)

**Options:**
- `--completed` - For plan tasks: undo all completed subtasks (not the plan itself)
- `--force` - Force undo despite conflicts (may lose manual edits)
- `--dry-run` - Show what would be undone without making changes
- `--backup` - Create backup branch before undoing (default: true)
- `--no-backup` - Skip backup branch creation

---

## Behavior

### Single Task Undo

```bash
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo
```

**Steps:**

1. **Verify task exists** and has changes
2. **Get modified files** using `aiki task diff <task-id> --summary`
3. **Compute baseline** using `parents(roots(task=<task-id>))`
4. **Detect conflicts** (see Conflict Detection below)
5. **Create backup branch** (unless `--no-backup`)
6. **Restore files** to baseline state using `jj restore`
7. **Output summary** of reverted files

**Example output:**

```
Creating backup: aiki/undo-backup-xnukpkuy (safe to delete later)

Undoing task xnukpkuyxzvrskvwtlxkppmmutwvysvo
  "Add authentication middleware"

Files reverted (3):
  M src/auth.rs         (restored to previous state)
  M src/middleware.rs   (restored to previous state)
  A tests/auth_test.rs  (file removed)

✓ Task changes undone successfully
```

### Multiple Tasks Undo (Union-Undo)

```bash
aiki task undo task1 task2 task3
```

**Behavior:**
- Compute a **union** of all tasks being undone (order-independent)
- For each file, restore to the state before the **first** task (among those being undone) touched it
- Detect conflicts by comparing working copy to the **combined final state** of all tasks being undone
- This avoids false conflicts when later tasks modified files that earlier tasks also touched

**Algorithm:**
1. Collect all files modified by any of the tasks being undone
2. For each file, find the earliest baseline (parent of first change among tasks)
3. Compute combined final state: `heads(task=id1 | task=id2 | task=id3)`
4. Conflict = working copy differs from combined final state (someone else changed it after)
5. Restore each file from its per-file baseline

**Order Independence:** The command argument order (`task1 task2 task3` vs `task3 task1 task2`) produces identical results because we compute baselines based on commit topology, not argument order.

### Plan Subtasks Undo

```bash
aiki task undo <plan-id> --completed
```

**Use case:** Undo all completed subtasks of a plan (for `--restart` implementation)

**Steps:**

1. **Verify plan exists** and is a parent task
2. **Query completed subtasks** with status `completed`
3. **Collect all modified files** from completed subtasks (union of all changes)
4. **Compute per-file baselines** - for each file, find the parent of the first subtask that touched it
5. **Detect conflicts** using union-undo algorithm (compare working copy to combined final state)
6. **Check for in-progress subtask conflicts** - abort if pending subtasks have dirty changes to affected files
7. **Create backup branch** (points to current working copy)
8. **Restore files** from their per-file baselines
9. **Output summary** showing which subtasks were undone

**⚠️ Important:** This restores each file to its state before the first completed subtask touched it. If there were manual edits or other task changes interleaved with the subtasks, those will also be reverted. Use `--dry-run` first to verify.

**Example output:**

```
Creating backup: aiki/undo-backup-nzwtoqqr

Undoing 3 completed subtasks from plan nzwtoqqrluppzupttosl:
  ✓ Subtask 1: Add database schema
  ✓ Subtask 2: Create API endpoints
  ✓ Subtask 3: Add middleware

Files reverted (6):
  M src/db/schema.rs
  A src/db/migrations/001.sql
  M src/api/endpoints.rs
  A src/api/auth.rs
  M src/middleware.rs
  A tests/integration_test.rs

✓ All subtask changes undone successfully
```

---

## Conflict Detection

Before undoing, check if files have been modified since the task(s) completed:

### Detection Algorithm (Single Task)

For each file modified by the task:

1. **Get task's final state** from `heads(task=<id>)`
2. **Get current working state** from working copy
3. **Compare:** If file differs from task's final state → conflict

### Detection Algorithm (Multiple Tasks)

For union-undo of multiple tasks:

1. **Compute combined final state:** `heads(task=id1 | task=id2 | ...)`
2. **For each file** modified by any task being undone:
   - Get file content from combined final state
   - Get file content from working copy
   - If they differ → conflict (someone else changed it after these tasks)
3. This correctly handles tasks that touched the same file sequentially

### Conflict Scenarios

| Scenario | Detection | Behavior |
|----------|-----------|----------|
| File modified after task completed | Working copy ≠ combined final state | **Abort** with error |
| File deleted after task created it | File doesn't exist, task added it | **Skip** (already gone) |
| File created after task deleted it | File exists, task deleted it | **Abort** with error |
| Multiple tasks modified same file | Union-undo: compare to combined final state | Revert all (if no conflicts) |
| In-progress task modified file | See "In-Progress Task Check" below | **Abort** with error |

### In-Progress Task Check

Before undoing, detect if any **other** in-progress tasks have modified the same files:

```bash
# Get files modified by in-progress tasks (not being undone)
jj log -r 'description(glob:"aiki:task:*") & ~description(glob:"aiki:task:*:closed")' --no-graph -T 'change_id'
```

**Algorithm:**
1. Query all in-progress task change IDs (have task marker but no `:closed` marker)
2. Exclude the task(s) being undone from this set
3. For each remaining in-progress task, get its modified files via `jj diff`
4. Intersect with files to be undone
5. If intersection is non-empty → **conflict** (unless `--force`)

**Error message:**
```
Error: Cannot undo - in-progress tasks have modified the same files

In-progress tasks affecting these files:
  - Task abc123: "Implement feature X" modified src/auth.rs
  - Task def456: "Fix bug Y" modified src/middleware.rs

Options:
  1. Complete or stop those tasks first
  2. Use --force to undo anyway (may cause issues for in-progress work)
```

### Conflict Error Example

```bash
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo

Error: Cannot undo task due to conflicts

Files modified after task completed:
  - src/auth.rs (task modified, then manually edited)
  - src/middleware.rs (task created, then manually edited)

Suggestions:
  1. Review changes: git diff src/auth.rs
  2. Stash manual edits: git stash
  3. Commit current changes first
  4. Use --force to undo anyway (WARNING: loses manual edits)

To see what would be undone: aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo --dry-run
```

### Force Undo

```bash
aiki task undo <task-id> --force
```

**Behavior:**
- Skip conflict detection
- Revert files even if modified after task
- **Warning:** May lose manual edits
- Still creates backup branch (unless `--no-backup`)

---

## Dry Run Mode

```bash
aiki task undo <task-id> --dry-run
```

**Output:**

```
[DRY RUN] Would undo task xnukpkuyxzvrskvwtlxkppmmutwvysvo
  "Add authentication middleware"

Files that would be reverted (3):
  M src/auth.rs         → restore to previous state
  M src/middleware.rs   → restore to previous state
  A tests/auth_test.rs  → remove file

No conflicts detected ✓

To perform this undo: aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo
```

---

## Backup Branches

By default, create a safety backup before undoing:

### Backup Branch Naming

```bash
# Format: always includes timestamp to avoid collisions
aiki/undo-backup-<YYYYMMDD-HHMMSS>-<short-task-id>

# For multiple tasks, use hash of all task IDs
aiki/undo-backup-<YYYYMMDD-HHMMSS>-<hash8>
```

### What Gets Backed Up

The backup branch points to the **current working copy commit** (before restore), not the baseline. This allows you to recover your exact state if the undo was a mistake.

**Example:**
```bash
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo

Creating backup: aiki/undo-backup-20260205-143022-xnukpkuy
  (points to current working copy: abc123def)
```

**To recover from undo:**
```bash
# List undo backups
jj branch list | grep aiki/undo-backup

# Restore to pre-undo state
jj new aiki/undo-backup-20260205-143022-xnukpkuy
```

**Cleanup:**
```bash
# User can delete backup later if satisfied
jj branch delete aiki/undo-backup-20260205-143022-xnukpkuy
```

**Skip backup:**
```bash
aiki task undo <task-id> --no-backup
```

---

## Implementation Notes

### Reusing `task diff` Infrastructure

The `run_diff` function in `cli/src/commands/task.rs` already implements baseline computation:

```rust
// From run_diff():
let from_revset = format!("parents(roots({}))", pattern);
let to_revset = format!("heads({})", pattern);
```

**For `task undo`:**
- Reuse `from_revset` to compute baseline
- Reuse file change detection from `--summary` mode
- Add conflict detection by comparing working copy with `to_revset`

### Core Implementation Steps

1. **Compute per-file baselines for union-undo:**
   ```rust
   fn get_per_file_baselines(
       cwd: &Path,
       task_ids: &[String]
   ) -> Result<HashMap<PathBuf, String>> {
       // For each file modified by any task:
       // 1. Find which tasks touched it
       // 2. Find the earliest task (by commit order)
       // 3. Baseline = parent of that task's first commit touching this file
       // Returns map of file -> baseline revset
   }
   ```

2. **Compute combined final state:**
   ```rust
   fn get_combined_final_state(task_ids: &[String]) -> String {
       // heads(task=id1 | task=id2 | ...)
       let patterns: Vec<String> = task_ids.iter()
           .map(|id| build_task_revset_pattern(id))
           .collect();
       format!("heads({})", patterns.join(" | "))
   }
   ```

3. **Add conflict detection function:**
   ```rust
   fn detect_conflicts(
       cwd: &Path,
       task_ids: &[String],
       files: &[PathBuf]
   ) -> Result<Vec<PathBuf>> {
       // Compare working copy with combined final state
       // Return files that differ (modified by someone else after tasks)
   }
   ```

4. **Check for in-progress task conflicts:**
   ```rust
   fn check_in_progress_conflicts(
       cwd: &Path,
       task_ids: &[String],  // tasks being undone
       files: &[PathBuf]
   ) -> Result<Vec<(String, PathBuf)>> {
       // Find in-progress tasks not in task_ids
       // Get their modified files
       // Return (task_id, file) pairs that conflict
   }
   ```

5. **Add restore function:**
   ```rust
   fn restore_files(
       cwd: &Path,
       file_baselines: &HashMap<PathBuf, String>
   ) -> Result<()> {
       // For each file, jj restore --from <baseline> <file>
       // Group by baseline to minimize jj calls
   }
   ```

6. **Add backup creation:**
   ```rust
   fn create_backup_branch(
       cwd: &Path,
       task_ids: &[String]
   ) -> Result<String> {
       let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
       let suffix = if task_ids.len() == 1 {
           task_ids[0][..8].to_string()
       } else {
           // Hash of all task IDs
           hash_task_ids(task_ids)[..8].to_string()
       };
       let branch_name = format!("aiki/undo-backup-{}-{}", timestamp, suffix);
       // jj branch create <branch_name> -r @
   }
   ```

### Integration with `--restart`

The `aiki plan --restart` and `aiki build --restart` commands can use `task undo` internally:

```rust
// In plan.rs or build.rs:
if restart {
    // Get completed subtasks
    let completed_subtasks = get_completed_subtasks(plan_id)?;
    
    // Undo them
    if !completed_subtasks.is_empty() {
        run_undo(
            cwd,
            &completed_subtasks.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
            false, // force
            false, // dry_run
            true,  // backup
        )?;
    }
    
    // Close old plan and create new one
    close_task(plan_id, "Restarted by user")?;
    create_new_plan(spec_path)?;
}
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Task doesn't exist | Error: "Task not found: <id>" |
| Task has no changes | Error: "Task has no file changes to undo" |
| Conflicts detected (no `--force`) | Abort with detailed conflict list |
| JJ command fails | Return JJ error with context |
| Backup branch already exists | Use timestamped name instead |
| Invalid task ID format | Error: "Invalid task ID format" |
| Plan task with no subtasks | Error: "Plan has no subtasks to undo" |

---

## Output Format

**Stdout (machine-readable):**
```xml
<aiki_task_undo status="ok">
  <undone task="xnukpkuyxzvrskvwtlxkppmmutwvysvo" files="3"/>
  <backup branch="aiki/undo-backup-xnukpkuy"/>
</aiki_task_undo>
```

**Stderr (human-readable):**
```
Creating backup: aiki/undo-backup-xnukpkuy

Undoing task xnukpkuyxzvrskvwtlxkppmmutwvysvo
  "Add authentication middleware"

Files reverted (3):
  M src/auth.rs
  M src/middleware.rs
  A tests/auth_test.rs

✓ Task changes undone successfully
```

---

## Usage Examples

### Example 1: Undo a Single Task

```bash
# Made a mistake in implementation
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo

# Check what was undone
aiki task diff xnukpkuyxzvrskvwtlxkppmmutwvysvo
```

### Example 2: Restart a Plan (Internal Usage)

```bash
# User runs:
aiki plan ops/now/feature.md --restart

# Internally, this does:
# 1. aiki task undo <plan-id> --completed
# 2. aiki task close <plan-id> --wont-do
# 3. Create new plan from spec
```

### Example 3: Preview Before Undoing

```bash
# See what would be undone
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo --dry-run

# If satisfied, perform the undo
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo
```

### Example 4: Force Undo with Conflicts

```bash
# Try to undo
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo
# Error: conflicts detected

# Force it anyway (loses manual edits)
aiki task undo xnukpkuyxzvrskvwtlxkppmmutwvysvo --force

# If needed, restore from backup
jj new aiki/undo-backup-xnukpkuy
```

---

## Prerequisites

- `aiki task diff` infrastructure ✅ (already implemented)
- Baseline computation via `parents(roots(task=<id>))` ✅ (already implemented)
- JJ `restore` command access
- Ability to query subtasks by parent ID ✅ (already implemented)

---

## Files to Create/Modify

### New Files
- `cli/src/commands/task/undo.rs` - Undo command implementation

### Modified Files
- `cli/src/commands/task.rs` - Add undo subcommand, export helper functions
- `cli/src/commands/plan.rs` - Use `task undo` for `--restart`
- `cli/src/commands/build.rs` - Use `task undo` for `--restart`

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_undo_single_task() {
    // Create task with changes
    // Undo task
    // Verify files restored
}

#[test]
fn test_undo_detects_conflicts() {
    // Create task with changes
    // Manually edit files
    // Attempt undo
    // Verify conflict error
}

#[test]
fn test_undo_completed_subtasks() {
    // Create plan with 3 completed, 2 pending subtasks
    // Undo completed subtasks
    // Verify only completed changes reverted
}
```

### Integration Tests

```bash
# Test full workflow
aiki task add "Test task"
aiki task start <id>
# Make changes
aiki task close <id>
aiki task undo <id>
# Verify changes reverted
```

---

## Future Enhancements (v2)

**Selective Undo:**
```bash
# Undo only specific files from a task
aiki task undo <task-id> --files src/auth.rs src/middleware.rs
```

**Undo History:**
```bash
# List recent undos
aiki task undo --list

# Redo an undo (restore from backup)
aiki task undo --redo <undo-id>
```

**Interactive Mode:**
```bash
# Interactively choose which files to revert
aiki task undo <task-id> --interactive
```

---

## Benefits

1. **Reusable Primitive** - Powers both manual undo and `--restart` flag
2. **Safe by Default** - Detects conflicts, creates backups
3. **Transparent** - Shows exactly what will be undone
4. **Leverages Existing Infrastructure** - Reuses `task diff` baseline computation
5. **Composable** - Works with single tasks, multiple tasks, or plan subtasks

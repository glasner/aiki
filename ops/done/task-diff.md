# Task Diff Command

**Date**: 2026-01-29
**Status**: Planning
**Purpose**: Native `aiki task diff` command to show changes made while working on a task

**Related Documents**:
- [Task-to-Change Linkage](../done/task-change-linkage.md) - Bidirectional task/change tracking via provenance
- [Review and Fix Commands](review-and-fix.md) - Uses task diffs for code review

---

## Executive Summary

Add native `aiki task diff <task-id>` command to show code changes made while working on a task. Combined with enhanced `aiki task show` for closed tasks, this provides a streamlined review workflow.

**Key Features**:
- `aiki task diff` - Pure diff output (no wrapper), pass-through to jj
- `aiki task show` - For closed tasks, includes diff summary alongside task metadata
- Baseline derived from jj history via provenance - no stored state needed
- Handles parent tasks with subtasks automatically

---

## Motivation

Currently, task instructions tell agents to run raw jj commands:

```yaml
instructions: |
  Examine the code changes to understand what was modified.
  
  Commands to use:
  - `jj diff --revision @` - Show full diff of working copy
  - `jj show @` - Show change description and summary
  - `jj log -r @` - Show change in log context
```

**Problems**:
1. **Not task-aware**: `jj diff -r @` only shows working copy, not all changes for a task
2. **Manual correlation**: Agent must manually find which changes belong to the task
3. **Incomplete for subtasks**: Parent tasks with subtasks have changes spread across multiple change_ids
4. **Verbose instructions**: Need to explain jj commands instead of task-oriented workflow

**Solution**: `aiki task diff <task-id>` provides task-centric diff viewing.

---

## Design

### Command Interface

```bash
aiki task diff <task-id> [options]
```

**Arguments**:
- `<task-id>` - Task ID to show diff for (required)

**Options**:
- `-s, --summary` - Show summary (file paths with +/- counts)
- `--stat` - Show histogram of changes
- `--name-only` - Show only changed file names

**Default**: Show full diff with jj's native format

### How It Works

Uses jj revsets to derive baseline from provenance metadata - no stored state needed.

1. **Build revset pattern for task**:
   ```bash
   # For single task
   description("task=<task-id>")

   # For parent task with subtasks (matches task=123, task=123.1, task=123.2, etc.)
   description("task=<task-id>") | description("task=<task-id>.")
   ```

2. **Find baseline and final via revset**:
   - `roots(pattern)` - earliest change(s) for the task
   - `parents(roots(pattern))` - state before task started (baseline)
   - `heads(pattern)` - latest change(s) for the task (final)

3. **Generate diff between baseline and final**:
   ```bash
   jj diff \
     --from 'parents(roots(<pattern>))' \
     --to 'heads(<pattern>)'
   ```

4. **Format output**:
   - Default: jj's native format (file paths, line numbers, context)
   - `--summary`: File names with +/- counts
   - `--stat`: Histogram of changes

**Key insight**: This shows the **net result** of all task work, not individual changes. If an agent edited a file 3 times, you see the final diff, not 3 separate diffs.

### Output Format

**Pure diff output** - no XML wrapper, just jj's native output.

**Default format**:
```
Modified regular file src/auth.ts:
   40   40: function validateUser(user) {
   41   41:   if (!user) {
   42     +:     throw new Error('User is null');
   43     +:   }
   44   42:   return user.name;
   45   43: }

Modified regular file src/middleware.ts:
   10     +: import { validateUser } from './auth';
   ...
```

**Summary format** (`--summary`):
```
src/auth.ts       | 7 +++++--
src/middleware.ts | 4 +++-
2 files changed, 8 insertions(+), 3 deletions(-)
```

**Key point**: This is the **net result**. If a file was edited multiple times during the task, you see the final diff, not intermediate steps.

---

## Integration with Review Workflow

### The Review Flow

**Step 1: Understand task + scope** (one command)
```bash
aiki task show xqrmnpst
```
```xml
<aiki_task cmd="show" status="ok">
  <task id="xqrmnpst" name="Implement auth validation" status="completed" priority="p2">
    <subtasks>
      <task id="xqrmnpst.1" status="completed" name="Add validation function"/>
      <task id="xqrmnpst.2" status="completed" name="Update middleware"/>
    </subtasks>
    <progress completed="2" total="2" percentage="100"/>
    <files_changed total="2">              <!-- NEW: diff summary for closed tasks -->
      <file path="src/auth.ts" insertions="7" deletions="2" />
      <file path="src/middleware.ts" insertions="4" deletions="1" />
    </files_changed>
  </task>
  <context>
    <in_progress/>
    <list ready="0"/>
  </context>
</aiki_task>
```

**Step 2: Read the code** (when ready to dive in)
```bash
aiki task diff xqrmnpst
```
```
Modified regular file src/auth.ts:
   40   40: function validateUser(user) {
...
```

### Updated Review Template

**Before** (manual jj commands):
```yaml
instructions: |
  Examine the code changes to understand what was modified.

  Commands to use:
  - `jj diff --revision @` - Show full diff of working copy
  - `jj show @` - Show change description and summary
  - `jj log -r @` - Show change in log context
```

**After** (task-aware commands):
```yaml
instructions: |
  Review the completed task.

  1. Run `aiki task show ${task_id}` to see task details and change summary
  2. Run `aiki task diff ${task_id}` to read the actual code changes
```

### Benefits for Review

1. **Two-command workflow**: `show` for context + scope, `diff` for code
2. **Automatic task correlation**: No need to manually find which changes belong to the task
3. **Subtask aggregation**: Parent task diffs automatically include all subtask changes
4. **Clean output**: No XML wrapper on diff - just the diff

---

## Use Cases

### Use Case 1: Agent Reviewing a Completed Task

```yaml
instructions: |
  Review the implementation for task ${parent_task_id}.

  1. Run `aiki task show ${parent_task_id}` to understand:
     - What the task was supposed to do
     - What files were changed (summary)
  2. Run `aiki task diff ${parent_task_id}` to read the actual changes
  3. Report any issues found
```

Agent workflow:
```bash
# Step 1: Get context + scope
aiki task show xqrmnpst
# Shows: task details, instructions, subtasks, AND diff summary

# Step 2: Read the code
aiki task diff xqrmnpst
# Shows: full diff output
```

### Use Case 2: Human Debugging a Task

User wants to see what an agent changed:

```bash
aiki task show xqrmnpst    # Quick overview
aiki task diff xqrmnpst    # Full details
```

### Use Case 3: Parent Task with Subtasks

Parent task `xqrmnpst` has subtasks `.1`, `.2`, `.3`:

```bash
aiki task diff xqrmnpst
```

Automatically includes changes from all subtasks.

---

## Implementation

### Phase 1: `aiki task diff` Command

**Files**:
- `cli/src/commands/task.rs` - Add `diff` subcommand

**Functionality**:
- Build revset pattern from task ID
- Execute `jj diff --from <baseline> --to <final>` using revsets
- Output jj's native output directly (no wrapper)

**Core logic**:
```bash
jj diff \
  --from 'parents(roots(description("task=<id>")))' \
  --to 'heads(description("task=<id>"))'
```

### Phase 2: Enhance `aiki task show` for Closed Tasks

**Files**:
- `cli/src/commands/task.rs` - Modify `show` subcommand

**Functionality**:
- For closed/completed tasks, append `<files_changed>` element to XML output
- Parse diff stats from jj to extract insertions/deletions per file
- Use same revset logic as `diff` command

**Output addition**:
```xml
<files_changed total="2">
  <file path="src/auth.ts" insertions="7" deletions="2" />
  <file path="src/middleware.ts" insertions="4" deletions="1" />
</files_changed>
```

---

## Technical Details

### Building the Revset Pattern

```rust
fn build_task_pattern(task_id: &str, include_subtasks: bool) -> String {
    if include_subtasks {
        // Match task=123 AND task=123.1, task=123.2, etc.
        format!(
            "description(\"task={}\") | description(\"task={}.\")",
            task_id, task_id
        )
    } else {
        format!("description(\"task={}\")", task_id)
    }
}
```

### Generating the Diff (Baseline → Final)

```rust
fn generate_task_diff(
    repo_path: &Path,
    task_id: &str,
    include_subtasks: bool,
    format: DiffFormat,
) -> Result<String> {
    let pattern = build_task_pattern(task_id, include_subtasks);

    // Derive baseline from jj history:
    // - roots(pattern) = earliest changes for task
    // - parents(roots(...)) = state before task started
    // - heads(pattern) = latest changes for task
    let from_revset = format!("parents(roots({}))", pattern);
    let to_revset = format!("heads({})", pattern);

    let mut cmd = Command::new("jj");
    cmd.arg("diff")
        .arg("--from").arg(&from_revset)
        .arg("--to").arg(&to_revset)
        .current_dir(repo_path);

    match format {
        DiffFormat::Default => {},
        DiffFormat::Summary => { cmd.arg("--summary"); },
        DiffFormat::Stat => { cmd.arg("--stat"); },
        DiffFormat::NameOnly => { cmd.arg("--name-only"); },
    }

    let output = cmd.output()?;
    Ok(String::from_utf8(output.stdout)?)
}
```

### Why This Works

The approach derives baseline from provenance metadata rather than storing it:

1. **Every task change has `task=<id>` in its description** (via provenance)
2. **`roots()` finds the earliest changes** - those with no ancestors in the set
3. **`parents(roots())` gives the baseline** - the state before any task work
4. **`heads()` gives the final state** - latest changes for the task

**Benefits**:
- No storage needed - derived from jj history
- Retroactive - works on existing tasks without migration
- Self-healing - survives rebases and amends
- Single source of truth - jj history is authoritative

### Edge Cases

| Scenario | Behavior |
|----------|----------|
| No changes found | Error: "No changes found for task" |
| Multiple roots (branched work) | Uses all roots' parents as baseline |
| Task in progress | Shows diff to current heads |
| Subtasks on different branches | Combined diff across all branches |

---

## Error Handling

### No Changes Found (No Provenance)

When no changes have `task=<id>` in their description:

```
No changes found for task xqrmnpst.

The task exists but has no associated code changes in jj history.
This may happen if:
- Task has no code changes yet
- Changes were made without aiki provenance tracking
```

Exit code: 0 (not an error - just informational)

### Task Not Found

```
Error: Task not found: invalid_id
```

Exit code: 1 (error)

### Orphaned Changes (No Baseline)

If `roots()` returns changes with no parents (unlikely but possible):

```
Warning: Task changes have no common baseline - showing full content
```

Falls back to showing the content of the changes without a baseline comparison.

---

## Benefits

1. **Two-command review flow**: `show` for context, `diff` for code - natural progression
2. **Net result, not noise**: Shows final diff, not intermediate edits
3. **No storage required**: Baseline derived from jj history via provenance
4. **Retroactive**: Works on existing tasks without migration
5. **Automatic subtask inclusion**: Parent tasks include all subtask changes
6. **Clean output**: No XML wrapper on diff - just the diff
7. **Task-centric**: Agents think in tasks, not version control primitives

---

## Future Enhancements

### Incremental Diffs

Show only changes since last review:

```bash
aiki task diff xqrmnpst --since <review-task-id>
```

### File Filtering

Show diff for specific files only:

```bash
aiki task diff xqrmnpst src/auth.ts
```

### Interactive Mode

Browse changes interactively:

```bash
aiki task diff xqrmnpst --interactive
```

### Diff Between Tasks

Compare changes between two tasks:

```bash
aiki task diff --from task1 --to task2
```

---

## Summary

Two commands provide a streamlined review workflow:

1. **`aiki task show <id>`** - Task context + diff summary (for closed tasks)
2. **`aiki task diff <id>`** - Pure diff output, no wrapper

### Key Design Decisions

1. **Baseline → Final diff**: Shows net result, not aggregated changes
2. **Derived from history**: Baseline computed from jj revsets, no stored state
3. **Clean separation**: `show` for context, `diff` for code
4. **No XML wrapper**: `diff` outputs jj's native format directly

### Review Workflow

```bash
# Step 1: Understand what you're reviewing
aiki task show xqrmnpst
# Shows: task details, instructions, subtasks, AND diff summary

# Step 2: Read the actual code
aiki task diff xqrmnpst
# Shows: full diff (jj native format)
```

### Implementation Approach

```bash
# The core jj command for diff
jj diff \
  --from 'parents(roots(description("task=<id>") | description("task=<id>.")))' \
  --to 'heads(description("task=<id>") | description("task=<id>."))'
```

# Add --output summary option to task show

## Goal

Make it easier for users to quickly view just the summary/completion comment of a task without seeing all the metadata, instructions, and source references.

## Motivation

The getting-started guide now encourages users to run `aiki task show <task-id>` to see task summaries after Claude or Codex complete work. However, the current output includes a lot of information:
- Full task metadata (ID, status, priority, assignee)
- Instructions (if `--with-instructions`)
- Source references (if `--with-source`)
- Full diff (if `--diff`)
- All comments/events

For the getting-started workflow, users mainly care about **what was done** (the summary/completion comment). A `--output summary` flag would show just that.

## Current State

### OutputFormat enum (cli/src/commands/mod.rs)
```rust
pub enum OutputFormat {
    /// Bare task ID (full 32-char), one per line
    Id,
}
```

Currently only supports `--output id` for bare task IDs.

### Show command (cli/src/commands/task.rs:601-620)
```rust
Show {
    /// Task ID to show
    id: Option<String>,

    /// Show full diffs for all changes made during this task
    #[arg(long)]
    diff: bool,

    /// Expand source references
    #[arg(long)]
    with_source: bool,

    /// Include instructions in output
    #[arg(long)]
    with_instructions: bool,

    /// Output format (e.g., `id` for bare task ID)
    #[arg(long, short = 'o', value_name = "FORMAT")]
    output: Option<super::OutputFormat>,
}
```

### run_show function (cli/src/commands/task.rs:3905)
```rust
fn run_show(
    cwd: &Path,
    id: Option<String>,
    show_diff: bool,
    with_source: bool,
    with_instructions: bool,
    output_format: Option<super::OutputFormat>,
) -> Result<()>
```

Currently handles `OutputFormat::Id` by printing bare task ID.

## Proposed Changes

### 1. Add a separate TaskOutputFormat enum in task.rs

**File:** `cli/src/commands/task.rs`

Rather than adding `Summary` to the shared `OutputFormat` enum in `mod.rs` (which is used by other commands that don't need a summary variant), introduce a task-specific `TaskOutputFormat` enum scoped to `task.rs`:

```rust
/// Output format for task commands that support summary output.
#[derive(Clone, Debug, PartialEq, clap::ValueEnum)]
pub enum TaskOutputFormat {
    /// Bare task ID (full 32-char), one per line
    Id,
    /// Task summary/completion comment only
    Summary,
}
```

The shared `OutputFormat` in `mod.rs` remains unchanged with only `Id`. The `Show` command's `output` field changes from `Option<super::OutputFormat>` to `Option<TaskOutputFormat>`.

### 2. Update run_show to handle TaskOutputFormat

**File:** `cli/src/commands/task.rs` in `run_show` function

The function signature uses `TaskOutputFormat` instead of the shared enum. After resolving the task ID:

```rust
// If --output id, print bare task ID and return
if matches!(output_format, Some(TaskOutputFormat::Id)) {
    println!("{}", task_id);
    return Ok(());
}

// If --output summary, print task summary only (closed tasks only)
if matches!(output_format, Some(TaskOutputFormat::Summary)) {
    if task.status != TaskStatus::Closed {
        return Err(AikiError::InvalidArgument(
            format!("Task {} has no summary (not yet closed)", short_id(&task_id))
        ));
    }
    match task.effective_summary() {
        Some(summary) => {
            println!("{}", summary);
        }
        None => {
            return Err(AikiError::InvalidArgument(
                format!("Task {} is closed but has no summary", short_id(&task_id))
            ));
        }
    }
    return Ok(());
}
```

Uses `task.effective_summary()` to retrieve the summary rather than manually iterating events.

### 3. Update getting-started.md examples

**File:** `cli/docs/getting-started.md`

Update step 4.3 and 4.5 to use the new flag:

```bash
# Before
aiki task show <task-id>

# After (for just the summary)
aiki task show <task-id> --output summary
```

## Implementation Steps

1. **Add TaskOutputFormat enum** in `cli/src/commands/task.rs` with `Id` and `Summary` variants (keep shared `OutputFormat` in `mod.rs` unchanged)
2. **Update run_show** in `cli/src/commands/task.rs` to handle `TaskOutputFormat::Summary`
   - Check task is closed, return error if not
   - Use `task.effective_summary()` to extract the summary
   - Return error if closed but missing summary
3. **Test manually** with a closed task that has a summary
4. **Update getting-started.md** to use `--output summary` in examples
5. **Consider adding tests** in `cli/tests/task_tests.rs` for the new output format

## Edge Cases

Since summaries are required when closing tasks (via `--summary` flag), the edge cases are simplified:

- **Task not closed yet**: Task status is not `Closed` → return `InvalidArgument` error
- **Task closed without summary**: `effective_summary()` returns `None` → return `InvalidArgument` error
- **Summary retrieval**: Uses `task.effective_summary()` which handles the event history internally
- **Won't-do tasks**: These also require `--summary`, so they work the same way

## Alternative Considered

We could also add `--summary-only` as a boolean flag instead of using `--output summary`. However, using `--output` is more consistent with the existing `--output id` pattern and allows for future output formats (e.g., `--output json`, `--output markdown`).

## Success Criteria

After implementation:
```bash
$ aiki task show abc123 --output summary
Fixed authentication bug by adding null check before token access
```

Clean, concise, machine-parseable output showing just what was accomplished.

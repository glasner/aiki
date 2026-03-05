# Fix duplicate subtask creation during decompose

## Root Cause

Two issues compound to produce consistent duplicate subtasks:

### 1. `aiki task add` doesn't support `--output id` (primary cause)

The decompose template instructs the agent to run:
```bash
TASK_ID=$(aiki task add "<step>" --subtask-of {{data.target}} --output id)
aiki task set $TASK_ID --instructions <<'MD'
...
MD
```

But `aiki task add` has no `--output` flag. Clap rejects the command entirely:
```
error: unexpected argument '--output' found
```

The task is **never created** (clap validates before execution). `$TASK_ID` is empty, so `aiki task set` also fails. The agent sees two failures, adapts its approach, and retries — but during adaptation it may re-attempt creation for tasks it already recovered, producing duplicates.

This was latent before commit e485178 but didn't trigger because the old output behavior (`is_tty_stdout()` check) made `aiki task show` return bare IDs when piped, so the agent never actually SAW the template's `--output id` instructions. After e485178 removed the TTY gate, `aiki task show` always returns full content, so the agent now reads and follows the broken template literally.

### 2. No dedup guard on `aiki task add --subtask-of` (amplifier)

`run_add()` in task.rs creates tasks unconditionally. Two calls with the same name and same `--subtask-of` parent produce two identical subtasks. There's no check for existing same-name siblings.

## Changes

### 1. Add `--output` flag to `aiki task add`
**File:** `cli/src/commands/task.rs` (TaskCommands::Add, ~line 305-409)

Add `--output` option to the `Add` variant:
```rust
/// Output format (e.g., `id` for bare task ID on stdout)
#[arg(long, short = 'o', value_name = "FORMAT")]
output: Option<super::OutputFormat>,
```

Thread it through `run()` dispatch and into `run_add()`. In `run_add()`, when `output == Some(OutputFormat::Id)`, print only the task ID to stdout (no `format_action_added`):
```rust
if matches!(output_format, Some(super::OutputFormat::Id)) {
    println!("{}", new_task.id);
} else {
    aiki_print(&format_action_added(&new_task));
}
```

Apply the same logic for the template-based creation path (~line 1697).

### 2. Add dedup guard for `--subtask-of` creation
**File:** `cli/src/commands/task.rs` (run_add, ~line 1748-1855)

After resolving the parent task, check for existing open subtasks with the same name:
```rust
if let Some(ref parent_id) = parent {
    let parent_task = find_task_in_graph(&graph, parent_id)?;
    let parent_id = &parent_task.id;

    // Dedup: reject if an open subtask with same name already exists
    let existing = get_subtasks(&graph, parent_id);
    if let Some(dup) = existing.iter().find(|t| t.name == name && t.status != TaskStatus::Closed) {
        if matches!(output_format, Some(super::OutputFormat::Id)) {
            println!("{}", dup.id);
        } else {
            aiki_print(&format!("Exists {} — {}\n", short_id(&dup.id), dup.name));
        }
        return Ok(());
    }
    // ... existing parent logic continues
}
```

This is idempotent: if the subtask already exists, return its ID instead of creating a duplicate. The agent gets a valid ID either way.

### 3. Add tests

**Dedup test:** Create a parent, add subtask "Foo", add subtask "Foo" again — verify only one exists and the second call returns the first's ID.

**`--output id` test:** Run `aiki task add "Test" --output id` and verify stdout contains only the 32-char task ID.

## Verification

```bash
cargo test --lib
# Manual: run `aiki task add "test" --output id` and confirm bare ID output
# Manual: run `aiki task add "dup" --subtask-of <parent> && aiki task add "dup" --subtask-of <parent>` and confirm no duplicate
```

# Simplify Non-TTY Output Across All Commands

## Context

Multiple commands handle TTY vs non-TTY output, but there are inconsistencies in approach, format, and implementation. While the basic pattern is established, it's scattered across the codebase without centralized utilities or consistent conventions.

**Referenced in:** `ops/now/consolidate-wait.md:24` — "we should probably think about simplifying our non-TTY output on all commands"

## Current State

### What Works

**Pattern established:** Most commands follow this:
```rust
// Human-readable to stderr
eprintln!("{}", markdown_output);

// Machine-readable to stdout when piped
if !std::io::stdout().is_terminal() {
    println!("{}", task_id);
}
```

**Commands following pattern:**
- `plan.rs` — outputs plain task ID
- `review.rs` — outputs plain task ID
- `fix.rs` — outputs plain task ID  
- `explore.rs` — outputs plain task ID
- `epic.rs` — outputs plain task ID via `output_utils::emit()`
- `build.rs` — outputs plain task ID via `output_utils::emit()`

**Shared output formatting:**
- `output.rs` provides `CommandOutput` struct and `format_command_output()` for review/fix
- Ensures consistency between `review` and `fix` commands

### What's Inconsistent

1. **Output format inconsistency** ✅ Resolved
   - All commands now output plain task IDs via `output_utils::emit()`

2. **No centralized TTY detection utility**
   - Every command repeats `if !std::io::stdout().is_terminal() { ... }`
   - 20+ occurrences across 8 files
   - No shared function to DRY this up

3. **Inconsistent stderr usage**
   - Most commands: `eprintln!()` for human output
   - Some commands: mix of `println!()` and `eprintln!()`
   - No clear convention documented

4. **Task command lacks non-TTY optimization**
   - `task.rs` has 38 `println!` / `eprintln!` calls
   - No TTY detection for simplified output
   - Commands like `aiki task list` output rich tables even when piped
   - Should output bare task IDs when stdout is piped

5. **No shared utilities**
   - Each command reimplements output logic
   - No helper for "output ID if piped, rich output if TTY"
   - No shared exit code conventions

## Problems This Causes

1. **Maintenance burden** — Any change to output behavior requires editing 8+ files
2. **Inconsistent UX** — Users must learn different formats for different commands
3. ~~**Harder to compose**~~ ✅ Resolved — all commands now output plain IDs
4. **No clear conventions** — New commands don't have guidance on what format to use
5. **Missed opportunities** — `task` commands could be pipe-friendly but aren't

## Goals

1. **Centralize TTY detection** — Single source of truth for "is stdout a TTY?"
2. **Standardize output format** — Consistent convention across all commands
3. **Create shared utilities** — DRY up common patterns
4. **Document conventions** — Clear guidance for new commands
5. **Optimize task commands** — Make `aiki task list` and friends pipe-friendly

## Proposed Solution

### 1. Centralize in `cli/src/output_utils.rs` (already exists)

> **Note:** `cli/src/output_utils.rs` already exists with `emit()`, `emit_stderr()`, and `emit_stdout()` helpers. The implementation chose a simpler API than the originally proposed `HasId` trait + generic functions. No new file creation needed.

Current API in `cli/src/output_utils.rs`:

```rust
use std::io::IsTerminal;

/// Emit formatted output to stderr (lazy) and an ID to stdout (when piped).
pub fn emit(id: &str, formatter: impl FnOnce() -> String) { ... }

/// Emit formatted output to stderr only (lazy).
pub fn emit_stderr(formatter: impl FnOnce() -> String) { ... }

/// Emit an ID to stdout when piped (non-TTY stdout).
pub fn emit_stdout(id: &str) { ... }
```

Already used by: `plan.rs`, `review.rs`, `fix.rs`, `explore.rs`, `build.rs`, `epic.rs`, `task.rs`.

**Performance optimization:**

The helpers check `stderr.is_terminal()` before calling the formatting closure. This means:

| Context | stderr TTY? | stdout TTY? | Behavior |
|---------|-------------|-------------|----------|
| Interactive terminal | Yes | Yes | Format + show on stderr, no stdout |
| Typical pipe: `aiki task \| head` | Yes | No | Format + show on stderr, IDs on stdout |
| Fully piped: `aiki task \| cat 2>&1` | No | No | Skip formatting, IDs on stdout only |
| Background job: `aiki task &` | No | No | Skip formatting, IDs on stdout only |

This gives us:
- **Zero formatting overhead** when running in fully automated contexts (CI, cron, background agents)
- **User feedback** when running interactively with pipes (`aiki task | head` still shows progress)
- **Clean logs** in CI/automation (no stderr noise when not needed)


### 2. Establish Output Conventions

Document this in code comments and CLAUDE.md:

| Context | stdout | stderr |
|---------|--------|--------|
| TTY (interactive) | (empty) | Human-readable markdown, colors, tables |
| Piped (non-TTY) | Machine-readable IDs only | Same as TTY (logs, progress) |

**Format conventions:**
- **Single task ID:** Plain text, one ID per line: `abc123`
- **Multiple task IDs:** Plain text, one per line
- **Complex data (build/epic with multiple IDs):** Plain IDs, one per line (not XML)

**Exit code conventions:**
- 0 = success
- Non-zero = failure (document what triggers this per command)

### 3. Standardize Build/Epic Output (already done)

> **Note:** Build and epic commands already emit plain IDs for piped execution via `output_utils::emit()`. The XML output format described in the original plan was stale — no migration needed. Both `build.rs` and `epic.rs` already use `output_utils` and support `--output id` for explicit ID-only output.

### 4. Refactor Commands to Use Utilities (largely done)

**Before:**
```rust
eprintln!("{}", markdown);
if !std::io::stdout().is_terminal() {
    println!("{}", task_id);
}
```

**After:**
```rust
use crate::output_utils;

output_utils::emit(&task_id, || {
    format!("## Task Created\n- **ID:** {}\n", task_id)
});
```

**Files already updated:**
- `cli/src/commands/plan.rs` — uses `output_utils::emit()`
- `cli/src/commands/review.rs` — uses `output_utils::emit()`
- `cli/src/commands/fix.rs` — uses `output_utils::emit()`
- `cli/src/commands/explore.rs` — uses `output_utils::emit()`
- `cli/src/commands/epic.rs` — uses `output_utils::emit()`
- `cli/src/commands/build.rs` — uses `output_utils::emit()`

**Still needs work:**
- `cli/src/commands/task.rs` — task listing/show commands should use `output_utils` for consistent piped output

### 5. Add Non-TTY Mode to Task Commands

**Commands that should output IDs when piped:**

| Command | TTY Output | Piped Output |
|---------|------------|--------------|
| `aiki task` | Markdown table | Task IDs (ready tasks only) |
| `aiki task list` | Markdown table | Task IDs (all tasks) |
| `aiki task list --status ready` | Markdown table | Task IDs (ready tasks) |
| `aiki task list --status in_progress` | Markdown table | Task IDs (in-progress tasks) |
| `aiki task show <id>` | Full details | Task ID (echo input) |

**Implementation:**
```rust
// In cli/src/commands/task.rs
use crate::output_utils;

fn run_list(...) -> Result<()> {
    let tasks = /* ... get tasks ... */;
    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();

    // Emit formatted list to stderr, bare IDs to stdout when piped
    output_utils::emit_stderr(|| format_task_table(&tasks));
    for id in ids {
        output_utils::emit_stdout(id);
    }
    Ok(())
}
```

**Use case enabled:**
```bash
# Start all ready tasks
aiki task list --status ready | xargs -I {} aiki task start {}

# Close all in-progress tasks (dangerous but possible)
aiki task list --status in_progress | xargs -I {} aiki task close {} --summary "Batch close"

# Get first ready task
TASK=$(aiki task | head -1)
aiki task start $TASK
```

### 6. Update `consolidate-wait.md`

When implementing `aiki task wait` consolidation (per `ops/now/consolidate-wait.md`), ensure it uses the new utilities:

```rust
use crate::output_utils;

// After waiting completes
output_utils::emit_stderr(|| format_wait_results(&tasks));
for task in &tasks {
    output_utils::emit_stdout(&task.id);
}
```

## Implementation Plan

**Phase 1: Foundation** ✅ Done
1. ~~Create~~ `cli/src/output_utils.rs` already exists with `emit()`, `emit_stderr()`, `emit_stdout()`
2. Already in `cli/src/lib.rs` and `cli/src/main.rs`
3. Conventions documented in module-level doc comments

**Phase 2: Refactor Existing Commands** ✅ Done
4. `plan.rs`, `review.rs`, `fix.rs`, `explore.rs` — all use `output_utils::emit()`

**Phase 3: Standardize Build/Epic** ✅ Already correct
5. `epic.rs` and `build.rs` already emit plain IDs via `output_utils::emit()` — no XML migration needed

**Phase 4: Task Command Optimization** (remaining work)
6. Add `output_utils` usage to `task.rs` `run_list()` for pipe-friendly output
7. Add `output_utils` usage to `run_default()` (default `aiki task` output)
8. Add `output_utils` usage to `task wait` for consistent piped output
9. Test pipe scenarios: `aiki task | head -1`, etc.

**Phase 5: Consolidate Wait** (covered in separate doc)
10. Implement per `ops/now/consolidate-wait.md`
11. Use `output_utils` from Phase 1

## Testing

**Manual tests:**
```bash
# Verify TTY output (should show rich formatting)
aiki task list
aiki plan test.md
aiki build test.md

# Verify piped output (should show IDs only)
aiki task list | cat
aiki plan test.md | cat
aiki build test.md | cat

# Verify composition works
TASK=$(aiki plan test.md | cat)
echo $TASK  # Should be a task ID

# Verify task pipe scenarios
aiki task | head -1
aiki task list --status ready | wc -l
```

**Unit tests:**
```rust
#[test]
fn test_output_if_piped() {
    // Mock stdout as piped
    // Verify output appears on stdout
}

#[test]
fn test_output_if_tty() {
    // Mock stdout as TTY
    // Verify no output on stdout
}
```

## Non-Goals

- **No JSON output format** — Not needed yet, can add `--json` flag later if required
- **No color in piped output** — Already handled by most terminal libraries (they auto-detect TTY)
- **No changes to error handling** — Errors still go to stderr regardless of TTY status
- **No changes to interactive prompts** — Separate concern (handled in `polish-workflow-commands-ux.md`)

## References

- `ops/now/consolidate-wait.md` — Wait command consolidation (depends on Phase 1 utilities)
- `ops/now/polish-workflow-commands-ux.md` — Broader UX polish (mentions non-TTY but focuses on prompts/rendering)
- `cli/src/commands/output.rs` — Existing shared output formatting (review/fix only)

## Benefits

1. **DRY** — Single source of truth for TTY detection and piped output
2. **Consistency** — All commands follow same pattern
3. **Composability** — Task commands become pipe-friendly
4. **Maintainability** — Changes to output behavior happen in one place
5. **Discoverability** — New contributors see utilities and follow the pattern
6. **Documentation** — Conventions are explicit and centralized

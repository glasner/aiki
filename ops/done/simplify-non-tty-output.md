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
- `epic.rs` — outputs XML: `<aiki_epic epic_id="..."/>`
- `build.rs` — outputs XML: `<aiki_build build_id="..." epic_id="..."/>`

**Shared output formatting:**
- `output.rs` provides `CommandOutput` struct and `format_command_output()` for review/fix
- Ensures consistency between `review` and `fix` commands

### What's Inconsistent

1. **Output format inconsistency**
   - Most commands: plain task IDs (`abc123`)
   - Build/epic: XML attributes (`<aiki_build build_id="..." epic_id="..."/>`)
   - No clear rationale for when to use which format

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
3. **Harder to compose** — XML parsing required for `build` output but not others
4. **No clear conventions** — New commands don't have guidance on what format to use
5. **Missed opportunities** — `task` commands could be pipe-friendly but aren't

## Goals

1. **Centralize TTY detection** — Single source of truth for "is stdout a TTY?"
2. **Standardize output format** — Consistent convention across all commands
3. **Create shared utilities** — DRY up common patterns
4. **Document conventions** — Clear guidance for new commands
5. **Optimize task commands** — Make `aiki task list` and friends pipe-friendly

## Proposed Solution

### 1. Create `cli/src/output_utils.rs`

Centralized utilities for output handling:

```rust
//! Output utilities for TTY vs non-TTY contexts

use std::io::IsTerminal;

/// Returns true if stdout is connected to a terminal (not piped).
pub fn is_tty_stdout() -> bool {
    std::io::stdout().is_terminal()
}

/// Returns true if stderr is connected to a terminal.
pub fn is_tty_stderr() -> bool {
    std::io::stderr().is_terminal()
}

/// Trait for types that have an ID field.
///
/// Implement this for domain types to enable automatic ID extraction
/// in output helpers.
pub trait HasId {
    fn id(&self) -> String;
}

/// Output a single item with its ID.
///
/// Uses lazy evaluation to skip expensive formatting when stderr is not a TTY.
/// Automatically extracts the ID via the `HasId` trait.
///
/// # Example
/// ```
/// // With closure
/// output_item(&task, |t| format!("## Task Created\n- **ID:** {}\n", t.id));
/// 
/// // With function pointer
/// output_item(&task, format_task_details);
/// ```
pub fn output_item<T: HasId, F>(item: &T, formatter: F)
where
    F: FnOnce(&T) -> String,
{
    let id = item.id();
    
    // Only format if stderr is a TTY (someone is watching)
    if is_tty_stderr() {
        eprintln!("{}", formatter(item));
    }
    
    // Output ID to stdout when piped
    if !is_tty_stdout() {
        println!("{}", id);
    }
}

/// Output a collection of items with their IDs.
///
/// Uses lazy evaluation to skip expensive formatting when stderr is not a TTY.
/// Automatically extracts IDs via the `HasId` trait.
///
/// # Example
/// ```
/// // With function pointer
/// output_collection(&tasks, format_task_table);
/// 
/// // With closure
/// output_collection(&tasks, |tasks| format!("Found {} tasks", tasks.len()));
/// ```
pub fn output_collection<T: HasId, F>(items: &[T], formatter: F)
where
    F: FnOnce(&[T]) -> String,
{
    let ids: Vec<String> = items.iter().map(|item| item.id()).collect();
    
    // Only format if stderr is a TTY (someone is watching)
    if is_tty_stderr() {
        eprintln!("{}", formatter(items));
    }
    
    // Output IDs to stdout when piped (one per line)
    if !is_tty_stdout() {
        for id in ids {
            println!("{}", id);
        }
    }
}

/// Fallback for outputting raw IDs without domain objects.
///
/// Use this when you only have IDs and no rich formatting.
///
/// # Example
/// ```
/// output_ids(&task_ids, || format!("Created {} tasks", task_ids.len()));
/// ```
pub fn output_ids<F>(ids: &[String], formatter: F)
where
    F: FnOnce() -> String,
{
    // Only format if stderr is a TTY
    if is_tty_stderr() {
        eprintln!("{}", formatter());
    }
    
    // Output IDs to stdout when piped
    if !is_tty_stdout() {
        for id in ids {
            println!("{}", id);
        }
    }
}
```

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

### 3. Standardize Build/Epic Output

**Current:** XML output for piped contexts
```xml
<aiki_build build_id="abc123" epic_id="def456"/>
```

**Proposed:** Plain IDs, one per line (matches other commands)
```
abc123
def456
```

**Rationale:**
- Consistent with other commands
- Easier to consume with standard Unix tools (`head -1`, `tail -1`, etc.)
- No XML parsing needed
- If consumers need structured output, we can add `--json` flag later

**Migration:**
- Update `build.rs` to output plain IDs
- Update `epic.rs` to output plain IDs
- Grep for XML parsing in consumers (unlikely to exist outside aiki itself)
- Document breaking change if any external tools depend on XML

### 4. Refactor Commands to Use Utilities

**Before:**
```rust
eprintln!("{}", markdown);
if !std::io::stdout().is_terminal() {
    println!("{}", task_id);
}
```

**After:**
```rust
use crate::output_utils::output_item;

output_item(&task, |t| format!("## Task Created\n- **ID:** {}\n", t.id));
```

**Note:** Commands need to implement `HasId` for their task types:
```rust
use crate::output_utils::HasId;

impl HasId for Task {
    fn id(&self) -> String {
        self.id.clone()
    }
}
```

**Files to update:**
- `cli/src/commands/plan.rs` (1 occurrence)
- `cli/src/commands/review.rs` (3 occurrences)
- `cli/src/commands/fix.rs` (6 occurrences)
- `cli/src/commands/explore.rs` (3 occurrences)
- `cli/src/commands/epic.rs` (2 occurrences → also change format)
- `cli/src/commands/build.rs` (4 occurrences → also change format)

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
use crate::output_utils::output_collection;

fn run_list(...) -> Result<()> {
    let tasks = /* ... get tasks ... */;
    
    // Clean and simple - automatic ID extraction via HasId trait
    output_collection(&tasks, format_task_table);
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
use crate::output_utils::output_collection;

// After waiting completes
output_collection(&tasks, format_wait_results);
```

## Implementation Plan

**Phase 1: Foundation** (can land independently)
1. Create `cli/src/output_utils.rs` with utilities
2. Add to `cli/src/lib.rs`: `pub mod output_utils;`
3. Write unit tests for utilities
4. Document conventions in comments

**Phase 2: Refactor Existing Commands** (low risk)
5. Update `plan.rs` to use `output_item()`
6. Update `review.rs` to use `output_item()`
7. Update `fix.rs` to use `output_item()`
8. Update `explore.rs` to use `output_item()`

**Phase 3: Standardize Build/Epic** (breaking change)
9. Update `epic.rs` — change XML to plain IDs + use utilities
10. Update `build.rs` — change XML to plain IDs + use utilities
11. Search for XML consumers (likely none outside aiki)
12. Document breaking change in commit message

**Phase 4: Task Command Optimization** (new feature)
13. Add TTY detection to `run_list()` in `task.rs`
14. Add TTY detection to `run_default()` (default `aiki task` output)
15. Test pipe scenarios: `aiki task | head -1`, etc.

**Phase 5: Consolidate Wait** (covered in separate doc)
16. Implement per `ops/now/consolidate-wait.md`
17. Use new utilities from Phase 1

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

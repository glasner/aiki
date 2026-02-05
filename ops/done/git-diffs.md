# Git-Format Diffs with Extended Context

**Date**: 2026-01-30
**Status**: Implemented
**Purpose**: Change diff output format to git format with 5 lines of context for better agent comprehension

**Related Documents**:
- [Task Diff Command](task-diff.md) - `aiki task diff` implementation
- [Review and Fix Commands](review-and-fix.md) - Uses task diffs for code review

---

## Executive Summary

Change all diff output in aiki from jj's native format to **git format with 5 lines of context** (`--git --context 5`). Git diff format is more recognizable to AI agents and more context helps them understand changes better.

**Key Changes**:
- Add `--git --context 5` flags to all jj diff commands
- Applies to `aiki task diff`, flow event payloads, and any other diff output
- No breaking changes - just improved output format

---

## Motivation

### Why Git Format?

AI agents are trained on massive amounts of git diffs from GitHub, GitLab, and open source repositories. Git's unified diff format is:
- **Instantly recognizable**: Agents have seen millions of git diffs
- **Standard format**: `@@` hunk headers, `+`/`-` line prefixes
- **Widely documented**: Easier to find context about the format

### Why 5 Lines of Context?

The default 3 lines of context is often insufficient:
- **Function signatures**: Often need 5+ lines to see the function being modified
- **Control flow**: Need to see surrounding if/else/loop structure
- **Import context**: May need to see what's imported to understand a change
- **Variable declarations**: Variables used in changes may be declared above

Using `-U5` (5 lines) is a good balance between context and noise.

---

## Current State

### jj diff Native Format

```
Modified regular file src/auth.ts:
   40   40: function validateUser(user) {
   41   41:   if (!user) {
   42     +:     throw new Error('User is null');
   43     +:   }
   44   42:   return user.name;
   45   43: }
```

### Git Format with --git --context 5

```diff
diff --git a/src/auth.ts b/src/auth.ts
index abc123..def456 100644
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -38,8 +38,10 @@ import { logger } from './utils';

 export const AUTH_TIMEOUT = 30000;

 function validateUser(user) {
   if (!user) {
+    throw new Error('User is null');
+  }
   return user.name;
 }
```

**Benefits of git format**:
- Standard `diff --git` header
- `@@` hunk headers with line numbers
- Clear `+`/`-` prefixes
- More familiar to agents

---

## Implementation

### Changes Required

#### 1. `aiki task diff` Command

**File**: `cli/src/commands/task.rs`

Current call to jj diff:
```rust
cmd.arg("diff")
   .arg("--from").arg(&from_revset)
   .arg("--to").arg(&to_revset);
```

Updated:
```rust
cmd.arg("diff")
   .arg("--from").arg(&from_revset)
   .arg("--to").arg(&to_revset)
   .arg("--git")
   .arg("--context").arg("5");
```

#### 2. Helper Function for Consistency

Add a helper to ensure all diffs use consistent formatting:

```rust
/// Add standard diff formatting flags: git format with 5 lines of context
fn add_diff_format_args(cmd: &mut Command) -> &mut Command {
    cmd.arg("--git").arg("--context").arg("5")
}
```

#### 3. Audit All Diff Calls

Locations that generate diffs:

| Location | File | Purpose | Status |
|----------|------|---------|--------|
| `aiki task diff` | `commands/task.rs:2466` | Task diff output | ✅ Done |
| `get_change_diff()` | `commands/task.rs:2004` | Per-change diff in `task show --diff` | ✅ Done |
| `get_task_changed_files()` | `commands/task.rs:2549` | File change detection | N/A (uses `--summary`) |
| `run_jj_diff_summary()` | `jj/diff.rs:169` | Delete/move detection | N/A (uses `--summary`) |

**Note**: `jj diff --summary` calls (for file path detection) don't need these flags since they only output file status, not actual diffs.

---

## Testing

### Manual Testing

```bash
# Before: jj native format
jj diff -r @

# After: git format with context
jj diff -r @ --git --context 5

# Task diff
aiki task diff <task-id>
```

### Expected Output

```diff
diff --git a/src/auth.ts b/src/auth.ts
index abc123..def456 100644
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -38,10 +38,12 @@ import { logger } from './utils';

 export const AUTH_TIMEOUT = 30000;

 function validateUser(user) {
   if (!user) {
+    throw new Error('User is null');
+  }
   return user.name;
 }
```

---

## Considerations

### --summary Flag

The `--summary` flag produces a different output format (file status only):
```
M src/auth.ts
A src/new-file.ts
D src/old-file.ts
```

The `--git` and `--context` flags don't apply to `--summary` output. Keep `--summary` calls unchanged.

### --stat Flag

Similarly, `--stat` produces histogram output that isn't affected by diff format flags.

### Performance

Git format adds minimal overhead. The `--context 5` may include slightly more output, but this is negligible.

---

## Implementation Plan

### Phase 1: Core Implementation

1. Add `--git --context 5` to `aiki task diff` command
2. Verify output format is correct
3. Test with various task types (single task, parent with subtasks)

### Phase 2: Audit and Update

1. Grep for all `jj diff` invocations
2. Update non-summary calls to use git format
3. Add helper function for consistency

### Phase 3: Documentation

1. Update task-diff.md examples to show git format
2. Update review template examples

---

## Summary

| Aspect | Before | After |
|--------|--------|-------|
| Format | jj native | git unified |
| Context lines | 3 (default) | 5 |
| Hunk headers | Line numbers only | `@@` with context |
| Line prefixes | `+:` / space | `+` / `-` / space |

**Command change**:
```bash
# Before
jj diff --from <baseline> --to <final>

# After
jj diff --from <baseline> --to <final> --git --context 5
```

**Why this matters**: Agents trained on git diffs will more easily parse and understand the changes, leading to better code review quality.

# Rename `review:` Source Prefix to `task:`

**Date**: 2026-01-18
**Status**: Implemented
**Purpose**: Unify task source linking by replacing `review:` with `task:`

---

## Summary

Replace the `review:` source prefix with `task:` to enable any task to link to any other task as its origin. Since code reviews are already tasks, having a separate `review:` prefix is redundant and limits follow-up linking to only review-originated tasks.

### Before
```
file:     → design docs, plans
review:   → code review tasks only
comment:  → specific review comment
issue:    → external issue tracker
prompt:   → user prompt
```

### After
```
file:     → design docs, plans
task:     → any task (reviews, follow-ups, continuations, etc.)
comment:  → specific comment within a task (unchanged)
issue:    → external issue tracker
prompt:   → user prompt
```

---

## Motivation

1. **Reviews are tasks** - Code reviews create tasks in our system, so `review:abc123` is really just `task:abc123`

2. **Follow-up tasks need linking** - Currently no way to say "this task was spawned from that task" for non-review scenarios:
   - Task discovered during work: "While fixing X, found Y needs attention"
   - Task continuation: "Task A was too big, spawning Task B for remaining work"
   - Dependent work: "Task B exists because Task A revealed this issue"

3. **Simpler mental model** - One fewer concept to remember

---

## Changes Required

### 1. Update Valid Source Prefixes

**File**: `cli/src/commands/task.rs:31`

```rust
// Before
const VALID_SOURCE_PREFIXES: &[&str] = &["file:", "review:", "comment:", "issue:", "prompt:"];

// After
const VALID_SOURCE_PREFIXES: &[&str] = &["file:", "task:", "comment:", "issue:", "prompt:"];
```

### 2. Update Error Message

**File**: `cli/src/error.rs:208`

```rust
// Before
#[error("Invalid task source: '{0}'. Sources must have a prefix: 'file:', 'review:', 'comment:', 'issue:', or 'prompt:'")]

// After
#[error("Invalid task source: '{0}'. Sources must have a prefix: 'file:', 'task:', 'comment:', 'issue:', or 'prompt:'")]
```

### 3. Update Documentation Comments

**File**: `cli/src/tasks/types.rs:111` and `types.rs:177`

```rust
// Before
/// Sources that spawned this task (e.g., "file:ops/now/design.md", "review:abc123")

// After
/// Sources that spawned this task (e.g., "file:ops/now/design.md", "task:abc123")
```

**File**: `cli/src/commands/task.rs:140`

```rust
// Before
/// Source that spawned this task (e.g., "file:ops/now/design.md", "review:abc123")

// After
/// Source that spawned this task (e.g., "file:ops/now/design.md", "task:abc123")
```

### 4. Update AGENTS.md

**File**: `AGENTS.md` - Task Sources section

Update the table:
```markdown
| Prefix | Meaning | Example |
|--------|---------|---------|
| `file:` | File path (design doc, plan) | `file:ops/now/design.md` |
| `task:` | Another task (follow-up, review) | `task:xqrmnpst` |
| `comment:` | Specific comment within a task | `comment:c1a2b3c4` |
| `prompt:` | User prompt that triggered work | `prompt:nzwtoqqr` |
```

Update examples that use `review:`:
```bash
# Before
aiki task add "Fix auth bug" --source review:abc123 --source comment:c1a2b3c4

# After
aiki task add "Fix auth bug" --source task:abc123 --source comment:c1a2b3c4
```

### 5. Update Design Docs

**File**: `ops/done/task-change-linkage.md`

Update the source format table and examples to use `task:` instead of `review:`.

---

## Backwards Compatibility

### Option A: No Migration (Recommended)

The `review:` prefix is new and likely not yet used in production data. Simply change the code and documentation.

### Option B: Accept Both (If Needed)

If there's existing data with `review:` sources:

```rust
const VALID_SOURCE_PREFIXES: &[&str] = &[
    "file:",
    "task:",      // New canonical prefix
    "review:",    // Deprecated, kept for backwards compat
    "comment:",
    "issue:",
    "prompt:"
];
```

Add a note that `review:` is deprecated and will be removed in a future version.

---

## Testing

1. **Unit test**: Verify `task:` prefix is accepted in `validate_sources()`
2. **Integration test**: Create task with `--source task:abc123` and verify it's stored
3. **Query test**: `aiki task list --source task:abc123` returns matching tasks

---

## Non-Changes

- `comment:` prefix stays the same - comments exist within tasks (reviews or otherwise)
- No change to how source filtering/matching works
- No change to storage format (still `source=task:abc123` in change descriptions)

---

## Implementation Order

1. Update `VALID_SOURCE_PREFIXES` constant
2. Update error message
3. Update doc comments in types.rs and task.rs
4. Update AGENTS.md
5. Update ops/done/task-change-linkage.md
6. Run tests to verify nothing breaks

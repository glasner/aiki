# Task Types

**Status**: Future Idea  
**Related**: Task System Phase 1-5

---

## Motivation

Add semantic categorization to distinguish different kinds of work.

---

## Design

```rust
pub enum TaskType {
    Bug,      // Fix incorrect behavior
    Feature,  // Add new functionality
    Chore,    // Refactor, cleanup, docs
    Spike,    // Investigation/research
}
```

---

## Benefits

- Filter tasks by type: `aiki task list --bug`
- Visual distinction in output (color coding, icons)
- Auto-priority based on type (bugs → p0, features → p2)
- Team coordination ("I'll handle bugs, you handle features")

---

## Why Not Phase 1

- Task name already conveys semantic meaning ("Fix X" vs "Add Y")
- Priority is more important than type for queue sorting
- Adds ceremony to task creation (`--type` flag)
- Start simple, add complexity only if users request it

---

## Implementation Notes

- Consider type aliases for common patterns
- Could auto-detect type from task name patterns
- Task ID prefixes could reflect type (`bug-abc`, `feat-def`)

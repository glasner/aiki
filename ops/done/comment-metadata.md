# Structured Comment Metadata

**Status**: Implemented
**Related**: [Review and Fix](review-and-fix.md)

---

## Summary

Add structured key-value data to task comments, enabling machine-readable metadata alongside human-readable text.

## Motivation

Currently `TaskComment` only has `text: String` and `timestamp`. For workflows like code review, comments need structured fields:

- `file` - Path to the file being referenced
- `line` - Line number in the file
- `severity` - error, warning, info
- `category` - functionality, quality, security, performance

Structured data enables:
- Template iteration over comments (create subtask per comment)
- Filtering/querying comments by metadata
- Extracting actionable information without parsing markdown

## Changes

### Data Model

Add `data` field to `TaskComment`:

```rust
// tasks/types.rs
struct TaskComment {
    text: String,
    timestamp: DateTime<Utc>,
    data: HashMap<String, String>,  // NEW
}
```

### CLI

Add `--data key=value` flag to `aiki task comment`:

```bash
aiki task comment --id xqrmnpst \
  --data file=src/auth.ts \
  --data line=42 \
  --data severity=error \
  --data category=quality \
  "Potential null pointer dereference when accessing user.name."
```

**Flag behavior:**
- Can be specified multiple times
- Each `--data` adds one key-value pair
- Keys are strings, values are strings (stored verbatim, no type coercion)
- Stored in `CommentAdded` event alongside text

### Event Storage

```yaml
---
aiki_task_event: v1
task_id: xqrmnpst
event: comment_added
timestamp: 2025-01-15T10:05:00Z
text: |
  Potential null pointer dereference when accessing user.name.
data:
  file: src/auth.ts
  line: "42"
  severity: error
  category: quality
---
```

## Implementation

### Files

- `cli/src/tasks/types.rs` - Add `data: HashMap<String, String>` to `TaskComment`
- `cli/src/commands/task.rs` - Add `--data key=value` to `comment` subcommand
- `cli/src/tasks/storage.rs` - Serialize/deserialize data field in events

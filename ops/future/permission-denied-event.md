# Permission Denied Events

## Status: Future Work

This is a placeholder for future implementation. Not currently prioritized.

## Problem Statement

When a hook rejects a file operation (read, write, or delete), there's no standardized event emitted. This makes it difficult to:
- Log blocked operations for auditing
- Notify users why an operation was rejected
- Build custom flows that react to denied permissions

## Proposed Solution

Add `*.permission_denied` events that fire when hooks block operations:

| Event | When Emitted |
|-------|--------------|
| `read.permission_denied` | Hook blocked a file read |
| `write.permission_denied` | Hook blocked a file write |
| `delete.permission_denied` | Hook blocked a file deletion |

## Payload Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiWritePermissionDeniedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub file_paths: Vec<String>,
    pub reason: Option<String>,  // Why the hook denied it (if provided)
    pub hook_name: Option<String>,  // Which hook blocked it
}

// Similar structures for Read and Delete
```

## Prerequisites

Before implementing this feature:

1. **Standardized hook rejection signal** - Hooks need a way to communicate "denied" vs "error"
   - Currently hooks can fail, but there's no distinction between "rejected" and "crashed"
   - Need to define exit codes or output format for denial

2. **Hook identification** - Need to track which hook blocked the operation
   - Currently hooks run in sequence but aren't individually identified in the event system

3. **Reason extraction** - Hooks should be able to provide a human-readable reason
   - Could use stdout/stderr or a structured format

## Implementation Notes

- These events would be emitted by the hook execution layer, not the vendor layer
- Should not block the main refactor (`ops/current/plan.md`) - can be added later
- No breaking changes required - purely additive

## Use Cases

1. **Audit logging** - Track all blocked operations for compliance
2. **User feedback** - Show why an operation was blocked in the UI
3. **Custom flows** - Trigger alternative actions when permission is denied
4. **Security monitoring** - Alert on suspicious patterns of denied operations

## Related

- `ops/current/plan.md` - Main file operation event refactor (implements `*.permission_asked` and `*.completed`)
- Hook system documentation (TBD)

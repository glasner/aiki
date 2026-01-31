# Lazy Loading for Event Payload Fields

**Status**: Implemented
**Related**: [Task Lifecycle Events](task-events.md)

---

## Summary

Add lazy loading support for expensive event payload fields (like `task.files` and `task.changes`) so JJ queries only run when flow handlers actually use these variables.

## Motivation

Currently, `task.closed` events eagerly query JJ for:
- `task.changes` - JJ change IDs with this task in provenance
- `task.files` - Files modified in those changes

These queries run even if no flow handler uses these variables. For repos with many changes, this adds unnecessary latency to every `aiki task close`.

**Goal**: Only query JJ when a flow handler references `$event.task.files` or `$event.task.changes`.

## Design

### Approach: Lazy Variable Resolver

Extend `VariableResolver` to support lazy computation of specific variables, similar to how it already handles env vars via `set_env_lookup`.

### New API

```rust
// In flows/variables.rs
impl VariableResolver {
    /// Register a lazy variable that computes its value on first access
    pub fn add_lazy_var<F>(&mut self, key: impl Into<String>, compute: F)
    where
        F: FnOnce() -> String + 'static,
    {
        // Store compute function, call it only when variable is resolved
    }
}
```

### Implementation

#### 1. Update VariableResolver

Add lazy variable storage:

```rust
pub struct VariableResolver {
    variables: HashMap<String, String>,
    // NEW: Lazy variables - computed on first access
    lazy_variables: HashMap<String, Box<dyn FnOnce() -> String>>,
    // ... existing fields
}
```

Update `resolve()` to check lazy vars:

```rust
fn resolve_single_var(&mut self, key: &str) -> Option<String> {
    // Check regular variables first
    if let Some(value) = self.variables.get(key) {
        return Some(value.clone());
    }

    // Check lazy variables - compute and cache on first access
    if let Some(compute) = self.lazy_variables.remove(key) {
        let value = compute();
        self.variables.insert(key.to_string(), value.clone());
        return Some(value);
    }

    // Fall back to env lookup
    // ...
}
```

#### 2. Update Event Payload

Remove eager `files` and `changes` from `TaskEventPayload`:

```rust
pub struct TaskEventPayload {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub status: String,
    pub assignee: Option<String>,
    pub outcome: Option<String>,
    pub source: Option<String>,
    // REMOVE: files and changes - now lazy loaded
}
```

No changes needed to `AikiTaskClosedPayload` - the task ID is already available via `payload.task.id`:

```rust
pub struct AikiTaskClosedPayload {
    pub task: TaskEventPayload,  // Already contains .id field
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}
```

#### 3. Update Variable Resolution in Engine

In `create_resolver()`, register lazy vars instead of eager ones:

```rust
crate::events::AikiEvent::TaskClosed(e) => {
    // ... regular vars ...

    // Register lazy provenance vars (using payload.task.id directly)
    let cwd = e.cwd.clone();
    let task_id = e.task.id.clone();  // Use existing task.id field

    resolver.add_lazy_var("event.task.changes", move || {
        let changes = crate::jj::get_changes_for_task(&cwd, &task_id);
        changes.join(" ")
    });

    let cwd2 = e.cwd.clone();
    let task_id2 = e.task.id.clone();
    resolver.add_lazy_var("event.task.files", move || {
        let files = crate::jj::get_files_for_task(&cwd2, &task_id2);
        files.join(" ")
    });
}
```

### Files to Modify

1. **`cli/src/flows/variables.rs`** - Add `lazy_variables` and `add_lazy_var()`
2. **`cli/src/flows/engine.rs`** - Register lazy vars in `create_resolver()`
3. **`cli/src/events/task_started.rs`** - Remove `files`/`changes` from `TaskEventPayload` (or keep as Option for serialization but don't populate)
4. **`cli/src/commands/task.rs`** - Remove eager JJ queries from event emission

### Considerations

#### Thread Safety

`FnOnce` closures can only be called once. After computing, the value is cached in `variables`. This is fine since we resolve each variable at most once per event.

#### Serialization

If event payloads are serialized (e.g., for logging), lazy fields won't be included unless already resolved. This is probably fine - we don't need to log provenance data.

#### Multiple Access

After first access, value is cached in regular `variables` HashMap, so subsequent accesses are fast.

#### Error Handling

JJ queries can fail. Options:
1. Return empty string on error (simple)
2. Return error placeholder like `"<error: jj query failed>"`
3. Propagate error (complex, changes resolver API)

Recommend option 1 for simplicity.

### Future Extensions

This pattern could be used for other expensive computations:
- `$event.diff` - Full diff content (expensive for large changes)
- `$event.blame` - Blame info for changed lines
- `$session.history` - Session command history

## Testing

1. **Unit test**: Verify lazy var is not computed until accessed
2. **Unit test**: Verify value is cached after first access
3. **Integration test**: Verify `task.close` is fast when no hooks use provenance vars
4. **Integration test**: Verify provenance vars work when used in hooks

## Migration

1. Implement lazy loading in `VariableResolver`
2. Update `create_resolver()` to use lazy vars
3. Remove eager JJ queries from `run_close()`
4. Update tests

No breaking changes to flow YAML syntax - `$event.task.files` and `$event.task.changes` continue to work.

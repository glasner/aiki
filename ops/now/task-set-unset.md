# Replace `task update` with `task set` / `task unset`

## Goal

Replace `aiki task update` with two clear commands:
- `aiki task set` — set fields on a task
- `aiki task unset` — clear optional fields back to None

This eliminates the growing collection of unsetting hacks (`--unassign`, empty-string-means-delete for data, `Option<Option<>>` in the event model) and makes unsetting fields a first-class operation.

## Background

The current `task update` command has accumulated workarounds for clearing fields:

| Field | Set | Unset | Hack |
|-------|-----|-------|------|
| assignee | `--for agent` | `--unassign` | Separate bool flag + `Option<Option<String>>` in event |
| data | `--data key=value` | `--data key=` (empty value) | Convention: empty string = delete |
| instructions | `--instructions` (stdin) | ❌ impossible | No mechanism |
| name | `--name "..."` | ❌ impossible | N/A (probably fine — name is required) |
| priority | `--p0`/`--p1`/`--p2`/`--p3` | ❌ impossible | N/A (has default, not nullable) |

The `Option<Option<String>>` pattern for assignee is confusing in both the CLI args and the event model. As we add more optional fields (instructions, possibly others), every one needs its own `--un*` flag and double-option type.

## Design

### CLI surface

**`aiki task set`** — identical to today's `task update`, minus the unset hacks:

```bash
# Set fields (same as today's update)
aiki task set <id> --name "New name"
aiki task set <id> --for claude-code
aiki task set <id> --p0
aiki task set <id> --data key=value
aiki task set <id> --instructions <<'MD'
Do the thing.
MD
```

**`aiki task unset`** — clear optional fields by name:

```bash
# Clear fields
aiki task unset <id> assignee
aiki task unset <id> instructions
aiki task unset <id> data.mykey         # remove single data key
aiki task unset <id> assignee instructions  # multiple at once
```

The positional args to `unset` are field names. Only optional/clearable fields are valid targets:
- `assignee`
- `instructions`
- `data.<key>` (dot-delimited to target specific data keys)

Non-clearable fields (name, priority, status) are rejected with an error message.

### Event model

**New event variant: `TaskEvent::FieldsCleared`**

```rust
FieldsCleared {
    task_id: String,
    /// Field names that were cleared (e.g., ["assignee", "instructions", "data.mykey"])
    fields: Vec<String>,
    timestamp: DateTime<Utc>,
},
```

This is cleaner than extending `Updated` with more `Option<Option<>>` wrappers. A separate event also makes the intent explicit in the event log: "these fields were deliberately cleared" vs "these fields weren't mentioned in the update."

**Simplify `TaskEvent::Updated`**: Remove the `Option<Option<String>>` for assignee. It becomes plain `Option<String>` (Some = set to this, None = no change). The "unset" path goes through `FieldsCleared` instead.

```rust
Updated {
    task_id: String,
    name: Option<String>,
    priority: Option<TaskPriority>,
    assignee: Option<String>,         // simplified: Some = set, None = no change
    data: Option<HashMap<String, String>>,  // no more empty-string-means-delete
    instructions: Option<String>,
    timestamp: DateTime<Utc>,
},
```

### Blank value rejection

`set` rejects blank/empty values and points users to `unset`:

- `--name ""` → error: "Name cannot be empty"
- `--for ""` → error: "Use `aiki task unset <id> assignee` to clear the assignee"
- `--data key=` → error: "Use `aiki task unset <id> data.key` to remove a data key"
- `--instructions` with empty stdin → error: "Use `aiki task unset <id> instructions` to clear instructions"

This keeps things consistent: `set` always sets a non-empty value, `unset` always clears.

### Data field handling

With `set`/`unset` split:
- `aiki task set <id> --data key=value` — sets or overwrites a data key (value must be non-empty)
- `aiki task unset <id> data.mykey` — removes the key

The empty-string-means-delete convention in `Updated` events is removed. Existing events with empty-string values remain valid (backwards compat) but new events won't use that pattern — instead, `FieldsCleared { fields: ["data.mykey"] }` is emitted.

### Materialization

When applying `FieldsCleared`:
```rust
for field in &fields {
    if field == "assignee" {
        task.assignee = None;
    } else if field == "instructions" {
        task.instructions = None;
    } else if let Some(key) = field.strip_prefix("data.") {
        task.data.remove(key);
    }
}
```

### Backwards compatibility

- `aiki task update` becomes an alias for `aiki task set` (print deprecation warning to stderr on first use, remove after a few versions)
- Existing `Updated` events with `Option<Option<String>>` assignee and empty-string data values still materialize correctly — the migration is in the CLI and new event generation, not in the event schema
- Old `Updated` events with `assignee: Some(None)` (unassign) still work via backwards-compat deserialization

## Changes

### 1. Add `FieldsCleared` variant to `TaskEvent`

**File:** `cli/src/tasks/types.rs`

```rust
FieldsCleared {
    task_id: String,
    fields: Vec<String>,
    timestamp: DateTime<Utc>,
},
```

### 2. Simplify `Updated` assignee type

**File:** `cli/src/tasks/types.rs`

Change `assignee: Option<Option<String>>` to `assignee: Option<String>`.

Keep backwards-compat deserialization in storage.rs for old events that used the double-option encoding.

### 3. Rename `Update` to `Set` in `TaskCommands`

**File:** `cli/src/commands/task.rs`

Rename the variant. Remove `--unassign` flag (it moves to `unset`). Remove empty-string convention from `--data` docs. Keep everything else the same.

### 4. Add `Unset` variant to `TaskCommands`

**File:** `cli/src/commands/task.rs`

```rust
/// Clear optional fields on a task
Unset {
    /// Task ID (defaults to current in-progress task)
    id: Option<String>,

    /// Field names to clear (assignee, instructions, data.<key>)
    #[arg(required = true)]
    fields: Vec<String>,
},
```

### 5. Implement `run_set` (renamed from `run_update`)

**File:** `cli/src/commands/task.rs`

Mostly the same as today's `run_update` but:
- Remove `unassign` parameter
- Assignee is `Option<String>` not `Option<Option<String>>`
- Data empty-string convention removed

### 6. Implement `run_unset`

**File:** `cli/src/commands/task.rs`

- Validate field names against allowed set: `assignee`, `instructions`, `data.*`
- Reject non-clearable fields (name, priority) with a clear error
- Write `FieldsCleared` event
- Update in-memory task and print confirmation

### 7. Wire `FieldsCleared` through storage

**File:** `cli/src/tasks/storage.rs`

Serialize: `fields_cleared` event type with comma-separated field names in metadata.
Deserialize: parse back to `FieldsCleared` variant.

### 8. Handle in materialization

**File:** `cli/src/tasks/graph.rs`

Apply `FieldsCleared` by clearing the specified fields on the task.

### 9. Remove `update` subcommand entirely

No alias, no deprecation shim. Clean break — `aiki task update` is gone, replaced by `set`/`unset`.

### 10. Remove `--unassign` from docs/CLAUDE.md

**File:** `CLAUDE.md` (quick reference section mentions `--unassign`)

Update examples to use `aiki task unset <id> assignee`.

## Testing

- Unit test: `FieldsCleared` event round-trips through storage
- Unit test: `unset assignee` on a task with an assignee → assignee becomes None
- Unit test: `unset instructions` → instructions becomes None
- Unit test: `unset data.key` → key removed from data map
- Unit test: `unset name` → rejected with error
- Unit test: backwards compat — old `Updated` events with `assignee: Some(None)` still materialize as unassign
- Unit test: `set --data key=` now stores empty string (not delete)
- Manual: `aiki task set <id> --for claude-code` then `aiki task unset <id> assignee` → assignee cleared

## Non-goals

- No changes to `task add` or `task start` (those create tasks, not modify)
- No changes to `task link` / `task unlink` (different system)
- No changes to `task comment` (comments aren't "fields")
- No migration of old events (they just keep working)

# Add --data Flag to aiki task update

**Date**: 2026-02-05
**Status**: Spec
**Priority**: P1 (blocking plan/build commands)
**Related**: [Plan and Build Commands](plan-and-build-commands.md)

---

## Problem

The `aiki task update` command can update task name, priority, and assignee, but cannot update task data fields. This is needed for the build workflow where we need to store the plan ID after creating it:

```bash
# Build agent creates a plan
aiki plan {{data.spec}}
# Output: plan_id="nzwtoqqrluppzupttosl"

# Need to store this in the build task for reference
aiki task update {{task.id}} --data plan=nzwtoqqrluppzupttosl
```

Currently, task data can only be set at creation time via `aiki task add --data key=value`.

---

## Requirements

### Command Syntax

```bash
aiki task update <task-id> --data <key>=<value>
```

**Arguments:**
- `<task-id>` - Task ID to update (if omitted, updates current in-progress task)

**Options:**
- `--data <key>=<value>` - Set or update a data field (can be specified multiple times)

### Behavior

1. Load the task by ID (or current in-progress task)
2. Parse each `--data key=value` argument
3. Update the task's data fields (merge with existing data)
4. Save the updated task
5. Output confirmation

### Data Handling

- **Merge semantics**: New data fields are added, existing fields are updated
- **Multiple --data flags**: Can specify multiple times to set multiple fields
- **Value types**: All values are stored as strings (same as `aiki task add --data`)
- **No deletion**: To remove a data field, use `--data key=` (empty value)

### Examples

```bash
# Set single data field
aiki task update abc123 --data plan=nzwtoqqrluppzupttosl

# Set multiple data fields
aiki task update abc123 --data plan=xyz789 --data status=building

# Update current in-progress task
aiki task update --data progress=50

# Remove a data field (set to empty)
aiki task update abc123 --data old_field=
```

### Output

```xml
<aiki_task cmd="update" status="ok">
  <updated>
    <task id="abc123" name="Build: ops/now/feature.md"/>
    <data>
      <field key="spec" value="ops/now/feature.md"/>
      <field key="plan" value="nzwtoqqrluppzupttosl"/>
    </data>
  </updated>
</aiki_task>
```

---

## Use Cases

### 1. Build Workflow (Primary Use Case)

The build template needs to store the plan ID after creating it:

```bash
# In build template
aiki plan {{data.spec}}
# Capture plan_id from output
aiki task update {{task.id}} --data plan=$PLAN_ID
```

Later steps can reference `{{data.plan}}` in the template.

### 2. Progress Tracking

```bash
aiki task update --data completed=3 --data total=10
```

### 3. Metadata Updates

```bash
aiki task update --data version=1.2.0 --data commit=abc123
```

---

## Design Considerations

### Why Not Just Use Comments?

Comments are for human-readable progress notes. Data fields are for:
- Machine-readable metadata
- Template variable access via `{{data.key}}`
- Query/filter capabilities (`aiki task list --data key=value`)

### Merge vs Replace

We use **merge semantics** (update existing fields, add new ones) rather than replace because:
- Build templates often set multiple data fields over time
- Less error-prone (don't accidentally wipe existing data)
- Consistent with how `aiki task add --data` works

### Empty Values for Deletion

Setting a field to empty (`--data key=`) removes it from the task data. This provides a way to clean up fields without requiring a separate `--remove-data` flag.

---

## Implementation Notes

### Code Changes

**File**: `cli/src/commands/task.rs` (or `cli/src/commands/task/update.rs`)

1. Add `--data` option to `UpdateCommand` struct:
   ```rust
   #[arg(long, value_name = "KEY=VALUE")]
   data: Vec<String>,
   ```

2. Parse key=value pairs and validate:
   ```rust
   let data_updates: HashMap<String, String> = parse_data_fields(&cmd.data)?;
   
   // Validate all keys using centralized validator
   for key in data_updates.keys() {
       if !crate::validation::is_valid_template_identifier(key) {
           return Err(AikiError::InvalidDataKey(key.to_string()));
       }
   }
   ```

3. Load task and merge data:
   ```rust
   let mut task = tasks::get_task(&task_id)?;
   for (key, value) in data_updates {
       if value.is_empty() {
           task.data.remove(&key);
       } else {
           task.data.insert(key, value);
       }
   }
   tasks::save_task(&task)?;
   ```

### Error Handling

| Scenario | Behavior |
|----------|----------|
| Task not found | Error: "Task not found: <id>" |
| Invalid key=value format | Error: "Invalid --data format. Expected: key=value" |
| No task ID and no in-progress task | Error: "No task specified and no in-progress task" |
| Key contains invalid chars | Error: "Data key must be alphanumeric with underscores" |

### Validation

- Keys must match `[a-z_][a-z0-9_]*` (lowercase, underscore, no spaces)
  - Use existing `is_valid_template_identifier()` from `cli/src/validation.rs`
  - Same validator used for template loop variables and other identifiers
- Values can be any string
- Multiple `--data` flags allowed

---

## Testing

### Unit Tests

```bash
# Set data on task
aiki task add "Test task" --data foo=bar
TASK_ID=$(get last task id)

# Update existing field
aiki task update $TASK_ID --data foo=baz
assert_data $TASK_ID foo=baz

# Add new field
aiki task update $TASK_ID --data new=value
assert_data $TASK_ID foo=baz new=value

# Remove field
aiki task update $TASK_ID --data foo=
assert_data $TASK_ID new=value
```

### Integration Test (Build Workflow)

```bash
# Create build task
aiki task add "Build: test.md" --data spec=ops/now/test.md

# Start it and update with plan ID
aiki task start $BUILD_ID
aiki task update --data plan=xyz123

# Verify data is set
aiki task show $BUILD_ID | grep "plan: xyz123"
```

---

## Acceptance Criteria

- [ ] `aiki task update --data key=value` sets/updates data field
- [ ] Multiple `--data` flags work (set multiple fields at once)
- [ ] Empty value removes field (`--data key=`)
- [ ] Works with task ID or current in-progress task
- [ ] Merges with existing data (doesn't replace all)
- [ ] Error on invalid key format
- [ ] XML output shows updated data
- [ ] Build template can use `aiki task update {{task.id}} --data plan=$PLAN_ID`

---

## Timeline

**Effort**: ~2-4 hours
- Parse --data arguments (~30 min)
- Load/merge/save task data (~1 hour)
- Error handling and validation (~1 hour)
- Tests (~1-2 hours)

**Blocking**: Plan and Build commands implementation
**Priority**: P1 - should be implemented before `aiki plan` and `aiki build`
